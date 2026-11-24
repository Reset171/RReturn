use horizon_client::fast_hash::FxHashMap;
use horizon_core::{
    geometry::Vec3,
    nbt::{self, Limits as NbtLimits, Tag},
    random::LegacyRandom,
};
use horizon_world::compression::Compression;
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub const RRTL_MAGIC: [u8; 4] = *b"RRTL";
pub const RRTL_VERSION: u16 = 1;
pub const RRTL_HEADER_LEN: u64 = 64;
pub const RRTL_SEGMENT_TICKS: usize = 100;
pub const MAX_RRTL_SERVER_RAM_BYTES: usize = 96 * 1024 * 1024;
pub const MAX_RRTL_CLIENT_RAM_BYTES: usize = 32 * 1024 * 1024;
const PARTICLE_LRU_CAP: usize = 8;

pub const RRTL_STREAM_MOBS: u8 = 3;
pub const RRTL_STREAM_PARTICLES: u8 = 6;
pub const RRTL_STREAM_CLIENT_MOBS: u8 = 7;

#[derive(Clone, Copy, Debug)]
pub struct MobPoseRecord {
    pub id: u32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub body_yaw: f32,
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub limb_position: f32,
    pub limb_speed: f32,
    pub attack_anim: f32,
    pub swell: f32,
    pub health: f32,
    pub alive: bool,
    pub hurt_time: i32,
    pub death_time: i32,
    pub sheared: bool,
    pub color: u8,
    pub size: i32,
    pub profession: u8,
    pub villager_level: i32,
    pub variant: u8,
    pub variant2: u8,
    pub age: i32,
    pub sitting: bool,
    pub sitting_pose: bool,
}

pub fn rrtl_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[derive(Clone, Copy, Debug)]
pub struct RrtlDiskSlice {
    pub offset: u64,
    pub len: u32,
    pub crc32: u32,
}

#[derive(Clone)]
pub enum RrtlBlob {
    RamLz4 {
        data: Vec<u8>,
        raw_len: usize,
    },
    DiskZlib {
        slice: RrtlDiskSlice,
        raw_len: usize,
    },
}

impl RrtlBlob {
    pub fn ram_bytes(&self) -> usize {
        match self {
            Self::RamLz4 { data, .. } => data.len(),
            Self::DiskZlib { .. } => 0,
        }
    }

    pub fn disk_bytes(&self) -> usize {
        match self {
            Self::RamLz4 { .. } => 0,
            Self::DiskZlib { slice, .. } => slice.len as usize,
        }
    }

    pub fn raw_bytes(&self) -> usize {
        match self {
            Self::RamLz4 { raw_len, .. } | Self::DiskZlib { raw_len, .. } => *raw_len,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RrtlBranchMeta {
    pub branch_id: u32,
    pub parent_branch_id: Option<u32>,
    pub split_tick: i64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RrtlStats {
    pub active_branch_id: u32,
    pub branch_count: usize,
    pub server_ram_bytes: usize,
    pub server_disk_bytes: usize,
    pub server_raw_bytes: usize,
    pub server_sealed_segments: usize,
    pub client_ram_bytes: usize,
    pub client_disk_bytes: usize,
    pub client_raw_bytes: usize,
    pub client_sealed_segments: usize,
    pub spilled_blobs: usize,
}

struct RrtlDiskFile {
    path: PathBuf,
    file: File,
    write_pos: u64,
    world_seed: i64,
}

impl RrtlDiskFile {
    fn open_fresh(world_root: &Path, world_seed: i64) -> Option<Self> {
        let dir = world_root.join("rreturn");
        fs::create_dir_all(&dir).ok()?;
        let path = dir.join("timeline.rrtl");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .ok()?;
        let _ = file.try_lock();
        let mut header = [0u8; 64];
        header[0..4].copy_from_slice(&RRTL_MAGIC);
        header[4..6].copy_from_slice(&RRTL_VERSION.to_le_bytes());
        header[6..8].copy_from_slice(&1u16.to_le_bytes());
        header[8..16].copy_from_slice(&world_seed.to_le_bytes());
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        header[16..24].copy_from_slice(&now_ms.to_le_bytes());
        file.write_all(&header).ok()?;
        Some(Self {
            path,
            file,
            write_pos: RRTL_HEADER_LEN,
            world_seed,
        })
    }

    fn append_zlib(
        &mut self,
        seg_id: u64,
        branch_id: u32,
        parent_seg_id: u64,
        stream_kind: u8,
        start_tick: i64,
        end_tick: i64,
        zlib_bytes: &[u8],
    ) -> Option<RrtlDiskSlice> {
        let len = u32::try_from(zlib_bytes.len()).ok()?;
        let crc = rrtl_crc32(zlib_bytes);
        self.file.seek(SeekFrom::Start(self.write_pos)).ok()?;
        let mut frame_hdr = [0u8; 48];
        frame_hdr[0..8].copy_from_slice(&seg_id.to_le_bytes());
        frame_hdr[8..12].copy_from_slice(&branch_id.to_le_bytes());
        frame_hdr[12..20].copy_from_slice(&parent_seg_id.to_le_bytes());
        frame_hdr[20] = stream_kind;
        frame_hdr[21] = Compression::Zlib as u8;
        frame_hdr[24..32].copy_from_slice(&start_tick.to_le_bytes());
        frame_hdr[32..40].copy_from_slice(&end_tick.to_le_bytes());
        frame_hdr[40..44].copy_from_slice(&len.to_le_bytes());
        frame_hdr[44..48].copy_from_slice(&crc.to_le_bytes());
        self.file.write_all(&frame_hdr).ok()?;
        let payload_offset = self.write_pos + 48;
        self.file.write_all(zlib_bytes).ok()?;
        self.write_pos = payload_offset + u64::from(len);
        Some(RrtlDiskSlice {
            offset: payload_offset,
            len,
            crc32: crc,
        })
    }

    fn read_slice(&mut self, slice: RrtlDiskSlice) -> Option<Vec<u8>> {
        self.file.seek(SeekFrom::Start(slice.offset)).ok()?;
        let mut buf = vec![0u8; slice.len as usize];
        self.file.read_exact(&mut buf).ok()?;
        if rrtl_crc32(&buf) != slice.crc32 {
            return None;
        }
        Some(buf)
    }
}

struct RrtlClientSegment {
    seg_id: u64,
    start_tick: i64,
    end_tick: i64,
    blob: RrtlBlob,
}

#[derive(Default)]
struct RrtlGlobalStore {
    world_root: Option<PathBuf>,
    world_seed: i64,
    disk: Option<RrtlDiskFile>,
    active_branch_id: u32,
    next_branch_id: u32,
    branches: Vec<RrtlBranchMeta>,
    stats: RrtlStats,
    particle_segments: VecDeque<RrtlClientSegment>,
    particle_lru: VecDeque<(u64, Vec<Vec<ParticleTickState>>)>,
    client_mob_segments: VecDeque<RrtlClientSegment>,
    client_mob_lru: VecDeque<(u64, Vec<Vec<MobPoseRecord>>)>,
}

impl RrtlGlobalStore {
    fn ensure_disk(&mut self) -> Option<&mut RrtlDiskFile> {
        if self.disk.is_none() {
            let root = self.world_root.as_ref()?;
            self.disk = RrtlDiskFile::open_fresh(root, self.world_seed);
        }
        self.disk.as_mut()
    }

    fn spill_lz4_to_disk(
        &mut self,
        seg_id: u64,
        branch_id: u32,
        parent_seg_id: u64,
        stream_kind: u8,
        start_tick: i64,
        end_tick: i64,
        lz4_bytes: &[u8],
        raw_len: usize,
    ) -> Option<RrtlBlob> {
        let raw = Compression::Lz4
            .decode(lz4_bytes, raw_len.saturating_add(64).max(64 * 1024 * 1024))
            .ok()?;
        let zlib = Compression::Zlib.encode(&raw).ok()?;
        let disk = self.ensure_disk()?;
        let slice = disk.append_zlib(
            seg_id,
            branch_id,
            parent_seg_id,
            stream_kind,
            start_tick,
            end_tick,
            &zlib,
        )?;
        self.stats.spilled_blobs += 1;
        Some(RrtlBlob::DiskZlib { slice, raw_len })
    }

    fn decode_blob_raw(&mut self, blob: &RrtlBlob) -> Option<Vec<u8>> {
        match blob {
            RrtlBlob::RamLz4 { data, raw_len } => Compression::Lz4
                .decode(data, raw_len.saturating_add(64).max(64 * 1024 * 1024))
                .ok(),
            RrtlBlob::DiskZlib { slice, raw_len } => {
                let disk = self.disk.as_mut()?;
                let zlib = disk.read_slice(*slice)?;
                Compression::Zlib
                    .decode(&zlib, raw_len.saturating_add(64).max(64 * 1024 * 1024))
                    .ok()
            }
        }
    }

    fn recompute_client_stats(&mut self) {
        let mut ram = 0usize;
        let mut disk = 0usize;
        let mut raw = 0usize;
        for seg in self
            .particle_segments
            .iter()
            .chain(self.client_mob_segments.iter())
        {
            ram += seg.blob.ram_bytes();
            disk += seg.blob.disk_bytes();
            raw += seg.blob.raw_bytes();
        }
        self.stats.client_ram_bytes = ram;
        self.stats.client_disk_bytes = disk;
        self.stats.client_raw_bytes = raw;
        self.stats.client_sealed_segments = self.particle_segments.len();
    }

    fn maybe_spill_client(&mut self) {
        self.recompute_client_stats();
        let branch_id = self.active_branch_id;
        let mut ram_now = self.stats.client_ram_bytes;
        if ram_now <= MAX_RRTL_CLIENT_RAM_BYTES {
            return;
        }
        let plen = self.particle_segments.len();
        for i in 0..plen.saturating_sub(2) {
            if ram_now <= MAX_RRTL_CLIENT_RAM_BYTES {
                break;
            }
            let (sid, s_t, e_t, data_opt) = {
                let seg = &self.particle_segments[i];
                match &seg.blob {
                    RrtlBlob::RamLz4 { data, raw_len } => (
                        seg.seg_id,
                        seg.start_tick,
                        seg.end_tick,
                        Some((data.clone(), *raw_len)),
                    ),
                    RrtlBlob::DiskZlib { .. } => (seg.seg_id, seg.start_tick, seg.end_tick, None),
                }
            };
            if let Some((data, rlen)) = data_opt {
                let freed = data.len();
                if let Some(spilled) = self.spill_lz4_to_disk(
                    sid,
                    branch_id,
                    0,
                    RRTL_STREAM_PARTICLES,
                    s_t,
                    e_t,
                    &data,
                    rlen,
                ) {
                    self.particle_segments[i].blob = spilled;
                    ram_now = ram_now.saturating_sub(freed);
                }
            }
        }
        let mlen = self.client_mob_segments.len();
        for i in 0..mlen.saturating_sub(2) {
            if ram_now <= MAX_RRTL_CLIENT_RAM_BYTES {
                break;
            }
            let (sid, s_t, e_t, data_opt) = {
                let seg = &self.client_mob_segments[i];
                match &seg.blob {
                    RrtlBlob::RamLz4 { data, raw_len } => (
                        seg.seg_id,
                        seg.start_tick,
                        seg.end_tick,
                        Some((data.clone(), *raw_len)),
                    ),
                    RrtlBlob::DiskZlib { .. } => (seg.seg_id, seg.start_tick, seg.end_tick, None),
                }
            };
            if let Some((data, rlen)) = data_opt {
                let freed = data.len();
                if let Some(spilled) = self.spill_lz4_to_disk(
                    sid,
                    branch_id,
                    0,
                    RRTL_STREAM_CLIENT_MOBS,
                    s_t,
                    e_t,
                    &data,
                    rlen,
                ) {
                    self.client_mob_segments[i].blob = spilled;
                    ram_now = ram_now.saturating_sub(freed);
                }
            }
        }
        self.recompute_client_stats();
    }

    fn clear_all(&mut self) {
        if let Some(d) = self.disk.take() {
            let path = d.path.clone();
            drop(d);
            let _ = fs::remove_file(path);
        }
        self.particle_segments.clear();
        self.particle_lru.clear();
        self.client_mob_segments.clear();
        self.client_mob_lru.clear();
        self.active_branch_id = 0;
        self.next_branch_id = 1;
        self.branches.clear();
        self.stats = RrtlStats::default();
    }
}

static RRTL_STORE: OnceLock<Mutex<RrtlGlobalStore>> = OnceLock::new();

fn rrtl_store() -> &'static Mutex<RrtlGlobalStore> {
    RRTL_STORE.get_or_init(|| {
        Mutex::new(RrtlGlobalStore {
            next_branch_id: 1,
            ..Default::default()
        })
    })
}

pub fn rrtl_configure_world(world_root: &Path, world_seed: i64) {
    if let Ok(mut st) = rrtl_store().lock() {
        let changed = st.world_root.as_deref() != Some(world_root) || st.world_seed != world_seed;
        if changed {
            st.clear_all();
            st.world_root = Some(world_root.to_path_buf());
            st.world_seed = world_seed;
            st.branches.push(RrtlBranchMeta {
                branch_id: 0,
                parent_branch_id: None,
                split_tick: 0,
            });
            st.stats.branch_count = 1;
        }
    }
}

pub fn rrtl_begin_branch(split_tick: i64) -> u32 {
    if let Ok(mut st) = rrtl_store().lock() {
        let parent = st.active_branch_id;
        let next_id = st.next_branch_id;
        st.next_branch_id = st.next_branch_id.wrapping_add(1).max(1);
        st.active_branch_id = next_id;
        st.branches.push(RrtlBranchMeta {
            branch_id: next_id,
            parent_branch_id: Some(parent),
            split_tick,
        });
        st.stats.active_branch_id = next_id;
        st.stats.branch_count = st.branches.len();
        next_id
    } else {
        0
    }
}

pub fn rrtl_active_branch() -> u32 {
    rrtl_store()
        .lock()
        .map(|s| s.active_branch_id)
        .unwrap_or(0)
}

pub fn rrtl_update_server_stats(
    ram_bytes: usize,
    disk_bytes: usize,
    raw_bytes: usize,
    sealed_segments: usize,
) {
    if let Ok(mut st) = rrtl_store().lock() {
        st.stats.server_ram_bytes = ram_bytes;
        st.stats.server_disk_bytes = disk_bytes;
        st.stats.server_raw_bytes = raw_bytes;
        st.stats.server_sealed_segments = sealed_segments;
    }
}

pub fn rrtl_spill_server_blob(
    seg_id: u64,
    branch_id: u32,
    parent_seg_id: u64,
    start_tick: i64,
    end_tick: i64,
    blob: &mut RrtlBlob,
) -> bool {
    let RrtlBlob::RamLz4 { data, raw_len } = blob else {
        return false;
    };
    let raw_len = *raw_len;
    if let Ok(mut st) = rrtl_store().lock()
        && let Some(spilled) = st.spill_lz4_to_disk(
            seg_id,
            branch_id,
            parent_seg_id,
            RRTL_STREAM_MOBS,
            start_tick,
            end_tick,
            data,
            raw_len,
        )
    {
        *blob = spilled;
        return true;
    }
    false
}

pub fn rrtl_decode_blob_raw(blob: &RrtlBlob) -> Option<Vec<u8>> {
    rrtl_store().lock().ok()?.decode_blob_raw(blob)
}

pub fn rrtl_memory_stats() -> RrtlStats {
    rrtl_store()
        .lock()
        .map(|s| s.stats)
        .unwrap_or_default()
}

pub fn rrtl_clear_all() {
    if let Ok(mut st) = rrtl_store().lock() {
        st.clear_all();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParticleTickState {
    pub id: u32,
    pub template_id: u32,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub size: f32,
    pub rgba: [f32; 4],
    pub age: i32,
}

fn encode_particles_soa_raw(frames: &[Vec<ParticleTickState>]) -> Vec<u8> {
    let mut raw = Vec::new();
    raw.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    for states in frames {
        let n = states.len();
        raw.extend_from_slice(&(n as u32).to_le_bytes());
        let mut prev_id = 0u32;
        for s in states {
            raw.extend_from_slice(&s.id.wrapping_sub(prev_id).to_le_bytes());
            prev_id = s.id;
        }
        let mut prev_tpl = 0u32;
        for s in states {
            raw.extend_from_slice(&(s.template_id ^ prev_tpl).to_le_bytes());
            prev_tpl = s.template_id;
        }
        for axis in 0..3 {
            let mut prev = 0u64;
            for s in states {
                let bits = s.pos[axis].to_bits();
                raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
                prev = bits;
            }
        }
        for axis in 0..3 {
            let mut prev = 0u64;
            for s in states {
                let bits = s.vel[axis].to_bits();
                raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
                prev = bits;
            }
        }
        let mut prev_sz = 0u32;
        for s in states {
            let bits = s.size.to_bits();
            raw.extend_from_slice(&(bits ^ prev_sz).to_le_bytes());
            prev_sz = bits;
        }
        for ch in 0..4 {
            let mut prev = 0u32;
            for s in states {
                let bits = s.rgba[ch].to_bits();
                raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
                prev = bits;
            }
        }
        for s in states {
            raw.extend_from_slice(&s.age.to_le_bytes());
        }
    }
    raw
}

fn decode_particles_soa_raw(raw: &[u8]) -> Option<Vec<Vec<ParticleTickState>>> {
    let mut pos = 0usize;
    let read_u32 = |buf: &[u8], idx: &mut usize| -> Option<u32> {
        let slice = buf.get(*idx..*idx + 4)?;
        *idx += 4;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    };
    let read_u64 = |buf: &[u8], idx: &mut usize| -> Option<u64> {
        let slice = buf.get(*idx..*idx + 8)?;
        *idx += 8;
        Some(u64::from_le_bytes(slice.try_into().ok()?))
    };
    let frame_count = read_u32(raw, &mut pos)? as usize;
    let mut out = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        let n = read_u32(raw, &mut pos)? as usize;
        let mut ids = Vec::with_capacity(n);
        let mut prev_id = 0u32;
        for _ in 0..n {
            prev_id = prev_id.wrapping_add(read_u32(raw, &mut pos)?);
            ids.push(prev_id);
        }
        let mut tpls = Vec::with_capacity(n);
        let mut prev_tpl = 0u32;
        for _ in 0..n {
            prev_tpl ^= read_u32(raw, &mut pos)?;
            tpls.push(prev_tpl);
        }
        let mut read_f64_col = |buf: &[u8], idx: &mut usize| -> Option<Vec<f64>> {
            let mut col = Vec::with_capacity(n);
            let mut prev = 0u64;
            for _ in 0..n {
                prev ^= read_u64(buf, idx)?;
                col.push(f64::from_bits(prev));
            }
            Some(col)
        };
        let mut read_f32_col = |buf: &[u8], idx: &mut usize| -> Option<Vec<f32>> {
            let mut col = Vec::with_capacity(n);
            let mut prev = 0u32;
            for _ in 0..n {
                prev ^= read_u32(buf, idx)?;
                col.push(f32::from_bits(prev));
            }
            Some(col)
        };
        let px = read_f64_col(raw, &mut pos)?;
        let py = read_f64_col(raw, &mut pos)?;
        let pz = read_f64_col(raw, &mut pos)?;
        let vx = read_f64_col(raw, &mut pos)?;
        let vy = read_f64_col(raw, &mut pos)?;
        let vz = read_f64_col(raw, &mut pos)?;
        let sz = read_f32_col(raw, &mut pos)?;
        let r = read_f32_col(raw, &mut pos)?;
        let g = read_f32_col(raw, &mut pos)?;
        let b = read_f32_col(raw, &mut pos)?;
        let a = read_f32_col(raw, &mut pos)?;
        let mut frame = Vec::with_capacity(n);
        for i in 0..n {
            let age = read_u32(raw, &mut pos)? as i32;
            frame.push(ParticleTickState {
                id: ids[i],
                template_id: tpls[i],
                pos: [px[i], py[i], pz[i]],
                vel: [vx[i], vy[i], vz[i]],
                size: sz[i],
                rgba: [r[i], g[i], b[i], a[i]],
                age,
            });
        }
        out.push(frame);
    }
    Some(out)
}

fn encode_client_mobs_soa_raw(frames: &[Vec<MobPoseRecord>]) -> Vec<u8> {
    let mut raw = Vec::new();
    let total: usize = frames.iter().map(Vec::len).sum();
    raw.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    raw.extend_from_slice(&(total as u32).to_le_bytes());
    for f in frames {
        raw.extend_from_slice(&(f.len() as u32).to_le_bytes());
    }
    let flat: Vec<&MobPoseRecord> = frames.iter().flat_map(|f| f.iter()).collect();
    let mut prev_id = 0u32;
    for p in &flat {
        raw.extend_from_slice(&p.id.wrapping_sub(prev_id).to_le_bytes());
        prev_id = p.id;
    }
    for axis in 0..3 {
        let mut prev = 0u64;
        for p in &flat {
            let bits = p.pos[axis].to_bits();
            raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
            prev = bits;
        }
    }
    macro_rules! c_f32 {
        ($get:expr) => {{
            let mut prev = 0u32;
            for p in &flat {
                let bits = ($get)(*p).to_bits();
                raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
                prev = bits;
            }
        }};
    }
    macro_rules! c_i32 {
        ($get:expr) => {{
            let mut prev = 0u32;
            for p in &flat {
                let bits = ($get)(*p) as u32;
                raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
                prev = bits;
            }
        }};
    }
    c_f32!(|p: &MobPoseRecord| p.yaw);
    c_f32!(|p: &MobPoseRecord| p.body_yaw);
    c_f32!(|p: &MobPoseRecord| p.head_yaw);
    c_f32!(|p: &MobPoseRecord| p.head_pitch);
    c_f32!(|p: &MobPoseRecord| p.limb_position);
    c_f32!(|p: &MobPoseRecord| p.limb_speed);
    c_f32!(|p: &MobPoseRecord| p.attack_anim);
    c_f32!(|p: &MobPoseRecord| p.swell);
    c_f32!(|p: &MobPoseRecord| p.health);
    c_i32!(|p: &MobPoseRecord| p.hurt_time);
    c_i32!(|p: &MobPoseRecord| p.death_time);
    c_i32!(|p: &MobPoseRecord| p.size);
    c_i32!(|p: &MobPoseRecord| p.villager_level);
    c_i32!(|p: &MobPoseRecord| p.age);
    for p in &flat {
        let mut flags = 0u8;
        if p.alive {
            flags |= 1;
        }
        if p.sheared {
            flags |= 2;
        }
        if p.sitting {
            flags |= 4;
        }
        if p.sitting_pose {
            flags |= 8;
        }
        raw.push(flags);
    }
    for p in &flat {
        raw.push(p.color);
    }
    for p in &flat {
        raw.push(p.profession);
    }
    for p in &flat {
        raw.push(p.variant);
    }
    for p in &flat {
        raw.push(p.variant2);
    }
    raw
}

fn decode_client_mobs_soa_raw(raw: &[u8]) -> Option<Vec<Vec<MobPoseRecord>>> {
    let mut pos = 0usize;
    let read_u32 = |buf: &[u8], idx: &mut usize| -> Option<u32> {
        let slice = buf.get(*idx..*idx + 4)?;
        *idx += 4;
        Some(u32::from_le_bytes(slice.try_into().ok()?))
    };
    let read_u64 = |buf: &[u8], idx: &mut usize| -> Option<u64> {
        let slice = buf.get(*idx..*idx + 8)?;
        *idx += 8;
        Some(u64::from_le_bytes(slice.try_into().ok()?))
    };
    let frame_count = read_u32(raw, &mut pos)? as usize;
    let total = read_u32(raw, &mut pos)? as usize;
    let mut lens = Vec::with_capacity(frame_count);
    for _ in 0..frame_count {
        lens.push(read_u32(raw, &mut pos)? as usize);
    }
    let mut ids = Vec::with_capacity(total);
    let mut prev_id = 0u32;
    for _ in 0..total {
        prev_id = prev_id.wrapping_add(read_u32(raw, &mut pos)?);
        ids.push(prev_id);
    }
    let mut read_f64_col = |buf: &[u8], idx: &mut usize| -> Option<Vec<f64>> {
        let mut col = Vec::with_capacity(total);
        let mut prev = 0u64;
        for _ in 0..total {
            prev ^= read_u64(buf, idx)?;
            col.push(f64::from_bits(prev));
        }
        Some(col)
    };
    let mut read_f32_col = |buf: &[u8], idx: &mut usize| -> Option<Vec<f32>> {
        let mut col = Vec::with_capacity(total);
        let mut prev = 0u32;
        for _ in 0..total {
            prev ^= read_u32(buf, idx)?;
            col.push(f32::from_bits(prev));
        }
        Some(col)
    };
    let mut read_i32_col = |buf: &[u8], idx: &mut usize| -> Option<Vec<i32>> {
        let mut col = Vec::with_capacity(total);
        let mut prev = 0u32;
        for _ in 0..total {
            prev ^= read_u32(buf, idx)?;
            col.push(prev as i32);
        }
        Some(col)
    };
    let px = read_f64_col(raw, &mut pos)?;
    let py = read_f64_col(raw, &mut pos)?;
    let pz = read_f64_col(raw, &mut pos)?;
    let yaw = read_f32_col(raw, &mut pos)?;
    let byaw = read_f32_col(raw, &mut pos)?;
    let hyaw = read_f32_col(raw, &mut pos)?;
    let hpitch = read_f32_col(raw, &mut pos)?;
    let lpos = read_f32_col(raw, &mut pos)?;
    let lspd = read_f32_col(raw, &mut pos)?;
    let atk = read_f32_col(raw, &mut pos)?;
    let sw = read_f32_col(raw, &mut pos)?;
    let hp = read_f32_col(raw, &mut pos)?;
    let ht = read_i32_col(raw, &mut pos)?;
    let dt = read_i32_col(raw, &mut pos)?;
    let sz = read_i32_col(raw, &mut pos)?;
    let vill_lvl = read_i32_col(raw, &mut pos)?;
    let age = read_i32_col(raw, &mut pos)?;
    let flags = raw.get(pos..pos + total)?;
    pos += total;
    let colors = raw.get(pos..pos + total)?;
    pos += total;
    let professions = raw.get(pos..pos + total)?;
    pos += total;
    let variants = raw.get(pos..pos + total)?;
    pos += total;
    let variants2 = raw.get(pos..pos + total)?;
    pos += total;
    let mut out = Vec::with_capacity(frame_count);
    let mut cursor = 0usize;
    for len in lens {
        let mut frame = Vec::with_capacity(len);
        for _ in 0..len {
            let i = cursor;
            cursor += 1;
            let f = flags[i];
            frame.push(MobPoseRecord {
                id: ids[i],
                pos: [px[i], py[i], pz[i]],
                yaw: yaw[i],
                body_yaw: byaw[i],
                head_yaw: hyaw[i],
                head_pitch: hpitch[i],
                limb_position: lpos[i],
                limb_speed: lspd[i],
                attack_anim: atk[i],
                swell: sw[i],
                health: hp[i],
                alive: f & 1 != 0,
                hurt_time: ht[i],
                death_time: dt[i],
                sheared: f & 2 != 0,
                color: colors[i],
                size: sz[i],
                profession: professions[i],
                villager_level: vill_lvl[i],
                variant: variants[i],
                variant2: variants2[i],
                age: age[i],
                sitting: f & 4 != 0,
                sitting_pose: f & 8 != 0,
            });
        }
        out.push(frame);
    }
    Some(out)
}

pub fn rrtl_seal_client_segment(
    seg_id: u64,
    start_tick: i64,
    end_tick: i64,
    particle_frames: Vec<Vec<ParticleTickState>>,
    mob_frames: Vec<Vec<MobPoseRecord>>,
) {
    let p_raw = encode_particles_soa_raw(&particle_frames);
    let p_raw_len = p_raw.len();
    let p_lz4 = Compression::Lz4.encode(&p_raw).unwrap_or(p_raw);
    let m_raw = encode_client_mobs_soa_raw(&mob_frames);
    let m_raw_len = m_raw.len();
    let m_lz4 = Compression::Lz4.encode(&m_raw).unwrap_or(m_raw);
    if let Ok(mut st) = rrtl_store().lock() {
        st.particle_segments.push_back(RrtlClientSegment {
            seg_id,
            start_tick,
            end_tick,
            blob: RrtlBlob::RamLz4 {
                data: p_lz4,
                raw_len: p_raw_len,
            },
        });
        st.client_mob_segments.push_back(RrtlClientSegment {
            seg_id,
            start_tick,
            end_tick,
            blob: RrtlBlob::RamLz4 {
                data: m_lz4,
                raw_len: m_raw_len,
            },
        });
        st.maybe_spill_client();
    }
}

pub fn rrtl_load_particle_frame(seg_id: u64, offset: usize) -> Vec<ParticleTickState> {
    let Ok(mut st) = rrtl_store().lock() else {
        return Vec::new();
    };
    if let Some(pos) = st.particle_lru.iter().position(|(id, _)| *id == seg_id) {
        let entry = st.particle_lru.remove(pos).unwrap();
        let res = entry.1.get(offset).cloned().unwrap_or_default();
        st.particle_lru.push_back(entry);
        return res;
    }
    let blob = match st.particle_segments.iter().find(|s| s.seg_id == seg_id) {
        Some(seg) => seg.blob.clone(),
        None => return Vec::new(),
    };
    let Some(raw) = st.decode_blob_raw(&blob) else {
        return Vec::new();
    };
    let Some(decoded) = decode_particles_soa_raw(&raw) else {
        return Vec::new();
    };
    let res = decoded.get(offset).cloned().unwrap_or_default();
    if st.particle_lru.len() >= PARTICLE_LRU_CAP {
        st.particle_lru.pop_front();
    }
    st.particle_lru.push_back((seg_id, decoded));
    res
}

pub fn rrtl_load_client_mobs_frame(seg_id: u64, offset: usize) -> Vec<MobPoseRecord> {
    let Ok(mut st) = rrtl_store().lock() else {
        return Vec::new();
    };
    if let Some(pos) = st.client_mob_lru.iter().position(|(id, _)| *id == seg_id) {
        let entry = st.client_mob_lru.remove(pos).unwrap();
        let res = entry.1.get(offset).cloned().unwrap_or_default();
        st.client_mob_lru.push_back(entry);
        return res;
    }
    let blob = match st.client_mob_segments.iter().find(|s| s.seg_id == seg_id) {
        Some(seg) => seg.blob.clone(),
        None => return Vec::new(),
    };
    let Some(raw) = st.decode_blob_raw(&blob) else {
        return Vec::new();
    };
    let Some(decoded) = decode_client_mobs_soa_raw(&raw) else {
        return Vec::new();
    };
    let res = decoded.get(offset).cloned().unwrap_or_default();
    if st.client_mob_lru.len() >= PARTICLE_LRU_CAP {
        st.client_mob_lru.pop_front();
    }
    st.client_mob_lru.push_back((seg_id, decoded));
    res
}

pub fn rrtl_truncate_client_after(max_tick: i64) {
    if let Ok(mut st) = rrtl_store().lock() {
        st.particle_segments.retain(|s| s.start_tick <= max_tick);
        st.particle_lru.clear();
        st.client_mob_segments.retain(|s| s.start_tick <= max_tick);
        st.client_mob_lru.clear();
        st.recompute_client_stats();
    }
}

pub fn rrtl_clear_client() {
    if let Ok(mut st) = rrtl_store().lock() {
        st.particle_segments.clear();
        st.particle_lru.clear();
        st.client_mob_segments.clear();
        st.client_mob_lru.clear();
        st.recompute_client_stats();
    }
}

#[derive(Clone, Copy)]
pub struct MobCompactPatch {
    pub id: u32,
    pub position: Vec3,
    pub previous: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub previous_yaw: f32,
    pub body_yaw: f32,
    pub previous_body_yaw: f32,
    pub head_yaw: f32,
    pub previous_head_yaw: f32,
    pub head_pitch: f32,
    pub previous_head_pitch: f32,
    pub limb_position: f32,
    pub limb_speed: f32,
    pub previous_limb_speed: f32,
    pub attack_anim: f32,
    pub health: f32,
    pub hurt_time: i32,
    pub invulnerable: i32,
    pub last_hurt: f32,
    pub death_time: i32,
    pub age: u32,
    pub swell: i32,
    pub previous_swell: i32,
    pub ambient_timer: i32,
    pub rng_state: u64,
    pub rng_gaussian: Option<f64>,
    pub anger: i32,
    pub anger_end: i64,
    pub counter: i32,
    pub counter2: i32,
    pub anim_ticks: i32,
    pub egg_time: i32,
    pub flag: bool,
    pub flag2: bool,
    pub sheared: bool,
    pub color: u8,
    pub size: i32,
    pub mount: Option<u32>,
    pub rider: Option<u32>,
    pub leader: Option<u32>,
    pub aggressive: bool,
    pub vector: [f64; 3],
    pub ints: [i32; 6],
    pub profession: u8,
    pub villager_level: i32,
    pub variant: u8,
    pub variant2: u8,
}

impl MobCompactPatch {
    pub fn from_mob(m: &super::super::Mob) -> Self {
        Self {
            id: m.id,
            position: m.position,
            previous: m.previous,
            velocity: m.velocity,
            yaw: m.yaw,
            previous_yaw: m.previous_yaw,
            body_yaw: m.body_yaw,
            previous_body_yaw: m.previous_body_yaw,
            head_yaw: m.head_yaw,
            previous_head_yaw: m.previous_head_yaw,
            head_pitch: m.head_pitch,
            previous_head_pitch: m.previous_head_pitch,
            limb_position: m.limb_position,
            limb_speed: m.limb_speed,
            previous_limb_speed: m.previous_limb_speed,
            attack_anim: m.anim.attack_anim,
            health: m.health,
            hurt_time: m.hurt_time,
            invulnerable: m.invulnerable,
            last_hurt: m.last_hurt,
            death_time: m.death_time,
            age: m.age,
            swell: m.swell,
            previous_swell: m.previous_swell,
            ambient_timer: m.ambient_timer,
            rng_state: m.random.state(),
            rng_gaussian: m.random.cached_gaussian(),
            anger: m.extra.anger,
            anger_end: m.extra.anger_end,
            counter: m.extra.counter,
            counter2: m.extra.counter2,
            anim_ticks: m.extra.anim_ticks,
            egg_time: m.extra.egg_time,
            flag: m.extra.flag,
            flag2: m.extra.flag2,
            sheared: m.extra.sheared,
            color: m.extra.color,
            size: m.extra.size,
            mount: m.extra.mount,
            rider: m.extra.rider,
            leader: m.extra.leader,
            aggressive: m.ai.aggressive,
            vector: m.extra.vector,
            ints: m.extra.ints,
            profession: m.extra.profession.index().unwrap_or(0),
            villager_level: m.extra.villager_level,
            variant: m.extra.variant,
            variant2: m.extra.variant2,
        }
    }

    pub fn apply_to(&self, m: &mut super::super::Mob) {
        m.position = self.position;
        m.previous = self.previous;
        m.velocity = self.velocity;
        m.yaw = self.yaw;
        m.previous_yaw = self.previous_yaw;
        m.body_yaw = self.body_yaw;
        m.previous_body_yaw = self.previous_body_yaw;
        m.head_yaw = self.head_yaw;
        m.previous_head_yaw = self.previous_head_yaw;
        m.head_pitch = self.head_pitch;
        m.previous_head_pitch = self.previous_head_pitch;
        m.limb_position = self.limb_position;
        m.limb_speed = self.limb_speed;
        m.previous_limb_speed = self.previous_limb_speed;
        m.anim.attack_anim = self.attack_anim;
        m.anim.attack_anim_o = self.attack_anim;
        m.health = self.health;
        m.hurt_time = self.hurt_time;
        m.invulnerable = self.invulnerable;
        m.last_hurt = self.last_hurt;
        m.death_time = self.death_time;
        m.age = self.age;
        m.swell = self.swell;
        m.previous_swell = self.previous_swell;
        m.ambient_timer = self.ambient_timer;
        if self.rng_state != 0 || self.rng_gaussian.is_some() {
            m.random = horizon_core::random::EntityRandom::own(LegacyRandom::from((
                self.rng_state,
                self.rng_gaussian,
            )));
        }
        m.extra.anger = self.anger;
        m.extra.anger_end = self.anger_end;
        m.extra.counter = self.counter;
        m.extra.counter2 = self.counter2;
        m.extra.anim_ticks = self.anim_ticks;
        m.extra.egg_time = self.egg_time;
        m.extra.flag = self.flag;
        m.extra.flag2 = self.flag2;
        m.extra.sheared = self.sheared;
        m.extra.color = self.color;
        m.extra.size = self.size;
        m.extra.mount = self.mount;
        m.extra.rider = self.rider;
        m.extra.leader = self.leader;
        m.ai.aggressive = self.aggressive;
        m.extra.vector = self.vector;
        m.extra.ints = self.ints;
        m.extra.profession = horizon_client::mobs::villager::ProfessionState::from_index(self.profession);
        m.extra.villager_level = self.villager_level;
        m.extra.variant = self.variant;
        m.extra.variant2 = self.variant2;
    }
}

pub fn encode_mob_patch_batches_rrtl(
    batches: &[(u16, u16, Vec<MobCompactPatch>)],
) -> RrtlBlob {
    let mut raw = Vec::new();
    let total: usize = batches.iter().map(|(_, _, v)| v.len()).sum();
    raw.extend_from_slice(&(batches.len() as u32).to_le_bytes());
    raw.extend_from_slice(&(total as u32).to_le_bytes());
    for (tick_idx, dim_idx, list) in batches {
        raw.extend_from_slice(&tick_idx.to_le_bytes());
        raw.extend_from_slice(&dim_idx.to_le_bytes());
        raw.extend_from_slice(&(list.len() as u32).to_le_bytes());
    }
    let flat: Vec<&MobCompactPatch> = batches.iter().flat_map(|(_, _, v)| v.iter()).collect();
    let mut prev_u32 = 0u32;
    for p in &flat {
        raw.extend_from_slice(&p.id.wrapping_sub(prev_u32).to_le_bytes());
        prev_u32 = p.id;
    }
    macro_rules! col_f64 {
        ($get:expr) => {{
            let mut prev = 0u64;
            for p in &flat {
                let bits = ($get)(*p).to_bits();
                raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
                prev = bits;
            }
        }};
    }
    macro_rules! col_f32 {
        ($get:expr) => {{
            let mut prev = 0u32;
            for p in &flat {
                let bits = ($get)(*p).to_bits();
                raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
                prev = bits;
            }
        }};
    }
    macro_rules! col_i32 {
        ($get:expr) => {{
            let mut prev = 0u32;
            for p in &flat {
                let bits = ($get)(*p) as u32;
                raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
                prev = bits;
            }
        }};
    }
    macro_rules! col_u64 {
        ($get:expr) => {{
            let mut prev = 0u64;
            for p in &flat {
                let bits = ($get)(*p);
                raw.extend_from_slice(&(bits ^ prev).to_le_bytes());
                prev = bits;
            }
        }};
    }

    col_f64!(|p: &MobCompactPatch| p.position.x);
    col_f64!(|p: &MobCompactPatch| p.position.y);
    col_f64!(|p: &MobCompactPatch| p.position.z);
    col_f64!(|p: &MobCompactPatch| p.previous.x);
    col_f64!(|p: &MobCompactPatch| p.previous.y);
    col_f64!(|p: &MobCompactPatch| p.previous.z);
    col_f64!(|p: &MobCompactPatch| p.velocity.x);
    col_f64!(|p: &MobCompactPatch| p.velocity.y);
    col_f64!(|p: &MobCompactPatch| p.velocity.z);
    col_f64!(|p: &MobCompactPatch| p.vector[0]);
    col_f64!(|p: &MobCompactPatch| p.vector[1]);
    col_f64!(|p: &MobCompactPatch| p.vector[2]);

    col_f32!(|p: &MobCompactPatch| p.yaw);
    col_f32!(|p: &MobCompactPatch| p.previous_yaw);
    col_f32!(|p: &MobCompactPatch| p.body_yaw);
    col_f32!(|p: &MobCompactPatch| p.previous_body_yaw);
    col_f32!(|p: &MobCompactPatch| p.head_yaw);
    col_f32!(|p: &MobCompactPatch| p.previous_head_yaw);
    col_f32!(|p: &MobCompactPatch| p.head_pitch);
    col_f32!(|p: &MobCompactPatch| p.previous_head_pitch);
    col_f32!(|p: &MobCompactPatch| p.limb_position);
    col_f32!(|p: &MobCompactPatch| p.limb_speed);
    col_f32!(|p: &MobCompactPatch| p.previous_limb_speed);
    col_f32!(|p: &MobCompactPatch| p.attack_anim);
    col_f32!(|p: &MobCompactPatch| p.health);
    col_f32!(|p: &MobCompactPatch| p.last_hurt);

    col_i32!(|p: &MobCompactPatch| p.hurt_time);
    col_i32!(|p: &MobCompactPatch| p.invulnerable);
    col_i32!(|p: &MobCompactPatch| p.death_time);
    col_i32!(|p: &MobCompactPatch| p.age as i32);
    col_i32!(|p: &MobCompactPatch| p.swell);
    col_i32!(|p: &MobCompactPatch| p.previous_swell);
    col_i32!(|p: &MobCompactPatch| p.ambient_timer);
    col_i32!(|p: &MobCompactPatch| p.anger);
    col_i32!(|p: &MobCompactPatch| p.counter);
    col_i32!(|p: &MobCompactPatch| p.counter2);
    col_i32!(|p: &MobCompactPatch| p.anim_ticks);
    col_i32!(|p: &MobCompactPatch| p.egg_time);
    col_i32!(|p: &MobCompactPatch| p.size);
    for idx in 0..6 {
        col_i32!(|p: &MobCompactPatch| p.ints[idx]);
    }
    col_i32!(|p: &MobCompactPatch| p.mount.map_or(-1, |v| v as i32));
    col_i32!(|p: &MobCompactPatch| p.rider.map_or(-1, |v| v as i32));
    col_i32!(|p: &MobCompactPatch| p.leader.map_or(-1, |v| v as i32));
    col_i32!(|p: &MobCompactPatch| p.villager_level);

    col_u64!(|p: &MobCompactPatch| p.rng_state);
    col_u64!(|p: &MobCompactPatch| p.anger_end as u64);
    col_u64!(|p: &MobCompactPatch| p.rng_gaussian.map_or(0, f64::to_bits));

    for p in &flat {
        let mut flags = 0u8;
        if p.flag {
            flags |= 1;
        }
        if p.flag2 {
            flags |= 2;
        }
        if p.sheared {
            flags |= 4;
        }
        if p.aggressive {
            flags |= 8;
        }
        if p.rng_gaussian.is_some() {
            flags |= 16;
        }
        raw.push(flags);
    }
    for p in &flat {
        raw.push(p.color);
    }
    for p in &flat {
        raw.push(p.profession);
    }
    for p in &flat {
        raw.push(p.variant);
    }
    for p in &flat {
        raw.push(p.variant2);
    }

    let raw_len = raw.len();
    let data = Compression::Lz4.encode(&raw).unwrap_or(raw);
    RrtlBlob::RamLz4 { data, raw_len }
}

pub fn decode_mob_patch_batches_rrtl(
    blob: &RrtlBlob,
) -> Option<Vec<(u16, u16, Vec<MobCompactPatch>)>> {
    let raw = rrtl_decode_blob_raw(blob)?;
    let mut pos = 0usize;
    let read_u16 = |buf: &[u8], idx: &mut usize| -> Option<u16> {
        let s = buf.get(*idx..*idx + 2)?;
        *idx += 2;
        Some(u16::from_le_bytes(s.try_into().ok()?))
    };
    let read_u32 = |buf: &[u8], idx: &mut usize| -> Option<u32> {
        let s = buf.get(*idx..*idx + 4)?;
        *idx += 4;
        Some(u32::from_le_bytes(s.try_into().ok()?))
    };
    let read_u64 = |buf: &[u8], idx: &mut usize| -> Option<u64> {
        let s = buf.get(*idx..*idx + 8)?;
        *idx += 8;
        Some(u64::from_le_bytes(s.try_into().ok()?))
    };

    let batch_count = read_u32(&raw, &mut pos)? as usize;
    let total = read_u32(&raw, &mut pos)? as usize;
    let mut headers = Vec::with_capacity(batch_count);
    for _ in 0..batch_count {
        let t = read_u16(&raw, &mut pos)?;
        let d = read_u16(&raw, &mut pos)?;
        let n = read_u32(&raw, &mut pos)? as usize;
        headers.push((t, d, n));
    }

    let mut ids = Vec::with_capacity(total);
    let mut prev_id = 0u32;
    for _ in 0..total {
        prev_id = prev_id.wrapping_add(read_u32(&raw, &mut pos)?);
        ids.push(prev_id);
    }

    let mut read_f64_col = |buf: &[u8], idx: &mut usize| -> Option<Vec<f64>> {
        let mut out = Vec::with_capacity(total);
        let mut prev = 0u64;
        for _ in 0..total {
            prev ^= read_u64(buf, idx)?;
            out.push(f64::from_bits(prev));
        }
        Some(out)
    };
    let mut read_f32_col = |buf: &[u8], idx: &mut usize| -> Option<Vec<f32>> {
        let mut out = Vec::with_capacity(total);
        let mut prev = 0u32;
        for _ in 0..total {
            prev ^= read_u32(buf, idx)?;
            out.push(f32::from_bits(prev));
        }
        Some(out)
    };
    let mut read_i32_col = |buf: &[u8], idx: &mut usize| -> Option<Vec<i32>> {
        let mut out = Vec::with_capacity(total);
        let mut prev = 0u32;
        for _ in 0..total {
            prev ^= read_u32(buf, idx)?;
            out.push(prev as i32);
        }
        Some(out)
    };
    let mut read_u64_col = |buf: &[u8], idx: &mut usize| -> Option<Vec<u64>> {
        let mut out = Vec::with_capacity(total);
        let mut prev = 0u64;
        for _ in 0..total {
            prev ^= read_u64(buf, idx)?;
            out.push(prev);
        }
        Some(out)
    };

    let px = read_f64_col(&raw, &mut pos)?;
    let py = read_f64_col(&raw, &mut pos)?;
    let pz = read_f64_col(&raw, &mut pos)?;
    let prx = read_f64_col(&raw, &mut pos)?;
    let pry = read_f64_col(&raw, &mut pos)?;
    let prz = read_f64_col(&raw, &mut pos)?;
    let vx = read_f64_col(&raw, &mut pos)?;
    let vy = read_f64_col(&raw, &mut pos)?;
    let vz = read_f64_col(&raw, &mut pos)?;
    let vec0 = read_f64_col(&raw, &mut pos)?;
    let vec1 = read_f64_col(&raw, &mut pos)?;
    let vec2 = read_f64_col(&raw, &mut pos)?;

    let yaw = read_f32_col(&raw, &mut pos)?;
    let pyaw = read_f32_col(&raw, &mut pos)?;
    let byaw = read_f32_col(&raw, &mut pos)?;
    let pbyaw = read_f32_col(&raw, &mut pos)?;
    let hyaw = read_f32_col(&raw, &mut pos)?;
    let phyaw = read_f32_col(&raw, &mut pos)?;
    let hpitch = read_f32_col(&raw, &mut pos)?;
    let phpitch = read_f32_col(&raw, &mut pos)?;
    let limb_pos = read_f32_col(&raw, &mut pos)?;
    let limb_spd = read_f32_col(&raw, &mut pos)?;
    let plimb_spd = read_f32_col(&raw, &mut pos)?;
    let attack = read_f32_col(&raw, &mut pos)?;
    let health = read_f32_col(&raw, &mut pos)?;
    let last_hurt = read_f32_col(&raw, &mut pos)?;

    let hurt_time = read_i32_col(&raw, &mut pos)?;
    let invuln = read_i32_col(&raw, &mut pos)?;
    let death_time = read_i32_col(&raw, &mut pos)?;
    let age = read_i32_col(&raw, &mut pos)?;
    let swell = read_i32_col(&raw, &mut pos)?;
    let pswell = read_i32_col(&raw, &mut pos)?;
    let amb = read_i32_col(&raw, &mut pos)?;
    let anger = read_i32_col(&raw, &mut pos)?;
    let counter = read_i32_col(&raw, &mut pos)?;
    let counter2 = read_i32_col(&raw, &mut pos)?;
    let anim_ticks = read_i32_col(&raw, &mut pos)?;
    let egg_time = read_i32_col(&raw, &mut pos)?;
    let size = read_i32_col(&raw, &mut pos)?;
    let i0 = read_i32_col(&raw, &mut pos)?;
    let i1 = read_i32_col(&raw, &mut pos)?;
    let i2 = read_i32_col(&raw, &mut pos)?;
    let i3 = read_i32_col(&raw, &mut pos)?;
    let i4 = read_i32_col(&raw, &mut pos)?;
    let i5 = read_i32_col(&raw, &mut pos)?;
    let mount = read_i32_col(&raw, &mut pos)?;
    let rider = read_i32_col(&raw, &mut pos)?;
    let leader = read_i32_col(&raw, &mut pos)?;
    let vill_lvl = read_i32_col(&raw, &mut pos)?;

    let rng_state = read_u64_col(&raw, &mut pos)?;
    let anger_end = read_u64_col(&raw, &mut pos)?;
    let rng_g_bits = read_u64_col(&raw, &mut pos)?;

    let flags = raw.get(pos..pos + total)?;
    pos += total;
    let colors = raw.get(pos..pos + total)?;
    pos += total;
    let professions = raw.get(pos..pos + total)?;
    pos += total;
    let variants = raw.get(pos..pos + total)?;
    pos += total;
    let variants2 = raw.get(pos..pos + total)?;
    pos += total;

    let mut out = Vec::with_capacity(batch_count);
    let mut cursor = 0usize;
    for (tick_idx, dim_idx, count) in headers {
        let mut list = Vec::with_capacity(count);
        for _ in 0..count {
            let i = cursor;
            cursor += 1;
            let f = flags[i];
            list.push(MobCompactPatch {
                id: ids[i],
                position: Vec3::new(px[i], py[i], pz[i]),
                previous: Vec3::new(prx[i], pry[i], prz[i]),
                velocity: Vec3::new(vx[i], vy[i], vz[i]),
                yaw: yaw[i],
                previous_yaw: pyaw[i],
                body_yaw: byaw[i],
                previous_body_yaw: pbyaw[i],
                head_yaw: hyaw[i],
                previous_head_yaw: phyaw[i],
                head_pitch: hpitch[i],
                previous_head_pitch: phpitch[i],
                limb_position: limb_pos[i],
                limb_speed: limb_spd[i],
                previous_limb_speed: plimb_spd[i],
                attack_anim: attack[i],
                health: health[i],
                hurt_time: hurt_time[i],
                invulnerable: invuln[i],
                last_hurt: last_hurt[i],
                death_time: death_time[i],
                age: age[i] as u32,
                swell: swell[i],
                previous_swell: pswell[i],
                ambient_timer: amb[i],
                rng_state: rng_state[i],
                rng_gaussian: (f & 16 != 0).then(|| f64::from_bits(rng_g_bits[i])),
                anger: anger[i],
                anger_end: anger_end[i] as i64,
                counter: counter[i],
                counter2: counter2[i],
                anim_ticks: anim_ticks[i],
                egg_time: egg_time[i],
                flag: f & 1 != 0,
                flag2: f & 2 != 0,
                sheared: f & 4 != 0,
                color: colors[i],
                size: size[i],
                mount: (mount[i] >= 0).then_some(mount[i] as u32),
                rider: (rider[i] >= 0).then_some(rider[i] as u32),
                leader: (leader[i] >= 0).then_some(leader[i] as u32),
                aggressive: f & 8 != 0,
                vector: [vec0[i], vec1[i], vec2[i]],
                ints: [i0[i], i1[i], i2[i], i3[i], i4[i], i5[i]],
                profession: professions[i],
                villager_level: vill_lvl[i],
                variant: variants[i],
                variant2: variants2[i],
            });
        }
        out.push((tick_idx, dim_idx, list));
    }
    Some(out)
}

pub fn rrtl_encode_tag_bytes(tag: &Tag) -> Option<Vec<u8>> {
    nbt::write_network(tag, NbtLimits::saved()).ok()
}

pub fn rrtl_decode_tag_bytes(bytes: &[u8]) -> Option<Tag> {
    nbt::read_network(bytes, NbtLimits::saved())
        .ok()
        .map(|(t, _)| t)
}