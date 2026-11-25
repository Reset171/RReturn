use super::super::{Dim, Game};
use super::benchmark::*;
use super::capture::capture_level_snapshot;
use super::hydration::hydrate_slot_from_disk_on_server;
use super::restore_level::apply_level_snapshot_in_place;
use super::restore_players::{restore_offline_files, restore_players_on_rollback};
use super::snapshots::*;
use super::storage::{restore_disk_files_for_slot_opts, slot_dir};
use super::tracker::loop_tracker;
use super::types::*;
use horizon_client::bossbar::BossBars;
use std::{
    collections::HashSet,
    fs,
    sync::Arc,
};

pub fn rollback_checkpoint_on_server(game: &mut Game<'_>) -> bool {
    let slot = with_state(|st| st.current_slot.clone());
    rollback_slot_checkpoint_on_server(game, &slot)
}

fn resolve_rollback_target_dim(
    game: &Game<'_>,
    snap: &TimelineSnapshot,
    live_policy: &RollbackPolicy,
    is_scrub: bool,
) -> Dim {
    let primary_ident = find_local_player_identity(game);
    let primary_keeps_pos = with_state(|st| {
        if let Some((u, n)) = &primary_ident {
            !resolve_player_effective_policy(st, live_policy, is_scrub, *u, Some(n.as_str()))
                .player_position
        } else {
            !live_policy.player_position
        }
    });
    if primary_keeps_pos {
        game.dim
    } else {
        snap.guests
            .values()
            .next()
            .map(|g| Dim::from_id(&g.dimension))
            .unwrap_or_else(|| snap.levels.keys().copied().next().unwrap_or(Dim::Overworld))
    }
}

pub fn rollback_slot_checkpoint_on_server(game: &mut Game<'_>, slot: &str) -> bool {
    let t_hydrate = std::time::Instant::now();
    let mut snap = match get_slot_snapshot_clone(slot) {
        Some(s) => s,
        None => match hydrate_slot_from_disk_on_server(game, slot) {
            Some(s) => s,
            None => return false,
        },
    };

    let live_policy = with_state(|st| st.policy);
    let target_dim = resolve_rollback_target_dim(game, &snap, &live_policy, false);

    if !snap.levels.contains_key(&target_dim) {
        if let Some(hydrated) = hydrate_slot_from_disk_on_server(game, slot) {
            snap = hydrated;
        }
    }
    bench_accum(Phase::HydrateSnapshot, t_hydrate);

    apply_timeline_snapshot_on_server(game, &snap, RollbackOrigin::DiskSlot(slot))
}

/// Re-register already-lit chunks in `Streaming` after replacing it or inserting snapshot chunks,
/// so `Streaming::is_lit` matches `World::chunks` and `/tp` does not stall in `hold_for_chunks`.
fn sync_streaming_lit_from_world(game: &mut Game<'_>) {
    let missing_lit: Vec<(i32, i32)> = game
        .world
        .chunks
        .iter()
        .filter(|(pos, chunk)| {
            !game.streaming.is_lit(pos.0, pos.1) && (chunk.lit || chunk.light_loaded)
        })
        .map(|(pos, _)| *pos)
        .collect();
    if missing_lit.is_empty() {
        return;
    }
    let prev_generated = game.streaming.generated_total;
    for pos in missing_lit {
        if let Some(chunk) = game.world.chunks.get(&pos) {
            let chunk_clone = (**chunk).clone();
            game.streaming.insert_remote(&mut game.world, chunk_clone);
        }
    }
    game.streaming.generated_total = prev_generated;
    if !game.streaming.mesh {
        game.world.dirty_sections.clear();
    }
}

pub fn apply_timeline_snapshot_on_server(
    game: &mut Game<'_>,
    snap: &TimelineSnapshot,
    origin: RollbackOrigin<'_>,
) -> bool {
    let t_start = std::time::Instant::now();
    let is_scrub = matches!(origin, RollbackOrigin::ScrubSnapshot { .. });
    let prev_dim = game.dim;
    let bench_name = match origin {
        RollbackOrigin::DiskSlot(slot) => format!("ROLLBACK RESTORE [{}]", slot),
        RollbackOrigin::ScrubSnapshot { target_tick, .. } => {
            format!("ROLLBACK RESTORE [SCRUB @ {}]", target_tick)
        }
    };
    bench_begin(&bench_name, game.world.game_time);

    let t_flush = std::time::Instant::now();
    let _ = game.save.flush();
    for parked in game.levels.parked.values() {
        let _ = parked.save.flush();
    }
    game.world.loaded_entities.clear();
    bench_accum(Phase::FlushSave, t_flush);

    let live_policy = with_state(|st| if is_scrub { st.scrub_policy } else { st.policy });
    let target_dim = resolve_rollback_target_dim(game, snap, &live_policy, is_scrub);

    let (need_force_disk, is_hot_rollback, disk_files_changed) = match origin {
        RollbackOrigin::DiskSlot(slot) => {
            let need_force = !snap.disk_restored;
            let is_hot = slot == "default"
                && game.dim == target_dim
                && snap.levels.contains_key(&game.dim)
                && !need_force;
            let t_disk = std::time::Instant::now();
            let changed = restore_disk_files_for_slot_opts(&game.world_root, slot, need_force);
            bench_accum(Phase::RestoreDiskFiles, t_disk);
            (need_force, is_hot, changed)
        }
        RollbackOrigin::ScrubSnapshot { .. } => (false, true, 0),
    };

    let t_savedir = std::time::Instant::now();
    if need_force_disk || !is_hot_rollback || disk_files_changed > 0 {
        let new_save = horizon_client::save::SaveDir::new(
            game.save.root.clone(),
            game.save.shape,
        );
        if let Some(tables) = game.save.tables.get() {
            let _ = new_save.tables.set(Arc::clone(tables));
        } else {
            let tables = Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
            let _ = new_save.tables.set(tables);
        }
        game.save = Arc::new(new_save);
    }
    bench_accum(Phase::SaveDirStreaming, t_savedir);

    bench_add_players(snap.guests.len());
    let policy = &live_policy;

    let t_globals = std::time::Instant::now();
    if policy.world_time {
        game.world.game_time = snap.game_time;
        game.world.day_time = snap.day_time;
        game.ui.clock.rate = snap.time_rate;
        game.ui.clock.partial = snap.time_partial;
        game.ui.clock.paused = snap.time_paused;
        game.ui.command_clocks = snap.command_clocks.clone();
    }
    if policy.world_weather {
        game.world.weather = snap.weather;
    }
    if policy.server_game_rules {
        game.ui.rules = snap.rules.clone();
        game.world.health_rules = snap.health_rules;
        game.sync_rules();
        let _ = game.clock.stop_sprint();
        let _ = game.clock.stop_step();
        game.clock.set_frozen_ticks_to_run(0);
        game.clock.set_rate(snap.clock_rate);
        game.clock.set_frozen(snap.clock_frozen);
    }
    if policy.world_level_rng {
        let _ = horizon_client::random_source::set_thread_uniquifier(
            game.level.seed ^ snap.game_time.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64),
        );
    }
    game.portals = Default::default();
    game.load_portal_arrivals();
    let locator = game.end.locator();
    game.end = Default::default();
    game.end.set_locator(locator);
    if policy.entities_presence {
        game.portals.entities = snap.portals_entities.clone();
        game.end
            .rreturn_set_gateway_cooldowns(&snap.end_gateway_cooldowns);
    }
    game.server_trips.clear();
    game.teleport_notes.clear();
    game.spawn_wait.clear();
    game.watched_vehicles.clear();
    game.clear_game_tests();
    game.xp_delay = 0;
    if policy.world_difficulty {
        game.ui.difficulty = snap.difficulty;
        game.level.set_difficulty(snap.difficulty);
        game.level.difficulty_locked = snap.difficulty_locked;
        game.sync_rules();
    }
    if policy.world_spawn {
        game.level.world_spawn = snap.world_spawn;
        game.level.world_spawn_angle = snap.world_spawn_angle;
    }
    if policy.server_world_border {
        game.border = snap.border.clone();
    }
    if policy.world_scoreboard {
        game.load_scoreboard();
        game.score.board = snap.scoreboard.clone();
        match &origin {
            RollbackOrigin::DiskSlot(slot) => {
                let cp_slot_dir = slot_dir(&game.world_root, slot);
                game.score.storage =
                    horizon_client::command_storage::CommandStorage::load(&cp_slot_dir);
                if let Some(elapsed) = &snap.stopwatches_elapsed {
                    game.score.stopwatches =
                        restore_stopwatches_elapsed(&game.world_root, elapsed);
                } else {
                    game.score.stopwatches =
                        horizon_client::stopwatches::Stopwatches::load(&cp_slot_dir);
                }
            }
            RollbackOrigin::ScrubSnapshot { .. } => {
                if let Some(storage) = &snap.command_storage {
                    game.score.storage = storage.clone();
                }
                if let Some(elapsed) = &snap.stopwatches_elapsed {
                    game.score.stopwatches =
                        restore_stopwatches_elapsed(&game.world_root, elapsed);
                }
            }
        }
        game.sync_score_registries();
    }
    bench_accum(Phase::ServerGlobals, t_globals);
    let globals_ms = (std::time::Instant::now() - t_globals).as_secs_f64() * 1000.0;

    let t_bosses = std::time::Instant::now();
    if policy.server_bossbars {
        game.load_bossbars();
        let bars = BossBars::unpack(&snap.bossbars);
        game.bosses.bars = bars;
        game.ui.registries.bossbars = game.bosses.bars.ids();
    }
    if policy.world_dragon_fight {
        if matches!(origin, RollbackOrigin::DiskSlot(_)) {
            let fight_path = fight_dat_path(&game.world_root);
            match &snap.dragon_fight_dat {
                Some(bytes) => {
                    if let Some(parent) = fight_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::write(&fight_path, bytes);
                }
                None => {
                    let _ = fs::remove_file(&fight_path);
                }
            }
        }
        if let Some(ftag) = &snap.fight_tag {
            game.fight = super::super::dragon::Fight::rreturn_unpack(ftag);
        } else {
            game.fight = Default::default();
        }
    }

    game.sculk.tracker = snap.sculk_tracker;
    game.sculk.tracker_loaded = true;
    game.sculk.darkness = snap.sculk_darkness;
    game.adv.toasts.clear();
    game.adv.last_inventory = None;
    game.adv.levitation = None;
    game.adv.marks = Default::default();
    game.adv.changed.clear();
    game.adv.unlocked.clear();
    game.adv.announced.clear();
    game.next_entity = snap.next_entity;
    game.ticked_entities_through = snap.ticked_entities_through;
    game.ordered.announced = snap.ordered_announced;
    game.creatures.cat.next_tick = snap.cat_tick;
    game.stats.rreturn_set_phantom_tick(snap.phantom_tick);
    game.level.player_extra = snap.player_extra.clone();
    game.creatures.shoulder_time = snap.shoulder_time.clone();
    if policy.entities_presence {
        game.fangs.list = snap.fangs.clone();
        game.ominous_spawners.clear();
        let saved_next = game.next_entity;
        for (oid, onbt) in &snap.ominous_spawners {
            game.load_ominous_item_spawner(onbt.clone());
            let last_idx = game.ominous_spawners.len().saturating_sub(1);
            if !game.ominous_spawners.is_empty() {
                game.ominous_spawners.update(last_idx, |last| {
                    last.id = *oid;
                });
            }
        }
        game.next_entity = saved_next;
    }
    game.contact_drops = 0..0;
    game.lectern_pulses = snap.lectern_pulses.clone();
    game.player_beds = snap.player_beds.clone();
    game.sleep_seen = snap.sleep_seen;
    game.lead_killers = snap.lead_killers.clone();
    if policy.block_entities {
        game.lid_openers = snap.lid_openers.clone();
    }
    game.entity_attachments = snap.entity_attachments.clone();
    game.player_state.attachments.clone_from(&snap.player_attachments);
    if let Some(host) = game.lan.as_mut() {
        host.hub.guests.update_all(|g| {
            if let Some(att) = snap.guest_attachments.get(&g.uuid) {
                *g.attachments = att.clone();
            }
        });
    }
    if policy.player_stats {
        game.stats.events.clear();
    }
    game.fns.schedule = snap.scheduled_events.clone();
    game.sync_schedule_registry();
    game.cmds.post_effects = snap.cmd_post_effects.clone();
    game.exec = Default::default();
    game.maps = Default::default();
    game.rand_sequences = snap.rand_sequences.clone();

    match &origin {
        RollbackOrigin::DiskSlot(_) => {
            let trader_path = trader_dat_path(&game.world_root);
            match &snap.trader_dat {
                Some(bytes) => {
                    if let Some(parent) = trader_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::write(&trader_path, bytes);
                }
                None => {
                    let _ = fs::remove_file(&trader_path);
                }
            }
            game.creatures.reset_trader();
            game.load_trader_data();
        }
        RollbackOrigin::ScrubSnapshot { .. } => {
            if let Some(tag) = &snap.trader_tag {
                game.creatures.reset_trader();
                game.creatures.trader.load(tag.get("data"));
            } else if game.creatures.trader.to_save().is_some() {
                game.creatures.reset_trader();
                game.creatures.trader.load(None);
            }
        }
    }
    bench_accum(Phase::Bosses, t_bosses);

    let t_mca = std::time::Instant::now();
    let mut deleted_region_files = false;
    let mut restored_unloaded_count = 0usize;
    if let RollbackOrigin::DiskSlot(_) = origin {
        let subfolders = ["region", "entities", "poi"];
        let dims = [
            "dimensions/minecraft/overworld",
            "dimensions/minecraft/the_nether",
            "dimensions/minecraft/the_end",
        ];
        for dim_path in dims {
            for sub in subfolders {
                let rdir = game.world_root.join(dim_path).join(sub);
                if let Ok(entries) = fs::read_dir(rdir) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_file() && !snap.region_files.contains(&p) {
                            let _ = fs::remove_file(&p);
                            deleted_region_files = true;
                        }
                    }
                }
            }
        }
    } else if let RollbackOrigin::ScrubSnapshot {
        ref touched_unloaded,
        ref future_region_files,
        ..
    } = origin
    {
        if !touched_unloaded.is_empty()
            && (policy.blocks || policy.biomes || policy.block_entities || policy.entities_presence)
        {
            let mut by_dim: HashMap<Dim, Vec<&UnloadedChunkRestore>> = HashMap::new();
            for item in touched_unloaded {
                by_dim.entry(item.dim).or_default().push(item);
            }
            let saved_dim = game.dim;
            for (dim, items) in by_dim {
                if game.dim != dim && game.enter_level(dim).is_err() {
                    continue;
                }
                let mut temp_keys = Vec::new();
                let mut temp_items: Vec<&UnloadedChunkRestore> = Vec::new();
                for item in items {
                    if let Some(chunk) = &item.chunk {
                        if !game.world.chunks.contains_key(&item.pos) {
                            game.world.chunks.insert(item.pos, Arc::clone(chunk));
                            if !item.block_entities.is_empty() {
                                game.world
                                    .block_entities
                                    .load_chunk(item.block_entities.clone());
                            }
                            for (pos, name) in &item.block_names {
                                game.world.block_names.insert(*pos, name.clone());
                            }
                            temp_keys.push(item.pos);
                            temp_items.push(item);
                        }
                    }
                }
                if !temp_keys.is_empty() {
                    if policy.blocks || policy.biomes || policy.block_entities {
                        let chunks_to_write = game.with_entities(
                            temp_keys.iter().filter_map(|k| game.world.chunks.get(k)),
                        );
                        let _ = game.save.save_later(
                            &chunks_to_write,
                            game.pack.assets.registry,
                            game.res.biomes(),
                        );
                    }
                    if policy.entities_presence {
                        let saved_mobs = std::mem::take(&mut game.mobs);
                        let saved_dropped = std::mem::take(&mut game.dropped);
                        let saved_orbs = std::mem::take(&mut game.orbs);
                        let saved_proj = std::mem::take(&mut game.creatures.projectiles);
                        let saved_boats = std::mem::take(&mut game.boats);
                        let saved_carts = std::mem::take(&mut game.carts);
                        let saved_tnts = std::mem::take(&mut game.tnts);
                        let saved_falling = std::mem::take(&mut game.falling);
                        let saved_paintings = std::mem::take(&mut game.paintings);
                        let saved_frames = std::mem::take(&mut game.item_frames);
                        let saved_stands = std::mem::take(&mut game.armor_stands);
                        let saved_cushions = std::mem::take(&mut game.cushions);
                        let saved_others = std::mem::take(&mut game.others);
                        let saved_crystals = std::mem::take(&mut game.crystals);
                        let saved_tridents = std::mem::take(&mut game.tridents);
                        let saved_rockets = std::mem::take(&mut game.rockets);
                        let saved_breath = std::mem::take(&mut game.breath_clouds);
                        let saved_fangs = std::mem::take(&mut game.fangs.list);
                        let saved_ominous = std::mem::take(&mut game.ominous_spawners);
                        let saved_persist = std::mem::take(&mut game.persist);
                        let saved_next = game.next_entity;

                        let registry = game.pack.assets.registry;
                        for item in &temp_items {
                            let ent = &item.entities;
                            for m in &ent.mobs {
                                game.mobs.push(m.clone());
                            }
                            for d in &ent.dropped {
                                game.dropped.push(d.clone());
                            }
                            for o in &ent.orbs {
                                game.orbs.push(o.clone());
                            }
                            for p in &ent.projectiles {
                                game.creatures.projectiles.push(p.clone());
                            }
                            for b in &ent.boats {
                                game.boats.push(b.clone());
                            }
                            for c in &ent.carts {
                                game.carts.push(c.clone());
                            }
                            for t in &ent.tnts {
                                game.tnts.push(t.clone());
                            }
                            for f in &ent.falling {
                                game.falling.push(f.clone());
                            }
                            for p in &ent.paintings {
                                game.paintings.push(p.clone());
                            }
                            for f in &ent.item_frames {
                                game.item_frames.push(f.clone());
                            }
                            for a in &ent.armor_stands {
                                game.armor_stands.push(a.clone());
                            }
                            for c in &ent.cushions {
                                game.cushions.push(c.clone());
                            }
                            for o in &ent.others {
                                game.others.push(o.clone());
                            }
                            for c in &ent.crystals {
                                game.crystals.push(c.clone());
                            }
                            for ts in &ent.tridents {
                                if let Some(n) = horizon_client::entity_nbt::trident_from_nbt(&ts.nbt)
                                {
                                    let mut t = horizon_gameplay::trident::ThrownTrident::from_nbt(
                                        n, &ts.nbt, registry,
                                    );
                                    t.id = ts.id;
                                    t.previous = ts.previous;
                                    t.in_ground_time = ts.in_ground_time;
                                    t.no_physics = ts.no_physics;
                                    t.owner = ts.owner;
                                    t.guest = ts.guest;
                                    game.tridents.push(t);
                                }
                            }
                            for rs in &ent.rockets {
                                if let Some(rn) =
                                    horizon_client::entity_nbt::firework_rocket_from_nbt(
                                        &rs.nbt, None,
                                    )
                                {
                                    let mut r = horizon_gameplay::fireworks::Rocket::from_nbt(rn);
                                    r.id = rs.id;
                                    r.attached = rs.attached;
                                    r.guest = rs.guest;
                                    r.owner = rs.owner;
                                    r.attached_to = rs.attached_to;
                                    game.rockets.push(r);
                                }
                            }
                            for bs in &ent.breath_clouds {
                                if let Some((_, mut cloud)) =
                                    super::super::potions::potion_cloud_from_nbt(&bs.nbt)
                                {
                                    cloud.id = bs.id;
                                    cloud.motion = bs.motion;
                                    cloud.from_dragon = bs.from_dragon;
                                    cloud.victims = bs.victims.clone();
                                    game.breath_clouds.push(cloud);
                                }
                            }
                            for f in &ent.fangs {
                                game.fangs.list.push(f.clone());
                            }
                            for (oid, onbt) in &ent.ominous_spawners {
                                game.load_ominous_item_spawner(onbt.clone());
                                if !game.ominous_spawners.is_empty() {
                                    let last = game.ominous_spawners.len() - 1;
                                    game.ominous_spawners.update(last, |o| o.id = *oid);
                                }
                            }
                            for (&id, &u) in &ent.persist_uuids {
                                game.persist.uuids.insert(id, u);
                            }
                            for (&id, tag) in &ent.persist_raw {
                                game.persist.raw.insert(id, tag.clone());
                            }
                            for (&id, slots) in &ent.persist_passenger_slots {
                                game.persist.passenger_slots.insert(id, slots.clone());
                            }
                        }

                        let key_set: HashSet<(i32, i32)> = temp_keys.iter().copied().collect();
                        let ent_chunks = game.entity_chunks(&key_set, false);
                        let _ = game.save.save_entities_later(ent_chunks);

                        game.mobs = saved_mobs;
                        game.dropped = saved_dropped;
                        game.orbs = saved_orbs;
                        game.creatures.projectiles = saved_proj;
                        game.boats = saved_boats;
                        game.carts = saved_carts;
                        game.tnts = saved_tnts;
                        game.falling = saved_falling;
                        game.paintings = saved_paintings;
                        game.item_frames = saved_frames;
                        game.armor_stands = saved_stands;
                        game.cushions = saved_cushions;
                        game.others = saved_others;
                        game.crystals = saved_crystals;
                        game.tridents = saved_tridents;
                        game.rockets = saved_rockets;
                        game.breath_clouds = saved_breath;
                        game.fangs.list = saved_fangs;
                        game.ominous_spawners = saved_ominous;
                        game.persist = saved_persist;
                        game.next_entity = saved_next;
                    }
                    let _ = game.save.flush();
                    restored_unloaded_count += temp_keys.len();
                    for k in &temp_keys {
                        let _ = game.world.unload_chunk(*k);
                        game.world.block_entities.unload_chunks(&[*k]);
                    }
                }
            }
            if game.dim != saved_dim {
                let _ = game.enter_level(saved_dim);
            }
        }
        for p in future_region_files {
            if p.is_file() {
                let _ = fs::remove_file(p);
                deleted_region_files = true;
            }
        }
    }
    bench_accum(Phase::FutureMca, t_mca);

    if target_dim != game.dim
        || deleted_region_files
        || restored_unloaded_count > 0
        || disk_files_changed > 0
        || !is_hot_rollback
    {
        let t_stream = std::time::Instant::now();
        if let RollbackOrigin::DiskSlot(_) = origin {
            restore_offline_files(&snap.offline_player_data, policy, is_scrub);
        }
        game.streaming.wait(std::time::Duration::from_millis(2));
        if game.dim != target_dim {
            let _ = game.enter_level(target_dim);
        }
        if let Ok((mut fresh_streaming, fresh_save, _)) = super::super::world_loading::make_streaming(
            &game.res,
            &game.pack,
            &game.options,
            &game.settings,
            &game.world_root,
            target_dim,
            game.world.shape,
            game.level.seed,
            &game.level.enabled_features,
            game.res.biomes().clone(),
            None,
        ) {
            fresh_streaming.center = game.streaming.center;
            fresh_streaming.extra = game.streaming.extra.clone();
            fresh_streaming.mesh = game.streaming.mesh;
            fresh_streaming.paused = game.streaming.paused;
            if let Some(tables) = game.save.tables.get() {
                let _ = fresh_save.tables.set(Arc::clone(tables));
            } else {
                let tables = Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
                let _ = fresh_save.tables.set(tables);
            }
            game.streaming = fresh_streaming;
            game.save = fresh_save;
        }
        bench_accum(Phase::SaveDirStreaming, t_stream);
    }

    if game.dim != target_dim {
        let _ = game.enter_level(target_dim);
    }

    let saved_dims: HashSet<Dim> = snap.levels.keys().copied().collect();
    game.levels.parked.retain(|dim, _| saved_dims.contains(dim));
    for parked in game.levels.parked.values_mut() {
        if policy.server_world_border {
            parked.border_sent = None;
        }
        parked.lan_states.clear();
    }

    let other_dims: Vec<Dim> = snap
        .levels
        .keys()
        .copied()
        .filter(|&d| d != target_dim)
        .collect();

    let mut acc_blocks_ms = 0.0;
    let mut acc_containers_ms = 0.0;
    let mut acc_entities_ms = 0.0;

    for dim in other_dims {
        if let Some(level_snap) = snap.levels.get(&dim) {
            if game.enter_level(dim).is_ok() {
                if need_force_disk || !is_hot_rollback || disk_files_changed > 0 {
                    let new_save = horizon_client::save::SaveDir::new(
                        game.save.root.clone(),
                        game.save.shape,
                    );
                    if let Some(tables) = game.save.tables.get() {
                        let _ = new_save.tables.set(Arc::clone(tables));
                    } else {
                        let tables =
                            Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
                        let _ = new_save.tables.set(tables);
                    }
                    game.save = Arc::new(new_save);
                }
                let (b, c, e) =
                    apply_level_snapshot_in_place(game, dim, level_snap, policy, snap.game_time, is_scrub);
                sync_streaming_lit_from_world(game);
                acc_blocks_ms += b;
                acc_containers_ms += c;
                acc_entities_ms += e;
            }
        }
    }
    if game.dim != target_dim {
        let _ = game.enter_level(target_dim);
        if need_force_disk || !is_hot_rollback || disk_files_changed > 0 {
            let new_save = horizon_client::save::SaveDir::new(
                game.save.root.clone(),
                game.save.shape,
            );
            if let Some(tables) = game.save.tables.get() {
                let _ = new_save.tables.set(Arc::clone(tables));
            } else {
                let tables = Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
                let _ = new_save.tables.set(tables);
            }
            game.save = Arc::new(new_save);
        }
    }

    if let Some(level_snap) = snap.levels.get(&target_dim) {
        let (b, c, e) =
            apply_level_snapshot_in_place(game, target_dim, level_snap, policy, snap.game_time, is_scrub);
        acc_blocks_ms += b;
        acc_containers_ms += c;
        acc_entities_ms += e;
    } else {
        let (px, pz) = game
            .player
            .as_ref()
            .map(|p| ((p.position.x as i32) >> 4, (p.position.z as i32) >> 4))
            .or_else(|| {
                game.lan
                    .as_ref()
                    .and_then(|h| h.hub.guests.first())
                    .map(|g| ((g.position[0] as i32) >> 4, (g.position[2] as i32) >> 4))
            })
            .or_else(|| {
                snap.guests.values().next().map(|g| ((g.position[0] as i32) >> 4, (g.position[2] as i32) >> 4))
            })
            .unwrap_or((0, 0));

        let mut candidate_chunks = game.world.modified.clone();
        for dz in -3..=3 {
            for dx in -3..=3 {
                candidate_chunks.insert((px + dx, pz + dz));
            }
        }

        for (cx, cz) in candidate_chunks {
            if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)) {
                if let Some(disk_chunk) = game.save.load(cx, cz, game.pack.assets.registry, game.res.biomes()) {
                    let min_y = disk_chunk.min_y();
                    let mut chunk_diff = false;
                    for (s_idx, disk_sec) in disk_chunk.sections.iter().enumerate() {
                        let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                            continue;
                        };
                        if disk_sec.blocks == live_sec.blocks {
                            continue;
                        }
                        chunk_diff = true;
                        let y_base = min_y + (s_idx as i32) * 16;
                        let x_base = cx * 16;
                        let z_base = cz * 16;
                        for sy in 0..16 {
                            for sz in 0..16 {
                                for sx in 0..16 {
                                    let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                                    let want = disk_sec.get(idx);
                                    if live_sec.get(idx) != want {
                                        let pos = (x_base + sx, y_base + sy, z_base + sz);
                                        if let Some(log) = game.world.block_log.as_mut() {
                                            log.push(pos);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if chunk_diff {
                        game.world.chunks.insert((cx, cz), Arc::new(disk_chunk));
                    }
                }
            }
        }
        game.world.modified.clear();
    }
    sync_streaming_lit_from_world(game);

    let t_player = std::time::Instant::now();
    restore_players_on_rollback(game, snap, policy, prev_dim, target_dim, is_scrub);
    bench_accum(Phase::PlayersOnline, t_player);
    let player_ms = (std::time::Instant::now() - t_player).as_secs_f64() * 1000.0;
    let total_ms = (std::time::Instant::now() - t_start).as_secs_f64() * 1000.0;

    with_state_mut(|st| {
        st.last_save_tick = snap.game_time;
        st.pending_death_rollback = None;
        st.current_intensity = 0.0;
        st.peak_intensity = 0.0;
        st.relax_ticks = 0;
        st.last_benchmark = Some(RollbackBenchmark {
            total_ms,
            globals_ms,
            blocks_ms: acc_blocks_ms,
            containers_ms: acc_containers_ms,
            entities_ms: acc_entities_ms,
            player_ms,
        });
    });

    let t_cs = std::time::Instant::now();
    let slot_key = match origin {
        RollbackOrigin::DiskSlot(slot) => {
            if need_force_disk || !snap.levels.contains_key(&target_dim) {
                let mut updated = (*snap).clone();
                updated.disk_restored = true;
                if !updated.levels.contains_key(&target_dim) {
                    updated
                        .levels
                        .insert(target_dim, capture_level_snapshot(game, target_dim));
                }
                set_slot_snapshot(slot, Arc::new(updated));
            }
            slot
        }
        RollbackOrigin::ScrubSnapshot { .. } => "scrub",
    };

    let client_snap = ClientBlockSnapshot {
        dim: game.dim,
        chunks: (&*game.world.chunks).clone(),
    };
    set_client_slot_snapshot(slot_key, client_snap);
    bump_rollback_epoch();
    bench_accum(Phase::ClientSnapUpdate, t_cs);

    let bench_report = bench_end_and_log();
    log::info!("{}", bench_report);
    append_to_rreturn_log(&game.world_root, &bench_report);

    if let Ok(mut tr) = loop_tracker().write() {
        tr.on_rollback(snap.game_time);
    }

    true
}
