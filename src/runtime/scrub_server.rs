use super::super::{Dim, Game};
use super::capture::snapshot_stopwatches_elapsed;
use super::det_rng::*;
use super::rrtl::*;
use super::scrub_perf::*;
use super::scrub_server_diff::*;
use super::scrub_server_journal::*;
use super::scrub_server_types::*;
use super::snapshots::*;
use super::types::*;
use horizon_client::{
    chunk::Chunk,
    containers::BlockEntity,
    fast_hash::FxHashMap,
    gamerules::GameRules,
    items::ItemStack,
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
    ticks::ChunkTicks,
};
use horizon_world::level::Ticks;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

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
            (anchors, st.scrub_policy)
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
            det_rng_uniquifier: current_det_rng_uniquifier(),
            ordered_state: game.rreturn_capture_ordered_state(),
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
        let max_limit = scrub_max_history_ticks().min(self.max_ticks);
        while self.total_ticks > max_limit {
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
                    died: (*lrec.died).clone(),
                },
            );
        }

        let guests: HashMap<u128, GuestSnapshot> = rec
            .guests
            .iter()
            .map(|g| (g.uuid, g.to_guest_snapshot(rec.game_time)))
            .collect();

        let live_scrub_policy = with_state(|st| st.scrub_policy);
        Some(Arc::new(TimelineSnapshot {
            anchor_ids: (*rec.anchor_ids).clone(),
            policy: live_scrub_policy,
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
            ordered_state: rec.ordered_state.clone(),
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
            det_rng_uniquifier: rec.det_rng_uniquifier,
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
    record_server_tick_mode(game, false);
}

pub fn record_initial_server_tick(game: &Game<'_>) {
    record_server_tick_mode(game, true);
}

fn record_server_tick_mode(game: &Game<'_>, only_if_empty: bool) {
    if !with_state(|st| st.scrub_enabled) {
        return;
    }
    if !only_if_empty && !game.clock.runs_normally() {
        return;
    }
    SERVER_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        if only_if_empty && !j.segments.is_empty() {
            return;
        }
        j.record_tick_inner(game);
        update_cached_server_range(j.time_range());
    });
}

pub fn handle_server_chunk_unload(game: &mut Game<'_>, dimension: &str, chunk: [i32; 2]) {
    if !with_state(|st| st.scrub_enabled) {
        return;
    }
    let dim = Dim::from_id(dimension);
    let now = game.world.game_time;
    SERVER_JOURNAL.with(|cell| {
        cell.borrow_mut()
            .unloaded
            .on_chunk_unloaded(game, dim, chunk[0], chunk[1], now);
    });
}

#[derive(Clone)]
struct CachedServerScrub {
    target_tick: i64,
    result: (Arc<TimelineSnapshot>, Vec<UnloadedChunkRestore>, Vec<PathBuf>),
}

thread_local! {
    static CACHED_SERVER_SCRUB: RefCell<Option<CachedServerScrub>> = const { RefCell::new(None) };
}

pub fn clear_cached_materialization() {
    CACHED_SERVER_SCRUB.with(|c| *c.borrow_mut() = None);
}

pub fn pre_materialize_server_scrub(game: &Game<'_>, target_tick: i64) {
    let already_cached = CACHED_SERVER_SCRUB.with(|c| {
        c.borrow().as_ref().map_or(false, |m| m.target_tick == target_tick)
    });
    if already_cached {
        return;
    }
    let _ = materialize_server_scrub(game, target_tick);
}

pub fn materialize_server_scrub(
    game: &Game<'_>,
    target_tick: i64,
) -> Option<(Arc<TimelineSnapshot>, Vec<UnloadedChunkRestore>, Vec<PathBuf>)> {
    if let Some(cached) = CACHED_SERVER_SCRUB.with(|c| {
        c.borrow().as_ref().and_then(|m| {
            if m.target_tick == target_tick {
                Some(m.result.clone())
            } else {
                None
            }
        })
    }) {
        return Some(cached);
    }
    SERVER_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        let snap = j.materialize_tick(target_tick)?;
        let (touched_unloaded, future_region_files) =
            j.unloaded.build_unloaded_restores(game, snap.game_time);
        let res = (snap, touched_unloaded, future_region_files);
        CACHED_SERVER_SCRUB.with(|c| {
            *c.borrow_mut() = Some(CachedServerScrub {
                target_tick,
                result: res.clone(),
            });
        });
        Some(res)
    })
}

pub fn get_server_snapshot_for_tick(target_tick: i64) -> Option<Arc<TimelineSnapshot>> {
    SERVER_JOURNAL.with(|cell| cell.borrow().materialize_tick(target_tick))
}

pub fn truncate_server_scrub_after(max_tick: i64) {
    clear_cached_materialization();
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
    clear_cached_materialization();
    SERVER_JOURNAL.with(|cell| {
        let mut j = cell.borrow_mut();
        j.clear_inner();
        update_cached_server_range(None);
    });
}
