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

pub fn rollback_slot_checkpoint_on_server(game: &mut Game<'_>, slot: &str) -> bool {
    let t_start = std::time::Instant::now();
    let prev_dim = game.dim;
    bench_begin(&format!("ROLLBACK RESTORE [{}]", slot), game.world.game_time);

    let t_flush = std::time::Instant::now();
    let _ = game.save.flush();
    for parked in game.levels.parked.values() {
        let _ = parked.save.flush();
    }
    game.world.loaded_entities.clear();
    bench_accum(Phase::FlushSave, t_flush);

    let t_hydrate = std::time::Instant::now();
    let mut snap = match get_slot_snapshot_clone(slot) {
        Some(s) => s,
        None => match hydrate_slot_from_disk_on_server(game, slot) {
            Some(s) => s,
            None => return false,
        },
    };

    let target_dim = snap
        .guests
        .values()
        .next()
        .map(|g| Dim::from_id(&g.dimension))
        .unwrap_or_else(|| {
            snap.levels.keys().copied().next().unwrap_or(Dim::Overworld)
        });

    if !snap.levels.contains_key(&target_dim) {
        if let Some(hydrated) = hydrate_slot_from_disk_on_server(game, slot) {
            snap = hydrated;
        }
    }
    bench_accum(Phase::HydrateSnapshot, t_hydrate);

    let need_force_disk = !snap.disk_restored;
    let is_hot_rollback = slot == "default"
        && game.dim == target_dim
        && snap.levels.contains_key(&game.dim)
        && !need_force_disk;
    let t_disk = std::time::Instant::now();
    let disk_files_changed =
        restore_disk_files_for_slot_opts(&game.world_root, slot, need_force_disk);
    bench_accum(Phase::RestoreDiskFiles, t_disk);

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
    let policy = &snap.policy;

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
    game.portals.player.cooldown = 80;
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
        let cp_slot_dir = slot_dir(&game.world_root, slot);
        game.score.storage =
            horizon_client::command_storage::CommandStorage::load(&cp_slot_dir);
        game.score.stopwatches =
            horizon_client::stopwatches::Stopwatches::load(&cp_slot_dir);
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
    game.adv.last_inventory = 0;
    game.adv.levitation = None;
    game.adv.marks = Default::default();
    game.adv.changed.clear();
    game.adv.unlocked.clear();
    game.adv.announced.clear();
    game.next_entity = snap.next_entity;
    game.ticked_entities_through = snap.ticked_entities_through;
    game.ordered.announced = snap.ordered_announced;
    game.creatures.cat_tick = snap.cat_tick;
    game.stats.rreturn_set_phantom_tick(snap.phantom_tick);
    game.level.player_extra = snap.player_extra.clone();
    game.creatures.shoulder_time = snap.shoulder_time.clone();
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
    game.player_attachments = snap.player_attachments.clone();
    game.guest_attachments = snap.guest_attachments.clone();
    game.native_player_fx = Default::default();
    game.native_moves.clear();
    if policy.player_stats {
        game.stats.events.clear();
    }
    game.fns.schedule = snap.scheduled_events.clone();
    game.sync_function_registries();
    game.cmds.post_effects = snap.cmd_post_effects.clone();
    game.exec = Default::default();
    game.maps = Default::default();
    game.rand_sequences = snap.rand_sequences.clone();

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
    bench_accum(Phase::Bosses, t_bosses);

    let t_mca = std::time::Instant::now();
    let subfolders = ["region", "entities", "poi"];
    let dims = [
        "dimensions/minecraft/overworld",
        "dimensions/minecraft/the_nether",
        "dimensions/minecraft/the_end",
    ];
    let mut deleted_region_files = false;
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
    bench_accum(Phase::FutureMca, t_mca);

    if target_dim != game.dim || deleted_region_files || disk_files_changed > 0 || !is_hot_rollback {
        let t_stream = std::time::Instant::now();
        restore_offline_files(&snap.offline_player_data, policy.player_stats);
        game.streaming.wait(std::time::Duration::from_millis(2));
        if let Ok((fresh_streaming, fresh_save, _)) = super::super::world_loading::make_streaming(
            &game.res,
            &game.pack,
            &game.options,
            &game.settings,
            &game.world_root,
            target_dim,
            game.world.shape,
            game.level.seed,
            &game.level.enabled_features,
        ) {
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
        let _ = game.enter_level(None, target_dim);
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
            if game.enter_level(None, dim).is_ok() {
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
                    apply_level_snapshot_in_place(game, dim, level_snap, policy, snap.game_time);
                acc_blocks_ms += b;
                acc_containers_ms += c;
                acc_entities_ms += e;
            }
        }
    }
    if game.dim != target_dim {
        let _ = game.enter_level(None, target_dim);
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
            apply_level_snapshot_in_place(game, target_dim, level_snap, policy, snap.game_time);
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

    let t_player = std::time::Instant::now();
    restore_players_on_rollback(game, &snap, prev_dim, target_dim);
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

    let client_snap = ClientBlockSnapshot {
        dim: game.dim,
        chunks: (&*game.world.chunks).clone(),
    };
    set_client_slot_snapshot(slot, client_snap);
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
