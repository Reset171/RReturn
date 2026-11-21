use super::scrub_timeline::with_scrub_ring;
use super::super::Game;
use super::benchmark::*;
use super::snapshots::*;
use horizon_client::{
    chunk::Chunk,
    fast_hash::FxHashMap,
};
use horizon_world::registry::BlockStateId;
use std::{
    collections::HashSet,
    sync::Arc,
};

pub fn apply_client_level_rollback(game: &mut Game<'_>, slot: &str) {
    let t_start = std::time::Instant::now();
    let _ = game.stop_remote_mining();
    if let Some(r) = game.remote.as_mut() {
        r.rreturn_reset_client_state();
    }
    for (_, sound_id) in std::mem::take(&mut game.jukebox_sounds) {
        game.audio.stop_and_release(sound_id);
    }
    game.mount_menu_reset();
    game.mount = Default::default();
    game.ender_open = None;
    game.close_sign_editor();
    game.close_dialogs();
    game.close_book();
    game.close_mod_screen();
    game.kept_block_entities.get_mut().clear();
    game.close_container();
    game.carried = None;
    game.craft_grid.fill(None);
    game.craft_size = 2;
    game.station = None;
    game.container = None;
    game.craft_table = None;
    game.sign_edit = None;
    game.book = None;
    game.command_block_screen = None;
    game.mining = None;
    game.clicks.miss_time = 0;
    game.hotbar_pop = [0; 10];
    let prev_vignette = if game.screen_fx.vignette_brightness >= 0.99 {
        0.0
    } else {
        game.screen_fx.vignette_brightness
    };
    game.screen_fx = Default::default();
    game.screen_fx.vignette_brightness = prev_vignette;
    game.sculk.darkness = Default::default();
    game.stop_emote();
    game.clear_remote_hooks();
    game.end.clear_remote_eyes();
    game.maps = Default::default();
    game.recipe_ui.reset_screen();
    game.snap_player_model_pitch();
    game.water_plane_scan.set(None);
    game.water_plane_memo.set(None);
    game.adv.toasts.clear();
    game.cart_sounds.clear();
    game.riding_cart_sounds = None;
    game.pad.menu.stop_craft();
    game.pad_clear_focus();
    game.pad_player_replaced();
    game.far_beacons.clear();
    game.test_highlights.clear();
    game.spawner_tick_positions.clear();
    game.trial_spawner_tick_positions.clear();
    game.particles.clear();
    game.particles.sounds.clear();
    game.pickups.clear();
    game.transient_blocks.clear();
    game.destroying.clear();
    game.item_activation.reset();
    game.titles = Default::default();
    game.flash = 0;

    let client_snap = match get_client_slot_snapshot_clone(slot) {
        Some(s) => s,
        None => {
            let (px, pz) = game
                .player
                .as_ref()
                .map(|p| ((p.position.x as i32) >> 4, (p.position.z as i32) >> 4))
                .unwrap_or((0, 0));

            let mut candidate_chunks: HashSet<(i32, i32)> =
                game.world.modified.iter().copied().collect();
            for dz in -3..=3 {
                for dx in -3..=3 {
                    candidate_chunks.insert((px + dx, pz + dz));
                }
            }

            let mut diffs = Vec::new();
            let mut changed_biomes = Vec::new();
            for (cx, cz) in candidate_chunks {
                if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)) {
                    if let Some(disk_chunk) =
                        game.save
                            .load(cx, cz, game.pack.assets.registry, game.res.biomes())
                    {
                        let min_y = disk_chunk.min_y();
                        for (s_idx, disk_sec) in disk_chunk.sections.iter().enumerate() {
                            let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                                continue;
                            };
                            if disk_sec.biomes != live_sec.biomes {
                                changed_biomes.push((cx, cz, s_idx, disk_sec.biomes.clone()));
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
                                        let current = live_sec.get(idx);
                                        if current != want {
                                            diffs.push((
                                                x_base + sx,
                                                y_base + sy,
                                                z_base + sz,
                                                current,
                                                want,
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let dirty_before = game.world.dirty_sections.len();
            for &(x, y, z, _, want) in &diffs {
                let _ = game.world.set(x, y, z, BlockStateId(want));
            }
            if !changed_biomes.is_empty() {
                let num_sections = game.world.layout().sections() as i32;
                for (cx, cz, s_idx, biomes) in changed_biomes {
                    if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)).cloned() {
                        let mut chunk = (*live_chunk).clone();
                        if let Some(sec_arc) = chunk.sections.get_mut(s_idx) {
                            Arc::make_mut(sec_arc).biomes = biomes;
                        }
                        game.world.chunks.insert((cx, cz), Arc::new(chunk));
                    }
                    for dz in -1..=1 {
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                let s = s_idx as i32 + dy;
                                if (0..num_sections).contains(&s) {
                                    game.world.dirty_sections.insert((cx + dx, s, cz + dz));
                                }
                            }
                        }
                    }
                }
                game.world.revision = game.world.revision.wrapping_add(1);
            }
            let dirty_marked = game.world.dirty_sections.len().saturating_sub(dirty_before);
            let elapsed_ms = (t_start.elapsed().as_nanos() as f64) / 1_000_000.0;

            sync_client_epoch();

            let mut report = format!(
                "[RReturn][Client-Spark] === CLIENT ROLLBACK [{slot}]: {elapsed_ms:.2} ms ===\n\
                 |-- Source: Disk Fallback (RAM Cache MISS)\n\
                 |-- Blocks Changed ({}):",
                diffs.len()
            );
            for &(x, y, z, old_id, new_id) in diffs.iter().take(10) {
                let old_name = game.res.facts().get(BlockStateId(old_id)).name.clone();
                let new_name = game.res.facts().get(BlockStateId(new_id)).name.clone();
                report.push_str(&format!(
                    "\n|   |-- [{x}, {y}, {z}]: {old_name} ({old_id}) -> {new_name} ({new_id})"
                ));
            }
            if diffs.len() > 10 {
                report.push_str(&format!("\n|   |-- ... and {} more", diffs.len() - 10));
            }
            report.push_str(&format!("\n|-- Dirty Sections Marked: {dirty_marked}"));
            log::info!("{report}");
            append_to_rreturn_log(&game.world_root, &report);
            return;
        }
    };

    if game.dim != client_snap.dim {
        let elapsed_ms = (t_start.elapsed().as_nanos() as f64) / 1_000_000.0;
        let report = format!(
            "[RReturn][Client-Spark] === CLIENT ROLLBACK [{slot}]: {elapsed_ms:.2} ms ===\n\
             |-- Status: Dimension Mismatch (game: {}, snap: {})\n\
             |-- Blocks Changed: 0",
            game.dim.id(),
            client_snap.dim.id()
        );
        log::info!("{report}");
        append_to_rreturn_log(&game.world_root, &report);
        return;
    }

    let old_chunks: FxHashMap<(i32, i32), Arc<Chunk>> = (&*game.world.chunks).clone();
    let chunks_total = client_snap.chunks.len();
    let mut chunks_skipped_ptr = 0usize;
    let mut chunks_evaluated = 0usize;
    let mut sections_total = 0usize;
    let mut sections_skipped_ptr = 0usize;
    let mut sections_skipped_uniform = 0usize;
    let mut sections_skipped_eq = 0usize;
    let mut sections_evaluated = 0usize;
    let mut changed_coords = Vec::new();
    let mut changed_biomes = Vec::new();

    let (px, pz) = game
        .player
        .as_ref()
        .map(|p| ((p.position.x as i32) >> 4, (p.position.z as i32) >> 4))
        .unwrap_or((0, 0));

    let candidate_chunks: Option<HashSet<(i32, i32)>> = if slot == "scrub" {
        let mut touched = HashSet::new();
        with_scrub_ring(|ring| {
            for frame in ring.frames.iter() {
                for delta in &frame.block_deltas {
                    touched.insert((delta.x >> 4, delta.z >> 4));
                }
                for swap in &frame.section_swaps {
                    touched.insert((swap.cx, swap.cz));
                }
                for b in &frame.biome_deltas {
                    touched.insert((b.cx, b.cz));
                }
            }
        });
        for &c in game.world.modified.iter() {
            touched.insert(c);
        }
        for dz in -4..=4 {
            for dx in -4..=4 {
                touched.insert((px + dx, pz + dz));
            }
        }
        Some(touched)
    } else {
        None
    };

    let mut chunk_keys: Vec<(i32, i32)> = client_snap
        .chunks
        .keys()
        .copied()
        .filter(|coord| candidate_chunks.as_ref().map_or(true, |set| set.contains(coord)))
        .collect();
    chunk_keys.sort_unstable_by_key(|&(cx, cz)| (cx - px).abs().max((cz - pz).abs()));

    let t_scan = std::time::Instant::now();
    for &(cx, cz) in &chunk_keys {
        let snap_chunk = &client_snap.chunks[&(cx, cz)];
        if let Some(live_chunk) = old_chunks.get(&(cx, cz)) {
            if Arc::ptr_eq(live_chunk, snap_chunk) {
                chunks_skipped_ptr += 1;
                continue;
            }
            chunks_evaluated += 1;
            let min_y = snap_chunk.min_y();
            for (s_idx, snap_sec) in snap_chunk.sections.iter().enumerate() {
                sections_total += 1;
                let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                    continue;
                };
                if Arc::ptr_eq(live_sec, snap_sec) {
                    sections_skipped_ptr += 1;
                    continue;
                }
                if snap_sec.biomes != live_sec.biomes {
                    changed_biomes.push((cx, cz, s_idx, snap_sec.biomes.clone()));
                }
                if let (Some(u1), Some(u2)) =
                    (snap_sec.blocks.uniform(), live_sec.blocks.uniform())
                {
                    if u1 == u2 {
                        sections_skipped_uniform += 1;
                        continue;
                    }
                }
                if snap_sec.blocks == live_sec.blocks {
                    sections_skipped_eq += 1;
                    continue;
                }
                sections_evaluated += 1;

                let y_base = min_y + (s_idx as i32) * 16;
                let x_base = cx * 16;
                let z_base = cz * 16;
                for sy in 0..16 {
                    for sz in 0..16 {
                        for sx in 0..16 {
                            let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                            let want = snap_sec.get(idx);
                            let current = live_sec.get(idx);
                            if current != want {
                                changed_coords.push((
                                    x_base + sx,
                                    y_base + sy,
                                    z_base + sz,
                                    current,
                                    want,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    for (&(cx, cz), live_chunk) in old_chunks.iter() {
        if !client_snap.chunks.contains_key(&(cx, cz))
            && let Some(disk_chunk) =
                game.save
                    .load(cx, cz, game.pack.assets.registry, game.res.biomes())
        {
            chunks_evaluated += 1;
            let min_y = disk_chunk.min_y();
            for (s_idx, disk_sec) in disk_chunk.sections.iter().enumerate() {
                sections_total += 1;
                let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                    continue;
                };
                if disk_sec.biomes != live_sec.biomes {
                    changed_biomes.push((cx, cz, s_idx, disk_sec.biomes.clone()));
                }
                if disk_sec.blocks == live_sec.blocks {
                    sections_skipped_eq += 1;
                    continue;
                }
                sections_evaluated += 1;
                let y_base = min_y + (s_idx as i32) * 16;
                let x_base = cx * 16;
                let z_base = cz * 16;
                for sy in 0..16 {
                    for sz in 0..16 {
                        for sx in 0..16 {
                            let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                            let want = disk_sec.get(idx);
                            let current = live_sec.get(idx);
                            if current != want {
                                changed_coords.push((
                                    x_base + sx,
                                    y_base + sy,
                                    z_base + sz,
                                    current,
                                    want,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    let scan_ms = (t_scan.elapsed().as_nanos() as f64) / 1_000_000.0;

    let t_set = std::time::Instant::now();
    let dirty_before = game.world.dirty_sections.len();
    for &(wx, wy, wz, _, want) in &changed_coords {
        let _ = game.world.set(wx, wy, wz, BlockStateId(want));
    }
    if !changed_biomes.is_empty() {
        let num_sections = game.world.layout().sections() as i32;
        for (cx, cz, s_idx, biomes) in changed_biomes {
            if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)).cloned() {
                let mut chunk = (*live_chunk).clone();
                if let Some(sec_arc) = chunk.sections.get_mut(s_idx) {
                    Arc::make_mut(sec_arc).biomes = biomes;
                }
                game.world.chunks.insert((cx, cz), Arc::new(chunk));
            }
            for dz in -1..=1 {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let s = s_idx as i32 + dy;
                        if (0..num_sections).contains(&s) {
                            game.world.dirty_sections.insert((cx + dx, s, cz + dz));
                        }
                    }
                }
            }
        }
        game.world.revision = game.world.revision.wrapping_add(1);
    }
    let dirty_marked = game.world.dirty_sections.len().saturating_sub(dirty_before);
    let set_ms = (t_set.elapsed().as_nanos() as f64) / 1_000_000.0;
    let elapsed_ms = (t_start.elapsed().as_nanos() as f64) / 1_000_000.0;

    sync_client_epoch();

    let mut report = format!(
        "[RReturn][Client-Spark] === CLIENT ROLLBACK [{slot}]: {elapsed_ms:.2} ms ===\n\
         |-- Phase 1 (Cache Lookup): {:.2} ms [RAM HIT | dim: {}]\n\
         |-- Phase 2 (Chunks & Sections Scan): {scan_ms:.2} ms\n\
         |   |-- Chunks: {chunks_total} total | {chunks_skipped_ptr} ptr_eq | {chunks_evaluated} eval (nearest-first)\n\
         |   |-- Sections: {sections_total} in eval | {sections_skipped_ptr} ptr_eq | {sections_skipped_uniform} uniform | {sections_skipped_eq} content_eq | {sections_evaluated} eval\n\
         |-- Phase 3 (Block World Sets): {set_ms:.2} ms (blocks changed: {})\n\
         |-- Phase 4 (Mesher Invalidation): {dirty_marked} dirty sections marked",
        (t_scan.duration_since(t_start).as_nanos() as f64) / 1_000_000.0,
        game.dim.id(),
        changed_coords.len()
    );
    if !changed_coords.is_empty() {
        report.push_str("\n|-- Changed Blocks Detail:");
        for &(x, y, z, old_id, new_id) in changed_coords.iter().take(10) {
            let old_name = game.res.facts().get(BlockStateId(old_id)).name.clone();
            let new_name = game.res.facts().get(BlockStateId(new_id)).name.clone();
            report.push_str(&format!(
                "\n|   |-- [{x}, {y}, {z}]: {old_name} ({old_id}) -> {new_name} ({new_id})"
            ));
        }
        if changed_coords.len() > 10 {
            report.push_str(&format!(
                "\n|   |-- ... and {} more",
                changed_coords.len() - 10
            ));
        }
    }
    log::info!("{report}");
    append_to_rreturn_log(&game.world_root, &report);
}