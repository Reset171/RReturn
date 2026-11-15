use std::cell::RefCell;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    FlushSave = 0,
    HydrateSnapshot = 1,
    RestoreDiskFiles = 2,
    SaveDirStreaming = 3,
    PlayersOnline = 4,
    ServerGlobals = 5,
    Bosses = 6,
    FutureMca = 7,
    ChunkBlocks = 8,
    ChunkBeLoad = 9,
    SpawnNbt = 10,
    ClientSnapUpdate = 11,
    CapturePlayers = 12,
    CaptureGlobals = 13,
    CaptureRaids = 14,
    CaptureWorldState = 15,
    CaptureChunks = 16,
}

const PHASE_COUNT: usize = 17;

#[derive(Clone, Debug)]
pub struct BenchmarkData {
    pub open: bool,
    pub title: String,
    pub start_time: Option<Instant>,
    pub start_tick: i64,
    pub nanos: [u64; PHASE_COUNT],
    pub counts: [u32; PHASE_COUNT],
    pub blocks_changed: usize,
    pub chunks: usize,
    pub mobs: usize,
    pub players: usize,
    pub last_report: String,
}

impl Default for BenchmarkData {
    fn default() -> Self {
        Self {
            open: false,
            title: String::new(),
            start_time: None,
            start_tick: 0,
            nanos: [0; PHASE_COUNT],
            counts: [0; PHASE_COUNT],
            blocks_changed: 0,
            chunks: 0,
            mobs: 0,
            players: 0,
            last_report: String::new(),
        }
    }
}

std::thread_local! {
    static BENCHMARK: RefCell<BenchmarkData> = RefCell::new(BenchmarkData::default());
}

pub fn bench_begin(title: &str, current_tick: i64) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        b.open = true;
        b.title = title.to_owned();
        b.start_time = Some(Instant::now());
        b.start_tick = current_tick;
        b.nanos = [0; PHASE_COUNT];
        b.counts = [0; PHASE_COUNT];
        b.blocks_changed = 0;
        b.chunks = 0;
        b.mobs = 0;
        b.players = 0;
    });
}

pub fn bench_accum(phase: Phase, phase_start: Instant) {
    let elapsed = phase_start.elapsed().as_nanos() as u64;
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            let idx = phase as usize;
            if idx < PHASE_COUNT {
                b.nanos[idx] += elapsed;
                b.counts[idx] += 1;
            }
        }
    });
}

pub fn bench_add_blocks_changed(n: usize) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            b.blocks_changed += n;
        }
    });
}

pub fn bench_add_chunks(n: usize) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            b.chunks += n;
        }
    });
}

pub fn bench_add_mobs(n: usize) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            b.mobs += n;
        }
    });
}

pub fn bench_add_players(n: usize) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            b.players += n;
        }
    });
}

pub fn bench_end_and_log() -> String {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if !b.open {
            return b.last_report.clone();
        }
        b.open = false;

        let total_nanos = b.start_time.map_or(0, |t| t.elapsed().as_nanos() as u64);
        let is_rollback = b.title.starts_with("ROLLBACK");

        let top_level = if is_rollback {
            &[
                Phase::FlushSave,
                Phase::HydrateSnapshot,
                Phase::RestoreDiskFiles,
                Phase::SaveDirStreaming,
                Phase::PlayersOnline,
                Phase::ServerGlobals,
                Phase::Bosses,
                Phase::FutureMca,
                Phase::ChunkBlocks,
                Phase::ChunkBeLoad,
                Phase::SpawnNbt,
                Phase::ClientSnapUpdate,
            ][..]
        } else {
            &[
                Phase::CapturePlayers,
                Phase::CaptureGlobals,
                Phase::CaptureRaids,
                Phase::CaptureWorldState,
                Phase::CaptureChunks,
            ][..]
        };

        let work_nanos: u64 = top_level.iter().map(|&p| b.nanos[p as usize]).sum();

        let mut out = String::with_capacity(1024);
        out.push_str(&format!(
            "[RReturn][Benchmark] === {}: {:.2} ms (wall) | work {:.2} ms ===",
            b.title,
            total_nanos as f64 / 1_000_000.0,
            work_nanos as f64 / 1_000_000.0
        ));

        let nanos = b.nanos;
        let get_ms = |p: Phase| nanos[p as usize] as f64 / 1_000_000.0;
        let sum_ms = |phases: &[Phase]| -> f64 {
            phases
                .iter()
                .map(|&p| nanos[p as usize] as f64 / 1_000_000.0)
                .sum()
        };

        if is_rollback {
            let disk_init = sum_ms(&[
                Phase::FlushSave,
                Phase::HydrateSnapshot,
                Phase::RestoreDiskFiles,
                Phase::SaveDirStreaming,
            ]);
            if disk_init > 0.0 {
                out.push_str(&format!("\n|-- Disk & Snapshot Init: {:.2} ms", disk_init));
                let fs_ms = get_ms(Phase::FlushSave);
                if fs_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Flush & Save Live: {:.2} ms", fs_ms));
                }
                let hyd_ms = get_ms(Phase::HydrateSnapshot);
                if hyd_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Hydrate Snapshot: {:.2} ms", hyd_ms));
                }
                let disk_ms = get_ms(Phase::RestoreDiskFiles);
                if disk_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Restore Disk Files: {:.2} ms", disk_ms));
                }
                let st_ms = get_ms(Phase::SaveDirStreaming);
                if st_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- SaveDir & Streaming: {:.2} ms", st_ms));
                }
            }

            let players_work = get_ms(Phase::PlayersOnline);
            if players_work > 0.0 || b.players > 0 {
                out.push_str(&format!("\n|-- Players ({}): {:.2} ms", b.players, players_work));
            }

            let world = sum_ms(&[Phase::ServerGlobals, Phase::Bosses, Phase::FutureMca]);
            if world > 0.0 {
                out.push_str(&format!("\n|-- World & Globals: {:.2} ms", world));
                let sg = get_ms(Phase::ServerGlobals);
                if sg > 0.0 {
                    out.push_str(&format!("\n|   |-- ServerGlobals: {:.2} ms", sg));
                }
                let b_ms = get_ms(Phase::Bosses);
                if b_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Bosses & Sculk: {:.2} ms", b_ms));
                }
                let mca_ms = get_ms(Phase::FutureMca);
                if mca_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Clean Future MCA: {:.2} ms", mca_ms));
                }
            }

            let chunk_sum = sum_ms(&[Phase::ChunkBlocks, Phase::ChunkBeLoad]);
            if chunk_sum > 0.0 || b.chunks > 0 {
                out.push_str(&format!(
                    "\n|-- Chunks ({}, blocks changed {}): {:.2} ms",
                    b.chunks, b.blocks_changed, chunk_sum
                ));
                let cb = get_ms(Phase::ChunkBlocks);
                if cb > 0.0 {
                    out.push_str(&format!("\n|   |-- Blocks: {:.2} ms", cb));
                }
                let bel = get_ms(Phase::ChunkBeLoad);
                if bel > 0.0 {
                    out.push_str(&format!("\n|   |-- Block Entities: {:.2} ms", bel));
                }
            }

            let entity_sum = get_ms(Phase::SpawnNbt);
            if entity_sum > 0.0 || b.mobs > 0 {
                out.push_str(&format!("\n|-- Entities ({}): {:.2} ms", b.mobs, entity_sum));
            }

            let cs_ms = get_ms(Phase::ClientSnapUpdate);
            if cs_ms > 0.0 {
                out.push_str(&format!("\n|-- Client Snap Update: {:.2} ms", cs_ms));
            }
        } else {
            out.push_str(&format!(
                "\n|-- Globals: {:.2} ms",
                get_ms(Phase::CaptureGlobals)
            ));
            out.push_str(&format!(
                "\n|-- Raids & Dragon: {:.2} ms",
                get_ms(Phase::CaptureRaids)
            ));
            out.push_str(&format!(
                "\n|-- World State: {:.2} ms",
                get_ms(Phase::CaptureWorldState)
            ));
            out.push_str(&format!(
                "\n|-- Chunks ({}): {:.2} ms",
                b.chunks,
                get_ms(Phase::CaptureChunks)
            ));
        }

        b.last_report = out.clone();
        out
    })
}

pub fn append_to_rreturn_log(world_root: &std::path::Path, text: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;

    if let Some(instance_dir) = world_root.parent().and_then(|p| p.parent()) {
        let logs_dir = instance_dir.join("logs");
        if logs_dir.is_dir() {
            let log_file = logs_dir.join("rreturn.log");
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file) {
                let _ = writeln!(f, "{}", text);
            }
        }
    }

    let rreturn_dir = world_root.join("rreturn");
    let _ = std::fs::create_dir_all(&rreturn_dir);
    let world_log = rreturn_dir.join("benchmark.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(world_log) {
        let _ = writeln!(f, "{}", text);
    }
}