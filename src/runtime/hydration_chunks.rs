use super::super::{Dim, Game};
use super::snapshots::*;
use super::types::*;
use horizon_client::{
    chunk::Chunk,
    containers::BlockEntity,
    fast_hash::FxHashMap,
};
use horizon_core::{
    geometry::Direction,
    position::{BlockPos, ChunkPos},
    ticks::{ChunkTicks, SavedTick, ScheduledTick, TickPriority, TickTypeId},
};
use horizon_mod_runtime::attachments::Attachments;
use horizon_world::{
    level::{BlockEventData, BlockId, MovingPiston},
    nbt::Tag,
    registry::BlockStateId,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    sync::Arc,
};

fn u8_to_dir(v: u64) -> Direction {
    match v {
        0 => Direction::Down,
        1 => Direction::Up,
        2 => Direction::North,
        3 => Direction::South,
        4 => Direction::West,
        _ => Direction::East,
    }
}

fn decode_piston(v: &serde_json::Value) -> Option<MovingPiston> {
    Some(MovingPiston {
        moved_state: BlockStateId(v.get("moved_state")?.as_u64()? as u32),
        direction: u8_to_dir(v.get("direction")?.as_u64()?),
        extending: v.get("extending")?.as_bool()?,
        source: v.get("source")?.as_bool()?,
        progress: v.get("progress")?.as_f64()? as f32,
        progress_o: v.get("progress_o")?.as_f64()? as f32,
        last_ticked: v.get("last_ticked")?.as_i64()?,
        death_ticks: v.get("death_ticks")?.as_i64()? as i32,
        id: v.get("id")?.as_u64()?,
    })
}

/// Decodes live pending and queued ticks captured per chunk, preserving both intact.
fn decode_chunk_ticks_map(arr: &[serde_json::Value]) -> HashMap<(i32, i32), ChunkTicks> {
    let mut out = HashMap::new();
    for row in arr {
        let Some(pair) = row.as_array() else {
            continue;
        };
        if pair.len() != 4 {
            continue;
        }
        let (Some(cx), Some(cz), Some(pending_arr), Some(queued_arr)) = (
            pair[0].as_i64(),
            pair[1].as_i64(),
            pair[2].as_array(),
            pair[3].as_array(),
        ) else {
            continue;
        };
        let mut ct = if !pending_arr.is_empty() {
            let mut saved = Vec::with_capacity(pending_arr.len());
            for item in pending_arr {
                let Some(t) = item.as_array() else {
                    continue;
                };
                if t.len() == 6
                    && let (
                        Some(kind),
                        Some(x),
                        Some(y),
                        Some(z),
                        Some(delay),
                        Some(prio),
                    ) = (
                        t[0].as_u64(),
                        t[1].as_i64(),
                        t[2].as_i64(),
                        t[3].as_i64(),
                        t[4].as_i64(),
                        t[5].as_i64(),
                    )
                {
                    saved.push(SavedTick {
                        kind: TickTypeId(kind as u32),
                        pos: BlockPos::new(x as i32, y as i32, z as i32),
                        delay: delay as i32,
                        priority: TickPriority::by_value(prio as i32),
                    });
                }
            }
            ChunkTicks::from_saved(saved)
        } else {
            ChunkTicks::default()
        };
        for item in queued_arr {
            let Some(t) = item.as_array() else {
                continue;
            };
            if t.len() == 7
                && let (
                    Some(kind),
                    Some(x),
                    Some(y),
                    Some(z),
                    Some(trigger_tick),
                    Some(prio),
                    Some(sub_order),
                ) = (
                    t[0].as_u64(),
                    t[1].as_i64(),
                    t[2].as_i64(),
                    t[3].as_i64(),
                    t[4].as_i64(),
                    t[5].as_i64(),
                    t[6].as_i64(),
                )
            {
                let _ = ct.schedule(ScheduledTick {
                    kind: TickTypeId(kind as u32),
                    pos: BlockPos::new(x as i32, y as i32, z as i32),
                    trigger_tick,
                    priority: TickPriority::by_value(prio as i32),
                    sub_tick_order: sub_order,
                });
            }
        }
        out.insert((cx as i32, cz as i32), ct);
    }
    out
}

pub struct HydratedChunksResult {
    pub chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
    pub block_entities: HashMap<(i32, i32, i32), BlockEntity>,
    pub block_names: FxHashMap<(i32, i32, i32), Tag>,
    pub be_attachments: Vec<((i32, i32, i32), Attachments)>,
    pub world_attachments: Attachments,
    pub forced_chunks: HashSet<(i32, i32)>,
    pub inhabited: FxHashMap<(i32, i32), i64>,
    pub chunk_ticks: HashMap<(i32, i32), (ChunkTicks, ChunkTicks)>,
    pub chest_openers: Vec<((i32, i32, i32), (i32, f64))>,
    pub lid_openers: HashMap<(i32, i32, i32), i32>,
    pub lectern_pulses: Vec<((i32, i32, i32), i32)>,
    pub gateway_anims: Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)>,
    pub comparator_outputs: HashMap<BlockPos, i32>,
    pub torch_toggles: Vec<(BlockPos, i64)>,
    pub moving_pistons: Vec<(BlockPos, MovingPiston)>,
    pub piston_pushes: Vec<(BlockPos, MovingPiston, f32)>,
    pub block_events: VecDeque<BlockEventData>,
    pub hoppers: HashMap<(i32, i32, i32), super::super::machines::HopperState>,
    pub end_clock: i64,
    pub level_rng: Option<u64>,
    pub level_rng_gaussian: Option<f64>,
    pub random_tick_value: Option<i32>,
}

pub fn hydrate_chunks_from_disk(
    game: &mut Game<'_>,
    _cp_dir: &Path,
    cp_save: &horizon_client::save::SaveDir,
    dim: Dim,
    snap: &TimelineSnapshot,
    meta: Option<&serde_json::Value>,
) -> Option<HydratedChunksResult> {
    let mut candidate_set: HashSet<(i32, i32)> = HashSet::new();
    let mut disk_level_rng: Option<u64> = None;
    let mut disk_level_rng_gaussian: Option<f64> = None;
    let mut disk_random_tick_value: Option<i32> = None;
    let mut disk_end_clock: i64 = 0;
    let mut disk_comparator_outputs = HashMap::new();
    let mut disk_torch_toggles = Vec::new();
    let mut disk_moving_pistons = Vec::new();
    let mut disk_piston_pushes = Vec::new();
    let mut disk_block_events = VecDeque::new();
    let mut disk_chest_openers: Vec<((i32, i32, i32), (i32, f64))> = Vec::new();
    let mut disk_lid_openers: HashMap<(i32, i32, i32), i32> = HashMap::new();
    let mut disk_lectern_pulses: Vec<((i32, i32, i32), i32)> = Vec::new();
    let mut disk_gateway_anims: Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)> =
        Vec::new();
    let mut disk_hoppers: HashMap<(i32, i32, i32), super::super::machines::HopperState> =
        HashMap::new();
    let mut has_disk_block_ticks = false;
    let mut disk_block_ticks: HashMap<(i32, i32), ChunkTicks> = HashMap::new();
    let mut has_disk_fluid_ticks = false;
    let mut disk_fluid_ticks: HashMap<(i32, i32), ChunkTicks> = HashMap::new();
    let mut disk_inhabited: FxHashMap<(i32, i32), i64> = FxHashMap::default();
    let mut has_disk_inhabited = false;

    if let Some(meta) = meta {
        if let Some(arr) = meta["loaded_chunks"][dim.id()].as_array() {
            for item in arr {
                if let Some(pair) = item.as_array() {
                    if pair.len() == 2 {
                        if let (Some(cx), Some(cz)) = (pair[0].as_i64(), pair[1].as_i64()) {
                            candidate_set.insert((cx as i32, cz as i32));
                        }
                    }
                }
            }
        }
        disk_level_rng = meta["level_rng"][dim.id()].as_u64();
        disk_level_rng_gaussian = meta["level_rng_gaussian"][dim.id()]
            .as_u64()
            .map(f64::from_bits);
        disk_random_tick_value = meta["random_tick_values"][dim.id()]
            .as_i64()
            .map(|v| v as i32);
        disk_end_clock = meta["end_clocks"][dim.id()].as_i64().unwrap_or(0);
        let rs = &meta["level_redstone"][dim.id()];
        if let Some(arr) = rs["comparator_outputs"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 4
                    && let (Some(x), Some(y), Some(z), Some(out)) =
                        (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                {
                    disk_comparator_outputs.insert(
                        BlockPos::new(x as i32, y as i32, z as i32),
                        out as i32,
                    );
                }
            }
        }
        if let Some(arr) = rs["torch_toggles"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 4
                    && let (Some(x), Some(y), Some(z), Some(time)) =
                        (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                {
                    disk_torch_toggles.push((
                        BlockPos::new(x as i32, y as i32, z as i32),
                        time,
                    ));
                }
            }
        }
        if let Some(arr) = rs["moving_pistons"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 4
                    && let (Some(x), Some(y), Some(z), Some(piston)) = (
                        a[0].as_i64(),
                        a[1].as_i64(),
                        a[2].as_i64(),
                        decode_piston(&a[3]),
                    )
                {
                    disk_moving_pistons.push((
                        BlockPos::new(x as i32, y as i32, z as i32),
                        piston,
                    ));
                }
            }
        }
        if let Some(arr) = rs["piston_pushes"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 5
                    && let (Some(x), Some(y), Some(z), Some(piston), Some(next_p)) = (
                        a[0].as_i64(),
                        a[1].as_i64(),
                        a[2].as_i64(),
                        decode_piston(&a[3]),
                        a[4].as_f64(),
                    )
                {
                    disk_piston_pushes.push((
                        BlockPos::new(x as i32, y as i32, z as i32),
                        piston,
                        next_p as f32,
                    ));
                }
            }
        }
        if let Some(arr) = rs["block_events"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 6
                    && let (Some(x), Some(y), Some(z), Some(blk), Some(pa), Some(pb)) = (
                        a[0].as_i64(),
                        a[1].as_i64(),
                        a[2].as_i64(),
                        a[3].as_u64(),
                        a[4].as_i64(),
                        a[5].as_i64(),
                    )
                {
                    disk_block_events.push_back(BlockEventData {
                        pos: BlockPos::new(x as i32, y as i32, z as i32),
                        block: BlockId(blk as u16),
                        param_a: pa as i32,
                        param_b: pb as i32,
                        note_override: None,
                    });
                }
            }
        }
        if let Some(arr) = rs["chest_openers"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 5
                    && let (Some(x), Some(y), Some(z), Some(count), Some(range_bits)) = (
                        a[0].as_i64(),
                        a[1].as_i64(),
                        a[2].as_i64(),
                        a[3].as_i64(),
                        a[4].as_u64(),
                    )
                {
                    disk_chest_openers.push((
                        (x as i32, y as i32, z as i32),
                        (count as i32, f64::from_bits(range_bits)),
                    ));
                }
            }
        }
        if let Some(arr) = rs["lid_openers"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 4
                    && let (Some(x), Some(y), Some(z), Some(c)) =
                        (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                {
                    disk_lid_openers.insert((x as i32, y as i32, z as i32), c as i32);
                }
            }
        } else if dim == Dim::Overworld {
            disk_lid_openers = snap.lid_openers.clone();
        }
        if let Some(arr) = rs["lectern_pulses"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 4
                    && let (Some(x), Some(y), Some(z), Some(t)) =
                        (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                {
                    disk_lectern_pulses.push(((x as i32, y as i32, z as i32), t as i32));
                }
            }
        } else if dim == Dim::Overworld {
            disk_lectern_pulses = snap.lectern_pulses.clone();
        }
        if let Some(arr) = rs["gateway_anims"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 5
                    && let (Some(x), Some(y), Some(z), Some(age), Some(cd)) = (
                        a[0].as_i64(),
                        a[1].as_i64(),
                        a[2].as_i64(),
                        a[3].as_i64(),
                        a[4].as_i64(),
                    )
                {
                    disk_gateway_anims.push((
                        [x as i32, y as i32, z as i32],
                        horizon_client::block_entity_anim::EndGateway {
                            age,
                            teleport_cooldown: cd as i32,
                        },
                    ));
                }
            }
        }
        if let Some(arr) = rs["hoppers"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 4
                    && let (Some(x), Some(y), Some(z), Some(cd)) =
                        (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                {
                    let tag = Tag::compound([("TransferCooldown", Tag::Int(cd as i32))]);
                    disk_hoppers.insert(
                        (x as i32, y as i32, z as i32),
                        super::super::machines::HopperState::load(&tag),
                    );
                }
            }
        }
        if let Some(arr) = rs["chunk_block_ticks"].as_array() {
            has_disk_block_ticks = true;
            disk_block_ticks = decode_chunk_ticks_map(arr);
        }
        if let Some(arr) = rs["chunk_fluid_ticks"].as_array() {
            has_disk_fluid_ticks = true;
            disk_fluid_ticks = decode_chunk_ticks_map(arr);
        }
        if let Some(arr) = rs["inhabited"].as_array() {
            has_disk_inhabited = true;
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() >= 3
                    && let (Some(cx), Some(cz), Some(val)) =
                        (a[0].as_i64(), a[1].as_i64(), a[2].as_i64())
                {
                    disk_inhabited.insert((cx as i32, cz as i32), val);
                }
            }
        }
    }

    if game.dim == dim {
        candidate_set.extend(game.world.chunks.keys().copied());
    } else if let Some(parked) = game.levels.parked.get(&dim) {
        candidate_set.extend(parked.world.chunks.keys().copied());
    }

    for g in snap.guests.values() {
        if Dim::from_id(&g.dimension) == dim {
            let gx = (g.position[0].floor() as i32) >> 4;
            let gz = (g.position[2].floor() as i32) >> 4;
            for dz in -3..=3 {
                for dx in -3..=3 {
                    candidate_set.insert((gx + dx, gz + dz));
                }
            }
        }
    }

    let mut candidate_chunks: Vec<(i32, i32)> = candidate_set.into_iter().collect();
    candidate_chunks.sort_unstable();

    let mut chunks = FxHashMap::default();
    let mut block_entities = HashMap::new();
    let mut block_names: FxHashMap<(i32, i32, i32), Tag> = FxHashMap::default();
    let mut be_attachments = Vec::new();
    let mut inhabited = FxHashMap::default();
    let mut chunk_ticks = HashMap::new();

    for &(cx, cz) in &candidate_chunks {
        let Some(mut disk_chunk) =
            cp_save.load(cx, cz, game.pack.assets.registry, game.res.biomes())
        else {
            continue;
        };
        for &(pos, ref be) in &disk_chunk.block_entities {
            block_entities.insert(pos, be.clone());
        }
        for &(pos, ref name) in &disk_chunk.block_names {
            block_names.insert(pos, name.clone());
        }
        if let Some(saved) = disk_chunk.saved.as_deref() {
            if !has_disk_inhabited
                && let Some(Tag::Long(t)) = saved.root.get(&"InhabitedTime".into())
            {
                inhabited.insert((cx, cz), *t);
            }
            be_attachments.extend(saved.block_entity_attachments.iter().cloned());
        }
        if has_disk_inhabited
            && let Some(&val) = disk_inhabited.get(&(cx, cz))
        {
            inhabited.insert((cx, cz), val);
        }
        let b_ticks = if has_disk_block_ticks {
            disk_block_ticks.remove(&(cx, cz)).unwrap_or_default()
        } else if let Some(saved) = disk_chunk.saved.as_deref() {
            let mut bt = ChunkTicks::from_saved(saved.block_ticks.clone());
            bt.unpack(snap.game_time);
            bt
        } else {
            ChunkTicks::default()
        };
        let f_ticks = if has_disk_fluid_ticks {
            disk_fluid_ticks.remove(&(cx, cz)).unwrap_or_default()
        } else if let Some(saved) = disk_chunk.saved.as_deref() {
            let mut ft = ChunkTicks::from_saved(saved.fluid_ticks.clone());
            ft.unpack(snap.game_time);
            ft
        } else {
            ChunkTicks::default()
        };
        chunk_ticks.insert((cx, cz), (b_ticks, f_ticks));

        let live_chunk_opt = if game.dim == dim {
            game.world.chunks.get(&(cx, cz))
        } else {
            game.levels
                .parked
                .get(&dim)
                .and_then(|p| p.world.chunks.get(&(cx, cz)))
        };

        if let Some(live_chunk) = live_chunk_opt {
            for (s_idx, disk_sec) in disk_chunk.sections.iter_mut().enumerate() {
                if let Some(live_sec) = live_chunk.sections.get(s_idx)
                    && live_sec.blocks == disk_sec.blocks
                    && live_sec.biomes == disk_sec.biomes
                {
                    *disk_sec = Arc::clone(live_sec);
                }
            }
        }
        chunks.insert((cx, cz), Arc::new(disk_chunk));
    }

    if chunks.is_empty() {
        return None;
    }

    let forced_chunks = cp_save.load_forced_chunks();
    let world_attachments = cp_save.load_attachments();

    Some(HydratedChunksResult {
        chunks,
        block_entities,
        block_names,
        be_attachments,
        world_attachments,
        forced_chunks,
        inhabited,
        chunk_ticks,
        chest_openers: disk_chest_openers,
        lid_openers: disk_lid_openers,
        lectern_pulses: disk_lectern_pulses,
        gateway_anims: disk_gateway_anims,
        comparator_outputs: disk_comparator_outputs,
        torch_toggles: disk_torch_toggles,
        moving_pistons: disk_moving_pistons,
        piston_pushes: disk_piston_pushes,
        block_events: disk_block_events,
        hoppers: disk_hoppers,
        end_clock: disk_end_clock,
        level_rng: disk_level_rng,
        level_rng_gaussian: disk_level_rng_gaussian,
        random_tick_value: disk_random_tick_value,
    })
}
