use super::super::{Dim, Game};
use super::benchmark::*;
use super::policy::*;
use super::snapshots::*;
use super::types::*;
use horizon_client::{
    chunk::Chunk,
    containers::BlockEntity,
    fast_hash::FxHashMap,
};
use horizon_core::position::ChunkPos;
use horizon_world::registry::BlockStateId;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    sync::Arc,
};

pub fn apply_level_snapshot_in_place(
    game: &mut Game<'_>,
    dim: Dim,
    snap: &LevelSnapshot,
    policy: &RollbackPolicy,
    snap_game_time: i64,
) -> (f64, f64, f64) {
    let old_chunks: FxHashMap<(i32, i32), Arc<Chunk>> = (&*game.world.chunks).clone();

    let t_blocks = std::time::Instant::now();
    if policy.blocks || policy.biomes {
        let mut changed_coords = Vec::new();
        let mut biome_changed_chunks = Vec::new();

        for (&(cx, cz), snap_chunk) in snap.chunks.iter() {
            if let Some(live_chunk) = old_chunks.get(&(cx, cz)) {
                if Arc::ptr_eq(live_chunk, snap_chunk) {
                    continue;
                }
                let mut chunk_biome_changed = false;
                let min_y = snap_chunk.min_y();
                for (s_idx, snap_sec) in snap_chunk.sections.iter().enumerate() {
                    let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                        continue;
                    };
                    if Arc::ptr_eq(live_sec, snap_sec) {
                        continue;
                    }
                    if policy.biomes && snap_sec.biomes != live_sec.biomes {
                        chunk_biome_changed = true;
                    }
                    if !policy.blocks {
                        continue;
                    }
                    if let (Some(u1), Some(u2)) =
                        (snap_sec.blocks.uniform(), live_sec.blocks.uniform())
                    {
                        if u1 == u2 {
                            continue;
                        }
                    }
                    if snap_sec.blocks == live_sec.blocks {
                        continue;
                    }

                    let y_base = min_y + (s_idx as i32) * 16;
                    let x_base = cx * 16;
                    let z_base = cz * 16;
                    for sy in 0..16 {
                        for sz in 0..16 {
                            for sx in 0..16 {
                                let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                                let want = snap_sec.get(idx);
                                if live_sec.get(idx) != want {
                                    changed_coords.push((
                                        x_base + sx,
                                        y_base + sy,
                                        z_base + sz,
                                        want,
                                    ));
                                }
                            }
                        }
                    }
                }
                if chunk_biome_changed {
                    biome_changed_chunks.push((cx, cz));
                }
            }
        }
        for (&(cx, cz), live_chunk) in old_chunks.iter() {
            if !snap.chunks.contains_key(&(cx, cz)) {
                if let Some(disk_chunk) =
                    game.save.load(cx, cz, game.pack.assets.registry, game.res.biomes())
                {
                    let mut chunk_biome_changed = false;
                    let min_y = disk_chunk.min_y();
                    for (s_idx, disk_sec) in disk_chunk.sections.iter().enumerate() {
                        let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                            continue;
                        };
                        if policy.biomes && disk_sec.biomes != live_sec.biomes {
                            chunk_biome_changed = true;
                        }
                        if !policy.blocks {
                            continue;
                        }
                        if disk_sec.blocks == live_sec.blocks {
                            continue;
                        }
                        let y_base = min_y + (s_idx as i32) * 16;
                        let x_base = cx * 16;
                        let z_base = cz * 16;
                        for sy in 0..16 {
                            for sz in 0..16 {
                                for sx in 0..16 {
                                    let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                                    let want = disk_sec.get(idx);
                                    if live_sec.get(idx) != want {
                                        changed_coords.push((
                                            x_base + sx,
                                            y_base + sy,
                                            z_base + sz,
                                            want,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    if chunk_biome_changed {
                        biome_changed_chunks.push((cx, cz));
                    }
                }
            }
        }

        bench_add_chunks(snap.chunks.len());
        bench_add_blocks_changed(changed_coords.len());

        if policy.blocks {
            game.world.block_log_held.clear();
            for (wx, wy, wz, want) in changed_coords {
                let _ = game.world.set(wx, wy, wz, BlockStateId(want));
            }
        }
        bench_accum(Phase::ChunkBlocks, t_blocks);

        if policy.blocks && policy.biomes {
            game.world.chunks = Default::default();
            for (&k, v) in &snap.chunks {
                game.world.chunks.insert(k, v.clone());
            }
            game.world.inhabited = snap.inhabited.clone();
        } else {
            game.world.chunks = Default::default();
            for (&(cx, cz), snap_chunk) in &snap.chunks {
                if let Some(live_chunk) = old_chunks.get(&(cx, cz)) {
                    let mut c = (**live_chunk).clone();
                    for (s_idx, snap_sec) in snap_chunk.sections.iter().enumerate() {
                        if let Some(sec_arc) = c.sections.get_mut(s_idx) {
                            let sec = Arc::make_mut(sec_arc);
                            if policy.blocks {
                                sec.blocks = snap_sec.blocks.clone();
                            }
                            if policy.biomes {
                                sec.biomes = snap_sec.biomes.clone();
                            }
                        }
                    }
                    game.world.chunks.insert((cx, cz), Arc::new(c));
                } else {
                    game.world.chunks.insert((cx, cz), snap_chunk.clone());
                }
            }
        }
        if policy.biomes
            && !biome_changed_chunks.is_empty()
            && let Some(host) = game.lan.as_mut()
        {
            host.hub.resend_biomes(&game.world, &biome_changed_chunks);
        }
    }

    let blocks_ms = (std::time::Instant::now() - t_blocks).as_secs_f64() * 1000.0;
    let t_containers = std::time::Instant::now();
    if policy.block_entities {
        let mut be_positions: HashSet<(i32, i32, i32)> =
            game.world.block_entities.keys().copied().collect();
        be_positions.extend(snap.block_entities.keys().copied());
        game.world.block_entities.clear();
        let mut by_chunk: BTreeMap<(i32, i32), Vec<((i32, i32, i32), BlockEntity)>> =
            BTreeMap::new();
        for (&pos, be) in &snap.block_entities {
            if pos.1 == super::super::minecarts::CART_KEY_Y {
                continue;
            }
            by_chunk
                .entry((pos.0 >> 4, pos.2 >> 4))
                .or_default()
                .push((pos, be.clone()));
        }
        for (_, mut chunk_bes) in by_chunk {
            chunk_bes.sort_unstable_by_key(|(pos, _)| *pos);
            game.world.block_entities.load_chunk(chunk_bes);
        }
        for (pos, att) in &snap.be_attachments {
            game.world
                .block_entities
                .restore_attachments(*pos, att.clone());
        }
        game.world.block_names = snap.block_names.clone();
        game.hoppers = snap.hoppers.clone();
        if let Some(log) = game.world.block_log.as_mut() {
            for pos in be_positions {
                log.push(pos);
            }
        }
        bench_accum(Phase::ChunkBeLoad, t_containers);
    }
    let containers_ms = (std::time::Instant::now() - t_containers).as_secs_f64() * 1000.0;
    let t_entities = std::time::Instant::now();

    for &key in old_chunks.keys().chain(game.world.chunks.keys()) {
        game.random_tick_sections.remove(&key);
        game.portals.sections.remove(&key);
    }
    if game.tick_chunks.synchronize(&game.world.chunks) {
        let added = game.tick_chunks.take_added();
        game.tick_chunks.recycle_added(added);
    }
    game.update_chunk_tickets();
    game.start_ticking_chunks();

    if policy.block_ticks || policy.fluid_ticks {
        let mut old_ticks = FxHashMap::default();
        if !policy.block_ticks || !policy.fluid_ticks {
            for &(cx, cz) in old_chunks.keys() {
                let (old_b, old_f) = game.block_ticks.remove_chunk(ChunkPos::new(cx, cz));
                old_ticks.insert(
                    (cx, cz),
                    (old_b.unwrap_or_default(), old_f.unwrap_or_default()),
                );
            }
        }
        game.block_ticks.blocks = Default::default();
        game.block_ticks.fluids = Default::default();

        let mut snap_chunk_keys: Vec<(i32, i32)> = game.world.chunks.keys().copied().collect();
        snap_chunk_keys.sort_unstable();
        for (cx, cz) in snap_chunk_keys {
            let cpos = ChunkPos::new(cx, cz);
            let (old_b, old_f) = old_ticks.remove(&(cx, cz)).unwrap_or_default();
            let (snap_b, snap_f) = snap
                .chunk_ticks
                .get(&(cx, cz))
                .cloned()
                .unwrap_or_default();
            let b_to_add = if policy.block_ticks {
                let mut b = snap_b;
                b.unpack(snap_game_time);
                b
            } else {
                old_b
            };
            let f_to_add = if policy.fluid_ticks {
                let mut f = snap_f;
                f.unpack(snap_game_time);
                f
            } else {
                old_f
            };
            let _ = game.block_ticks.add_chunk(cpos, b_to_add, f_to_add);
        }
        game.block_ticks.random_tick_value = snap.random_tick_value;
        game.block_ticks.comparator_outputs = snap.comparator_outputs.clone();
        game.block_ticks.torch_toggles = snap.torch_toggles.clone();
    }

    if policy.block_events {
        game.block_ticks.moving_pistons.clear();
        for &(pos, piston) in &snap.moving_pistons {
            game.block_ticks.load_moving_piston(pos, piston);
        }
        game.world.moving_pistons = snap
            .moving_pistons
            .iter()
            .map(|&(pos, piston)| ([pos.x, pos.y, pos.z], piston.shape_state()))
            .collect();
        game.block_ticks.piston_pushes = snap.piston_pushes.clone();
        game.block_ticks.retain_block_events(|_| false);
        for &ev in &snap.block_events {
            game.block_ticks.queue_block_event(ev);
        }
    }

    if policy.world_time {
        game.world.end_clock = snap.end_clock;
    }

    if policy.world_level_rng {
        game.world.random = snap.random.clone();
    }

    if policy.server_world_border {
        game.border = snap.border.clone();
    }

    if policy.server_forced_chunks {
        game.streaming.forced = snap.forced_chunks.clone();
    }

    if policy.entities_presence {
        game.fangs.list = snap.fangs.clone();
        game.ominous_spawners.clear();
        let saved_next = game.next_entity;
        for (oid, onbt) in &snap.ominous_spawners {
            game.load_ominous_item_spawner(onbt.clone());
            if let Some(last) = game.ominous_spawners.last_mut() {
                last.id = *oid;
            }
        }
        game.next_entity = saved_next;
        game.portals.entities = snap.portals_entities.clone();
        bench_add_mobs(snap.mobs.len());
        game.mobs.clone_from(&snap.mobs);
        if policy.dropped_items {
            game.dropped.clone_from(&snap.dropped);
        }
        if policy.experience_orbs {
            game.orbs.clone_from(&snap.orbs);
        }
        game.creatures.projectiles.clone_from(&snap.projectiles);
        game.boats.clone_from(&snap.boats);
        game.carts.clone_from(&snap.carts);
        game.tnts.clone_from(&snap.tnts);
        game.falling.clone_from(&snap.falling);
        game.paintings.clone_from(&snap.paintings);
        game.item_frames.clone_from(&snap.item_frames);
        game.armor_stands.clone_from(&snap.armor_stands);
        game.cushions.clone_from(&snap.cushions);
        game.others.clone_from(&snap.others);
        game.crystals.clone_from(&snap.crystals);
        game.dragon = snap
            .dragon
            .as_ref()
            .and_then(|d| d.to_live(game.level.seed));
        game.knot_ids.clone_from(&snap.knot_ids);
        game.persist = Default::default();
        game.persist.uuids.clone_from(&snap.persist_uuids);
        game.persist.raw.clone_from(&snap.persist_raw);
        game.persist
            .passenger_slots
            .clone_from(&snap.persist_passenger_slots);
        game.persist.joined_mark = game.next_entity;
        game.tridents.clear();
        let registry = game.pack.assets.registry;
        for ts in &snap.tridents {
            if let Some(n) = horizon_client::entity_nbt::trident_from_nbt(&ts.nbt) {
                let mut t = super::super::trident::ThrownTrident::from_nbt(n, &ts.nbt, registry);
                t.id = ts.id;
                t.previous = ts.previous;
                t.in_ground_time = ts.in_ground_time;
                t.no_physics = ts.no_physics;
                t.owner = ts.owner;
                t.guest = ts.guest;
                game.tridents.push(t);
                if ts.join.is_joined()
                    && let Some(last) = game.tridents.last_mut()
                {
                    last.join = ts.join;
                }
            }
        }
        game.rockets.clear();
        for rs in &snap.rockets {
            if let Some(rn) = horizon_client::entity_nbt::firework_rocket_from_nbt(&rs.nbt, None) {
                let mut r = super::super::fireworks::Rocket::from_nbt(rn);
                r.id = rs.id;
                r.attached = rs.attached;
                r.guest = rs.guest;
                r.owner = rs.owner;
                r.attached_to = rs.attached_to;
                game.rockets.push(r);
            }
        }
        game.bolts.clear();
        game.breath_clouds.clear();
        for bs in &snap.breath_clouds {
            if let Some((_, mut cloud)) = super::super::potions::potion_cloud_from_nbt(&bs.nbt) {
                cloud.id = bs.id;
                cloud.motion = bs.motion;
                cloud.from_dragon = bs.from_dragon;
                cloud.victims = bs.victims.clone();
                game.breath_clouds.push(cloud);
            } else if bs.from_dragon || bs.potion_none {
                game.breath_clouds.push(super::super::dragon::Breath {
                    id: bs.id,
                    at: bs.at,
                    motion: bs.motion,
                    radius: bs.radius,
                    age: bs.age,
                    duration: bs.duration,
                    from_dragon: bs.from_dragon,
                    victims: bs.victims.clone(),
                    potion: None,
                });
            }
        }
        game.potions.clear_thrown();
        game.potions.clouds.clear();
    }

    if policy.world_raids {
        let path = raids_dat_path(&game.world_root, dim);
        match &snap.raids_dat {
            Some(bytes) => {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&path, bytes);
            }
            None => {
                let _ = fs::remove_file(&path);
            }
        }
        game.raiding = Default::default();
    }

    game.spawn_scratch = Default::default();
    game.mob_index = Default::default();
    let saved_tracker = game.sculk.tracker;
    let saved_tracker_loaded = game.sculk.tracker_loaded;
    let saved_darkness = game.sculk.darkness;
    game.sculk = Default::default();
    game.sculk.tracker = saved_tracker;
    game.sculk.tracker_loaded = saved_tracker_loaded;
    game.sculk.darkness = saved_darkness;
    game.sculk_prepare();
    game.sculk_sync_index();
    game.world.loaded_entities.clear();
    game.world.entities_pending.clear();
    game.world.loaded_chunks.clear();
    game.world.modified.clear();
    if policy.blocks {
        game.world.watched_writes.clear();
        if let Some(writes) = game.world.path_writes.as_mut() {
            writes.clear();
        }
        if policy.biomes {
            game.world.light_edits.clear();
            game.world.dirty_light.clear();
        }
    }
    if policy.blocks || policy.biomes || policy.block_entities {
        game.creaking = Default::default();
        game.structure_templates.clear();
        game.test_highlights.clear();
    }
    if policy.block_entities {
        game.world.chest_openers.clear();
        for &(k, v) in &snap.chest_openers {
            game.world.chest_openers.insert(k, v);
        }
        game.world.shulkers.clear();
    }
    if policy.block_events {
        game.world.moving_piston_fallback.clear();
    }
    game.world.attachments = snap.world_attachments.clone();
    game.world.attachment_syncs.clear();

    game.last_hurt_by_player = snap.last_hurt_by_player.clone();
    game.love_cause = snap.love_cause.clone();
    game.died.clear();
    game.combat = Default::default();
    game.guest_combat.clear();
    game.stabbed.clear();
    game.player_stabs.clear();
    game.kinetic_feedback.clear();
    game.brushing = Default::default();
    if policy.block_ticks || policy.block_entities {
        game.lectern_pulses = snap.lectern_pulses.clone();
    }
    let be_anim_kinds = std::mem::take(&mut game.be_anims.kinds);
    game.be_anims = Default::default();
    game.be_anims.kinds = be_anim_kinds;
    if game.be_anims.kinds.is_empty() {
        game.be_anims.classify(game.pack.assets.registry);
    }
    if policy.block_entities {
        game.lid_openers = snap.lid_openers.clone();
        for (pos, shulker) in &snap.shulker_anims {
            game.be_anims.shulkers.insert(*pos, shulker.clone());
        }
        for (pos, gw) in &snap.gateway_anims {
            game.be_anims.gateway_servers.insert(*pos, *gw);
        }
        game.sync_shulkers();
    }
    for (_, sound_id) in std::mem::take(&mut game.jukebox_sounds) {
        game.audio.stop_and_release(sound_id);
    }
    game.lan_songs.clear();
    game.lan_cushion_breaks.clear();
    game.lan_global_events.clear();

    game.creatures.wind_source = None;
    game.creatures.late_wind_pushes.clear();
    game.creatures.wind_bursts.clear();
    game.creatures.smash_pushes.clear();
    game.creatures.sweeps.clear();
    game.creatures.ticked.clear();
    game.creatures.deferred_hits = false;
    game.creatures.hook_targets.clear();
    game.creatures.hook_pulls.clear();
    game.creatures.tick_buffer.clear();
    game.creatures.lan_explosions.clear();
    game.creatures.lan_sounds.clear();
    game.creatures.lan_particles.clear();
    game.creatures.elder_apparition = 0;
    game.leash_boats.clear();
    game.spawner_tick_positions.clear();
    game.trial_spawner_tick_positions.clear();
    game.pass_mobs = None;
    game.item_columns = None;
    game.pickup_columns = None;
    game.tnt_refused = None;
    game.mount = Default::default();
    game.posed_rider = game.riding;
    game.use_delay = 0;
    game.forced_hit = None;
    game.tick_hit = None;
    game.tick_mob = None;
    game.use_failed = false;

    game.spill = Default::default();
    game.pending = Default::default();
    game.particles.clear();
    game.particles.sounds.clear();
    game.pickups.clear();
    game.transient_blocks.clear();
    game.destroying.clear();
    game.mining = None;
    game.using = None;
    game.lan_events.clear();
    game.lan_block_events.clear();
    game.lan_entity_events.clear();
    game.lan_critical_hits.clear();
    game.lan_kinetic_hits.clear();
    game.guest_equipment_breaks.clear();
    game.started_riding.clear();
    game.admin_feedback.clear();
    game.cmds.waypoint_dirty.clear();
    game.set_camera(None);
    game.cmds.guest_cameras.clear();
    game.cmds.camera_requests.clear();
    game.cmds.profile_fetches.clear();
    game.cmds.pregen = None;
    game.streaming.set_band(None);
    game.cmds.waypoints = None;
    game.cmds.waypoint_icons.clear();
    game.mount_menu_reset();
    game.ender_open = None;
    game.close_sign_editor();
    game.close_dialogs();
    game.close_book();
    game.close_mod_screen();
    game.mod_container_viewers.clear();
    game.kept_block_entities.get_mut().clear();
    game.close_container();
    game.station = None;
    game.container = None;
    game.craft_table = None;
    game.sign_edit = None;
    game.book = None;
    game.command_block_screen = None;

    bench_accum(Phase::SpawnNbt, t_entities);
    let entities_ms = (std::time::Instant::now() - t_entities).as_secs_f64() * 1000.0;
    (blocks_ms, containers_ms, entities_ms)
}