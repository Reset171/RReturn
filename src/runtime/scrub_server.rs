use super::super::{Dim, Game};
use super::capture::{capture_guests_snapshot, snapshot_stopwatches_elapsed};
use super::rrtl::*;
use super::scrub_perf::*;
use super::snapshots::*;
use super::types::*;
use horizon_client::{
    chunk::Chunk,
    command_storage::CommandStorage,
    containers::BlockEntity,
    fast_hash::FxHashMap,
    gamerules::GameRules,
    items::ItemStack,
    raid::Raids,
    save::block_entity_encode,
    scoreboard::Scoreboard,
    sculk::WardenSpawnTracker,
    world::{HealthRules, Weather},
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::Vec3,
    nbt::Tag,
    position::{BlockPos, ChunkPos},
    random::LevelRandom,
    ticks::ChunkTicks,
};
use horizon_mod_runtime::attachments::Attachments;
use horizon_world::level::{BlockEventData, MovingPiston, Ticks};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

pub const MAX_SERVER_SCRUB_TICKS: usize = 24_000;
pub const SERVER_SEGMENT_TICKS: usize = RRTL_SEGMENT_TICKS;
const HOT_MOB_SEGMENTS: usize = 1;
const HEADER_KEYFRAME_STRIDE: usize = 5;

fn in_chunk(pos: Vec3, cx: i32, cz: i32) -> bool {
    (pos.x.floor() as i32 >> 4) == cx && (pos.z.floor() as i32 >> 4) == cz
}

fn in_chunk_arr(pos: [f64; 3], cx: i32, cz: i32) -> bool {
    (pos[0].floor() as i32 >> 4) == cx && (pos[2].floor() as i32 >> 4) == cz
}

fn in_chunk_block(pos: [i32; 3], cx: i32, cz: i32) -> bool {
    (pos[0] >> 4) == cx && (pos[2] >> 4) == cz
}

fn capture_chunk_entities_active(game: &Game<'_>, cx: i32, cz: i32) -> UnloadedChunkEntities {
    let mut out = UnloadedChunkEntities::default();
    let mut ids = HashSet::new();
    for m in game.mobs.iter() {
        if !m.removed && in_chunk(m.position, cx, cz) {
            ids.insert(m.id);
            out.mobs.push(m.clone());
        }
    }
    for d in game.dropped.iter() {
        if in_chunk(d.position, cx, cz) {
            ids.insert(d.id);
            out.dropped.push(d.clone());
        }
    }
    for o in game.orbs.iter() {
        if in_chunk(o.position, cx, cz) {
            ids.insert(o.id);
            out.orbs.push(o.clone());
        }
    }
    for p in game.creatures.projectiles.iter() {
        if in_chunk(p.position, cx, cz) {
            ids.insert(p.id);
            out.projectiles.push(p.clone());
        }
    }
    for b in game.boats.iter() {
        if in_chunk(b.position, cx, cz) {
            ids.insert(b.id);
            out.boats.push(b.clone());
        }
    }
    for c in game.carts.iter() {
        if in_chunk(c.position, cx, cz) {
            ids.insert(c.id);
            out.carts.push(c.clone());
        }
    }
    for t in game.tnts.iter() {
        if in_chunk(t.position, cx, cz) {
            ids.insert(t.id);
            out.tnts.push(t.clone());
        }
    }
    for f in game.falling.iter() {
        if in_chunk(f.position, cx, cz) {
            ids.insert(f.id);
            out.falling.push(f.clone());
        }
    }
    for p in game.paintings.iter() {
        if in_chunk_block(p.pos, cx, cz) {
            ids.insert(p.id);
            out.paintings.push(p.clone());
        }
    }
    for f in game.item_frames.iter() {
        if in_chunk_block(f.pos, cx, cz) {
            ids.insert(f.id);
            out.item_frames.push(f.clone());
        }
    }
    for a in game.armor_stands.iter() {
        if in_chunk(a.position, cx, cz) {
            ids.insert(a.id);
            out.armor_stands.push(a.clone());
        }
    }
    for c in game.cushions.iter() {
        if in_chunk(c.position, cx, cz) {
            ids.insert(c.id);
            out.cushions.push(c.clone());
        }
    }
    for c in game.crystals.iter() {
        if in_chunk(c.position, cx, cz) {
            ids.insert(c.id);
            out.crystals.push(c.clone());
        }
    }
    let registry = game.pack.assets.registry;
    for t in game.tridents.iter() {
        if in_chunk(t.position, cx, cz) {
            ids.insert(t.id);
            let uuid = game.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&t.id);
            out.tridents.push(TridentSnapshot {
                join: t.join,
                id: t.id,
                nbt: t.to_nbt(uuid, registry, raw),
                previous: t.previous,
                in_ground_time: t.in_ground_time,
                no_physics: t.no_physics,
                owner: t.owner,
                guest: t.guest,
            });
        }
    }
    for r in game.rockets.iter() {
        if in_chunk(r.position, cx, cz) {
            ids.insert(r.id);
            let uuid = game.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&r.id);
            out.rockets.push(RocketSnapshot {
                id: r.id,
                nbt: r.to_nbt(uuid, None, raw),
                attached: r.attached,
                guest: r.guest,
                owner: r.owner,
                attached_to: r.attached_to,
            });
        }
    }
    for c in game.breath_clouds.iter() {
        if in_chunk_arr(c.at, cx, cz) {
            ids.insert(c.id);
            let uuid = game.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&c.id);
            out.breath_clouds.push(BreathSnapshot {
                id: c.id,
                nbt: super::super::potions::potion_cloud_to_nbt(c, uuid, raw),
                at: c.at,
                motion: c.motion,
                radius: c.radius,
                age: c.age,
                duration: c.duration,
                from_dragon: c.from_dragon,
                potion_none: c.potion.is_none(),
                victims: c.victims.clone(),
            });
        }
    }
    for f in game.fangs.list.iter() {
        if in_chunk_arr(f.pos, cx, cz) {
            ids.insert(f.id);
            out.fangs.push(f.clone());
        }
    }
    for s in game.ominous_spawners.iter() {
        if in_chunk_arr(s.position, cx, cz) {
            ids.insert(s.id);
            out.ominous_spawners.push((
                s.id,
                horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                    &s.item,
                    Vec3::new(s.position[0], s.position[1], s.position[2]),
                    s.spawn_item_after,
                    s.uuid,
                    s.raw.as_deref(),
                ),
            ));
        }
    }
    for id in ids {
        if let Some(&u) = game.persist.uuids.get(&id) {
            out.persist_uuids.insert(id, u);
        }
        if let Some(tag) = game.persist.raw.get(&id) {
            out.persist_raw.insert(id, tag.clone());
        }
        if let Some(slots) = game.persist.passenger_slots.get(&id) {
            out.persist_passenger_slots.insert(id, slots.clone());
        }
    }
    out
}

fn capture_chunk_entities_parked(
    game: &Game<'_>,
    p: &crate::game::levels::Parked,
    cx: i32,
    cz: i32,
) -> UnloadedChunkEntities {
    let mut out = UnloadedChunkEntities::default();
    let mut ids = HashSet::new();
    for m in p.mobs.iter() {
        if !m.removed && in_chunk(m.position, cx, cz) {
            ids.insert(m.id);
            out.mobs.push(m.clone());
        }
    }
    for d in p.dropped.iter() {
        if in_chunk(d.position, cx, cz) {
            ids.insert(d.id);
            out.dropped.push(d.clone());
        }
    }
    for o in p.orbs.iter() {
        if in_chunk(o.position, cx, cz) {
            ids.insert(o.id);
            out.orbs.push(o.clone());
        }
    }
    for pr in p.projectiles.iter() {
        if in_chunk(pr.position, cx, cz) {
            ids.insert(pr.id);
            out.projectiles.push(pr.clone());
        }
    }
    for b in p.boats.iter() {
        if in_chunk(b.position, cx, cz) {
            ids.insert(b.id);
            out.boats.push(b.clone());
        }
    }
    for c in p.carts.iter() {
        if in_chunk(c.position, cx, cz) {
            ids.insert(c.id);
            out.carts.push(c.clone());
        }
    }
    for t in p.tnts.iter() {
        if in_chunk(t.position, cx, cz) {
            ids.insert(t.id);
            out.tnts.push(t.clone());
        }
    }
    for f in p.falling.iter() {
        if in_chunk(f.position, cx, cz) {
            ids.insert(f.id);
            out.falling.push(f.clone());
        }
    }
    for pa in p.paintings.iter() {
        if in_chunk_block(pa.pos, cx, cz) {
            ids.insert(pa.id);
            out.paintings.push(pa.clone());
        }
    }
    for f in p.item_frames.iter() {
        if in_chunk_block(f.pos, cx, cz) {
            ids.insert(f.id);
            out.item_frames.push(f.clone());
        }
    }
    for a in p.armor_stands.iter() {
        if in_chunk(a.position, cx, cz) {
            ids.insert(a.id);
            out.armor_stands.push(a.clone());
        }
    }
    for c in p.cushions.iter() {
        if in_chunk(c.position, cx, cz) {
            ids.insert(c.id);
            out.cushions.push(c.clone());
        }
    }
    for c in p.crystals.iter() {
        if in_chunk(c.position, cx, cz) {
            ids.insert(c.id);
            out.crystals.push(c.clone());
        }
    }
    let registry = game.pack.assets.registry;
    for t in p.tridents.iter() {
        if in_chunk(t.position, cx, cz) {
            ids.insert(t.id);
            let uuid = p.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
            let raw = p.persist.raw.get(&t.id);
            out.tridents.push(TridentSnapshot {
                join: t.join,
                id: t.id,
                nbt: t.to_nbt(uuid, registry, raw),
                previous: t.previous,
                in_ground_time: t.in_ground_time,
                no_physics: t.no_physics,
                owner: t.owner,
                guest: t.guest,
            });
        }
    }
    for r in p.rockets.iter() {
        if in_chunk(r.position, cx, cz) {
            ids.insert(r.id);
            let uuid = p.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
            let raw = p.persist.raw.get(&r.id);
            out.rockets.push(RocketSnapshot {
                id: r.id,
                nbt: r.to_nbt(uuid, None, raw),
                attached: r.attached,
                guest: r.guest,
                owner: r.owner,
                attached_to: r.attached_to,
            });
        }
    }
    for c in p.breath_clouds.iter() {
        if in_chunk_arr(c.at, cx, cz) {
            ids.insert(c.id);
            let uuid = p.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
            let raw = p.persist.raw.get(&c.id);
            out.breath_clouds.push(BreathSnapshot {
                id: c.id,
                nbt: super::super::potions::potion_cloud_to_nbt(c, uuid, raw),
                at: c.at,
                motion: c.motion,
                radius: c.radius,
                age: c.age,
                duration: c.duration,
                from_dragon: c.from_dragon,
                potion_none: c.potion.is_none(),
                victims: c.victims.clone(),
            });
        }
    }
    for f in p.fangs.list.iter() {
        if in_chunk_arr(f.pos, cx, cz) {
            ids.insert(f.id);
            out.fangs.push(f.clone());
        }
    }
    for s in p.ominous_spawners.iter() {
        if in_chunk_arr(s.position, cx, cz) {
            ids.insert(s.id);
            out.ominous_spawners.push((
                s.id,
                horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                    &s.item,
                    Vec3::new(s.position[0], s.position[1], s.position[2]),
                    s.spawn_item_after,
                    s.uuid,
                    s.raw.as_deref(),
                ),
            ));
        }
    }
    for id in ids {
        if let Some(&u) = p.persist.uuids.get(&id) {
            out.persist_uuids.insert(id, u);
        }
        if let Some(tag) = p.persist.raw.get(&id) {
            out.persist_raw.insert(id, tag.clone());
        }
        if let Some(slots) = p.persist.passenger_slots.get(&id) {
            out.persist_passenger_slots.insert(id, slots.clone());
        }
    }
    out
}

#[derive(Clone)]
pub struct ChunkBaseline {
    pub first_seen_tick: i64,
    pub chunk: Arc<Chunk>,
    pub block_entities: Vec<((i32, i32, i32), BlockEntity)>,
    pub block_names: Vec<((i32, i32, i32), Tag)>,
    pub entities: UnloadedChunkEntities,
}

#[derive(Clone)]
pub struct ChunkHistoryEntry {
    pub tick: i64,
    pub chunk: Arc<Chunk>,
    pub block_entities: Vec<((i32, i32, i32), BlockEntity)>,
    pub block_names: Vec<((i32, i32, i32), Tag)>,
    pub entities: UnloadedChunkEntities,
}

#[derive(Default)]
pub struct UnloadedChunkJournal {
    pub baselines: FxHashMap<(Dim, i32, i32), ChunkBaseline>,
    pub last_seen_chunks: FxHashMap<(Dim, i32, i32), Arc<Chunk>>,
    pub last_modified_tick: FxHashMap<(Dim, i32, i32), i64>,
    pub history: FxHashMap<(Dim, i32, i32), Vec<ChunkHistoryEntry>>,
    pub unloaded_set: HashSet<(Dim, i32, i32)>,
    pub region_first_seen: FxHashMap<PathBuf, i64>,
    pub regions_initialized: bool,
    touch_cursors: HashMap<Dim, (u64, u64)>,
}

impl UnloadedChunkJournal {
    pub fn clear(&mut self) {
        self.baselines.clear();
        self.last_seen_chunks.clear();
        self.last_modified_tick.clear();
        self.history.clear();
        self.unloaded_set.clear();
        self.region_first_seen.clear();
        self.regions_initialized = false;
        self.touch_cursors.clear();
    }

    pub fn scan_regions(&mut self, world_root: &Path, now_tick: i64) {
        let mark_tick = if !self.regions_initialized {
            self.regions_initialized = true;
            i64::MIN
        } else {
            now_tick
        };
        let subfolders = ["region", "entities", "poi"];
        let dims = [
            "dimensions/minecraft/overworld",
            "dimensions/minecraft/the_nether",
            "dimensions/minecraft/the_end",
        ];
        for dim_path in dims {
            for sub in subfolders {
                let rdir = world_root.join(dim_path).join(sub);
                if let Ok(entries) = fs::read_dir(rdir) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_file() {
                            self.region_first_seen.entry(p).or_insert(mark_tick);
                        }
                    }
                }
            }
        }
    }

    fn collect_chunk_bes(
        world: &horizon_client::world::World,
        cx: i32,
        cz: i32,
    ) -> (Vec<((i32, i32, i32), BlockEntity)>, Vec<((i32, i32, i32), Tag)>) {
        let mut bes = Vec::new();
        let mut names = Vec::new();
        for &pos in world.block_entities.in_chunk((cx, cz)) {
            if let Some(be) = world.block_entities.get(&pos) {
                bes.push((pos, be.clone()));
            }
            if let Some(tag) = world.block_names.get(&pos) {
                names.push((pos, tag.clone()));
            }
        }
        (bes, names)
    }

    fn observe_chunk_active(
        &mut self,
        game: &Game<'_>,
        cx: i32,
        cz: i32,
        chunk: &Arc<Chunk>,
        tick: i64,
    ) {
        let key = (game.dim, cx, cz);
        self.unloaded_set.remove(&key);
        if !self.baselines.contains_key(&key) {
            let (block_entities, block_names) = Self::collect_chunk_bes(&game.world, cx, cz);
            let entities = capture_chunk_entities_active(game, cx, cz);
            self.baselines.insert(
                key,
                ChunkBaseline {
                    first_seen_tick: tick,
                    chunk: Arc::clone(chunk),
                    block_entities,
                    block_names,
                    entities,
                },
            );
            self.last_seen_chunks.insert(key, Arc::clone(chunk));
        } else if let Some(prev) = self.last_seen_chunks.get(&key)
            && !Arc::ptr_eq(prev, chunk)
        {
            self.last_modified_tick.insert(key, tick);
            self.last_seen_chunks.insert(key, Arc::clone(chunk));
        }
    }

    fn observe_chunk_parked(
        &mut self,
        game: &Game<'_>,
        dim: Dim,
        parked: &crate::game::levels::Parked,
        cx: i32,
        cz: i32,
        chunk: &Arc<Chunk>,
        tick: i64,
    ) {
        let key = (dim, cx, cz);
        self.unloaded_set.remove(&key);
        if !self.baselines.contains_key(&key) {
            let (block_entities, block_names) = Self::collect_chunk_bes(&parked.world, cx, cz);
            let entities = capture_chunk_entities_parked(game, parked, cx, cz);
            self.baselines.insert(
                key,
                ChunkBaseline {
                    first_seen_tick: tick,
                    chunk: Arc::clone(chunk),
                    block_entities,
                    block_names,
                    entities,
                },
            );
            self.last_seen_chunks.insert(key, Arc::clone(chunk));
        } else if let Some(prev) = self.last_seen_chunks.get(&key)
            && !Arc::ptr_eq(prev, chunk)
        {
            self.last_modified_tick.insert(key, tick);
            self.last_seen_chunks.insert(key, Arc::clone(chunk));
        }
    }

    pub fn observe_game(&mut self, game: &Game<'_>, tick: i64) {
        if !self.regions_initialized {
            self.scan_regions(&game.world_root, tick);
        }
        let active_cursor = self.touch_cursors.get(&game.dim).copied();
        let active_touched: Option<Vec<(i32, i32)>> = active_cursor
            .and_then(|c| game.world.chunks.touched_since(c).map(|it| it.copied().collect()));
        if let Some(keys) = active_touched {
            let mut seen = HashSet::with_capacity(keys.len());
            for (cx, cz) in keys {
                if seen.insert((cx, cz))
                    && let Some(chunk) = game.world.chunks.get(&(cx, cz))
                {
                    self.observe_chunk_active(game, cx, cz, chunk, tick);
                }
            }
        } else {
            for (&(cx, cz), chunk) in game.world.chunks.iter() {
                self.observe_chunk_active(game, cx, cz, chunk, tick);
            }
        }
        self.touch_cursors
            .insert(game.dim, game.world.chunks.touch_cursor());

        for (&dim, parked) in &game.levels.parked {
            let p_cursor = self.touch_cursors.get(&dim).copied();
            let p_touched: Option<Vec<(i32, i32)>> = p_cursor
                .and_then(|c| parked.world.chunks.touched_since(c).map(|it| it.copied().collect()));
            if let Some(keys) = p_touched {
                let mut seen = HashSet::with_capacity(keys.len());
                for (cx, cz) in keys {
                    if seen.insert((cx, cz))
                        && let Some(chunk) = parked.world.chunks.get(&(cx, cz))
                    {
                        self.observe_chunk_parked(game, dim, parked, cx, cz, chunk, tick);
                    }
                }
            } else {
                for (&(cx, cz), chunk) in parked.world.chunks.iter() {
                    self.observe_chunk_parked(game, dim, parked, cx, cz, chunk, tick);
                }
            }
            self.touch_cursors
                .insert(dim, parked.world.chunks.touch_cursor());
        }
    }

    pub fn on_chunk_unloaded(
        &mut self,
        game: &Game<'_>,
        dim: Dim,
        cx: i32,
        cz: i32,
        tick: i64,
    ) {
        self.scan_regions(&game.world_root, tick);
        let key = (dim, cx, cz);
        let was_modified = self.last_modified_tick.contains_key(&key);
        if !was_modified {
            self.baselines.remove(&key);
            self.last_seen_chunks.remove(&key);
            return;
        }
        self.unloaded_set.insert(key);
        let mod_tick = self.last_modified_tick.get(&key).copied().unwrap_or(tick);
        if let Some(last_chunk) = self.last_seen_chunks.remove(&key) {
            let (block_entities, block_names, entities) = if game.dim == dim {
                let (bes, names) = Self::collect_chunk_bes(&game.world, cx, cz);
                let ents = capture_chunk_entities_active(game, cx, cz);
                (bes, names, ents)
            } else if let Some(parked) = game.levels.parked.get(&dim) {
                let (bes, names) = Self::collect_chunk_bes(&parked.world, cx, cz);
                let ents = capture_chunk_entities_parked(game, parked, cx, cz);
                (bes, names, ents)
            } else {
                (Vec::new(), Vec::new(), UnloadedChunkEntities::default())
            };
            let hist = self.history.entry(key).or_default();
            if hist.last().is_none_or(|e| e.tick < mod_tick) {
                hist.push(ChunkHistoryEntry {
                    tick: mod_tick,
                    chunk: last_chunk,
                    block_entities,
                    block_names,
                    entities,
                });
            }
        }
    }

    pub fn build_unloaded_restores(
        &mut self,
        game: &Game<'_>,
        target_tick: i64,
    ) -> (Vec<UnloadedChunkRestore>, Vec<PathBuf>) {
        self.scan_regions(&game.world_root, game.world.game_time);
        let mut restores = Vec::new();
        for (&key, hist) in &self.history {
            let modified_after = hist.iter().any(|e| e.tick > target_tick);
            if !modified_after {
                continue;
            }
            let (dim, cx, cz) = key;
            let is_loaded_now = if game.dim == dim {
                game.world.chunks.contains_key(&(cx, cz))
            } else {
                game.levels
                    .parked
                    .get(&dim)
                    .is_some_and(|p| p.world.chunks.contains_key(&(cx, cz)))
            };
            if is_loaded_now {
                continue;
            }
            let at_target = hist
                .iter()
                .rev()
                .find(|e| e.tick <= target_tick)
                .map(|e| {
                    (
                        Arc::clone(&e.chunk),
                        e.block_entities.clone(),
                        e.block_names.clone(),
                        e.entities.clone(),
                    )
                })
                .or_else(|| {
                    self.baselines.get(&key).map(|b| {
                        (
                            Arc::clone(&b.chunk),
                            b.block_entities.clone(),
                            b.block_names.clone(),
                            b.entities.clone(),
                        )
                    })
                });
            if let Some((chunk, block_entities, block_names, entities)) = at_target {
                restores.push(UnloadedChunkRestore {
                    dim,
                    pos: (cx, cz),
                    chunk: Some(chunk),
                    block_entities,
                    block_names,
                    entities,
                });
            }
        }
        let mut future_regions = Vec::new();
        for (path, &first_seen) in &self.region_first_seen {
            if first_seen > target_tick && path.is_file() {
                future_regions.push(path.clone());
            }
        }
        (restores, future_regions)
    }

    pub fn evict_before(&mut self, min_tick: i64) {
        for hist in self.history.values_mut() {
            if let Some(pos) = hist.iter().rposition(|e| e.tick <= min_tick) {
                if pos > 0 {
                    hist.drain(0..pos);
                }
            }
        }
        self.history.retain(|_, v| !v.is_empty());
        self.last_modified_tick.retain(|_, &mut t| t >= min_tick);
    }

    pub fn truncate_after(&mut self, max_tick: i64) {
        for hist in self.history.values_mut() {
            hist.retain(|e| e.tick <= max_tick);
        }
        self.history.retain(|_, v| !v.is_empty());
        self.last_modified_tick.retain(|_, &mut t| t <= max_tick);
        self.region_first_seen.retain(|_, &mut t| t <= max_tick);
        self.touch_cursors.clear();
    }
}

fn mob_unchanged(live: &super::super::Mob, prev: &super::super::Mob) -> bool {
    live.id == prev.id
        && live.removed == prev.removed
        && live.age == prev.age
        && live.position == prev.position
        && live.previous == prev.previous
        && live.velocity == prev.velocity
        && live.yaw.to_bits() == prev.yaw.to_bits()
        && live.body_yaw.to_bits() == prev.body_yaw.to_bits()
        && live.head_yaw.to_bits() == prev.head_yaw.to_bits()
        && live.head_pitch.to_bits() == prev.head_pitch.to_bits()
        && live.health.to_bits() == prev.health.to_bits()
        && live.hurt_time == prev.hurt_time
        && live.invulnerable == prev.invulnerable
        && live.death_time == prev.death_time
        && live.swell == prev.swell
        && live.ambient_timer == prev.ambient_timer
        && live.random.state() == prev.random.state()
        && live.random.cached_gaussian().map(f64::to_bits)
            == prev.random.cached_gaussian().map(f64::to_bits)
        && live.limb_position.to_bits() == prev.limb_position.to_bits()
        && live.limb_speed.to_bits() == prev.limb_speed.to_bits()
        && live.anim.attack_anim.to_bits() == prev.anim.attack_anim.to_bits()
        && live.extra.anger == prev.extra.anger
        && live.extra.anger_end == prev.extra.anger_end
        && live.extra.counter == prev.extra.counter
        && live.extra.counter2 == prev.extra.counter2
        && live.extra.anim_ticks == prev.extra.anim_ticks
        && live.extra.egg_time == prev.extra.egg_time
        && live.extra.flag == prev.extra.flag
        && live.extra.flag2 == prev.extra.flag2
        && live.extra.sheared == prev.extra.sheared
        && live.extra.color == prev.extra.color
        && live.extra.size == prev.extra.size
        && live.extra.mount == prev.extra.mount
        && live.extra.rider == prev.extra.rider
        && live.extra.leader == prev.extra.leader
        && live.ai.aggressive == prev.ai.aggressive
}

fn chunk_ticks_eq(live: Option<&ChunkTicks>, cached: &ChunkTicks) -> bool {
    match live {
        None => cached.count() == 0,
        Some(t) => t.count() == cached.count() && t.queued() == cached.queued(),
    }
}

#[derive(Clone)]
struct CowGuestRecord {
    uuid: u128,
    dimension: Arc<str>,
    position: [f64; 3],
    velocity: Vec3,
    yaw: f32,
    pitch: f32,
    head_yaw: f32,
    on_ground: bool,
    ground: horizon_world::ground_support::GroundState,
    game_mode: i32,
    previous_game_mode: Option<i32>,
    flying: bool,
    flying_speed: f32,
    allow_flight: bool,
    sprinting: bool,
    sneaking: bool,
    fall_flying: bool,
    sleeping: bool,
    shoulders: [Option<i32>; 2],
    living_flags: i8,
    stuck: (i32, i32),
    bed: Option<[i32; 3]>,
    sleep_timer: u32,
    vehicle: Option<u32>,
    vehicle_order: i64,
    vehicle_good: Option<[f64; 3]>,
    spawn: Option<[f64; 3]>,
    respawn_elsewhere: Option<(String, [i32; 3], f32, f32, bool)>,
    last_death: Option<(String, [i32; 3])>,
    health: f32,
    absorption: f32,
    food: i32,
    saturation: f32,
    exhaustion: f32,
    food_timer: i32,
    fire_ticks: i32,
    air: i32,
    ticks_frozen: i32,
    fall_distance: f64,
    xp_level: i32,
    xp_progress: f32,
    xp_total: i32,
    xp_seed: i32,
    inventory: Arc<Vec<Option<ItemStack>>>,
    armor: Arc<[Option<ItemStack>; 4]>,
    offhand: Arc<Option<ItemStack>>,
    carried: Arc<Option<ItemStack>>,
    ender_items: Arc<Vec<Option<ItemStack>>>,
    selected: usize,
    craft_grid: Arc<[Option<ItemStack>; 4]>,
    craft_result: Arc<Option<ItemStack>>,
    effects: Arc<Vec<horizon_client::effects::EffectInstance>>,
    effects_tick_count: i32,
    effects_changes: u32,
    shield_cooldown: u32,
    cooldowns: Arc<horizon_client::cooldowns::Cooldowns>,
    recipe_book: Arc<horizon_client::recipe_book::Settings>,
    advancements: Arc<horizon_client::advancements::Progress>,
    stats: Arc<BTreeMap<String, BTreeMap<String, i32>>>,
    seen_credits: bool,
    post_effects: Arc<Vec<String>>,
    attachments: Arc<horizon_mod_runtime::attachments::ResidentAttachments>,
    saved: Arc<Option<Tag>>,
    hook: Option<HookSnapshot>,
    motion: Option<Arc<horizon_client::motion::PlayerMotion>>,
}

impl CowGuestRecord {
    fn to_guest_snapshot(&self, game_time: i64) -> GuestSnapshot {
        let motion = self.motion.as_ref().map(|m_arc| {
            let mut m = (**m_arc).clone();
            let pos = Vec3::new(self.position[0], self.position[1], self.position[2]);
            m.position = pos;
            m.old_position = pos;
            m.velocity = self.velocity;
            m.yaw = self.yaw;
            m.head_yaw = self.head_yaw;
            m.pitch = self.pitch;
            m.ground = self.ground;
            m.fall_distance = self.fall_distance;
            let _ = m.set_sprinting(self.sprinting);
            m.shift_key_down = self.sneaking;
            m.local_crouching = self.sneaking;
            m.fall_flying = self.fall_flying;
            m.sleeping = self.sleeping;
            m.fire_ticks = self.fire_ticks;
            m.air = self.air;
            m.ticks_frozen = self.ticks_frozen;
            let _ = m.set_health(self.health.max(1.0), false);
            let _ = m.internal_set_absorption(self.absorption);
            m.health.food.food = self.food;
            m.health.food.saturation = self.saturation;
            m.health.food.exhaustion = self.exhaustion;
            m.health.food.timer = self.food_timer;
            m.health.game_time = game_time;
            m.invulnerable = self.game_mode == 1 || self.game_mode == 3;
            m.may_fly = m.invulnerable || self.allow_flight;
            m.spectator = self.game_mode == 3;
            m.flying = self.flying;
            m.flying_speed = self.flying_speed;
            m
        });
        GuestSnapshot {
            uuid: self.uuid,
            dimension: self.dimension.to_string(),
            position: self.position,
            velocity: self.velocity,
            yaw: self.yaw,
            pitch: self.pitch,
            head_yaw: self.head_yaw,
            on_ground: self.on_ground,
            ground: self.ground,
            game_mode: self.game_mode,
            previous_game_mode: self.previous_game_mode,
            flying: self.flying,
            flying_speed: self.flying_speed,
            allow_flight: self.allow_flight,
            sprinting: self.sprinting,
            sneaking: self.sneaking,
            fall_flying: self.fall_flying,
            sleeping: self.sleeping,
            shoulders: self.shoulders,
            living_flags: self.living_flags,
            stuck: self.stuck,
            bed: self.bed,
            sleep_timer: self.sleep_timer,
            vehicle: self.vehicle,
            vehicle_order: self.vehicle_order,
            vehicle_good: self.vehicle_good,
            spawn: self.spawn,
            respawn_elsewhere: self.respawn_elsewhere.clone(),
            last_death: self.last_death.clone(),
            health: self.health,
            absorption: self.absorption,
            food: self.food,
            saturation: self.saturation,
            exhaustion: self.exhaustion,
            food_timer: self.food_timer,
            fire_ticks: self.fire_ticks,
            air: self.air,
            ticks_frozen: self.ticks_frozen,
            fall_distance: self.fall_distance,
            xp_level: self.xp_level,
            xp_progress: self.xp_progress,
            xp_total: self.xp_total,
            xp_seed: self.xp_seed,
            inventory: (*self.inventory).clone(),
            armor: (*self.armor).clone(),
            offhand: (*self.offhand).clone(),
            carried: (*self.carried).clone(),
            ender_items: (*self.ender_items).clone(),
            selected: self.selected,
            craft_grid: (*self.craft_grid).clone(),
            craft_result: (*self.craft_result).clone(),
            effects: (*self.effects).clone(),
            effects_tick_count: self.effects_tick_count,
            effects_changes: self.effects_changes,
            shield_cooldown: self.shield_cooldown,
            cooldowns: (*self.cooldowns).clone(),
            recipe_book: (*self.recipe_book).clone(),
            advancements: (*self.advancements).clone(),
            stats: (*self.stats).clone(),
            seen_credits: self.seen_credits,
            post_effects: (*self.post_effects).clone(),
            attachments: (*self.attachments).clone(),
            saved: (*self.saved).clone(),
            hook: self.hook.clone(),
            motion,
        }
    }
}

#[derive(Default)]
struct CowGuestPrev {
    dimension: Option<Arc<str>>,
    inventory: Option<Arc<Vec<Option<ItemStack>>>>,
    armor: Option<Arc<[Option<ItemStack>; 4]>>,
    offhand: Option<Arc<Option<ItemStack>>>,
    carried: Option<Arc<Option<ItemStack>>>,
    ender_items: Option<Arc<Vec<Option<ItemStack>>>>,
    craft_grid: Option<Arc<[Option<ItemStack>; 4]>>,
    craft_result: Option<Arc<Option<ItemStack>>>,
    advancements: Option<Arc<horizon_client::advancements::Progress>>,
    stats: Option<Arc<BTreeMap<String, BTreeMap<String, i32>>>>,
    post_effects: Option<Arc<Vec<String>>>,
    saved: Option<Arc<Option<Tag>>>,
    attachments: Option<Arc<horizon_mod_runtime::attachments::ResidentAttachments>>,
    recipe_book: Option<Arc<horizon_client::recipe_book::Settings>>,
    motion: Option<Arc<horizon_client::motion::PlayerMotion>>,
}

fn cow_capture_guests(
    game: &Game<'_>,
    prev_map: &mut HashMap<u128, CowGuestPrev>,
    force_full: bool,
) -> (Vec<CowGuestRecord>, Vec<u128>) {
    let (raw_guests, online) = capture_guests_snapshot(game);
    let mut out = Vec::with_capacity(raw_guests.len());
    for (uuid, g) in raw_guests {
        let p = prev_map.entry(uuid).or_default();
        let dimension = if let Some(prev) = &p.dimension
            && &**prev == g.dimension.as_str()
        {
            Arc::clone(prev)
        } else {
            let arc: Arc<str> = Arc::from(g.dimension.as_str());
            p.dimension = Some(Arc::clone(&arc));
            arc
        };
        let inventory = if let Some(prev) = &p.inventory
            && **prev == g.inventory
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.inventory);
            p.inventory = Some(Arc::clone(&arc));
            arc
        };
        let armor = if let Some(prev) = &p.armor
            && **prev == g.armor
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.armor);
            p.armor = Some(Arc::clone(&arc));
            arc
        };
        let offhand = if let Some(prev) = &p.offhand
            && **prev == g.offhand
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.offhand);
            p.offhand = Some(Arc::clone(&arc));
            arc
        };
        let carried = if let Some(prev) = &p.carried
            && **prev == g.carried
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.carried);
            p.carried = Some(Arc::clone(&arc));
            arc
        };
        let ender_items = if let Some(prev) = &p.ender_items
            && **prev == g.ender_items
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.ender_items);
            p.ender_items = Some(Arc::clone(&arc));
            arc
        };
        let craft_grid = if let Some(prev) = &p.craft_grid
            && **prev == g.craft_grid
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.craft_grid);
            p.craft_grid = Some(Arc::clone(&arc));
            arc
        };
        let craft_result = if let Some(prev) = &p.craft_result
            && **prev == g.craft_result
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.craft_result);
            p.craft_result = Some(Arc::clone(&arc));
            arc
        };
        let advancements = if let Some(prev) = &p.advancements
            && prev.criteria == g.advancements.criteria
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.advancements);
            p.advancements = Some(Arc::clone(&arc));
            arc
        };
        let stats = if let Some(prev) = &p.stats
            && **prev == g.stats
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.stats);
            p.stats = Some(Arc::clone(&arc));
            arc
        };
        let post_effects = if let Some(prev) = &p.post_effects
            && **prev == g.post_effects
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.post_effects);
            p.post_effects = Some(Arc::clone(&arc));
            arc
        };
        let saved = if let Some(prev) = &p.saved
            && **prev == g.saved
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.saved);
            p.saved = Some(Arc::clone(&arc));
            arc
        };
        let attachments = if !force_full
            && let Some(prev) = &p.attachments
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.attachments);
            p.attachments = Some(Arc::clone(&arc));
            arc
        };
        let recipe_book = if !force_full
            && let Some(prev) = &p.recipe_book
            && prev.open == g.recipe_book.open
            && prev.filtering == g.recipe_book.filtering
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.recipe_book);
            p.recipe_book = Some(Arc::clone(&arc));
            arc
        };
        let motion = if force_full || p.motion.is_none() {
            let arc = g.motion.map(Arc::new);
            p.motion = arc.clone();
            arc
        } else {
            p.motion.clone()
        };
        out.push(CowGuestRecord {
            uuid: g.uuid,
            dimension,
            position: g.position,
            velocity: g.velocity,
            yaw: g.yaw,
            pitch: g.pitch,
            head_yaw: g.head_yaw,
            on_ground: g.on_ground,
            ground: g.ground,
            game_mode: g.game_mode,
            previous_game_mode: g.previous_game_mode,
            flying: g.flying,
            flying_speed: g.flying_speed,
            allow_flight: g.allow_flight,
            sprinting: g.sprinting,
            sneaking: g.sneaking,
            fall_flying: g.fall_flying,
            sleeping: g.sleeping,
            shoulders: g.shoulders,
            living_flags: g.living_flags,
            stuck: g.stuck,
            bed: g.bed,
            sleep_timer: g.sleep_timer,
            vehicle: g.vehicle,
            vehicle_order: g.vehicle_order,
            vehicle_good: g.vehicle_good,
            spawn: g.spawn,
            respawn_elsewhere: g.respawn_elsewhere,
            last_death: g.last_death,
            health: g.health,
            absorption: g.absorption,
            food: g.food,
            saturation: g.saturation,
            exhaustion: g.exhaustion,
            food_timer: g.food_timer,
            fire_ticks: g.fire_ticks,
            air: g.air,
            ticks_frozen: g.ticks_frozen,
            fall_distance: g.fall_distance,
            xp_level: g.xp_level,
            xp_progress: g.xp_progress,
            xp_total: g.xp_total,
            xp_seed: g.xp_seed,
            inventory,
            armor,
            offhand,
            carried,
            ender_items,
            selected: g.selected,
            craft_grid,
            craft_result,
            effects: Arc::new(g.effects),
            effects_tick_count: g.effects_tick_count,
            effects_changes: g.effects_changes,
            shield_cooldown: g.shield_cooldown,
            cooldowns: Arc::new(g.cooldowns),
            recipe_book,
            advancements,
            stats,
            seen_credits: g.seen_credits,
            post_effects,
            attachments,
            saved,
            hook: g.hook,
            motion,
        });
    }
    (out, online)
}

#[derive(Clone)]
struct CowLevelRecord {
    chunk_changes: Vec<((i32, i32), Option<Arc<Chunk>>)>,
    be_changes: Vec<((i32, i32, i32), Option<Arc<BlockEntity>>)>,
    block_names: Arc<FxHashMap<(i32, i32, i32), Tag>>,
    be_attachments: Arc<Vec<((i32, i32, i32), Attachments)>>,
    world_attachments: Arc<Attachments>,
    chest_openers: Arc<Vec<((i32, i32, i32), (i32, f64))>>,
    border: Arc<WorldBorder>,
    shulker_anims: Arc<Vec<([i32; 3], horizon_client::block_entity_anim::Shulker)>>,
    gateway_anims: Arc<Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)>>,
    fangs: Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>>,
    ominous_spawners: Arc<Vec<(u32, Tag)>>,
    portals_entities: Arc<HashMap<u32, horizon_client::nether_portal::PortalState>>,
    lectern_pulses: Arc<Vec<((i32, i32, i32), i32)>>,
    lid_openers: Arc<HashMap<(i32, i32, i32), i32>>,
    inhabited_changes: Vec<((i32, i32), Option<i64>)>,
    end_clock: i64,
    random: LevelRandom,
    forced_chunks: Arc<HashSet<(i32, i32)>>,
    chunk_tick_changes: Vec<((i32, i32), Option<Arc<(ChunkTicks, ChunkTicks)>>)>,
    random_tick_value: i32,
    comparator_outputs: Arc<HashMap<BlockPos, i32>>,
    torch_toggles: Arc<Vec<(BlockPos, i64)>>,
    moving_pistons: Arc<Vec<(BlockPos, MovingPiston)>>,
    piston_pushes: Arc<Vec<(BlockPos, MovingPiston, f32)>>,
    block_events: Arc<VecDeque<BlockEventData>>,
    hoppers: Arc<HashMap<(i32, i32, i32), super::super::machines::HopperState>>,
    mob_changes: Vec<(u32, Option<Arc<super::super::Mob>>)>,
    mob_order_changed: Option<Arc<Vec<u32>>>,
    dropped: Arc<horizon_client::join_seq::JoinList<super::super::ItemEntity>>,
    orbs: Arc<horizon_client::join_seq::JoinList<super::super::ExperienceOrb>>,
    projectiles:
        Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>>,
    boats: Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>>,
    carts: Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>>,
    tnts: Arc<horizon_client::join_seq::JoinList<horizon_gameplay::tnt::PrimedTnt>>,
    falling: Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>>,
    paintings: Arc<horizon_client::join_seq::JoinList<horizon_client::entities::Painting>>,
    item_frames: Arc<horizon_client::join_seq::JoinList<horizon_client::entities::ItemFrame>>,
    armor_stands: Arc<horizon_client::join_seq::JoinList<horizon_client::entities::ArmorStand>>,
    cushions: Arc<horizon_client::join_seq::JoinList<horizon_client::entities::Cushion>>,
    others: Arc<horizon_client::join_seq::ResidentList<super::super::commands::OtherEntity>>,
    crystals: Arc<horizon_client::join_seq::JoinList<super::super::dragon::Crystal>>,
    dragon: Option<Arc<DragonSnapshot>>,
    tridents: Arc<Vec<TridentSnapshot>>,
    rockets: Arc<Vec<RocketSnapshot>>,
    breath_clouds: Arc<Vec<BreathSnapshot>>,
    thrown_potions: Arc<ThrownPotionsSnapshot>,
    knot_ids: Arc<HashMap<[i32; 3], u32>>,
    persist_uuid_changes: Vec<(u32, Option<[i32; 4]>)>,
    persist_raw_changes: Vec<(u32, Option<Arc<Tag>>)>,
    persist_passenger_slots: Arc<HashMap<u32, Vec<super::super::persist::PassengerSlot>>>,
    raids_mem: Option<Arc<Raids>>,
    last_hurt_by_player: Arc<HashMap<u32, super::super::combat::PlayerCredit>>,
    love_cause: Arc<HashMap<u32, Option<u32>>>,
}

#[derive(Clone)]
struct ServerTickRecord {
    anchor_ids: Arc<HashSet<u128>>,
    policy: RollbackPolicy,
    game_time: i64,
    day_time: i64,
    weather: Weather,
    health_rules: HealthRules,
    rules: Arc<GameRules>,
    border: Arc<WorldBorder>,
    bossbars: Arc<Tag>,
    sculk_tracker: WardenSpawnTracker,
    sculk_darkness: (f32, f32),
    fight_tag: Arc<Option<Tag>>,
    trader_tag: Arc<Option<Tag>>,
    rand_sequences: Arc<horizon_client::random_sequences::RandomSequences>,
    difficulty: u8,
    difficulty_locked: bool,
    clock_rate: f32,
    clock_frozen: bool,
    time_rate: f32,
    time_partial: f32,
    time_paused: bool,
    command_clocks: Arc<BTreeMap<String, (i64, super::super::chat::WorldClock)>>,
    world_spawn: Option<[i32; 3]>,
    world_spawn_angle: [f32; 2],
    next_entity: u32,
    ticked_entities_through: u32,
    ordered_announced: horizon_client::join_seq::JoinSeq,
    cat_tick: i32,
    phantom_tick: i32,
    player_extra: Arc<Option<Tag>>,
    shoulder_time: Arc<HashMap<[i32; 4], i64>>,
    fangs: Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>>,
    ominous_spawners: Arc<Vec<(u32, Tag)>>,
    portals_entities: Arc<HashMap<u32, horizon_client::nether_portal::PortalState>>,
    end_gateway_cooldowns: Arc<HashMap<u32, i32>>,
    lectern_pulses: Arc<Vec<((i32, i32, i32), i32)>>,
    player_beds: Arc<Vec<(Dim, [i32; 3])>>,
    sleep_seen: (i64, i64),
    lead_killers: Arc<HashMap<u32, (u128, Dim)>>,
    lid_openers: Arc<HashMap<(i32, i32, i32), i32>>,
    entity_attachments: Arc<FxHashMap<u32, Attachments>>,
    player_attachments: Arc<horizon_mod_runtime::attachments::ResidentAttachments>,
    guest_attachments: Arc<HashMap<u128, Attachments>>,
    scheduled_events: Arc<Vec<super::super::functions::Scheduled>>,
    scoreboard: Arc<Scoreboard>,
    command_storage: Arc<CommandStorage>,
    stopwatches_elapsed: Arc<BTreeMap<String, i64>>,
    cmd_post_effects: Arc<Vec<String>>,
    guests: Vec<CowGuestRecord>,
    levels: Vec<(Dim, CowLevelRecord)>,
    mob_players: Arc<FxHashMap<u32, u32>>,
    client_using: Option<(usize, u32)>,
    client_use_duration: i32,
    client_crossbow_sounds: [bool; 2],
}

struct ServerSegment {
    seg_id: u64,
    branch_id: u32,
    parent_seg_id: u64,
    is_keyframe: bool,
    start_tick: i64,
    end_tick: i64,
    sealed_mob_patches: Option<RrtlBlob>,
    dim_table: Vec<Dim>,
    header_chunks: HashMap<Dim, Arc<FxHashMap<(i32, i32), Arc<Chunk>>>>,
    header_chunk_ticks: HashMap<Dim, Arc<HashMap<(i32, i32), Arc<(ChunkTicks, ChunkTicks)>>>>,
    header_block_entities: HashMap<Dim, Arc<HashMap<(i32, i32, i32), Arc<BlockEntity>>>>,
    header_mobs: HashMap<Dim, Vec<Arc<super::super::Mob>>>,
    header_persist_raw: HashMap<Dim, Arc<HashMap<u32, Arc<Tag>>>>,
    header_persist_uuids: HashMap<Dim, Arc<HashMap<u32, [i32; 4]>>>,
    header_inhabited: HashMap<Dim, Arc<FxHashMap<(i32, i32), i64>>>,
    ticks: Vec<ServerTickRecord>,
}

impl ServerSegment {
    fn seal_mobs_to_rrtl(&mut self, known_by_dim: &mut HashMap<Dim, HashSet<u32>>) {
        if self.sealed_mob_patches.is_some() {
            return;
        }
        if self.is_keyframe {
            known_by_dim.clear();
            for (&dim, list) in &self.header_mobs {
                let set = list.iter().map(|m| m.id).collect();
                known_by_dim.insert(dim, set);
            }
        }
        let mut dim_table: Vec<Dim> = Vec::new();
        let mut batches: Vec<(u16, u16, Vec<MobCompactPatch>)> = Vec::new();
        for (t_idx, rec) in self.ticks.iter_mut().enumerate() {
            for (dim, lrec) in &mut rec.levels {
                let d_idx = match dim_table.iter().position(|d| d == dim) {
                    Some(i) => i as u16,
                    None => {
                        let i = dim_table.len() as u16;
                        dim_table.push(*dim);
                        i
                    }
                };
                let known = known_by_dim.entry(*dim).or_default();
                let mut kept_changes = Vec::new();
                let mut tick_patches = Vec::new();
                for (id, opt) in std::mem::take(&mut lrec.mob_changes) {
                    match opt {
                        None => {
                            known.remove(&id);
                            kept_changes.push((id, None));
                        }
                        Some(m) => {
                            if known.insert(id) {
                                kept_changes.push((id, Some(m)));
                            } else {
                                tick_patches.push(MobCompactPatch::from_mob(&m));
                            }
                        }
                    }
                }
                lrec.mob_changes = kept_changes;
                if !tick_patches.is_empty() {
                    batches.push((t_idx as u16, d_idx, tick_patches));
                }
            }
        }
        self.dim_table = dim_table;
        self.sealed_mob_patches = Some(encode_mob_patch_batches_rrtl(&batches));
    }
}

#[derive(Default)]
struct CowLevelPrev {
    chunk_touch_cursor: Option<(u64, u64)>,
    chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
    chunks_dirty_since_hdr: bool,
    chunks_hdr_arc: Option<Arc<FxHashMap<(i32, i32), Arc<Chunk>>>>,
    chunk_ticks: HashMap<(i32, i32), Arc<(ChunkTicks, ChunkTicks)>>,
    ticks_dirty_since_hdr: bool,
    ticks_hdr_arc: Option<Arc<HashMap<(i32, i32), Arc<(ChunkTicks, ChunkTicks)>>>>,
    block_entities: HashMap<(i32, i32, i32), Arc<BlockEntity>>,
    be_encoded_tags: FxHashMap<(i32, i32, i32), Tag>,
    be_dirty_since_hdr: bool,
    be_hdr_arc: Option<Arc<HashMap<(i32, i32, i32), Arc<BlockEntity>>>>,
    world_revision: u64,
    block_names: Option<Arc<FxHashMap<(i32, i32, i32), Tag>>>,
    be_attachments: Option<Arc<Vec<((i32, i32, i32), Attachments)>>>,
    world_attachments: Option<Arc<Attachments>>,
    border_tag: Option<Tag>,
    border: Option<Arc<WorldBorder>>,
    inhabited: FxHashMap<(i32, i32), i64>,
    inhabited_dirty_since_hdr: bool,
    inhabited_hdr_arc: Option<Arc<FxHashMap<(i32, i32), i64>>>,
    forced_chunks: Option<Arc<HashSet<(i32, i32)>>>,
    mobs: FxHashMap<u32, Arc<super::super::Mob>>,
    mob_order: Vec<u32>,
    dropped_stamp: Option<horizon_client::join_seq::ListStamp>,
    dropped: Option<Arc<horizon_client::join_seq::JoinList<super::super::ItemEntity>>>,
    orbs_stamp: Option<horizon_client::join_seq::ListStamp>,
    orbs: Option<Arc<horizon_client::join_seq::JoinList<super::super::ExperienceOrb>>>,
    tnts_stamp: Option<horizon_client::join_seq::ListStamp>,
    tnts: Option<Arc<horizon_client::join_seq::JoinList<horizon_gameplay::tnt::PrimedTnt>>>,
    proj_stamp: Option<horizon_client::join_seq::ListStamp>,
    projectiles:
        Option<Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>>>,
    boats_stamp: Option<horizon_client::join_seq::ListStamp>,
    boats: Option<Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>>>,
    carts_stamp: Option<horizon_client::join_seq::ListStamp>,
    carts:
        Option<Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>>>,
    falling_stamp: Option<horizon_client::join_seq::ListStamp>,
    falling: Option<Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>>>,
    paintings_stamp: Option<horizon_client::join_seq::ListStamp>,
    paintings: Option<Arc<horizon_client::join_seq::JoinList<horizon_client::entities::Painting>>>,
    frames_stamp: Option<horizon_client::join_seq::ListStamp>,
    item_frames: Option<Arc<horizon_client::join_seq::JoinList<horizon_client::entities::ItemFrame>>>,
    stands_stamp: Option<horizon_client::join_seq::ListStamp>,
    armor_stands: Option<Arc<horizon_client::join_seq::JoinList<horizon_client::entities::ArmorStand>>>,
    cushions_stamp: Option<horizon_client::join_seq::ListStamp>,
    cushions: Option<Arc<horizon_client::join_seq::JoinList<horizon_client::entities::Cushion>>>,
    crystals_stamp: Option<horizon_client::join_seq::ListStamp>,
    crystals: Option<Arc<horizon_client::join_seq::JoinList<super::super::dragon::Crystal>>>,
    knot_ids: Option<Arc<HashMap<[i32; 3], u32>>>,
    persist_uuids: HashMap<u32, [i32; 4]>,
    persist_uuids_dirty: bool,
    persist_uuids_hdr_arc: Option<Arc<HashMap<u32, [i32; 4]>>>,
    persist_raw: HashMap<u32, Arc<Tag>>,
    persist_raw_dirty: bool,
    persist_raw_hdr_arc: Option<Arc<HashMap<u32, Arc<Tag>>>>,
    raids_mem: Option<Arc<Raids>>,
}

#[derive(Default)]
struct CowGlobalPrev {
    anchor_ids: Option<Arc<HashSet<u128>>>,
    border_tag: Option<Tag>,
    border: Option<Arc<WorldBorder>>,
    bossbars: Option<Arc<Tag>>,
    fight_tag: Option<Arc<Option<Tag>>>,
    trader_disk_loaded: bool,
    trader_tag: Option<Arc<Option<Tag>>>,
    scoreboard_tag: Option<Tag>,
    scoreboard: Option<Arc<Scoreboard>>,
    rand_seq_tag: Option<Tag>,
    rand_sequences: Option<Arc<horizon_client::random_sequences::RandomSequences>>,
    command_storage: Option<Arc<CommandStorage>>,
    stopwatches_elapsed: Option<Arc<BTreeMap<String, i64>>>,
    player_extra: Option<Arc<Option<Tag>>>,
    shoulder_time: Option<Arc<HashMap<[i32; 4], i64>>>,
    end_gateway_cooldowns: Option<Arc<HashMap<u32, i32>>>,
    player_beds: Option<Arc<Vec<(Dim, [i32; 3])>>>,
    lead_killers: Option<Arc<HashMap<u32, (u128, Dim)>>>,
    cmd_post_effects: Option<Arc<Vec<String>>>,
    mob_players: Option<Arc<FxHashMap<u32, u32>>>,
    player_attachments: Option<Arc<horizon_mod_runtime::attachments::ResidentAttachments>>,
    guests: HashMap<u128, CowGuestPrev>,
    levels: HashMap<Dim, CowLevelPrev>,
}

fn empty_proj(
) -> Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_boats() -> Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_carts(
) -> Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_falling() -> Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>> {
    static E: OnceLock<Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_others(
) -> Arc<horizon_client::join_seq::ResidentList<super::super::commands::OtherEntity>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::ResidentList<super::super::commands::OtherEntity>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_fangs() -> Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_tridents() -> Arc<Vec<TridentSnapshot>> {
    static E: OnceLock<Arc<Vec<TridentSnapshot>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_rockets() -> Arc<Vec<RocketSnapshot>> {
    static E: OnceLock<Arc<Vec<RocketSnapshot>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_breath() -> Arc<Vec<BreathSnapshot>> {
    static E: OnceLock<Arc<Vec<BreathSnapshot>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_thrown_potions() -> Arc<ThrownPotionsSnapshot> {
    static E: OnceLock<Arc<ThrownPotionsSnapshot>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(ThrownPotionsSnapshot::default())))
}

fn empty_ominous() -> Arc<Vec<(u32, Tag)>> {
    static E: OnceLock<Arc<Vec<(u32, Tag)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_block_events() -> Arc<VecDeque<BlockEventData>> {
    static E: OnceLock<Arc<VecDeque<BlockEventData>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(VecDeque::new())))
}

fn empty_moving_pistons() -> Arc<Vec<(BlockPos, MovingPiston)>> {
    static E: OnceLock<Arc<Vec<(BlockPos, MovingPiston)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_piston_pushes() -> Arc<Vec<(BlockPos, MovingPiston, f32)>> {
    static E: OnceLock<Arc<Vec<(BlockPos, MovingPiston, f32)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_torch_toggles() -> Arc<Vec<(BlockPos, i64)>> {
    static E: OnceLock<Arc<Vec<(BlockPos, i64)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_comparators() -> Arc<HashMap<BlockPos, i32>> {
    static E: OnceLock<Arc<HashMap<BlockPos, i32>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_hoppers() -> Arc<HashMap<(i32, i32, i32), super::super::machines::HopperState>> {
    static E: OnceLock<Arc<HashMap<(i32, i32, i32), super::super::machines::HopperState>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_raids() -> Arc<Raids> {
    static E: OnceLock<Arc<Raids>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Raids::default())))
}

fn empty_chest_openers() -> Arc<Vec<((i32, i32, i32), (i32, f64))>> {
    static E: OnceLock<Arc<Vec<((i32, i32, i32), (i32, f64))>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_shulker_anims() -> Arc<Vec<([i32; 3], horizon_client::block_entity_anim::Shulker)>> {
    static E: OnceLock<Arc<Vec<([i32; 3], horizon_client::block_entity_anim::Shulker)>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_gateway_anims() -> Arc<Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)>> {
    static E: OnceLock<Arc<Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_portals_entities() -> Arc<HashMap<u32, horizon_client::nether_portal::PortalState>> {
    static E: OnceLock<Arc<HashMap<u32, horizon_client::nether_portal::PortalState>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_lectern_pulses() -> Arc<Vec<((i32, i32, i32), i32)>> {
    static E: OnceLock<Arc<Vec<((i32, i32, i32), i32)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_lid_openers() -> Arc<HashMap<(i32, i32, i32), i32>> {
    static E: OnceLock<Arc<HashMap<(i32, i32, i32), i32>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_passenger_slots() -> Arc<HashMap<u32, Vec<super::super::persist::PassengerSlot>>> {
    static E: OnceLock<Arc<HashMap<u32, Vec<super::super::persist::PassengerSlot>>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_hurt_by_player() -> Arc<HashMap<u32, super::super::combat::PlayerCredit>> {
    static E: OnceLock<Arc<HashMap<u32, super::super::combat::PlayerCredit>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_love_cause() -> Arc<HashMap<u32, Option<u32>>> {
    static E: OnceLock<Arc<HashMap<u32, Option<u32>>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_command_clocks() -> Arc<BTreeMap<String, (i64, super::super::chat::WorldClock)>> {
    static E: OnceLock<Arc<BTreeMap<String, (i64, super::super::chat::WorldClock)>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(BTreeMap::new())))
}

fn empty_scheduled() -> Arc<Vec<super::super::functions::Scheduled>> {
    static E: OnceLock<Arc<Vec<super::super::functions::Scheduled>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_entity_attachments() -> Arc<FxHashMap<u32, Attachments>> {
    static E: OnceLock<Arc<FxHashMap<u32, Attachments>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(FxHashMap::default())))
}

fn empty_guest_attachments() -> Arc<HashMap<u128, Attachments>> {
    static E: OnceLock<Arc<HashMap<u128, Attachments>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn diff_chunks_via_touch(
    world: &horizon_client::world::World,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<((i32, i32), Option<Arc<Chunk>>)> {
    let mut changes = Vec::new();
    let touched_opt: Option<Vec<(i32, i32)>> = prev
        .chunk_touch_cursor
        .and_then(|c| world.chunks.touched_since(c).map(|it| it.copied().collect()));
    if let Some(keys) = touched_opt {
        let mut seen = HashSet::with_capacity(keys.len());
        for pos in keys {
            if !seen.insert(pos) {
                continue;
            }
            match world.chunks.get(&pos) {
                Some(live_chunk) => {
                    let same = prev.chunks.get(&pos).is_some_and(|p| Arc::ptr_eq(p, live_chunk));
                    if !same {
                        let c = Arc::clone(live_chunk);
                        prev.chunks.insert(pos, Arc::clone(&c));
                        prev.chunks_dirty_since_hdr = true;
                        if !force_full {
                            changes.push((pos, Some(c)));
                        }
                    }
                }
                None => {
                    if prev.chunks.remove(&pos).is_some() {
                        prev.chunks_dirty_since_hdr = true;
                        if !force_full {
                            changes.push((pos, None));
                        }
                    }
                }
            }
        }
    } else {
        for (&pos, live_chunk) in world.chunks.iter() {
            let same = prev.chunks.get(&pos).is_some_and(|p| Arc::ptr_eq(p, live_chunk));
            if !same {
                let c = Arc::clone(live_chunk);
                prev.chunks.insert(pos, Arc::clone(&c));
                prev.chunks_dirty_since_hdr = true;
                if !force_full {
                    changes.push((pos, Some(c)));
                }
            }
        }
        let mut removed = Vec::new();
        for &pos in prev.chunks.keys() {
            if !world.chunks.contains_key(&pos) {
                removed.push(pos);
            }
        }
        for pos in removed {
            prev.chunks.remove(&pos);
            prev.chunks_dirty_since_hdr = true;
            if !force_full {
                changes.push((pos, None));
            }
        }
    }
    prev.chunk_touch_cursor = Some(world.chunks.touch_cursor());
    changes
}

fn diff_chunk_ticks_cow(
    live_chunks: &FxHashMap<(i32, i32), Arc<Chunk>>,
    block_ticks: &Ticks,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<((i32, i32), Option<Arc<(ChunkTicks, ChunkTicks)>>)> {
    let mut changes = Vec::new();
    for &(cx, cz) in live_chunks.keys() {
        let cpos = ChunkPos::new(cx, cz);
        let b = block_ticks.blocks.chunk(cpos);
        let f = block_ticks.fluids.chunk(cpos);
        let non_empty =
            b.is_some_and(|t| t.count() > 0) || f.is_some_and(|t| t.count() > 0);
        if non_empty {
            let unchanged = prev
                .chunk_ticks
                .get(&(cx, cz))
                .is_some_and(|p| chunk_ticks_eq(b, &p.0) && chunk_ticks_eq(f, &p.1));
            if !unchanged {
                let arc = Arc::new((
                    b.cloned().unwrap_or_default(),
                    f.cloned().unwrap_or_default(),
                ));
                prev.chunk_ticks.insert((cx, cz), Arc::clone(&arc));
                prev.ticks_dirty_since_hdr = true;
                if !force_full {
                    changes.push(((cx, cz), Some(arc)));
                }
            }
        } else if prev.chunk_ticks.remove(&(cx, cz)).is_some() {
            prev.ticks_dirty_since_hdr = true;
            if !force_full {
                changes.push(((cx, cz), None));
            }
        }
    }
    let mut unloaded_keys = Vec::new();
    for &k in prev.chunk_ticks.keys() {
        if !live_chunks.contains_key(&k) {
            unloaded_keys.push(k);
        }
    }
    for k in unloaded_keys {
        prev.chunk_ticks.remove(&k);
        prev.ticks_dirty_since_hdr = true;
        if !force_full {
            changes.push((k, None));
        }
    }
    changes
}

fn diff_mobs_cow(
    live_mobs: &horizon_client::join_seq::JoinList<super::super::Mob>,
    prev_mobs: &mut FxHashMap<u32, Arc<super::super::Mob>>,
    prev_order: &mut Vec<u32>,
    force_full: bool,
) -> (Vec<(u32, Option<Arc<super::super::Mob>>)>, Option<Arc<Vec<u32>>>) {
    let mut changes = Vec::new();
    let mut new_order = Vec::with_capacity(live_mobs.len());
    let mut seen = HashSet::with_capacity(live_mobs.len());
    for m in live_mobs.iter() {
        seen.insert(m.id);
        new_order.push(m.id);
        let unchanged = prev_mobs.get(&m.id).is_some_and(|p| mob_unchanged(m, p));
        if !unchanged {
            let arc = Arc::new(m.clone());
            prev_mobs.insert(m.id, Arc::clone(&arc));
            if !force_full {
                changes.push((m.id, Some(arc)));
            }
        }
    }
    let mut removed_ids = Vec::new();
    for &id in prev_mobs.keys() {
        if !seen.contains(&id) {
            removed_ids.push(id);
        }
    }
    for id in removed_ids {
        prev_mobs.remove(&id);
        if !force_full {
            changes.push((id, None));
        }
    }
    let order_changed = if !force_full && *prev_order != new_order {
        *prev_order = new_order;
        Some(Arc::new(prev_order.clone()))
    } else {
        if *prev_order != new_order {
            *prev_order = new_order;
        }
        None
    };
    (changes, order_changed)
}

fn diff_block_entities_native_cow(
    game: &Game<'_>,
    world: &horizon_client::world::World,
    hoppers: &HashMap<(i32, i32, i32), super::super::machines::HopperState>,
    lid_openers: &HashMap<(i32, i32, i32), i32>,
    chunk_changes: &[((i32, i32), Option<Arc<Chunk>>)],
    player_chunks: &[(i32, i32)],
    any_menu_open: bool,
    is_active_dim: bool,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<((i32, i32, i32), Option<Arc<BlockEntity>>)> {
    let mut changes = Vec::new();

    if prev.block_entities.is_empty() && !world.block_entities.is_empty() {
        for (&pos, be) in world.block_entities.iter() {
            let arc = Arc::new(be.clone());
            prev.block_entities.insert(pos, Arc::clone(&arc));
            prev.be_dirty_since_hdr = true;
            if !force_full {
                changes.push((pos, Some(arc)));
            }
        }
        return changes;
    }

    let mut candidates: HashSet<(i32, i32, i32)> = HashSet::new();

    if prev.block_entities.len() != world.block_entities.len() || !chunk_changes.is_empty() {
        let mut removed_pos = Vec::new();
        for &pos in prev.block_entities.keys() {
            if !(*world.block_entities).contains_key(&pos) {
                removed_pos.push(pos);
            }
        }
        for pos in removed_pos {
            prev.block_entities.remove(&pos);
            prev.be_encoded_tags.remove(&pos);
            prev.be_dirty_since_hdr = true;
            if !force_full {
                changes.push((pos, None));
            }
        }
        for &(cpos, _) in chunk_changes {
            for &pos in world.block_entities.in_chunk(cpos) {
                candidates.insert(pos);
            }
        }
        if prev.block_entities.len() != world.block_entities.len() {
            for &pos in world.block_entities.keys() {
                if !prev.block_entities.contains_key(&pos) {
                    candidates.insert(pos);
                }
            }
        }
    }

    for &cpos in world.modified.iter() {
        for &pos in world.block_entities.in_chunk(cpos) {
            candidates.insert(pos);
        }
    }
    for &pos in hoppers.keys() {
        candidates.insert(pos);
    }
    for &pos in lid_openers.keys() {
        candidates.insert(pos);
    }
    for &pos in world.chest_openers.keys() {
        candidates.insert(pos);
    }
    if any_menu_open {
        for &(px, pz) in player_chunks {
            for dz in -2..=2 {
                for dx in -2..=2 {
                    for &pos in world.block_entities.in_chunk((px + dx, pz + dz)) {
                        candidates.insert(pos);
                    }
                }
            }
        }
    }

    let tickers = world.block_entities.ticker_slots();
    for &(pos, _) in tickers.iter() {
        if world.block_entities.is_asleep(&pos) {
            continue;
        }
        if is_active_dim && !game.block_ticking_at(pos.0, pos.2) {
            continue;
        }
        candidates.insert(pos);
    }

    let facts = game.res.facts();
    for pos in candidates {
        let Some(be) = world.block_entities.get(&pos) else {
            if prev.block_entities.remove(&pos).is_some() {
                prev.be_encoded_tags.remove(&pos);
                prev.be_dirty_since_hdr = true;
                if !force_full {
                    changes.push((pos, None));
                }
            }
            continue;
        };
        let is_new = !prev.block_entities.contains_key(&pos);
        let block_name = &facts.get(world.block(pos.0, pos.1, pos.2)).name;
        let encoded = block_entity_encode::encode(pos, be, block_name.as_ref());
        let unchanged = !is_new
            && prev
                .be_encoded_tags
                .get(&pos)
                .is_some_and(|old_tag| *old_tag == encoded);
        if !unchanged {
            prev.be_encoded_tags.insert(pos, encoded);
            let arc = Arc::new(be.clone());
            prev.block_entities.insert(pos, Arc::clone(&arc));
            prev.be_dirty_since_hdr = true;
            if !force_full {
                changes.push((pos, Some(arc)));
            }
        }
    }

    changes
}

fn diff_persist_raw_cow(
    live: &HashMap<u32, Tag>,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<(u32, Option<Arc<Tag>>)> {
    let mut changes = Vec::new();
    let mut removed = Vec::new();
    for &id in prev.persist_raw.keys() {
        if !live.contains_key(&id) {
            removed.push(id);
        }
    }
    for id in removed {
        prev.persist_raw.remove(&id);
        prev.persist_raw_dirty = true;
        if !force_full {
            changes.push((id, None));
        }
    }
    for (&id, tag) in live.iter() {
        let same = prev.persist_raw.get(&id).is_some_and(|p| **p == *tag);
        if !same {
            let arc = Arc::new(tag.clone());
            prev.persist_raw.insert(id, Arc::clone(&arc));
            prev.persist_raw_dirty = true;
            if !force_full {
                changes.push((id, Some(arc)));
            }
        }
    }
    changes
}

fn diff_persist_uuids_cow(
    live: &HashMap<u32, [i32; 4]>,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<(u32, Option<[i32; 4]>)> {
    let mut changes = Vec::new();
    let mut removed = Vec::new();
    for &id in prev.persist_uuids.keys() {
        if !live.contains_key(&id) {
            removed.push(id);
        }
    }
    for id in removed {
        prev.persist_uuids.remove(&id);
        prev.persist_uuids_dirty = true;
        if !force_full {
            changes.push((id, None));
        }
    }
    for (&id, &u) in live.iter() {
        if prev.persist_uuids.get(&id).copied() != Some(u) {
            prev.persist_uuids.insert(id, u);
            prev.persist_uuids_dirty = true;
            if !force_full {
                changes.push((id, Some(u)));
            }
        }
    }
    changes
}

fn diff_inhabited_cow(
    live: &FxHashMap<(i32, i32), i64>,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<((i32, i32), Option<i64>)> {
    let mut changes = Vec::new();
    let mut removed = Vec::new();
    for &k in prev.inhabited.keys() {
        if !live.contains_key(&k) {
            removed.push(k);
        }
    }
    for k in removed {
        prev.inhabited.remove(&k);
        prev.inhabited_dirty_since_hdr = true;
        if !force_full {
            changes.push((k, None));
        }
    }
    for (&k, &v) in live.iter() {
        if prev.inhabited.get(&k).copied() != Some(v) {
            prev.inhabited.insert(k, v);
            prev.inhabited_dirty_since_hdr = true;
            if !force_full {
                changes.push((k, Some(v)));
            }
        }
    }
    changes
}

fn collect_dim_player_chunks(game: &Game<'_>, dim: Dim) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let dim_id = dim.id();
    if let Some(host) = &game.lan {
        for g in host.hub.guests.iter() {
            if g.placed && g.dimension == dim_id {
                out.push((
                    g.position[0].floor() as i32 >> 4,
                    g.position[2].floor() as i32 >> 4,
                ));
            }
        }
    }
    if out.is_empty()
        && game.dim == dim
        && let Some(p) = &game.player
    {
        out.push((
            p.position.x.floor() as i32 >> 4,
            p.position.z.floor() as i32 >> 4,
        ));
    }
    out
}

fn capture_cow_level_direct(
    game: &Game<'_>,
    dim: Dim,
    world: &horizon_client::world::World,
    prev: &mut CowLevelPrev,
    any_menu_open: bool,
    force_full: bool,
) -> CowLevelRecord {
    let m_chunks_0 = StatmMem::current();
    let live_chunks = &*world.chunks;
    let chunk_changes = diff_chunks_via_touch(world, prev, force_full);
    let m_chunks_1 = StatmMem::current();
    record_mem_cow_chunks(
        game.world.game_time,
        m_chunks_0,
        m_chunks_1,
        live_chunks.len(),
        chunk_changes.len(),
    );

    let player_chunks = collect_dim_player_chunks(game, dim);

    let m_be_0 = StatmMem::current();
    let be_changes = diff_block_entities_native_cow(
        game,
        world,
        &game.hoppers,
        &game.lid_openers,
        &chunk_changes,
        &player_chunks,
        any_menu_open,
        true,
        prev,
        force_full,
    );
    let m_be_1 = StatmMem::current();
    record_mem_cow_be(
        game.world.game_time,
        m_be_0,
        m_be_1,
        be_changes.len(),
        !be_changes.is_empty(),
    );

    let block_names = if !force_full
        && prev.world_revision == world.revision
        && let Some(p) = &prev.block_names
        && p.len() == world.block_names.len()
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.block_names.clone());
        prev.block_names = Some(Arc::clone(&arc));
        arc
    };

    let be_attachments = if !force_full
        && chunk_changes.is_empty()
        && be_changes.is_empty()
        && let Some(p) = &prev.be_attachments
    {
        Arc::clone(p)
    } else {
        let mut atts = Vec::new();
        for &(cx, cz) in live_chunks.keys() {
            atts.extend(world.block_entities.chunk_attachments(cx, cz));
        }
        let arc = Arc::new(atts);
        prev.be_attachments = Some(Arc::clone(&arc));
        arc
    };

    let world_attachments = if !force_full
        && prev.world_revision == world.revision
        && let Some(p) = &prev.world_attachments
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.attachments.clone());
        prev.world_attachments = Some(Arc::clone(&arc));
        arc
    };
    prev.world_revision = world.revision;

    let b_tag = game.border.pack();
    let border = if !force_full
        && prev.border_tag.as_ref() == Some(&b_tag)
        && let Some(p) = &prev.border
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(game.border.clone());
        prev.border_tag = Some(b_tag);
        prev.border = Some(Arc::clone(&arc));
        arc
    };

    let inhabited_changes = diff_inhabited_cow(&world.inhabited, prev, force_full);

    let forced_chunks = if !force_full
        && let Some(p) = &prev.forced_chunks
        && **p == game.streaming.forced
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(game.streaming.forced.clone());
        prev.forced_chunks = Some(Arc::clone(&arc));
        arc
    };

    macro_rules! cow_stamped {
        ($field:ident, $stamp_field:ident, $live:expr) => {{
            let st = $live.stamp();
            if !force_full && prev.$stamp_field == Some(st) && prev.$field.is_some() {
                Arc::clone(prev.$field.as_ref().unwrap())
            } else {
                let arc = Arc::new($live.clone());
                prev.$stamp_field = Some(st);
                prev.$field = Some(Arc::clone(&arc));
                arc
            }
        }};
    }

    macro_rules! cow_stamped_or_empty {
        ($field:ident, $stamp_field:ident, $live:expr, $empty_fn:expr) => {{
            if $live.is_empty() {
                prev.$stamp_field = Some($live.stamp());
                prev.$field = None;
                $empty_fn()
            } else {
                let st = $live.stamp();
                if !force_full && prev.$stamp_field == Some(st) && prev.$field.is_some() {
                    Arc::clone(prev.$field.as_ref().unwrap())
                } else {
                    let arc = Arc::new($live.clone());
                    prev.$stamp_field = Some(st);
                    prev.$field = Some(Arc::clone(&arc));
                    arc
                }
            }
        }};
    }

    let m_mobs_0 = StatmMem::current();
    let (mob_changes, mob_order_changed) =
        diff_mobs_cow(&game.mobs, &mut prev.mobs, &mut prev.mob_order, force_full);
    let m_mobs_1 = StatmMem::current();
    record_mem_cow_mobs(game.world.game_time, m_mobs_0, m_mobs_1, mob_changes.len());

    let dropped = cow_stamped!(dropped, dropped_stamp, game.dropped);
    let orbs = cow_stamped!(orbs, orbs_stamp, game.orbs);
    let tnts = cow_stamped!(tnts, tnts_stamp, game.tnts);
    let paintings = cow_stamped!(paintings, paintings_stamp, game.paintings);
    let item_frames = cow_stamped!(item_frames, frames_stamp, game.item_frames);
    let armor_stands = cow_stamped!(armor_stands, stands_stamp, game.armor_stands);
    let cushions = cow_stamped!(cushions, cushions_stamp, game.cushions);
    let crystals = cow_stamped!(crystals, crystals_stamp, game.crystals);

    let projectiles = cow_stamped_or_empty!(
        projectiles,
        proj_stamp,
        game.creatures.projectiles,
        empty_proj
    );
    let boats = cow_stamped_or_empty!(boats, boats_stamp, game.boats, empty_boats);
    let carts = cow_stamped_or_empty!(carts, carts_stamp, game.carts, empty_carts);
    let falling = cow_stamped_or_empty!(falling, falling_stamp, game.falling, empty_falling);

    let others = if game.others.is_empty() {
        empty_others()
    } else {
        Arc::new(game.others.clone())
    };
    let fangs = if game.fangs.list.is_empty() {
        empty_fangs()
    } else {
        Arc::new(game.fangs.list.clone())
    };

    let tridents = if game.tridents.is_empty() {
        empty_tridents()
    } else {
        let registry = game.pack.assets.registry;
        Arc::new(
            game.tridents
                .iter()
                .map(|t| {
                    let uuid = game.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
                    let raw = game.persist.raw.get(&t.id);
                    TridentSnapshot {
                        join: t.join,
                        id: t.id,
                        nbt: t.to_nbt(uuid, registry, raw),
                        previous: t.previous,
                        in_ground_time: t.in_ground_time,
                        no_physics: t.no_physics,
                        owner: t.owner,
                        guest: t.guest,
                    }
                })
                .collect(),
        )
    };

    let rockets = if game.rockets.is_empty() {
        empty_rockets()
    } else {
        Arc::new(
            game.rockets
                .iter()
                .map(|r| {
                    let uuid = game.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
                    let raw = game.persist.raw.get(&r.id);
                    RocketSnapshot {
                        id: r.id,
                        nbt: r.to_nbt(uuid, None, raw),
                        attached: r.attached,
                        guest: r.guest,
                        owner: r.owner,
                        attached_to: r.attached_to,
                    }
                })
                .collect(),
        )
    };

    let breath_clouds = if game.breath_clouds.is_empty() {
        empty_breath()
    } else {
        Arc::new(
            game.breath_clouds
                .iter()
                .map(|c| {
                    let uuid = game.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
                    let raw = game.persist.raw.get(&c.id);
                    BreathSnapshot {
                        id: c.id,
                        nbt: super::super::potions::potion_cloud_to_nbt(c, uuid, raw),
                        at: c.at,
                        motion: c.motion,
                        radius: c.radius,
                        age: c.age,
                        duration: c.duration,
                        from_dragon: c.from_dragon,
                        potion_none: c.potion.is_none(),
                        victims: c.victims.clone(),
                    }
                })
                .collect(),
        )
    };

    let thrown_potions = if game.potions.thrown.is_empty() {
        empty_thrown_potions()
    } else {
        Arc::new(game.potions.rreturn_capture_thrown())
    };

    let ominous_spawners = if game.ominous_spawners.is_empty() {
        empty_ominous()
    } else {
        Arc::new(
            game.ominous_spawners
                .iter()
                .map(|s| {
                    (
                        s.id,
                        horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                            &s.item,
                            Vec3::new(s.position[0], s.position[1], s.position[2]),
                            s.spawn_item_after,
                            s.uuid,
                            s.raw.as_deref(),
                        ),
                    )
                })
                .collect(),
        )
    };

    let m_ticks_0 = StatmMem::current();
    let chunk_tick_changes = diff_chunk_ticks_cow(live_chunks, &game.block_ticks, prev, force_full);
    let m_ticks_1 = StatmMem::current();
    record_mem_cow_ticks(
        game.world.game_time,
        m_ticks_0,
        m_ticks_1,
        chunk_tick_changes.len(),
    );

    let block_events = if game.block_ticks.block_events.is_empty() {
        empty_block_events()
    } else {
        Arc::new(game.block_ticks.block_events.clone())
    };
    let moving_pistons = if game.block_ticks.moving_pistons.is_empty() {
        empty_moving_pistons()
    } else {
        Arc::new(game.block_ticks.moving_pistons.clone())
    };
    let piston_pushes = if game.block_ticks.piston_pushes.is_empty() {
        empty_piston_pushes()
    } else {
        Arc::new(game.block_ticks.piston_pushes.clone())
    };
    let torch_toggles = if game.block_ticks.torch_toggles.is_empty() {
        empty_torch_toggles()
    } else {
        Arc::new(game.block_ticks.torch_toggles.clone())
    };
    let comparator_outputs = if game.block_ticks.comparator_outputs.is_empty() {
        empty_comparators()
    } else {
        Arc::new(game.block_ticks.comparator_outputs.clone())
    };
    let hoppers = if game.hoppers.is_empty() {
        empty_hoppers()
    } else {
        Arc::new(game.hoppers.clone())
    };

    let knot_ids = if !force_full
        && let Some(p) = &prev.knot_ids
        && **p == game.knot_ids
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(game.knot_ids.clone());
        prev.knot_ids = Some(Arc::clone(&arc));
        arc
    };

    let m_raw_0 = StatmMem::current();
    let persist_uuid_changes = diff_persist_uuids_cow(&game.persist.uuids, prev, force_full);
    let persist_raw_changes = diff_persist_raw_cow(&game.persist.raw, prev, force_full);
    let m_raw_1 = StatmMem::current();
    record_mem_cow_persist(
        game.world.game_time,
        m_raw_0,
        m_raw_1,
        persist_raw_changes.len(),
    );

    let raids_mem = if game.raiding.raids.map.is_empty() {
        Some(empty_raids())
    } else {
        let arc = Arc::new(game.raiding.raids.clone());
        prev.raids_mem = Some(Arc::clone(&arc));
        Some(arc)
    };

    let chest_openers = if world.chest_openers.is_empty() {
        empty_chest_openers()
    } else {
        Arc::new(world.chest_openers.iter().map(|(&k, &v)| (k, v)).collect())
    };
    let shulker_anims = if game.be_anims.shulkers.is_empty() {
        empty_shulker_anims()
    } else {
        Arc::new(
            game.be_anims
                .shulkers
                .iter()
                .map(|(&k, v)| (k, v.clone()))
                .collect(),
        )
    };
    let gateway_anims = if game.be_anims.gateway_servers.is_empty() {
        empty_gateway_anims()
    } else {
        Arc::new(
            game.be_anims
                .gateway_servers
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect(),
        )
    };
    let portals_entities = if game.portals.entities.is_empty() {
        empty_portals_entities()
    } else {
        Arc::new(game.portals.entities.clone())
    };
    let lectern_pulses = if game.lectern_pulses.is_empty() {
        empty_lectern_pulses()
    } else {
        Arc::new(game.lectern_pulses.clone())
    };
    let lid_openers = if game.lid_openers.is_empty() {
        empty_lid_openers()
    } else {
        Arc::new(game.lid_openers.clone())
    };
    let persist_passenger_slots = if game.persist.passenger_slots.is_empty() {
        empty_passenger_slots()
    } else {
        Arc::new(game.persist.passenger_slots.clone())
    };
    let last_hurt_by_player = if game.last_hurt_by_player.is_empty() {
        empty_hurt_by_player()
    } else {
        Arc::new(game.last_hurt_by_player.clone())
    };
    let love_cause = if game.love_cause.is_empty() {
        empty_love_cause()
    } else {
        Arc::new(game.love_cause.clone())
    };

    CowLevelRecord {
        chunk_changes,
        be_changes,
        block_names,
        be_attachments,
        world_attachments,
        chest_openers,
        border,
        shulker_anims,
        gateway_anims,
        fangs,
        ominous_spawners,
        portals_entities,
        lectern_pulses,
        lid_openers,
        inhabited_changes,
        end_clock: world.end_clock,
        random: world.random.clone(),
        forced_chunks,
        chunk_tick_changes,
        random_tick_value: game.block_ticks.random_tick_value,
        comparator_outputs,
        torch_toggles,
        moving_pistons,
        piston_pushes,
        block_events,
        hoppers,
        mob_changes,
        mob_order_changed,
        dropped,
        orbs,
        projectiles,
        boats,
        carts,
        tnts,
        falling,
        paintings,
        item_frames,
        armor_stands,
        cushions,
        others,
        crystals,
        dragon: game
            .dragon
            .as_ref()
            .map(DragonSnapshot::from_live)
            .map(Arc::new),
        tridents,
        rockets,
        breath_clouds,
        thrown_potions,
        knot_ids,
        persist_uuid_changes,
        persist_raw_changes,
        persist_passenger_slots,
        raids_mem,
        last_hurt_by_player,
        love_cause,
    }
}

fn capture_cow_parked_direct(
    game: &Game<'_>,
    dim: Dim,
    parked: &crate::game::levels::Parked,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> CowLevelRecord {
    let world = &parked.world;
    let live_chunks = &*world.chunks;
    let chunk_changes = diff_chunks_via_touch(world, prev, force_full);

    let player_chunks = collect_dim_player_chunks(game, dim);
    let be_changes = diff_block_entities_native_cow(
        game,
        world,
        &parked.hoppers,
        &parked.lid_openers,
        &chunk_changes,
        &player_chunks,
        false,
        false,
        prev,
        force_full,
    );

    let block_names = if !force_full
        && prev.world_revision == world.revision
        && let Some(p) = &prev.block_names
        && p.len() == world.block_names.len()
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.block_names.clone());
        prev.block_names = Some(Arc::clone(&arc));
        arc
    };

    let be_attachments = if !force_full
        && chunk_changes.is_empty()
        && be_changes.is_empty()
        && let Some(p) = &prev.be_attachments
    {
        Arc::clone(p)
    } else {
        let mut atts = Vec::new();
        for &(cx, cz) in live_chunks.keys() {
            atts.extend(world.block_entities.chunk_attachments(cx, cz));
        }
        let arc = Arc::new(atts);
        prev.be_attachments = Some(Arc::clone(&arc));
        arc
    };

    let world_attachments = if !force_full
        && prev.world_revision == world.revision
        && let Some(p) = &prev.world_attachments
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.attachments.clone());
        prev.world_attachments = Some(Arc::clone(&arc));
        arc
    };
    prev.world_revision = world.revision;

    let b_tag = parked.border.pack();
    let border = if !force_full
        && prev.border_tag.as_ref() == Some(&b_tag)
        && let Some(p) = &prev.border
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(parked.border.clone());
        prev.border_tag = Some(b_tag);
        prev.border = Some(Arc::clone(&arc));
        arc
    };

    let inhabited_changes = diff_inhabited_cow(&world.inhabited, prev, force_full);

    let forced_chunks = if !force_full
        && let Some(p) = &prev.forced_chunks
        && **p == parked.streaming.forced
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(parked.streaming.forced.clone());
        prev.forced_chunks = Some(Arc::clone(&arc));
        arc
    };

    macro_rules! cow_stamped {
        ($field:ident, $stamp_field:ident, $live:expr) => {{
            let st = $live.stamp();
            if !force_full && prev.$stamp_field == Some(st) && prev.$field.is_some() {
                Arc::clone(prev.$field.as_ref().unwrap())
            } else {
                let arc = Arc::new($live.clone());
                prev.$stamp_field = Some(st);
                prev.$field = Some(Arc::clone(&arc));
                arc
            }
        }};
    }

    macro_rules! cow_stamped_or_empty {
        ($field:ident, $stamp_field:ident, $live:expr, $empty_fn:expr) => {{
            if $live.is_empty() {
                prev.$stamp_field = Some($live.stamp());
                prev.$field = None;
                $empty_fn()
            } else {
                let st = $live.stamp();
                if !force_full && prev.$stamp_field == Some(st) && prev.$field.is_some() {
                    Arc::clone(prev.$field.as_ref().unwrap())
                } else {
                    let arc = Arc::new($live.clone());
                    prev.$stamp_field = Some(st);
                    prev.$field = Some(Arc::clone(&arc));
                    arc
                }
            }
        }};
    }

    let (mob_changes, mob_order_changed) =
        diff_mobs_cow(&parked.mobs, &mut prev.mobs, &mut prev.mob_order, force_full);
    let dropped = cow_stamped!(dropped, dropped_stamp, parked.dropped);
    let orbs = cow_stamped!(orbs, orbs_stamp, parked.orbs);
    let tnts = cow_stamped!(tnts, tnts_stamp, parked.tnts);
    let paintings = cow_stamped!(paintings, paintings_stamp, parked.paintings);
    let item_frames = cow_stamped!(item_frames, frames_stamp, parked.item_frames);
    let armor_stands = cow_stamped!(armor_stands, stands_stamp, parked.armor_stands);
    let cushions = cow_stamped!(cushions, cushions_stamp, parked.cushions);
    let crystals = cow_stamped!(crystals, crystals_stamp, parked.crystals);

    let projectiles = cow_stamped_or_empty!(
        projectiles,
        proj_stamp,
        parked.projectiles,
        empty_proj
    );
    let boats = cow_stamped_or_empty!(boats, boats_stamp, parked.boats, empty_boats);
    let carts = cow_stamped_or_empty!(carts, carts_stamp, parked.carts, empty_carts);
    let falling = cow_stamped_or_empty!(falling, falling_stamp, parked.falling, empty_falling);

    let others = if parked.others.is_empty() {
        empty_others()
    } else {
        Arc::new(parked.others.clone())
    };
    let fangs = if parked.fangs.list.is_empty() {
        empty_fangs()
    } else {
        Arc::new(parked.fangs.list.clone())
    };

    let tridents = if parked.tridents.is_empty() {
        empty_tridents()
    } else {
        let registry = game.pack.assets.registry;
        Arc::new(
            parked
                .tridents
                .iter()
                .map(|t| {
                    let uuid = parked.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
                    let raw = parked.persist.raw.get(&t.id);
                    TridentSnapshot {
                        join: t.join,
                        id: t.id,
                        nbt: t.to_nbt(uuid, registry, raw),
                        previous: t.previous,
                        in_ground_time: t.in_ground_time,
                        no_physics: t.no_physics,
                        owner: t.owner,
                        guest: t.guest,
                    }
                })
                .collect(),
        )
    };
    let rockets = if parked.rockets.is_empty() {
        empty_rockets()
    } else {
        Arc::new(
            parked
                .rockets
                .iter()
                .map(|r| {
                    let uuid = parked.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
                    let raw = parked.persist.raw.get(&r.id);
                    RocketSnapshot {
                        id: r.id,
                        nbt: r.to_nbt(uuid, None, raw),
                        attached: r.attached,
                        guest: r.guest,
                        owner: r.owner,
                        attached_to: r.attached_to,
                    }
                })
                .collect(),
        )
    };
    let breath_clouds = if parked.breath_clouds.is_empty() {
        empty_breath()
    } else {
        Arc::new(
            parked
                .breath_clouds
                .iter()
                .map(|c| {
                    let uuid = parked.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
                    let raw = parked.persist.raw.get(&c.id);
                    BreathSnapshot {
                        id: c.id,
                        nbt: super::super::potions::potion_cloud_to_nbt(c, uuid, raw),
                        at: c.at,
                        motion: c.motion,
                        radius: c.radius,
                        age: c.age,
                        duration: c.duration,
                        from_dragon: c.from_dragon,
                        potion_none: c.potion.is_none(),
                        victims: c.victims.clone(),
                    }
                })
                .collect(),
        )
    };
    let thrown_potions = empty_thrown_potions();
    let ominous_spawners = if parked.ominous_spawners.is_empty() {
        empty_ominous()
    } else {
        Arc::new(
            parked
                .ominous_spawners
                .iter()
                .map(|s| {
                    (
                        s.id,
                        horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                            &s.item,
                            Vec3::new(s.position[0], s.position[1], s.position[2]),
                            s.spawn_item_after,
                            s.uuid,
                            s.raw.as_deref(),
                        ),
                    )
                })
                .collect(),
        )
    };

    let chunk_tick_changes = diff_chunk_ticks_cow(live_chunks, &parked.block_ticks, prev, force_full);

    let block_events = if parked.block_ticks.block_events.is_empty() {
        empty_block_events()
    } else {
        Arc::new(parked.block_ticks.block_events.clone())
    };
    let moving_pistons = if parked.block_ticks.moving_pistons.is_empty() {
        empty_moving_pistons()
    } else {
        Arc::new(parked.block_ticks.moving_pistons.clone())
    };
    let piston_pushes = if parked.block_ticks.piston_pushes.is_empty() {
        empty_piston_pushes()
    } else {
        Arc::new(parked.block_ticks.piston_pushes.clone())
    };
    let torch_toggles = if parked.block_ticks.torch_toggles.is_empty() {
        empty_torch_toggles()
    } else {
        Arc::new(parked.block_ticks.torch_toggles.clone())
    };
    let comparator_outputs = if parked.block_ticks.comparator_outputs.is_empty() {
        empty_comparators()
    } else {
        Arc::new(parked.block_ticks.comparator_outputs.clone())
    };
    let hoppers = if parked.hoppers.is_empty() {
        empty_hoppers()
    } else {
        Arc::new(parked.hoppers.clone())
    };

    let knot_ids = if !force_full
        && let Some(p) = &prev.knot_ids
        && **p == parked.knot_ids
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(parked.knot_ids.clone());
        prev.knot_ids = Some(Arc::clone(&arc));
        arc
    };

    let persist_uuid_changes = diff_persist_uuids_cow(&parked.persist.uuids, prev, force_full);
    let persist_raw_changes = diff_persist_raw_cow(&parked.persist.raw, prev, force_full);

    let raids_mem = if parked.raiding.raids.map.is_empty() {
        Some(empty_raids())
    } else {
        let arc = Arc::new(parked.raiding.raids.clone());
        prev.raids_mem = Some(Arc::clone(&arc));
        Some(arc)
    };

    let chest_openers = if world.chest_openers.is_empty() {
        empty_chest_openers()
    } else {
        Arc::new(world.chest_openers.iter().map(|(&k, &v)| (k, v)).collect())
    };
    let shulker_anims = if parked.anims.shulkers.is_empty() {
        empty_shulker_anims()
    } else {
        Arc::new(
            parked
                .anims
                .shulkers
                .iter()
                .map(|(&k, v)| (k, v.clone()))
                .collect(),
        )
    };
    let gateway_anims = if parked.anims.gateway_servers.is_empty() {
        empty_gateway_anims()
    } else {
        Arc::new(
            parked
                .anims
                .gateway_servers
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect(),
        )
    };
    let portals_entities = if parked.portal_states.is_empty() {
        empty_portals_entities()
    } else {
        Arc::new(parked.portal_states.clone())
    };
    let lectern_pulses = if parked.lectern_pulses.is_empty() {
        empty_lectern_pulses()
    } else {
        Arc::new(parked.lectern_pulses.clone())
    };
    let lid_openers = if parked.lid_openers.is_empty() {
        empty_lid_openers()
    } else {
        Arc::new(parked.lid_openers.clone())
    };
    let persist_passenger_slots = if parked.persist.passenger_slots.is_empty() {
        empty_passenger_slots()
    } else {
        Arc::new(parked.persist.passenger_slots.clone())
    };

    CowLevelRecord {
        chunk_changes,
        be_changes,
        block_names,
        be_attachments,
        world_attachments,
        chest_openers,
        border,
        shulker_anims,
        gateway_anims,
        fangs,
        ominous_spawners,
        portals_entities,
        lectern_pulses,
        lid_openers,
        inhabited_changes,
        end_clock: world.end_clock,
        random: world.random.clone(),
        forced_chunks,
        chunk_tick_changes,
        random_tick_value: parked.block_ticks.random_tick_value,
        comparator_outputs,
        torch_toggles,
        moving_pistons,
        piston_pushes,
        block_events,
        hoppers,
        mob_changes,
        mob_order_changed,
        dropped,
        orbs,
        projectiles,
        boats,
        carts,
        tnts,
        falling,
        paintings,
        item_frames,
        armor_stands,
        cushions,
        others,
        crystals,
        dragon: parked
            .dragon
            .as_ref()
            .map(DragonSnapshot::from_live)
            .map(Arc::new),
        tridents,
        rockets,
        breath_clouds,
        thrown_potions,
        knot_ids,
        persist_uuid_changes,
        persist_raw_changes,
        persist_passenger_slots,
        raids_mem,
        last_hurt_by_player: empty_hurt_by_player(),
        love_cause: empty_love_cause(),
    }
}

fn snapshot_level_headers(
    prev: &mut CowLevelPrev,
) -> (
    Arc<FxHashMap<(i32, i32), Arc<Chunk>>>,
    Arc<HashMap<(i32, i32), Arc<(ChunkTicks, ChunkTicks)>>>,
    Arc<HashMap<(i32, i32, i32), Arc<BlockEntity>>>,
    Vec<Arc<super::super::Mob>>,
    Arc<HashMap<u32, Arc<Tag>>>,
    Arc<HashMap<u32, [i32; 4]>>,
    Arc<FxHashMap<(i32, i32), i64>>,
) {
    let h_chunks = if !prev.chunks_dirty_since_hdr
        && let Some(a) = &prev.chunks_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.chunks.clone());
        prev.chunks_hdr_arc = Some(Arc::clone(&a));
        prev.chunks_dirty_since_hdr = false;
        a
    };
    let h_ticks = if !prev.ticks_dirty_since_hdr
        && let Some(a) = &prev.ticks_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.chunk_ticks.clone());
        prev.ticks_hdr_arc = Some(Arc::clone(&a));
        prev.ticks_dirty_since_hdr = false;
        a
    };
    let h_be = if !prev.be_dirty_since_hdr
        && let Some(a) = &prev.be_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.block_entities.clone());
        prev.be_hdr_arc = Some(Arc::clone(&a));
        prev.be_dirty_since_hdr = false;
        a
    };
    let h_mobs: Vec<Arc<super::super::Mob>> = prev
        .mob_order
        .iter()
        .filter_map(|id| prev.mobs.get(id).map(Arc::clone))
        .collect();
    let h_raw = if !prev.persist_raw_dirty
        && let Some(a) = &prev.persist_raw_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.persist_raw.clone());
        prev.persist_raw_hdr_arc = Some(Arc::clone(&a));
        prev.persist_raw_dirty = false;
        a
    };
    let h_uuids = if !prev.persist_uuids_dirty
        && let Some(a) = &prev.persist_uuids_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.persist_uuids.clone());
        prev.persist_uuids_hdr_arc = Some(Arc::clone(&a));
        prev.persist_uuids_dirty = false;
        a
    };
    let h_inh = if !prev.inhabited_dirty_since_hdr
        && let Some(a) = &prev.inhabited_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.inhabited.clone());
        prev.inhabited_hdr_arc = Some(Arc::clone(&a));
        prev.inhabited_dirty_since_hdr = false;
        a
    };
    (h_chunks, h_ticks, h_be, h_mobs, h_raw, h_uuids, h_inh)
}

pub struct ServerScrubJournal {
    segments: VecDeque<ServerSegment>,
    total_ticks: usize,
    max_ticks: usize,
    next_seg_id: u64,
    segs_since_keyframe: usize,
    force_new_keyframe_segment: bool,
    seal_known_mobs: HashMap<Dim, HashSet<u32>>,
    last_recorded_tick: Option<i64>,
    cow_prev: CowGlobalPrev,
    pub unloaded: UnloadedChunkJournal,
}

impl Default for ServerScrubJournal {
    fn default() -> Self {
        Self {
            segments: VecDeque::with_capacity(240),
            total_ticks: 0,
            max_ticks: MAX_SERVER_SCRUB_TICKS,
            next_seg_id: 1,
            segs_since_keyframe: 0,
            force_new_keyframe_segment: true,
            seal_known_mobs: HashMap::new(),
            last_recorded_tick: None,
            cow_prev: CowGlobalPrev::default(),
            unloaded: UnloadedChunkJournal::default(),
        }
    }
}

impl ServerScrubJournal {
    fn update_rrtl_stats_and_spill(&mut self) {
        let mut ram = 0usize;
        let mut disk = 0usize;
        let mut raw = 0usize;
        let mut sealed = 0usize;
        for seg in &self.segments {
            if let Some(blob) = &seg.sealed_mob_patches {
                sealed += 1;
                ram += blob.ram_bytes();
                disk += blob.disk_bytes();
                raw += blob.raw_bytes();
            }
        }
        if ram > MAX_RRTL_SERVER_RAM_BYTES {
            let keep_hot = self.segments.len().saturating_sub(HOT_MOB_SEGMENTS + 2);
            for i in 0..keep_hot {
                if ram <= MAX_RRTL_SERVER_RAM_BYTES {
                    break;
                }
                let seg = &mut self.segments[i];
                if let Some(blob) = seg.sealed_mob_patches.as_mut() {
                    let before_ram = blob.ram_bytes();
                    if before_ram > 0
                        && rrtl_spill_server_blob(
                            seg.seg_id,
                            seg.branch_id,
                            seg.parent_seg_id,
                            seg.start_tick,
                            seg.end_tick,
                            blob,
                        )
                    {
                        ram = ram.saturating_sub(before_ram);
                        disk += blob.disk_bytes();
                    }
                }
            }
        }
        rrtl_update_server_stats(ram, disk, raw, sealed);
    }

    fn record_tick_inner(&mut self, game: &Game<'_>) {
        let t_start = std::time::Instant::now();
        let now = game.world.game_time;
        if self.last_recorded_tick == Some(now) {
            return;
        }
        rrtl_configure_world(&game.world_root, game.level.seed);
        if let Some(last) = self.last_recorded_tick
            && now < last
        {
            self.truncate_after_inner(now - 1);
        }

        let _m_tick_start = record_mem_server_tick_start(now);

        let t0 = std::time::Instant::now();
        let m_unl_0 = StatmMem::current();
        self.unloaded.observe_game(game, now);
        let m_unl_1 = StatmMem::current();
        let t_unloaded = t0.elapsed();
        let unl_baselines = self.unloaded.baselines.len();
        let unl_history = self
            .unloaded
            .history
            .values()
            .map(|v| v.len())
            .sum::<usize>();
        record_mem_unloaded(now, m_unl_0, m_unl_1, unl_baselines, unl_history);

        let start_new_segment = self.force_new_keyframe_segment
            || self
                .segments
                .back()
                .is_none_or(|seg| seg.ticks.len() >= SERVER_SEGMENT_TICKS);
        let is_keyframe_seg = start_new_segment
            && (self.force_new_keyframe_segment
                || self.segments.is_empty()
                || self.segs_since_keyframe >= HEADER_KEYFRAME_STRIDE);
        if start_new_segment {
            self.force_new_keyframe_segment = false;
        }

        let any_menu_open = game.container.is_some()
            || game
                .lan
                .as_ref()
                .is_some_and(|h| h.hub.guests.iter().any(|g| g.menu.is_some()));

        let t_globals_start = std::time::Instant::now();
        let m_glob_0 = StatmMem::current();
        let (guests_cow, online) =
            cow_capture_guests(game, &mut self.cow_prev.guests, start_new_segment);
        let (anchor_set, policy) = with_state(|st| {
            let anchors: HashSet<u128> =
                resolve_anchors(&st.anchor, &online, now, None).into_iter().collect();
            (anchors, st.policy)
        });

        let anchor_ids = if !start_new_segment
            && let Some(p) = &self.cow_prev.anchor_ids
            && **p == anchor_set
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(anchor_set);
            self.cow_prev.anchor_ids = Some(Arc::clone(&arc));
            arc
        };

        let gb_tag = game.border.pack();
        let border = if !start_new_segment
            && self.cow_prev.border_tag.as_ref() == Some(&gb_tag)
            && let Some(p) = &self.cow_prev.border
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.border.clone());
            self.cow_prev.border_tag = Some(gb_tag);
            self.cow_prev.border = Some(Arc::clone(&arc));
            arc
        };

        let bb_tag = game.bosses.bars.pack();
        let bossbars = if !start_new_segment
            && let Some(p) = &self.cow_prev.bossbars
            && **p == bb_tag
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(bb_tag);
            self.cow_prev.bossbars = Some(Arc::clone(&arc));
            arc
        };

        let f_tag = Some(game.fight.rreturn_pack());
        let fight_tag = if !start_new_segment
            && let Some(p) = &self.cow_prev.fight_tag
            && **p == f_tag
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(f_tag);
            self.cow_prev.fight_tag = Some(Arc::clone(&arc));
            arc
        };

        if !self.cow_prev.trader_disk_loaded {
            self.cow_prev.trader_disk_loaded = true;
            let disk_tag = read_dat(&trader_dat_path(&game.world_root));
            self.cow_prev.trader_tag = Some(Arc::new(disk_tag));
        }
        let trader_tag = if let Some(live_tag) = game.creatures.trader.to_save() {
            let live_opt = Some(live_tag);
            if !start_new_segment
                && let Some(p) = &self.cow_prev.trader_tag
                && **p == live_opt
            {
                Arc::clone(p)
            } else {
                let arc = Arc::new(live_opt);
                self.cow_prev.trader_tag = Some(Arc::clone(&arc));
                arc
            }
        } else if let Some(p) = &self.cow_prev.trader_tag {
            Arc::clone(p)
        } else {
            let arc = Arc::new(None);
            self.cow_prev.trader_tag = Some(Arc::clone(&arc));
            arc
        };

        let sb_tag = game.score.board.pack();
        let scoreboard = if !start_new_segment
            && self.cow_prev.scoreboard_tag.as_ref() == Some(&sb_tag)
            && let Some(p) = &self.cow_prev.scoreboard
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.score.board.clone());
            self.cow_prev.scoreboard_tag = Some(sb_tag);
            self.cow_prev.scoreboard = Some(Arc::clone(&arc));
            arc
        };

        let rs_tag = game.rand_sequences.to_nbt();
        let rand_sequences = if !start_new_segment
            && self.cow_prev.rand_seq_tag.as_ref() == Some(&rs_tag)
            && let Some(p) = &self.cow_prev.rand_sequences
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.rand_sequences.clone());
            self.cow_prev.rand_seq_tag = Some(rs_tag);
            self.cow_prev.rand_sequences = Some(Arc::clone(&arc));
            arc
        };

        let command_storage = if !start_new_segment
            && let Some(p) = &self.cow_prev.command_storage
            && **p == game.score.storage
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.score.storage.clone());
            self.cow_prev.command_storage = Some(Arc::clone(&arc));
            arc
        };

        let sw_map = snapshot_stopwatches_elapsed(&game.score.stopwatches);
        let stopwatches_elapsed = if !start_new_segment
            && let Some(p) = &self.cow_prev.stopwatches_elapsed
            && **p == sw_map
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(sw_map);
            self.cow_prev.stopwatches_elapsed = Some(Arc::clone(&arc));
            arc
        };

        let player_extra = if !start_new_segment
            && let Some(p) = &self.cow_prev.player_extra
            && **p == game.level.player_extra
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.level.player_extra.clone());
            self.cow_prev.player_extra = Some(Arc::clone(&arc));
            arc
        };

        let shoulder_time = if !start_new_segment
            && let Some(p) = &self.cow_prev.shoulder_time
            && **p == game.creatures.shoulder_time
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.creatures.shoulder_time.clone());
            self.cow_prev.shoulder_time = Some(Arc::clone(&arc));
            arc
        };

        let gw_cd = game.end.rreturn_gateway_cooldowns();
        let end_gateway_cooldowns = if !start_new_segment
            && let Some(p) = &self.cow_prev.end_gateway_cooldowns
            && **p == gw_cd
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(gw_cd);
            self.cow_prev.end_gateway_cooldowns = Some(Arc::clone(&arc));
            arc
        };

        let player_beds = if !start_new_segment
            && let Some(p) = &self.cow_prev.player_beds
            && **p == game.player_beds
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.player_beds.clone());
            self.cow_prev.player_beds = Some(Arc::clone(&arc));
            arc
        };

        let lead_killers = if !start_new_segment
            && let Some(p) = &self.cow_prev.lead_killers
            && **p == game.lead_killers
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.lead_killers.clone());
            self.cow_prev.lead_killers = Some(Arc::clone(&arc));
            arc
        };

        let cmd_post_effects = if !start_new_segment
            && let Some(p) = &self.cow_prev.cmd_post_effects
            && **p == game.cmds.post_effects
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.cmds.post_effects.clone());
            self.cow_prev.cmd_post_effects = Some(Arc::clone(&arc));
            arc
        };

        let mp_map = game
            .lan
            .as_ref()
            .map(|h| h.mob_players.clone())
            .unwrap_or_default();
        let mob_players = if !start_new_segment
            && let Some(p) = &self.cow_prev.mob_players
            && **p == mp_map
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(mp_map);
            self.cow_prev.mob_players = Some(Arc::clone(&arc));
            arc
        };

        let player_attachments = if !start_new_segment
            && let Some(p) = &self.cow_prev.player_attachments
        {
            Arc::clone(p)
        } else {
            let arc = Arc::new(game.player_state.attachments.clone());
            self.cow_prev.player_attachments = Some(Arc::clone(&arc));
            arc
        };

        let t_globals = t_globals_start.elapsed();
        let m_glob_1 = StatmMem::current();
        record_mem_globals(
            now,
            m_glob_0,
            m_glob_1,
            guests_cow.len(),
            game.persist.raw.len(),
        );

        let mut header_chunks = HashMap::new();
        let mut header_chunk_ticks = HashMap::new();
        let mut header_block_entities = HashMap::new();
        let mut header_mobs = HashMap::new();
        let mut header_persist_raw = HashMap::new();
        let mut header_persist_uuids = HashMap::new();
        let mut header_inhabited = HashMap::new();
        let mut cow_levels = Vec::with_capacity(1 + game.levels.parked.len());

        let t_cow_0 = std::time::Instant::now();
        {
            let active_prev = self.cow_prev.levels.entry(game.dim).or_default();
            let active_cow = capture_cow_level_direct(
                game,
                game.dim,
                &game.world,
                active_prev,
                any_menu_open,
                is_keyframe_seg,
            );
            if is_keyframe_seg {
                let (hc, ht, hbe, hm, hraw, hu, hinh) = snapshot_level_headers(active_prev);
                header_chunks.insert(game.dim, hc);
                header_chunk_ticks.insert(game.dim, ht);
                header_block_entities.insert(game.dim, hbe);
                header_mobs.insert(game.dim, hm);
                header_persist_raw.insert(game.dim, hraw);
                header_persist_uuids.insert(game.dim, hu);
                header_inhabited.insert(game.dim, hinh);
            }
            cow_levels.push((game.dim, active_cow));
        }

        for (&dim, parked) in &game.levels.parked {
            let p_prev = self.cow_prev.levels.entry(dim).or_default();
            let p_cow = capture_cow_parked_direct(game, dim, parked, p_prev, is_keyframe_seg);
            if is_keyframe_seg {
                let (hc, ht, hbe, hm, hraw, hu, hinh) = snapshot_level_headers(p_prev);
                header_chunks.insert(dim, hc);
                header_chunk_ticks.insert(dim, ht);
                header_block_entities.insert(dim, hbe);
                header_mobs.insert(dim, hm);
                header_persist_raw.insert(dim, hraw);
                header_persist_uuids.insert(dim, hu);
                header_inhabited.insert(dim, hinh);
            }
            cow_levels.push((dim, p_cow));
        }
        let t_cow = t_cow_0.elapsed();

        let fangs = if game.fangs.list.is_empty() {
            empty_fangs()
        } else {
            Arc::new(game.fangs.list.clone())
        };
        let ominous_spawners = if game.ominous_spawners.is_empty() {
            empty_ominous()
        } else {
            Arc::new(
                game.ominous_spawners
                    .iter()
                    .map(|s| {
                        (
                            s.id,
                            horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                                &s.item,
                                Vec3::new(s.position[0], s.position[1], s.position[2]),
                                s.spawn_item_after,
                                s.uuid,
                                s.raw.as_deref(),
                            ),
                        )
                    })
                    .collect(),
            )
        };
        let command_clocks = if game.ui.command_clocks.is_empty() {
            empty_command_clocks()
        } else {
            Arc::new(game.ui.command_clocks.clone())
        };
        let portals_entities = if game.portals.entities.is_empty() {
            empty_portals_entities()
        } else {
            Arc::new(game.portals.entities.clone())
        };
        let lectern_pulses = if game.lectern_pulses.is_empty() {
            empty_lectern_pulses()
        } else {
            Arc::new(game.lectern_pulses.clone())
        };
        let lid_openers = if game.lid_openers.is_empty() {
            empty_lid_openers()
        } else {
            Arc::new(game.lid_openers.clone())
        };
        let entity_attachments = if game.entity_attachments.is_empty() {
            empty_entity_attachments()
        } else {
            Arc::new(game.entity_attachments.clone())
        };
        let guest_attachments = match &game.lan {
            Some(host)
                if host
                    .hub
                    .guests
                    .iter()
                    .any(|g| !g.attachments.to_nbt().is_empty()) =>
            {
                Arc::new(
                    host.hub
                        .guests
                        .iter()
                        .map(|g| (g.uuid, (*g.attachments).clone()))
                        .collect(),
                )
            }
            _ => empty_guest_attachments(),
        };
        let scheduled_events = if game.fns.schedule.is_empty() {
            empty_scheduled()
        } else {
            Arc::new(game.fns.schedule.clone())
        };

        let record = ServerTickRecord {
            anchor_ids,
            policy,
            game_time: now,
            day_time: game.world.day_time,
            weather: game.world.weather,
            health_rules: game.world.health_rules,
            rules: Arc::new(game.ui.rules.clone()),
            border,
            bossbars,
            sculk_tracker: game.sculk.tracker,
            sculk_darkness: game.sculk.darkness,
            fight_tag,
            trader_tag,
            rand_sequences,
            difficulty: game.ui.difficulty,
            difficulty_locked: game.level.difficulty_locked,
            clock_rate: game.clock.rate(),
            clock_frozen: game.clock.frozen(),
            time_rate: game.ui.clock.rate,
            time_partial: game.ui.clock.partial,
            time_paused: game.ui.clock.paused,
            command_clocks,
            world_spawn: game.level.world_spawn,
            world_spawn_angle: game.level.world_spawn_angle,
            next_entity: game.next_entity,
            ticked_entities_through: game.ticked_entities_through,
            ordered_announced: game.ordered.announced,
            cat_tick: game.creatures.cat.next_tick,
            phantom_tick: game.stats.rreturn_phantom_tick(),
            player_extra,
            shoulder_time,
            fangs,
            ominous_spawners,
            portals_entities,
            end_gateway_cooldowns,
            lectern_pulses,
            player_beds,
            sleep_seen: game.sleep_seen,
            lead_killers,
            lid_openers,
            entity_attachments,
            player_attachments,
            guest_attachments,
            scheduled_events,
            scoreboard,
            command_storage,
            stopwatches_elapsed,
            cmd_post_effects,
            guests: guests_cow,
            levels: cow_levels,
            mob_players,
            client_using: game.using,
            client_use_duration: game.use_duration,
            client_crossbow_sounds: game.crossbow_sounds,
        };

        if start_new_segment {
            let seg_id = self.next_seg_id;
            self.next_seg_id += 1;
            let parent_seg_id = self.segments.back().map_or(0, |s| s.seg_id);
            let branch_id = rrtl_active_branch();
            if is_keyframe_seg {
                self.segs_since_keyframe = 1;
            } else {
                self.segs_since_keyframe += 1;
            }
            let mut ticks = Vec::with_capacity(SERVER_SEGMENT_TICKS);
            ticks.push(record);
            self.segments.push_back(ServerSegment {
                seg_id,
                branch_id,
                parent_seg_id,
                is_keyframe: is_keyframe_seg,
                start_tick: now,
                end_tick: now,
                sealed_mob_patches: None,
                dim_table: Vec::new(),
                header_chunks,
                header_chunk_ticks,
                header_block_entities,
                header_mobs,
                header_persist_raw,
                header_persist_uuids,
                header_inhabited,
                ticks,
            });
            if self.segments.len() > HOT_MOB_SEGMENTS {
                let idx = self.segments.len() - HOT_MOB_SEGMENTS - 1;
                let mut known = std::mem::take(&mut self.seal_known_mobs);
                self.segments[idx].seal_mobs_to_rrtl(&mut known);
                self.seal_known_mobs = known;
                self.update_rrtl_stats_and_spill();
                horizon_threads::release_free_memory();
            }
        } else if let Some(seg) = self.segments.back_mut() {
            seg.end_tick = now;
            seg.ticks.push(record);
        }

        self.total_ticks += 1;
        while self.total_ticks > self.max_ticks {
            let next_kf = (1..self.segments.len()).find(|&i| self.segments[i].is_keyframe);
            let Some(drop_count) = next_kf else {
                break;
            };
            for _ in 0..drop_count {
                if let Some(evicted) = self.segments.pop_front() {
                    self.total_ticks = self.total_ticks.saturating_sub(evicted.ticks.len());
                }
            }
            if let Some(oldest) = self.segments.front() {
                self.unloaded.evict_before(oldest.start_tick);
            }
        }
        self.last_recorded_tick = Some(now);
        let total_elapsed = t_start.elapsed();
        record_server_tick_perf(
            total_elapsed.as_micros() as u64,
            t_unloaded.as_micros() as u64,
            0,
            t_globals.as_micros() as u64,
            t_cow.as_micros() as u64,
        );
        let m_end = StatmMem::current();
        record_mem_server_tick_end(now, m_end);
    }

    fn materialize_tick(&self, target_tick: i64) -> Option<Arc<TimelineSnapshot>> {
        if self.segments.is_empty() {
            return None;
        }
        let seg_idx = match self
            .segments
            .binary_search_by_key(&target_tick, |s| s.start_tick)
        {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };
        let mut kf_idx = seg_idx;
        while kf_idx > 0 && !self.segments[kf_idx].is_keyframe {
            kf_idx -= 1;
        }
        let kf_seg = &self.segments[kf_idx];
        let target_seg = &self.segments[seg_idx];
        let tick_idx = match target_seg
            .ticks
            .binary_search_by_key(&target_tick, |r| r.game_time)
        {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => (i - 1).min(target_seg.ticks.len() - 1),
        };

        let mut reconstructed_chunks: HashMap<Dim, FxHashMap<(i32, i32), Arc<Chunk>>> = kf_seg
            .header_chunks
            .iter()
            .map(|(&d, m)| (d, (**m).clone()))
            .collect();
        let mut reconstructed_ticks: HashMap<
            Dim,
            HashMap<(i32, i32), Arc<(ChunkTicks, ChunkTicks)>>,
        > = kf_seg
            .header_chunk_ticks
            .iter()
            .map(|(&d, m)| (d, (**m).clone()))
            .collect();
        let mut reconstructed_bes: HashMap<Dim, HashMap<(i32, i32, i32), Arc<BlockEntity>>> =
            kf_seg
                .header_block_entities
                .iter()
                .map(|(&d, m)| (d, (**m).clone()))
                .collect();
        let mut reconstructed_raw: HashMap<Dim, HashMap<u32, Arc<Tag>>> = kf_seg
            .header_persist_raw
            .iter()
            .map(|(&d, m)| (d, (**m).clone()))
            .collect();
        let mut reconstructed_uuids: HashMap<Dim, HashMap<u32, [i32; 4]>> = kf_seg
            .header_persist_uuids
            .iter()
            .map(|(&d, m)| (d, (**m).clone()))
            .collect();
        let mut reconstructed_inhabited: HashMap<Dim, FxHashMap<(i32, i32), i64>> = kf_seg
            .header_inhabited
            .iter()
            .map(|(&d, m)| (d, (**m).clone()))
            .collect();

        let mut reconstructed_mobs: HashMap<Dim, FxHashMap<u32, Arc<super::super::Mob>>> =
            HashMap::new();
        let mut reconstructed_mob_order: HashMap<Dim, Vec<u32>> = HashMap::new();
        for (&dim, list) in &kf_seg.header_mobs {
            let mut map = FxHashMap::default();
            let mut order = Vec::with_capacity(list.len());
            for m in list {
                order.push(m.id);
                map.insert(m.id, Arc::clone(m));
            }
            reconstructed_mobs.insert(dim, map);
            reconstructed_mob_order.insert(dim, order);
        }

        for cur_seg_idx in kf_idx..=seg_idx {
            let s = &self.segments[cur_seg_idx];
            let end_ti = if cur_seg_idx == seg_idx {
                tick_idx
            } else {
                s.ticks.len().saturating_sub(1)
            };
            let mut decoded_patches: HashMap<(usize, Dim), Vec<MobCompactPatch>> = HashMap::new();
            if let Some(blob) = &s.sealed_mob_patches
                && let Some(batches) = decode_mob_patch_batches_rrtl(blob)
            {
                for (t_idx, d_idx, list) in batches {
                    let ti = t_idx as usize;
                    if ti <= end_ti
                        && let Some(&dim) = s.dim_table.get(d_idx as usize)
                    {
                        decoded_patches.insert((ti, dim), list);
                    }
                }
            }
            for (ti, rec) in s.ticks[..=end_ti].iter().enumerate() {
                for (dim, lrec) in &rec.levels {
                    let c_map = reconstructed_chunks.entry(*dim).or_default();
                    for (pos, opt_chunk) in &lrec.chunk_changes {
                        match opt_chunk {
                            Some(c) => {
                                c_map.insert(*pos, Arc::clone(c));
                            }
                            None => {
                                c_map.remove(pos);
                            }
                        }
                    }
                    let t_map = reconstructed_ticks.entry(*dim).or_default();
                    for (pos, opt_t) in &lrec.chunk_tick_changes {
                        match opt_t {
                            Some(t) => {
                                t_map.insert(*pos, Arc::clone(t));
                            }
                            None => {
                                t_map.remove(pos);
                            }
                        }
                    }
                    let be_map = reconstructed_bes.entry(*dim).or_default();
                    for (pos, opt_be) in &lrec.be_changes {
                        match opt_be {
                            Some(be) => {
                                be_map.insert(*pos, Arc::clone(be));
                            }
                            None => {
                                be_map.remove(pos);
                            }
                        }
                    }
                    let raw_map = reconstructed_raw.entry(*dim).or_default();
                    for (id, opt_tag) in &lrec.persist_raw_changes {
                        match opt_tag {
                            Some(t) => {
                                raw_map.insert(*id, Arc::clone(t));
                            }
                            None => {
                                raw_map.remove(id);
                            }
                        }
                    }
                    let uuid_map = reconstructed_uuids.entry(*dim).or_default();
                    for &(id, opt_u) in &lrec.persist_uuid_changes {
                        match opt_u {
                            Some(u) => {
                                uuid_map.insert(id, u);
                            }
                            None => {
                                uuid_map.remove(&id);
                            }
                        }
                    }
                    let inh_map = reconstructed_inhabited.entry(*dim).or_default();
                    for &(k, opt_v) in &lrec.inhabited_changes {
                        match opt_v {
                            Some(v) => {
                                inh_map.insert(k, v);
                            }
                            None => {
                                inh_map.remove(&k);
                            }
                        }
                    }
                    let m_map = reconstructed_mobs.entry(*dim).or_default();
                    for (id, opt_m) in &lrec.mob_changes {
                        match opt_m {
                            Some(m) => {
                                m_map.insert(*id, Arc::clone(m));
                            }
                            None => {
                                m_map.remove(id);
                            }
                        }
                    }
                    if let Some(patches) = decoded_patches.get(&(ti, *dim)) {
                        for patch in patches {
                            if let Some(m_arc) = m_map.get_mut(&patch.id) {
                                patch.apply_to(Arc::make_mut(m_arc));
                            }
                        }
                    }
                    if let Some(ord) = &lrec.mob_order_changed {
                        reconstructed_mob_order.insert(*dim, (**ord).clone());
                    }
                }
            }
        }

        let rec = &target_seg.ticks[tick_idx];
        let mut levels = HashMap::with_capacity(rec.levels.len());
        for (dim, lrec) in &rec.levels {
            let chunks = reconstructed_chunks.remove(dim).unwrap_or_default();
            let chunk_ticks: HashMap<(i32, i32), (ChunkTicks, ChunkTicks)> = reconstructed_ticks
                .remove(dim)
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, (*v).clone()))
                .collect();
            let block_entities: HashMap<(i32, i32, i32), BlockEntity> = reconstructed_bes
                .remove(dim)
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, (*v).clone()))
                .collect();
            let persist_raw: HashMap<u32, Tag> = reconstructed_raw
                .remove(dim)
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, (*v).clone()))
                .collect();
            let persist_uuids = reconstructed_uuids.remove(dim).unwrap_or_default();
            let inhabited = reconstructed_inhabited.remove(dim).unwrap_or_default();

            let mut mobs = horizon_client::join_seq::JoinList::default();
            let mut m_map = reconstructed_mobs.remove(dim).unwrap_or_default();
            let order = reconstructed_mob_order.remove(dim).unwrap_or_default();
            for id in order {
                if let Some(m_arc) = m_map.remove(&id) {
                    mobs.push((*m_arc).clone());
                }
            }
            for (_, m_arc) in m_map {
                mobs.push((*m_arc).clone());
            }

            levels.insert(
                *dim,
                LevelSnapshot {
                    chunks,
                    block_entities,
                    block_names: (*lrec.block_names).clone(),
                    be_attachments: (*lrec.be_attachments).clone(),
                    world_attachments: (*lrec.world_attachments).clone(),
                    chest_openers: (*lrec.chest_openers).clone(),
                    border: (*lrec.border).clone(),
                    shulker_anims: (*lrec.shulker_anims).clone(),
                    gateway_anims: (*lrec.gateway_anims).clone(),
                    fangs: (*lrec.fangs).clone(),
                    ominous_spawners: (*lrec.ominous_spawners).clone(),
                    portals_entities: (*lrec.portals_entities).clone(),
                    lectern_pulses: (*lrec.lectern_pulses).clone(),
                    lid_openers: (*lrec.lid_openers).clone(),
                    inhabited,
                    end_clock: lrec.end_clock,
                    random: lrec.random.clone(),
                    forced_chunks: (*lrec.forced_chunks).clone(),
                    chunk_ticks,
                    random_tick_value: lrec.random_tick_value,
                    comparator_outputs: (*lrec.comparator_outputs).clone(),
                    torch_toggles: (*lrec.torch_toggles).clone(),
                    moving_pistons: (*lrec.moving_pistons).clone(),
                    piston_pushes: (*lrec.piston_pushes).clone(),
                    block_events: (*lrec.block_events).clone(),
                    hoppers: (*lrec.hoppers).clone(),
                    mobs,
                    dropped: (*lrec.dropped).clone(),
                    orbs: (*lrec.orbs).clone(),
                    projectiles: (*lrec.projectiles).clone(),
                    boats: (*lrec.boats).clone(),
                    carts: (*lrec.carts).clone(),
                    tnts: (*lrec.tnts).clone(),
                    falling: (*lrec.falling).clone(),
                    paintings: (*lrec.paintings).clone(),
                    item_frames: (*lrec.item_frames).clone(),
                    armor_stands: (*lrec.armor_stands).clone(),
                    cushions: (*lrec.cushions).clone(),
                    others: (*lrec.others).clone(),
                    crystals: (*lrec.crystals).clone(),
                    dragon: lrec.dragon.as_deref().cloned(),
                    tridents: (*lrec.tridents).clone(),
                    rockets: (*lrec.rockets).clone(),
                    breath_clouds: (*lrec.breath_clouds).clone(),
                    thrown_potions: (*lrec.thrown_potions).clone(),
                    knot_ids: (*lrec.knot_ids).clone(),
                    persist_uuids,
                    persist_raw,
                    persist_passenger_slots: (*lrec.persist_passenger_slots).clone(),
                    raids_dat: None,
                    raids_mem: lrec.raids_mem.as_deref().cloned(),
                    last_hurt_by_player: (*lrec.last_hurt_by_player).clone(),
                    love_cause: (*lrec.love_cause).clone(),
                },
            );
        }

        let guests: HashMap<u128, GuestSnapshot> = rec
            .guests
            .iter()
            .map(|g| (g.uuid, g.to_guest_snapshot(rec.game_time)))
            .collect();

        Some(Arc::new(TimelineSnapshot {
            anchor_ids: (*rec.anchor_ids).clone(),
            policy: rec.policy,
            game_time: rec.game_time,
            day_time: rec.day_time,
            weather: rec.weather,
            health_rules: rec.health_rules,
            rules: (*rec.rules).clone(),
            border: (*rec.border).clone(),
            bossbars: (*rec.bossbars).clone(),
            sculk_tracker: rec.sculk_tracker,
            sculk_darkness: rec.sculk_darkness,
            dragon_fight_dat: None,
            fight_tag: (*rec.fight_tag).clone(),
            trader_dat: None,
            trader_tag: (*rec.trader_tag).clone(),
            rand_sequences: (*rec.rand_sequences).clone(),
            region_files: HashSet::new(),
            difficulty: rec.difficulty,
            difficulty_locked: rec.difficulty_locked,
            clock_rate: rec.clock_rate,
            clock_frozen: rec.clock_frozen,
            time_rate: rec.time_rate,
            time_partial: rec.time_partial,
            time_paused: rec.time_paused,
            command_clocks: (*rec.command_clocks).clone(),
            world_spawn: rec.world_spawn,
            world_spawn_angle: rec.world_spawn_angle,
            next_entity: rec.next_entity,
            ticked_entities_through: rec.ticked_entities_through,
            ordered_announced: rec.ordered_announced,
            cat_tick: rec.cat_tick,
            phantom_tick: rec.phantom_tick,
            player_extra: (*rec.player_extra).clone(),
            shoulder_time: (*rec.shoulder_time).clone(),
            fangs: (*rec.fangs).clone(),
            ominous_spawners: (*rec.ominous_spawners).clone(),
            portals_entities: (*rec.portals_entities).clone(),
            end_gateway_cooldowns: (*rec.end_gateway_cooldowns).clone(),
            lectern_pulses: (*rec.lectern_pulses).clone(),
            player_beds: (*rec.player_beds).clone(),
            sleep_seen: rec.sleep_seen,
            lead_killers: (*rec.lead_killers).clone(),
            lid_openers: (*rec.lid_openers).clone(),
            entity_attachments: (*rec.entity_attachments).clone(),
            player_attachments: (*rec.player_attachments).clone(),
            guest_attachments: (*rec.guest_attachments).clone(),
            scheduled_events: (*rec.scheduled_events).clone(),
            scoreboard: (*rec.scoreboard).clone(),
            command_storage: Some((*rec.command_storage).clone()),
            stopwatches_elapsed: Some((*rec.stopwatches_elapsed).clone()),
            sound_seed_state: None,
            cmd_post_effects: (*rec.cmd_post_effects).clone(),
            guests,
            offline_player_data: HashMap::new(),
            levels,
            mob_players: (*rec.mob_players).clone(),
            disk_restored: true,
            client_using: rec.client_using,
            client_use_duration: rec.client_use_duration,
            client_use_ticks_set: false,
            client_crossbow_sounds: rec.client_crossbow_sounds,
        }))
    }

    fn truncate_after_inner(&mut self, max_tick: i64) {
        let mut truncated_any = false;
        while let Some(back) = self.segments.back_mut() {
            if back.start_tick > max_tick {
                let removed = self.segments.pop_back().unwrap();
                self.total_ticks = self.total_ticks.saturating_sub(removed.ticks.len());
                truncated_any = true;
            } else {
                let before = back.ticks.len();
                back.ticks.retain(|r| r.game_time <= max_tick);
                let dropped = before - back.ticks.len();
                self.total_ticks = self.total_ticks.saturating_sub(dropped);
                if dropped > 0 {
                    truncated_any = true;
                }
                if back.ticks.is_empty() {
                    self.segments.pop_back();
                } else {
                    back.end_tick = back.ticks.last().unwrap().game_time;
                }
                break;
            }
        }
        if truncated_any {
            let _ = rrtl_begin_branch(max_tick);
        }
        self.force_new_keyframe_segment = true;
        self.segs_since_keyframe = 0;
        self.seal_known_mobs.clear();
        self.unloaded.truncate_after(max_tick);
        self.cow_prev = CowGlobalPrev::default();
        self.last_recorded_tick = self.segments.back().map(|s| s.end_tick);
        self.update_rrtl_stats_and_spill();
        horizon_threads::release_free_memory();
    }

    fn time_range(&self) -> Option<(i64, i64)> {
        let first = self.segments.front()?.start_tick;
        let last = self.segments.back()?.end_tick;
        Some((first, last))
    }

    fn clear_inner(&mut self) {
        self.segments.clear();
        self.total_ticks = 0;
        self.next_seg_id = 1;
        self.segs_since_keyframe = 0;
        self.force_new_keyframe_segment = true;
        self.seal_known_mobs.clear();
        self.last_recorded_tick = None;
        self.cow_prev = CowGlobalPrev::default();
        self.unloaded.clear();
        rrtl_clear_all();
        horizon_threads::release_free_memory();
    }
}

thread_local! {
    static SERVER_JOURNAL: RefCell<ServerScrubJournal> = RefCell::new(ServerScrubJournal::default());
}

static SERVER_MIN_TICK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1);
static SERVER_MAX_TICK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1);

pub fn update_cached_server_range(range: Option<(i64, i64)>) {
    if let Some((min, max)) = range {
        SERVER_MIN_TICK.store(min, std::sync::atomic::Ordering::Release);
        SERVER_MAX_TICK.store(max, std::sync::atomic::Ordering::Release);
    } else {
        SERVER_MIN_TICK.store(-1, std::sync::atomic::Ordering::Release);
        SERVER_MAX_TICK.store(-1, std::sync::atomic::Ordering::Release);
    }
}

pub fn cached_server_scrub_time_range() -> Option<(i64, i64)> {
    let min = SERVER_MIN_TICK.load(std::sync::atomic::Ordering::Acquire);
    let max = SERVER_MAX_TICK.load(std::sync::atomic::Ordering::Acquire);
    if min >= 0 && max >= min {
        Some((min, max))
    } else {
        None
    }
}

pub fn record_server_tick(game: &Game<'_>) {
    if !game.clock.runs_normally() {
        return;
    }
    SERVER_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        j.record_tick_inner(game);
        update_cached_server_range(j.time_range());
    });
}

pub fn handle_server_chunk_unload(game: &mut Game<'_>, dimension: &str, chunk: [i32; 2]) {
    let dim = Dim::from_id(dimension);
    let now = game.world.game_time;
    SERVER_JOURNAL.with(|cell| {
        cell.borrow_mut()
            .unloaded
            .on_chunk_unloaded(game, dim, chunk[0], chunk[1], now);
    });
}

pub fn materialize_server_scrub(
    game: &Game<'_>,
    target_tick: i64,
) -> Option<(Arc<TimelineSnapshot>, Vec<UnloadedChunkRestore>, Vec<PathBuf>)> {
    SERVER_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        let snap = j.materialize_tick(target_tick)?;
        let (touched_unloaded, future_region_files) =
            j.unloaded.build_unloaded_restores(game, snap.game_time);
        Some((snap, touched_unloaded, future_region_files))
    })
}

pub fn get_server_snapshot_for_tick(target_tick: i64) -> Option<Arc<TimelineSnapshot>> {
    SERVER_JOURNAL.with(|cell| cell.borrow().materialize_tick(target_tick))
}

pub fn truncate_server_scrub_after(max_tick: i64) {
    SERVER_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        j.truncate_after_inner(max_tick);
        update_cached_server_range(j.time_range());
    });
}

pub fn server_scrub_time_range() -> Option<(i64, i64)> {
    SERVER_JOURNAL.with(|cell| cell.borrow().time_range())
}

pub fn clear_server_scrub() {
    SERVER_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        j.clear_inner();
        update_cached_server_range(None);
    });
}