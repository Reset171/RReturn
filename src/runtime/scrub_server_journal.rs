use super::super::{Dim, Game};
use super::snapshots::*;
use super::types::*;
use horizon_client::{
    chunk::Chunk,
    containers::BlockEntity,
    fast_hash::FxHashMap,
};
use horizon_core::{
    geometry::Vec3,
    nbt::Tag,
    position::ChunkPos,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn in_chunk(pos: Vec3, cx: i32, cz: i32) -> bool {
    (pos.x.floor() as i32 >> 4) == cx && (pos.z.floor() as i32 >> 4) == cz
}

pub fn in_chunk_arr(pos: [f64; 3], cx: i32, cz: i32) -> bool {
    (pos[0].floor() as i32 >> 4) == cx && (pos[2].floor() as i32 >> 4) == cz
}

pub fn in_chunk_block(pos: [i32; 3], cx: i32, cz: i32) -> bool {
    (pos[0] >> 4) == cx && (pos[2] >> 4) == cz
}

pub fn capture_chunk_entities_active(game: &Game<'_>, cx: i32, cz: i32) -> UnloadedChunkEntities {
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

pub fn capture_chunk_entities_parked(
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
