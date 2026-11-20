use super::super::{Dim, Game};
use super::capture::{
    capture_chunk_ticks, capture_guests_snapshot, capture_level_snapshot_in_memory,
    capture_parked_snapshot_in_memory, snapshot_stopwatches_elapsed,
};
use super::snapshots::*;
use super::scrub_perf::*;
use super::types::*;
use horizon_client::{
    chunk::Chunk,
    command_storage::CommandStorage,
    containers::BlockEntity,
    fast_hash::FxHashMap,
    gamerules::GameRules,
    raid::Raids,
    scoreboard::Scoreboard,
    sculk::WardenSpawnTracker,
    world::{HealthRules, Weather},
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::Vec3,
    nbt::Tag,
    position::BlockPos,
    random::LevelRandom,
    ticks::ChunkTicks,
};
use horizon_mod_runtime::attachments::Attachments;
use horizon_world::level::{BlockEventData, MovingPiston};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub const MAX_SERVER_SCRUB_TICKS: usize = 24_000;
pub const SERVER_SEGMENT_TICKS: usize = 100;

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
    pub history: FxHashMap<(Dim, i32, i32), Vec<ChunkHistoryEntry>>,
    pub unloaded_set: HashSet<(Dim, i32, i32)>,
    pub region_first_seen: FxHashMap<PathBuf, i64>,
    pub regions_initialized: bool,
}

impl UnloadedChunkJournal {
    pub fn clear(&mut self) {
        self.baselines.clear();
        self.last_seen_chunks.clear();
        self.history.clear();
        self.unloaded_set.clear();
        self.region_first_seen.clear();
        self.regions_initialized = false;
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
        for (&pos, be) in world.block_entities.iter() {
            if (pos.0 >> 4) == cx && (pos.2 >> 4) == cz {
                bes.push((pos, be.clone()));
            }
        }
        let mut names = Vec::new();
        for (&pos, tag) in world.block_names.iter() {
            if (pos.0 >> 4) == cx && (pos.2 >> 4) == cz {
                names.push((pos, tag.clone()));
            }
        }
        (bes, names)
    }

    pub fn observe_game(&mut self, game: &Game<'_>, tick: i64) {
        if !self.regions_initialized {
            self.scan_regions(&game.world_root, tick);
        }
        for (&(cx, cz), chunk) in game.world.chunks.iter() {
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
                let (block_entities, block_names) = Self::collect_chunk_bes(&game.world, cx, cz);
                let entities = capture_chunk_entities_active(game, cx, cz);
                self.history.entry(key).or_default().push(ChunkHistoryEntry {
                    tick,
                    chunk: Arc::clone(chunk),
                    block_entities,
                    block_names,
                    entities,
                });
                self.last_seen_chunks.insert(key, Arc::clone(chunk));
            }
        }
        for (&dim, parked) in &game.levels.parked {
            for (&(cx, cz), chunk) in parked.world.chunks.iter() {
                let key = (dim, cx, cz);
                self.unloaded_set.remove(&key);
                if !self.baselines.contains_key(&key) {
                    let (block_entities, block_names) =
                        Self::collect_chunk_bes(&parked.world, cx, cz);
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
                    let (block_entities, block_names) =
                        Self::collect_chunk_bes(&parked.world, cx, cz);
                    let entities = capture_chunk_entities_parked(game, parked, cx, cz);
                    self.history.entry(key).or_default().push(ChunkHistoryEntry {
                        tick,
                        chunk: Arc::clone(chunk),
                        block_entities,
                        block_names,
                        entities,
                    });
                    self.last_seen_chunks.insert(key, Arc::clone(chunk));
                }
            }
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
        self.unloaded_set.insert(key);
        if let Some(last_chunk) = self.last_seen_chunks.get(&key).cloned() {
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
            if hist.last().is_none_or(|e| e.tick < tick) {
                hist.push(ChunkHistoryEntry {
                    tick,
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

    pub fn truncate_after(&mut self, max_tick: i64) {
        for hist in self.history.values_mut() {
            hist.retain(|e| e.tick <= max_tick);
        }
        self.history.retain(|_, v| !v.is_empty());
        self.region_first_seen.retain(|_, &mut t| t <= max_tick);
    }
}

#[derive(Clone)]
struct CowLevelRecord {
    chunk_changes: Vec<((i32, i32), Option<Arc<Chunk>>)>,
    block_entities: Arc<HashMap<(i32, i32, i32), BlockEntity>>,
    block_names: Arc<FxHashMap<(i32, i32, i32), Tag>>,
    be_attachments: Arc<Vec<((i32, i32, i32), Attachments)>>,
    world_attachments: Arc<Attachments>,
    chest_openers: Arc<Vec<((i32, i32, i32), (i32, f64))>>,
    border: WorldBorder,
    shulker_anims: Arc<Vec<([i32; 3], horizon_client::block_entity_anim::Shulker)>>,
    gateway_anims: Arc<Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)>>,
    fangs: Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>>,
    ominous_spawners: Arc<Vec<(u32, Tag)>>,
    portals_entities: Arc<HashMap<u32, horizon_client::nether_portal::PortalState>>,
    lectern_pulses: Arc<Vec<((i32, i32, i32), i32)>>,
    lid_openers: Arc<HashMap<(i32, i32, i32), i32>>,
    inhabited: Arc<FxHashMap<(i32, i32), i64>>,
    end_clock: i64,
    random: LevelRandom,
    forced_chunks: Arc<HashSet<(i32, i32)>>,
    chunk_ticks: Arc<HashMap<(i32, i32), (ChunkTicks, ChunkTicks)>>,
    random_tick_value: i32,
    comparator_outputs: Arc<HashMap<BlockPos, i32>>,
    torch_toggles: Arc<Vec<(BlockPos, i64)>>,
    moving_pistons: Arc<Vec<(BlockPos, MovingPiston)>>,
    piston_pushes: Arc<Vec<(BlockPos, MovingPiston, f32)>>,
    block_events: Arc<VecDeque<BlockEventData>>,
    hoppers: Arc<HashMap<(i32, i32, i32), super::super::machines::HopperState>>,
    mobs: Arc<horizon_client::join_seq::JoinList<super::super::Mob>>,
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
    persist_uuids: Arc<HashMap<u32, [i32; 4]>>,
    persist_raw: Arc<HashMap<u32, Tag>>,
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
    border: WorldBorder,
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
    guests: Arc<HashMap<u128, GuestSnapshot>>,
    levels: Vec<(Dim, CowLevelRecord)>,
    mob_players: Arc<FxHashMap<u32, u32>>,
    client_using: Option<(usize, u32)>,
    client_use_duration: i32,
    client_crossbow_sounds: [bool; 2],
}

struct ServerSegment {
    start_tick: i64,
    end_tick: i64,
    header_chunks: HashMap<Dim, FxHashMap<(i32, i32), Arc<Chunk>>>,
    ticks: Vec<ServerTickRecord>,
}

#[derive(Default)]
struct CowLevelPrev {
    chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
    world_revision: u64,
    be_len: usize,
    block_entities: Option<Arc<HashMap<(i32, i32, i32), BlockEntity>>>,
    block_names: Option<Arc<FxHashMap<(i32, i32, i32), Tag>>>,
    be_attachments: Option<Arc<Vec<((i32, i32, i32), Attachments)>>>,
    world_attachments: Option<Arc<Attachments>>,
    inhabited: Option<Arc<FxHashMap<(i32, i32), i64>>>,
    forced_chunks: Option<Arc<HashSet<(i32, i32)>>>,
    mobs_stamp: Option<horizon_client::join_seq::ListStamp>,
    mobs: Option<Arc<horizon_client::join_seq::JoinList<super::super::Mob>>>,
    dropped_stamp: Option<horizon_client::join_seq::ListStamp>,
    dropped: Option<Arc<horizon_client::join_seq::JoinList<super::super::ItemEntity>>>,
    orbs_stamp: Option<horizon_client::join_seq::ListStamp>,
    orbs: Option<Arc<horizon_client::join_seq::JoinList<super::super::ExperienceOrb>>>,
    tnts_stamp: Option<horizon_client::join_seq::ListStamp>,
    tnts: Option<Arc<horizon_client::join_seq::JoinList<horizon_gameplay::tnt::PrimedTnt>>>,
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
    persist_uuids: Option<Arc<HashMap<u32, [i32; 4]>>>,
    persist_raw: Option<Arc<HashMap<u32, Tag>>>,
    raids_mem: Option<Arc<Raids>>,
}

#[derive(Default)]
struct CowGlobalPrev {
    anchor_ids: Option<Arc<HashSet<u128>>>,
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
    levels: HashMap<Dim, CowLevelPrev>,
}

fn empty_proj() -> Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>> {
    static E: OnceLock<Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_boats() -> Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>> {
    static E: OnceLock<Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_carts() -> Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>> {
    static E: OnceLock<Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_falling() -> Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>> {
    static E: OnceLock<Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_others() -> Arc<horizon_client::join_seq::ResidentList<super::super::commands::OtherEntity>> {
    static E: OnceLock<Arc<horizon_client::join_seq::ResidentList<super::super::commands::OtherEntity>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_fangs() -> Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>> {
    static E: OnceLock<Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>>> = OnceLock::new();
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

fn has_ticking_be(world: &horizon_client::world::World) -> bool {
    world.block_entities.values().any(|be| {
        matches!(
            be,
            BlockEntity::Furnace(_)
                | BlockEntity::Brewing(_)
                | BlockEntity::Spawner(_)
                | BlockEntity::TrialSpawner(_)
                | BlockEntity::Jukebox(_)
                | BlockEntity::Beacon(_)
                | BlockEntity::Sculk(_)
                | BlockEntity::Campfire(_)
                | BlockEntity::Crafter(_)
                | BlockEntity::CreakingHeart(_)
                | BlockEntity::EndGateway(_)
                | BlockEntity::Vault(_)
                | BlockEntity::Beehive(_)
                | BlockEntity::Brushable(_)
                | BlockEntity::Conduit(_)
                | BlockEntity::PotentSulfur(_)
        )
    })
}

fn diff_chunk_maps(
    prev: &FxHashMap<(i32, i32), Arc<Chunk>>,
    curr: &FxHashMap<(i32, i32), Arc<Chunk>>,
) -> Vec<((i32, i32), Option<Arc<Chunk>>)> {
    let mut changes = Vec::new();
    for (&pos, chunk) in curr.iter() {
        let same = prev.get(&pos).is_some_and(|p| Arc::ptr_eq(p, chunk));
        if !same {
            changes.push((pos, Some(Arc::clone(chunk))));
        }
    }
    for &pos in prev.keys() {
        if !curr.contains_key(&pos) {
            changes.push((pos, None));
        }
    }
    changes
}

fn capture_cow_level_direct(
    game: &Game<'_>,
    _dim: Dim,
    world: &horizon_client::world::World,
    prev: &mut CowLevelPrev,
    any_menu_open: bool,
    force_full: bool,
) -> CowLevelRecord {
    let live_chunks = &*world.chunks;
    let chunk_changes = if force_full {
        Vec::new()
    } else {
        diff_chunk_maps(&prev.chunks, live_chunks)
    };
    if force_full {
        prev.chunks = live_chunks.clone();
    } else {
        for &(pos, ref opt) in &chunk_changes {
            match opt {
                Some(c) => {
                    prev.chunks.insert(pos, Arc::clone(c));
                }
                None => {
                    prev.chunks.remove(&pos);
                }
            }
        }
    }

    let be_dirty = force_full
        || prev.block_entities.is_none()
        || prev.world_revision != world.revision
        || prev.be_len != world.block_entities.len()
        || any_menu_open
        || !game.hoppers.is_empty()
        || has_ticking_be(world);
    let block_entities = if be_dirty {
        let arc = Arc::new((&*world.block_entities).clone());
        prev.block_entities = Some(Arc::clone(&arc));
        prev.be_len = world.block_entities.len();
        arc
    } else {
        Arc::clone(prev.block_entities.as_ref().unwrap())
    };

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
        && prev.world_revision == world.revision
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

    let inhabited = if !force_full
        && let Some(p) = &prev.inhabited
        && **p == world.inhabited
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.inhabited.clone());
        prev.inhabited = Some(Arc::clone(&arc));
        arc
    };

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

    macro_rules! cow_active_entities {
        ($field:ident, $live:expr) => {{
            if $live.is_empty() {
                if let Some(p) = &prev.$field && p.is_empty() {
                    Arc::clone(p)
                } else {
                    let arc = Arc::new(Default::default());
                    prev.$field = Some(Arc::clone(&arc));
                    arc
                }
            } else {
                Arc::new($live.clone())
            }
        }};
    }

    let mobs = cow_active_entities!(mobs, game.mobs);
    let dropped = cow_active_entities!(dropped, game.dropped);
    let orbs = cow_active_entities!(orbs, game.orbs);
    let tnts = cow_active_entities!(tnts, game.tnts);
    let paintings = cow_stamped!(paintings, paintings_stamp, game.paintings);
    let item_frames = cow_stamped!(item_frames, frames_stamp, game.item_frames);
    let armor_stands = cow_stamped!(armor_stands, stands_stamp, game.armor_stands);
    let cushions = cow_stamped!(cushions, cushions_stamp, game.cushions);
    let crystals = cow_stamped!(crystals, crystals_stamp, game.crystals);

    let projectiles = if game.creatures.projectiles.is_empty() {
        empty_proj()
    } else {
        Arc::new(game.creatures.projectiles.clone())
    };
    let boats = if game.boats.is_empty() {
        empty_boats()
    } else {
        Arc::new(game.boats.clone())
    };
    let carts = if game.carts.is_empty() {
        empty_carts()
    } else {
        Arc::new(game.carts.clone())
    };
    let falling = if game.falling.is_empty() {
        empty_falling()
    } else {
        Arc::new(game.falling.clone())
    };
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

    let chunk_ticks = Arc::new(capture_chunk_ticks(live_chunks, &game.block_ticks));

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

    let persist_uuids = if !force_full
        && let Some(p) = &prev.persist_uuids
        && **p == *game.persist.uuids
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new((*game.persist.uuids).clone());
        prev.persist_uuids = Some(Arc::clone(&arc));
        arc
    };

    let persist_raw = if !force_full
        && let Some(p) = &prev.persist_raw
        && **p == game.persist.raw
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(game.persist.raw.clone());
        prev.persist_raw = Some(Arc::clone(&arc));
        arc
    };

    let raids_mem = if game.raiding.raids.map.is_empty() {
        Some(empty_raids())
    } else {
        let arc = Arc::new(game.raiding.raids.clone());
        prev.raids_mem = Some(Arc::clone(&arc));
        Some(arc)
    };

    CowLevelRecord {
        chunk_changes,
        block_entities,
        block_names,
        be_attachments,
        world_attachments,
        chest_openers: Arc::new(world.chest_openers.iter().map(|(&k, &v)| (k, v)).collect()),
        border: game.border.clone(),
        shulker_anims: Arc::new(game.be_anims.shulkers.iter().map(|(&k, v)| (k, v.clone())).collect()),
        gateway_anims: Arc::new(game.be_anims.gateway_servers.iter().map(|(&k, &v)| (k, v)).collect()),
        fangs,
        ominous_spawners,
        portals_entities: Arc::new(game.portals.entities.clone()),
        lectern_pulses: Arc::new(game.lectern_pulses.clone()),
        lid_openers: Arc::new(game.lid_openers.clone()),
        inhabited,
        end_clock: world.end_clock,
        random: world.random.clone(),
        forced_chunks,
        chunk_ticks,
        random_tick_value: game.block_ticks.random_tick_value,
        comparator_outputs,
        torch_toggles,
        moving_pistons,
        piston_pushes,
        block_events,
        hoppers,
        mobs,
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
        dragon: game.dragon.as_ref().map(DragonSnapshot::from_live).map(Arc::new),
        tridents,
        rockets,
        breath_clouds,
        thrown_potions,
        knot_ids,
        persist_uuids,
        persist_raw,
        persist_passenger_slots: Arc::new(game.persist.passenger_slots.clone()),
        raids_mem,
        last_hurt_by_player: Arc::new(game.last_hurt_by_player.clone()),
        love_cause: Arc::new(game.love_cause.clone()),
    }
}

fn capture_cow_parked_direct(
    game: &Game<'_>,
    _dim: Dim,
    parked: &crate::game::levels::Parked,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> CowLevelRecord {
    let world = &parked.world;
    let live_chunks = &*world.chunks;
    let chunk_changes = if force_full {
        Vec::new()
    } else {
        diff_chunk_maps(&prev.chunks, live_chunks)
    };
    if force_full {
        prev.chunks = live_chunks.clone();
    } else {
        for &(pos, ref opt) in &chunk_changes {
            match opt {
                Some(c) => {
                    prev.chunks.insert(pos, Arc::clone(c));
                }
                None => {
                    prev.chunks.remove(&pos);
                }
            }
        }
    }

    let be_dirty = force_full
        || prev.block_entities.is_none()
        || prev.world_revision != world.revision
        || prev.be_len != world.block_entities.len()
        || !parked.hoppers.is_empty()
        || has_ticking_be(world);
    let block_entities = if be_dirty {
        let arc = Arc::new((&*world.block_entities).clone());
        prev.block_entities = Some(Arc::clone(&arc));
        prev.be_len = world.block_entities.len();
        arc
    } else {
        Arc::clone(prev.block_entities.as_ref().unwrap())
    };

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
        && prev.world_revision == world.revision
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

    let inhabited = if !force_full
        && let Some(p) = &prev.inhabited
        && **p == world.inhabited
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.inhabited.clone());
        prev.inhabited = Some(Arc::clone(&arc));
        arc
    };

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

    macro_rules! cow_active_entities {
        ($field:ident, $live:expr) => {{
            if $live.is_empty() {
                if let Some(p) = &prev.$field && p.is_empty() {
                    Arc::clone(p)
                } else {
                    let arc = Arc::new(Default::default());
                    prev.$field = Some(Arc::clone(&arc));
                    arc
                }
            } else {
                Arc::new($live.clone())
            }
        }};
    }

    let mobs = cow_active_entities!(mobs, parked.mobs);
    let dropped = cow_active_entities!(dropped, parked.dropped);
    let orbs = cow_active_entities!(orbs, parked.orbs);
    let tnts = cow_active_entities!(tnts, parked.tnts);
    let paintings = cow_stamped!(paintings, paintings_stamp, parked.paintings);
    let item_frames = cow_stamped!(item_frames, frames_stamp, parked.item_frames);
    let armor_stands = cow_stamped!(armor_stands, stands_stamp, parked.armor_stands);
    let cushions = cow_stamped!(cushions, cushions_stamp, parked.cushions);
    let crystals = cow_stamped!(crystals, crystals_stamp, parked.crystals);

    let projectiles = if parked.projectiles.is_empty() {
        empty_proj()
    } else {
        Arc::new(parked.projectiles.clone())
    };
    let boats = if parked.boats.is_empty() {
        empty_boats()
    } else {
        Arc::new(parked.boats.clone())
    };
    let carts = if parked.carts.is_empty() {
        empty_carts()
    } else {
        Arc::new(parked.carts.clone())
    };
    let falling = if parked.falling.is_empty() {
        empty_falling()
    } else {
        Arc::new(parked.falling.clone())
    };
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

    let chunk_ticks = Arc::new(capture_chunk_ticks(live_chunks, &parked.block_ticks));

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

    let persist_uuids = if !force_full
        && let Some(p) = &prev.persist_uuids
        && **p == *parked.persist.uuids
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new((*parked.persist.uuids).clone());
        prev.persist_uuids = Some(Arc::clone(&arc));
        arc
    };

    let persist_raw = if !force_full
        && let Some(p) = &prev.persist_raw
        && **p == parked.persist.raw
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(parked.persist.raw.clone());
        prev.persist_raw = Some(Arc::clone(&arc));
        arc
    };

    let raids_mem = if parked.raiding.raids.map.is_empty() {
        Some(empty_raids())
    } else {
        let arc = Arc::new(parked.raiding.raids.clone());
        prev.raids_mem = Some(Arc::clone(&arc));
        Some(arc)
    };

    CowLevelRecord {
        chunk_changes,
        block_entities,
        block_names,
        be_attachments,
        world_attachments,
        chest_openers: Arc::new(world.chest_openers.iter().map(|(&k, &v)| (k, v)).collect()),
        border: parked.border.clone(),
        shulker_anims: Arc::new(parked.anims.shulkers.iter().map(|(&k, v)| (k, v.clone())).collect()),
        gateway_anims: Arc::new(parked.anims.gateway_servers.iter().map(|(&k, &v)| (k, v)).collect()),
        fangs,
        ominous_spawners,
        portals_entities: Arc::new(parked.portal_states.clone()),
        lectern_pulses: Arc::new(parked.lectern_pulses.clone()),
        lid_openers: Arc::new(parked.lid_openers.clone()),
        inhabited,
        end_clock: world.end_clock,
        random: world.random.clone(),
        forced_chunks,
        chunk_ticks,
        random_tick_value: parked.block_ticks.random_tick_value,
        comparator_outputs,
        torch_toggles,
        moving_pistons,
        piston_pushes,
        block_events,
        hoppers,
        mobs,
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
        dragon: parked.dragon.as_ref().map(DragonSnapshot::from_live).map(Arc::new),
        tridents,
        rockets,
        breath_clouds,
        thrown_potions,
        knot_ids,
        persist_uuids,
        persist_raw,
        persist_passenger_slots: Arc::new(parked.persist.passenger_slots.clone()),
        raids_mem,
        last_hurt_by_player: Arc::new(HashMap::new()),
        love_cause: Arc::new(HashMap::new()),
    }
}

pub struct ServerScrubJournal {
    segments: VecDeque<ServerSegment>,
    total_ticks: usize,
    max_ticks: usize,
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
            last_recorded_tick: None,
            cow_prev: CowGlobalPrev::default(),
            unloaded: UnloadedChunkJournal::default(),
        }
    }
}

impl ServerScrubJournal {
    fn record_tick_inner(&mut self, game: &Game<'_>) {
        let t_start = std::time::Instant::now();
        let now = game.world.game_time;
        if self.last_recorded_tick == Some(now) {
            return;
        }
        if let Some(last) = self.last_recorded_tick
            && now < last
        {
            self.truncate_after_inner(now - 1);
        }

        let t0 = std::time::Instant::now();
        self.unloaded.observe_game(game, now);
        let t_unloaded = t0.elapsed();

        let start_new_segment = self
            .segments
            .back()
            .is_none_or(|seg| seg.ticks.len() >= SERVER_SEGMENT_TICKS);

        let any_menu_open = game.container.is_some()
            || game
                .lan
                .as_ref()
                .is_some_and(|h| h.hub.guests.iter().any(|g| g.menu.is_some()));

        let t_globals_start = std::time::Instant::now();
        let (guests_map, online) = capture_guests_snapshot(game);
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

        let t_globals = t_globals_start.elapsed();

        let mut header_chunks = HashMap::new();
        let mut cow_levels = Vec::with_capacity(1 + game.levels.parked.len());

        let t_cow_0 = std::time::Instant::now();
        if start_new_segment {
            header_chunks.insert(game.dim, (&*game.world.chunks).clone());
        }
        let active_prev = self.cow_prev.levels.entry(game.dim).or_default();
        let active_cow = capture_cow_level_direct(
            game,
            game.dim,
            &game.world,
            active_prev,
            any_menu_open,
            start_new_segment,
        );
        cow_levels.push((game.dim, active_cow));

        for (&dim, parked) in &game.levels.parked {
            if start_new_segment {
                header_chunks.insert(dim, (&*parked.world.chunks).clone());
            }
            let p_prev = self.cow_prev.levels.entry(dim).or_default();
            let p_cow = capture_cow_parked_direct(
                game,
                dim,
                parked,
                p_prev,
                start_new_segment,
            );
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

        let record = ServerTickRecord {
            anchor_ids,
            policy,
            game_time: now,
            day_time: game.world.day_time,
            weather: game.world.weather,
            health_rules: game.world.health_rules,
            rules: Arc::new(game.ui.rules.clone()),
            border: game.border.clone(),
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
            command_clocks: Arc::new(game.ui.command_clocks.clone()),
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
            portals_entities: Arc::new(game.portals.entities.clone()),
            end_gateway_cooldowns,
            lectern_pulses: Arc::new(game.lectern_pulses.clone()),
            player_beds,
            sleep_seen: game.sleep_seen,
            lead_killers,
            lid_openers: Arc::new(game.lid_openers.clone()),
            entity_attachments: Arc::new(game.entity_attachments.clone()),
            player_attachments: Arc::new(game.player_state.attachments.clone()),
            guest_attachments: Arc::new(
                game.lan
                    .as_ref()
                    .map(|host| {
                        host.hub
                            .guests
                            .iter()
                            .map(|g| (g.uuid, (*g.attachments).clone()))
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            scheduled_events: Arc::new(game.fns.schedule.clone()),
            scoreboard,
            command_storage,
            stopwatches_elapsed,
            cmd_post_effects,
            guests: Arc::new(guests_map),
            levels: cow_levels,
            mob_players,
            client_using: game.using,
            client_use_duration: game.use_duration,
            client_crossbow_sounds: game.crossbow_sounds,
        };

        if start_new_segment {
            let mut ticks = Vec::with_capacity(SERVER_SEGMENT_TICKS);
            ticks.push(record);
            self.segments.push_back(ServerSegment {
                start_tick: now,
                end_tick: now,
                header_chunks,
                ticks,
            });
        } else if let Some(seg) = self.segments.back_mut() {
            seg.end_tick = now;
            seg.ticks.push(record);
        }

        self.total_ticks += 1;
        while self.total_ticks > self.max_ticks && self.segments.len() > 1 {
            if let Some(evicted) = self.segments.pop_front() {
                self.total_ticks = self.total_ticks.saturating_sub(evicted.ticks.len());
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
        let seg = &self.segments[seg_idx];
        let tick_idx = match seg.ticks.binary_search_by_key(&target_tick, |r| r.game_time) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => (i - 1).min(seg.ticks.len() - 1),
        };

        let mut reconstructed_chunks: HashMap<Dim, FxHashMap<(i32, i32), Arc<Chunk>>> =
            seg.header_chunks.clone();
        for rec in &seg.ticks[..=tick_idx] {
            for (dim, lrec) in &rec.levels {
                let map = reconstructed_chunks.entry(*dim).or_default();
                for (pos, opt_chunk) in &lrec.chunk_changes {
                    match opt_chunk {
                        Some(c) => {
                            map.insert(*pos, Arc::clone(c));
                        }
                        None => {
                            map.remove(pos);
                        }
                    }
                }
            }
        }

        let rec = &seg.ticks[tick_idx];
        let mut levels = HashMap::with_capacity(rec.levels.len());
        for (dim, lrec) in &rec.levels {
            let chunks = reconstructed_chunks.remove(dim).unwrap_or_default();
            levels.insert(
                *dim,
                LevelSnapshot {
                    chunks,
                    block_entities: (*lrec.block_entities).clone(),
                    block_names: (*lrec.block_names).clone(),
                    be_attachments: (*lrec.be_attachments).clone(),
                    world_attachments: (*lrec.world_attachments).clone(),
                    chest_openers: (*lrec.chest_openers).clone(),
                    border: lrec.border.clone(),
                    shulker_anims: (*lrec.shulker_anims).clone(),
                    gateway_anims: (*lrec.gateway_anims).clone(),
                    fangs: (*lrec.fangs).clone(),
                    ominous_spawners: (*lrec.ominous_spawners).clone(),
                    portals_entities: (*lrec.portals_entities).clone(),
                    lectern_pulses: (*lrec.lectern_pulses).clone(),
                    lid_openers: (*lrec.lid_openers).clone(),
                    inhabited: (*lrec.inhabited).clone(),
                    end_clock: lrec.end_clock,
                    random: lrec.random.clone(),
                    forced_chunks: (*lrec.forced_chunks).clone(),
                    chunk_ticks: (*lrec.chunk_ticks).clone(),
                    random_tick_value: lrec.random_tick_value,
                    comparator_outputs: (*lrec.comparator_outputs).clone(),
                    torch_toggles: (*lrec.torch_toggles).clone(),
                    moving_pistons: (*lrec.moving_pistons).clone(),
                    piston_pushes: (*lrec.piston_pushes).clone(),
                    block_events: (*lrec.block_events).clone(),
                    hoppers: (*lrec.hoppers).clone(),
                    mobs: (*lrec.mobs).clone(),
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
                    persist_uuids: (*lrec.persist_uuids).clone(),
                    persist_raw: (*lrec.persist_raw).clone(),
                    persist_passenger_slots: (*lrec.persist_passenger_slots).clone(),
                    raids_dat: None,
                    raids_mem: lrec.raids_mem.as_deref().cloned(),
                    last_hurt_by_player: (*lrec.last_hurt_by_player).clone(),
                    love_cause: (*lrec.love_cause).clone(),
                },
            );
        }

        Some(Arc::new(TimelineSnapshot {
            anchor_ids: (*rec.anchor_ids).clone(),
            policy: rec.policy,
            game_time: rec.game_time,
            day_time: rec.day_time,
            weather: rec.weather,
            health_rules: rec.health_rules,
            rules: (*rec.rules).clone(),
            border: rec.border.clone(),
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
            guests: (*rec.guests).clone(),
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
        while let Some(back) = self.segments.back_mut() {
            if back.start_tick > max_tick {
                let removed = self.segments.pop_back().unwrap();
                self.total_ticks = self.total_ticks.saturating_sub(removed.ticks.len());
            } else {
                let before = back.ticks.len();
                back.ticks.retain(|r| r.game_time <= max_tick);
                self.total_ticks = self.total_ticks.saturating_sub(before - back.ticks.len());
                if back.ticks.is_empty() {
                    self.segments.pop_back();
                } else {
                    back.end_tick = back.ticks.last().unwrap().game_time;
                }
                break;
            }
        }
        self.unloaded.truncate_after(max_tick);
        self.cow_prev = CowGlobalPrev::default();
        self.last_recorded_tick = self.segments.back().map(|s| s.end_tick);
    }

    fn time_range(&self) -> Option<(i64, i64)> {
        let first = self.segments.front()?.start_tick;
        let last = self.segments.back()?.end_tick;
        Some((first, last))
    }

    fn clear_inner(&mut self) {
        self.segments.clear();
        self.total_ticks = 0;
        self.last_recorded_tick = None;
        self.cow_prev = CowGlobalPrev::default();
        self.unloaded.clear();
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