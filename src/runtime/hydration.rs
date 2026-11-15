use super::super::{Dim, Game};
use super::load_disk::load_slot_from_disk;
use super::snapshots::*;
use super::storage::*;
use super::types::*;
use horizon_client::{
    fast_hash::FxHashMap,
    save::read_dat,
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::{Direction, Vec3},
    position::BlockPos,
    random::{LegacyRandom, LevelRandom},
    ticks::ChunkTicks,
};
use horizon_world::{
    level::{BlockEventData, BlockId, MovingPiston},
    nbt::Tag,
    registry::BlockStateId,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    sync::Arc,
};

pub fn hydrate_level_from_disk(
    game: &mut Game<'_>,
    slot: &str,
    dim: Dim,
    snap: &TimelineSnapshot,
) -> Option<LevelSnapshot> {
    let cp_dir = slot_dir(&game.world_root, slot);
    let cp_dim_dir = horizon_client::save::dimension_dir(&cp_dir, dim.id());
    if !cp_dim_dir.is_dir() {
        return None;
    }

    let shape = game.level_shapes.get(dim.id());
    let cp_save = horizon_client::save::SaveDir::new(cp_dim_dir.clone(), shape);
    if let Some(tables) = game.save.tables.get() {
        let _ = cp_save.tables.set(Arc::clone(tables));
    } else {
        let tables = Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
        let _ = cp_save.tables.set(tables);
    }

    let u8_to_dir = |v: u64| match v {
        0 => Direction::Down,
        1 => Direction::Up,
        2 => Direction::North,
        3 => Direction::South,
        4 => Direction::West,
        _ => Direction::East,
    };
    let decode_piston = |v: &serde_json::Value| -> Option<MovingPiston> {
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
    };

    let mut candidate_set: HashSet<(i32, i32)> = HashSet::new();
    let mut disk_level_rng: Option<u64> = None;
    let mut disk_random_tick_value: Option<i32> = None;
    let mut disk_end_clock: i64 = 0;
    let mut disk_comparator_outputs = HashMap::new();
    let mut disk_torch_toggles = Vec::new();
    let mut disk_moving_pistons = Vec::new();
    let mut disk_piston_pushes = Vec::new();
    let mut disk_block_events = VecDeque::new();
    let mut disk_last_hurt_by_player = HashMap::new();
    let mut disk_love_cause = HashMap::new();
    let mut disk_uuid_to_id: HashMap<[i32; 4], u32> = HashMap::new();
    let mut disk_mob_rt: HashMap<u32, DiskMobRuntime> = HashMap::new();
    let mut disk_item_rt: HashMap<u32, DiskItemRuntime> = HashMap::new();
    let mut disk_orb_rt: HashMap<u32, u32> = HashMap::new();
    let mut disk_chest_openers: Vec<((i32, i32, i32), (i32, f64))> = Vec::new();
    let mut disk_lid_openers: HashMap<(i32, i32, i32), i32> = HashMap::new();
    let mut disk_lectern_pulses: Vec<((i32, i32, i32), i32)> = Vec::new();
    let mut disk_portals_entities: HashMap<u32, horizon_client::nether_portal::PortalState> =
        HashMap::new();
    let mut disk_gateway_anims: Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)> =
        Vec::new();

    if let Ok(meta_str) = fs::read_to_string(cp_dir.join("meta.json")) {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
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
                        });
                    }
                }
            }
            if let Some(arr) = rs["last_hurt_by_player"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 2
                        && let Some(k) = a[0].as_u64()
                    {
                        disk_last_hurt_by_player.insert(k as u32, a[1].as_u64().map(|v| v as u32));
                    }
                }
            }
            if let Some(arr) = rs["love_cause"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 2
                        && let Some(k) = a[0].as_u64()
                    {
                        disk_love_cause.insert(k as u32, a[1].as_u64().map(|v| v as u32));
                    }
                }
            }
            if let Some(arr) = rs["persist_uuids"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 5
                        && let (Some(id), Some(u0), Some(u1), Some(u2), Some(u3)) = (
                            a[0].as_u64(),
                            a[1].as_i64(),
                            a[2].as_i64(),
                            a[3].as_i64(),
                            a[4].as_i64(),
                        )
                    {
                        disk_uuid_to_id
                            .insert([u0 as i32, u1 as i32, u2 as i32, u3 as i32], id as u32);
                    }
                }
            }
            if let Some(arr) = rs["mob_runtime"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() >= 7
                        && let (
                            Some(id),
                            Some(age),
                            Some(inv),
                            Some(lh),
                            Some(sw),
                            Some(psw),
                            Some(amb),
                        ) = (
                            a[0].as_u64(),
                            a[1].as_u64(),
                            a[2].as_i64(),
                            a[3].as_u64(),
                            a[4].as_i64(),
                            a[5].as_i64(),
                            a[6].as_i64(),
                        )
                    {
                        let rng_state = a.get(7).and_then(|v| v.as_u64()).unwrap_or(0);
                        let anger = a.get(8).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let anger_end = a.get(9).and_then(|v| v.as_i64()).unwrap_or(0);
                        let anger_stamp = a
                            .get(10)
                            .and_then(|v| v.as_array())
                            .and_then(|pair| {
                                if pair.len() == 2 {
                                    Some((pair[0].as_i64()? as i32, pair[1].as_u64()? as u32))
                                } else {
                                    None
                                }
                            });
                        let counter = a.get(11).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let counter2 = a.get(12).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let anim_ticks = a.get(13).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let egg_time = a.get(14).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let flag = a.get(15).and_then(|v| v.as_bool()).unwrap_or(false);
                        let flag2 = a.get(16).and_then(|v| v.as_bool()).unwrap_or(false);
                        let vector = a
                            .get(17)
                            .and_then(|v| v.as_array())
                            .and_then(|va| {
                                if va.len() == 3 {
                                    Some([
                                        f64::from_bits(va[0].as_u64()?),
                                        f64::from_bits(va[1].as_u64()?),
                                        f64::from_bits(va[2].as_u64()?),
                                    ])
                                } else {
                                    None
                                }
                            })
                            .unwrap_or([0.0; 3]);
                        let ints = a
                            .get(18)
                            .and_then(|v| v.as_array())
                            .and_then(|ia| {
                                if ia.len() == 6 {
                                    Some([
                                        ia[0].as_i64()? as i32,
                                        ia[1].as_i64()? as i32,
                                        ia[2].as_i64()? as i32,
                                        ia[3].as_i64()? as i32,
                                        ia[4].as_i64()? as i32,
                                        ia[5].as_i64()? as i32,
                                    ])
                                } else {
                                    None
                                }
                            })
                            .unwrap_or([0; 6]);
                        let body_adopted = a
                            .get(19)
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        disk_mob_rt.insert(
                            id as u32,
                            DiskMobRuntime {
                                age: age as u32,
                                invulnerable: inv as i32,
                                last_hurt: f32::from_bits(lh as u32),
                                swell: sw as i32,
                                previous_swell: psw as i32,
                                ambient_timer: amb as i32,
                                rng_state,
                                anger,
                                anger_end,
                                anger_stamp,
                                counter,
                                counter2,
                                anim_ticks,
                                egg_time,
                                flag,
                                flag2,
                                vector,
                                ints,
                                body_adopted,
                            },
                        );
                    }
                }
            }
            if let Some(arr) = rs["item_runtime"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() >= 3
                        && let (Some(id), Some(ticks), Some(bob)) =
                            (a[0].as_u64(), a[1].as_u64(), a[2].as_u64())
                    {
                        let rng_state = a.get(3).and_then(|v| v.as_u64()).unwrap_or(0);
                        disk_item_rt.insert(
                            id as u32,
                            DiskItemRuntime {
                                ticks: ticks as u32,
                                bob_offset: f32::from_bits(bob as u32),
                                rng_state,
                            },
                        );
                    }
                }
            }
            if let Some(arr) = rs["orb_runtime"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 2
                        && let (Some(id), Some(ticks)) = (a[0].as_u64(), a[1].as_u64())
                    {
                        disk_orb_rt.insert(id as u32, ticks as u32);
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
            if let Some(arr) = rs["portals_entities"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 2
                        && let (Some(id), Some(cd)) = (a[0].as_u64(), a[1].as_i64())
                    {
                        let mut st = horizon_client::nether_portal::PortalState::default();
                        st.cooldown = cd as i32;
                        disk_portals_entities.insert(id as u32, st);
                    }
                }
            } else if dim == Dim::Overworld {
                disk_portals_entities = snap.portals_entities.clone();
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
            if let Some(Tag::Long(t)) = saved.root.get(&"InhabitedTime".into()) {
                inhabited.insert((cx, cz), *t);
            }
            be_attachments.extend(saved.block_entity_attachments.iter().cloned());
            let mut b_ticks = ChunkTicks::from_saved(saved.block_ticks.clone());
            let mut f_ticks = ChunkTicks::from_saved(saved.fluid_ticks.clone());
            b_ticks.unpack(snap.game_time);
            f_ticks.unpack(snap.game_time);
            chunk_ticks.insert((cx, cz), (b_ticks, f_ticks));
        } else {
            chunk_ticks.insert((cx, cz), (ChunkTicks::default(), ChunkTicks::default()));
        }

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
    let raids_dat = fs::read(raids_dat_path(&cp_dir, dim)).ok();

    let saved_game_time = game.world.game_time;
    let saved_random = game.world.random.clone();
    let saved_next_entity = game.next_entity;
    let saved_loaded_entities = std::mem::take(&mut game.world.loaded_entities);
    let saved_mobs = std::mem::take(&mut game.mobs);
    let saved_dropped = std::mem::take(&mut game.dropped);
    let saved_orbs = std::mem::take(&mut game.orbs);
    let saved_projectiles = std::mem::take(&mut game.creatures.projectiles);
    let saved_boats = std::mem::take(&mut game.boats);
    let saved_carts = std::mem::take(&mut game.carts);
    let saved_tnts = std::mem::take(&mut game.tnts);
    let saved_falling = std::mem::take(&mut game.falling);
    let saved_paintings = std::mem::take(&mut game.paintings);
    let saved_item_frames = std::mem::take(&mut game.item_frames);
    let saved_armor_stands = std::mem::take(&mut game.armor_stands);
    let saved_cushions = std::mem::take(&mut game.cushions);
    let saved_others = std::mem::take(&mut game.others);
    let saved_crystals = std::mem::take(&mut game.crystals);
    let saved_dragon = game.dragon.take();
    let saved_knot_ids = std::mem::take(&mut game.knot_ids);
    let saved_persist = std::mem::take(&mut game.persist);
    let saved_tridents = std::mem::take(&mut game.tridents);
    let saved_rockets = std::mem::take(&mut game.rockets);
    let saved_breath = std::mem::take(&mut game.breath_clouds);
    let saved_fangs = std::mem::take(&mut game.fangs.list);
    let saved_ominous = std::mem::take(&mut game.ominous_spawners);

    game.world.game_time = snap.game_time;
    game.next_entity = saved_next_entity.max(snap.next_entity);

    let mut chunk_keys: Vec<(i32, i32)> = chunks.keys().copied().collect();
    chunk_keys.sort_unstable();
    for (cx, cz) in chunk_keys {
        let tags = cp_save.load_entities(cx, cz);
        if !tags.is_empty() {
            game.world.queue_entity_nbt((cx, cz), tags);
        }
    }
    game.spawn_loaded_entities();

    if !disk_uuid_to_id.is_empty() {
        let mut id_remap: HashMap<u32, u32> = HashMap::new();
        for (&spawned_id, uuid) in &game.persist.uuids {
            if let Some(&orig_id) = disk_uuid_to_id.get(uuid) {
                id_remap.insert(spawned_id, orig_id);
            }
        }
        let map_id = |id: &mut u32| {
            if let Some(&orig) = id_remap.get(id) {
                *id = orig;
            }
        };
        let map_opt_id = |opt: &mut Option<u32>| {
            if let Some(id) = opt
                && let Some(&orig) = id_remap.get(id)
            {
                *id = orig;
            }
        };
        let map_who = |who: &mut horizon_client::mobs::Who| {
            if let horizon_client::mobs::Who::Mob(id) = who
                && let Some(&orig) = id_remap.get(id)
            {
                *id = orig;
            }
        };
        for m in game.mobs.iter_mut() {
            map_id(&mut m.id);
            m.random = horizon_client::mobs::entity_random(m.id, m.position);
            map_opt_id(&mut m.extra.mount);
            map_opt_id(&mut m.extra.rider);
            map_opt_id(&mut m.extra.leader);
            if let Some(target) = m.ai.target.as_mut() {
                map_who(target);
            }
            if let Some(hurt_by) = m.ai.last_hurt_by.as_mut() {
                map_who(hurt_by);
            }
            if let Some(angry_at) = m.extra.angry_at.as_mut() {
                map_who(angry_at);
            }
            if let Some(
                horizon_client::mobs::leash::Holder::Mob(id)
                | horizon_client::mobs::leash::Holder::Boat(id),
            ) = m.extra.leash.as_mut()
            {
                map_id(id);
            }
            if let Some(bee) = m.extra.bee.as_mut()
                && let Some(angry_at) = bee.angry_at.as_mut()
            {
                map_who(angry_at);
            }
            map_opt_id(&mut m.extra.frog.tongue_target);
            if let Some(rt) = disk_mob_rt.get(&m.id) {
                m.age = rt.age;
                m.invulnerable = rt.invulnerable;
                m.last_hurt = rt.last_hurt;
                m.swell = rt.swell;
                m.previous_swell = rt.previous_swell;
                m.ambient_timer = rt.ambient_timer;
                if rt.rng_state != 0 {
                    m.random = horizon_core::random::EntityRandom::own(LegacyRandom::from_state(
                        rt.rng_state,
                    ));
                }
                m.extra.anger = rt.anger;
                m.extra.anger_end = rt.anger_end;
                m.extra.anger_stamp = rt.anger_stamp;
                m.extra.counter = rt.counter;
                m.extra.counter2 = rt.counter2;
                m.extra.anim_ticks = rt.anim_ticks;
                m.extra.egg_time = rt.egg_time;
                m.extra.flag = rt.flag;
                m.extra.flag2 = rt.flag2;
                m.extra.vector = rt.vector;
                m.extra.ints = rt.ints;
                if rt.body_adopted {
                    horizon_client::mobs::sulfur_cube::adopt_body(m);
                }
            }
        }
        for d in game.dropped.iter_mut() {
            map_id(&mut d.id);
            if let Some(rt) = disk_item_rt.get(&d.id) {
                d.ticks = rt.ticks;
                d.bob_offset = rt.bob_offset;
                if rt.rng_state != 0 {
                    d.random = LegacyRandom::from_state(rt.rng_state);
                }
            }
        }
        for o in game.orbs.iter_mut() {
            map_id(&mut o.id);
            if let Some(&ticks) = disk_orb_rt.get(&o.id) {
                o.ticks = ticks;
            }
        }
        for p in game.creatures.projectiles.iter_mut() {
            map_id(&mut p.id);
            if let Some(owner) = p.owner.as_mut() {
                map_who(owner);
            }
        }
        for b in game.boats.iter_mut() {
            map_id(&mut b.id);
        }
        for c in game.carts.iter_mut() {
            map_id(&mut c.id);
        }
        for t in game.tnts.iter_mut() {
            map_id(&mut t.id);
        }
        for f in game.falling.iter_mut() {
            map_id(&mut f.id);
        }
        for p in &mut game.paintings {
            map_id(&mut p.id);
        }
        for f in &mut game.item_frames {
            map_id(&mut f.id);
        }
        for a in game.armor_stands.iter_mut() {
            map_id(&mut a.id);
            map_opt_id(&mut a.vehicle);
        }
        for c in &mut game.cushions {
            map_id(&mut c.id);
        }
        for o in &mut game.others {
            map_id(&mut o.id);
        }
        for c in &mut game.crystals {
            map_id(&mut c.id);
        }
        if let Some(d) = game.dragon.as_mut() {
            map_id(&mut d.id);
        }
        for t in &mut game.tridents {
            map_id(&mut t.id);
            map_id(&mut t.owner);
        }
        for r in &mut game.rockets {
            map_id(&mut r.id);
            if let Some(owner) = r.owner.as_mut() {
                map_who(owner);
            }
            map_opt_id(&mut r.attached_to);
        }
        for c in &mut game.breath_clouds {
            map_id(&mut c.id);
        }
        for f in &mut game.fangs.list {
            map_id(&mut f.id);
        }
        for o in &mut game.ominous_spawners {
            map_id(&mut o.id);
        }
        for id in game.knot_ids.values_mut() {
            map_id(id);
        }
        let old_uuids = std::mem::take(&mut game.persist.uuids);
        for (mut k, v) in old_uuids {
            map_id(&mut k);
            game.persist.uuids.insert(k, v);
        }
        let old_raw = std::mem::take(&mut game.persist.raw);
        for (mut k, v) in old_raw {
            map_id(&mut k);
            game.persist.raw.insert(k, v);
        }
        let old_pslots = std::mem::take(&mut game.persist.passenger_slots);
        for (mut vid, mut slots) in old_pslots {
            map_id(&mut vid);
            for slot in &mut slots {
                if let super::super::persist::PassengerSlot::Live(rider_id) = slot {
                    map_id(rider_id);
                }
            }
            game.persist.passenger_slots.insert(vid, slots);
        }
    }

    let registry = game.pack.assets.registry;
    let tridents = game
        .tridents
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
        .collect();
    let rockets = game
        .rockets
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
        .collect();
    let breath_clouds = game
        .breath_clouds
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
        .collect();
    let ominous_spawners = game
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
        .collect();
    let fangs = std::mem::replace(&mut game.fangs.list, saved_fangs);

    let mobs = std::mem::replace(&mut game.mobs, saved_mobs);
    let dropped = std::mem::replace(&mut game.dropped, saved_dropped);
    let orbs = std::mem::replace(&mut game.orbs, saved_orbs);
    let projectiles = std::mem::replace(&mut game.creatures.projectiles, saved_projectiles);
    let boats = std::mem::replace(&mut game.boats, saved_boats);
    let carts = std::mem::replace(&mut game.carts, saved_carts);
    let tnts = std::mem::replace(&mut game.tnts, saved_tnts);
    let falling = std::mem::replace(&mut game.falling, saved_falling);
    let paintings = std::mem::replace(&mut game.paintings, saved_paintings);
    let item_frames = std::mem::replace(&mut game.item_frames, saved_item_frames);
    let armor_stands = std::mem::replace(&mut game.armor_stands, saved_armor_stands);
    let cushions = std::mem::replace(&mut game.cushions, saved_cushions);
    let others = std::mem::replace(&mut game.others, saved_others);
    let crystals = std::mem::replace(&mut game.crystals, saved_crystals);
    let dragon = game.dragon.take().as_ref().map(DragonSnapshot::from_live);
    game.dragon = saved_dragon;
    let knot_ids = std::mem::replace(&mut game.knot_ids, saved_knot_ids);
    let persist_mem = std::mem::replace(&mut game.persist, saved_persist);
    game.tridents = saved_tridents;
    game.rockets = saved_rockets;
    game.breath_clouds = saved_breath;
    game.ominous_spawners = saved_ominous;
    game.world.loaded_entities = saved_loaded_entities;
    game.world.game_time = saved_game_time;
    game.world.random = saved_random.clone();
    game.next_entity = saved_next_entity;

    let hydrated_random = disk_level_rng
        .map(|st| LevelRandom::for_level(LegacyRandom::from_state(st)))
        .unwrap_or(saved_random);
    let hydrated_rtv = disk_random_tick_value.unwrap_or(game.block_ticks.random_tick_value);

    let globals_tag = read_dat(&cp_dir.join("rreturn_globals.dat"));
    let border_key = match dim {
        Dim::Overworld => "border_overworld",
        Dim::Nether => "border_nether",
        Dim::End => "border_end",
    };
    let hydrated_border = globals_tag
        .as_ref()
        .and_then(|t| t.get(border_key))
        .map(WorldBorder::unpack)
        .or_else(|| {
            read_dat(&cp_dim_dir.join("data/minecraft/world_border.dat"))
                .and_then(|t| t.get("data").map(WorldBorder::unpack))
        })
        .unwrap_or_else(|| {
            if dim == Dim::Overworld {
                snap.border.clone()
            } else {
                WorldBorder::default()
            }
        });

    Some(LevelSnapshot {
        chunks,
        block_entities,
        block_names,
        be_attachments,
        world_attachments,
        chest_openers: disk_chest_openers,
        border: hydrated_border,
        shulker_anims: Vec::new(),
        gateway_anims: disk_gateway_anims,
        fangs,
        ominous_spawners,
        portals_entities: disk_portals_entities,
        lectern_pulses: disk_lectern_pulses,
        lid_openers: disk_lid_openers,
        inhabited,
        end_clock: disk_end_clock,
        random: hydrated_random,
        forced_chunks,
        chunk_ticks,
        random_tick_value: hydrated_rtv,
        comparator_outputs: disk_comparator_outputs,
        torch_toggles: disk_torch_toggles,
        moving_pistons: disk_moving_pistons,
        piston_pushes: disk_piston_pushes,
        block_events: disk_block_events,
        hoppers: HashMap::new(),
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
        dragon,
        tridents,
        rockets,
        breath_clouds,
        knot_ids,
        persist_uuids: persist_mem.uuids,
        persist_raw: persist_mem.raw,
        persist_passenger_slots: persist_mem.passenger_slots,
        raids_dat,
        last_hurt_by_player: disk_last_hurt_by_player,
        love_cause: disk_love_cause,
    })
}

pub fn hydrate_slot_from_disk_on_server(
    game: &mut Game<'_>,
    slot: &str,
) -> Option<Arc<TimelineSnapshot>> {
    let mut snap = match get_slot_snapshot_clone(slot) {
        Some(existing) => (*existing).clone(),
        None => load_slot_from_disk(&game.world_root, slot)?,
    };

    for dim in [Dim::Overworld, Dim::Nether, Dim::End] {
        if !snap.levels.contains_key(&dim) {
            if let Some(lsnap) = hydrate_level_from_disk(game, slot, dim, &snap) {
                if dim == game.dim {
                    set_client_slot_snapshot(
                        slot,
                        ClientBlockSnapshot {
                            dim,
                            chunks: lsnap.chunks.clone(),
                        },
                    );
                }
                snap.levels.insert(dim, lsnap);
            }
        }
    }

    let arc = Arc::new(snap);
    set_slot_snapshot(slot, Arc::clone(&arc));
    Some(arc)
}