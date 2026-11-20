use std::sync::Mutex;

#[derive(Default, Clone)]
pub struct SubCounter {
    pub count: u64,
    pub total_us: u64,
    pub max_us: u64,
    pub min_us: u64,
}

impl SubCounter {
    pub fn record(&mut self, us: u64) {
        self.count += 1;
        self.total_us += us;
        self.max_us = self.max_us.max(us);
        if self.min_us == 0 || us < self.min_us {
            self.min_us = us;
        }
    }

    pub fn avg_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            (self.total_us as f64 / self.count as f64) / 1000.0
        }
    }

    pub fn max_ms(&self) -> f64 {
        self.max_us as f64 / 1000.0
    }

    pub fn min_ms(&self) -> f64 {
        self.min_us as f64 / 1000.0
    }
}

#[derive(Default)]
pub struct ServerTickPerf {
    pub total: SubCounter,
    pub unloaded_scan: SubCounter,
    pub raw_capture: SubCounter,
    pub raw_chunks: SubCounter,
    pub raw_mobs: SubCounter,
    pub raw_entities: SubCounter,
    pub globals_pack: SubCounter,
    pub cow_process: SubCounter,
}

#[derive(Default)]
pub struct ClientTickPerf {
    pub total: SubCounter,
    pub deltas: SubCounter,
    pub entity_pool: SubCounter,
    pub poses: SubCounter,
    pub particles: SubCounter,
    pub misc: SubCounter,
}

#[derive(Default)]
pub struct FrameRenderPerf {
    pub total: SubCounter,
    pub frame_lookup: SubCounter,
    pub frame_clone: SubCounter,
    pub mob_lerp: SubCounter,
    pub particles_sim: SubCounter,
    pub block_rewind: SubCounter,
    pub be_anims: SubCounter,
}

#[derive(Default)]
pub struct CommitPerf {
    pub total: SubCounter,
    pub materialize: SubCounter,
    pub server_rollback: SubCounter,
    pub client_rollback: SubCounter,
}

#[derive(Default)]
pub struct ScrubPerfState {
    pub server_tick: ServerTickPerf,
    pub client_tick: ClientTickPerf,
    pub frame_render: FrameRenderPerf,
    pub commit: CommitPerf,
}

static PERF_STATE: Mutex<Option<ScrubPerfState>> = Mutex::new(None);

fn with_perf<F: FnOnce(&mut ScrubPerfState)>(f: F) {
    if let Ok(mut guard) = PERF_STATE.lock() {
        let state = guard.get_or_insert_with(ScrubPerfState::default);
        f(state);
    }
}

pub fn record_server_tick_perf(
    total_us: u64,
    unloaded_us: u64,
    raw_capture_us: u64,
    globals_us: u64,
    cow_us: u64,
) {
    with_perf(|s| {
        s.server_tick.total.record(total_us);
        s.server_tick.unloaded_scan.record(unloaded_us);
        s.server_tick.raw_capture.record(raw_capture_us);
        s.server_tick.globals_pack.record(globals_us);
        s.server_tick.cow_process.record(cow_us);
    });
}

pub fn record_raw_capture_sub_perf(chunks_us: u64, mobs_us: u64, entities_us: u64) {
    with_perf(|s| {
        s.server_tick.raw_chunks.record(chunks_us);
        s.server_tick.raw_mobs.record(mobs_us);
        s.server_tick.raw_entities.record(entities_us);
    });
}

pub fn record_client_tick_perf(
    total_us: u64,
    deltas_us: u64,
    entity_pool_us: u64,
    poses_us: u64,
    particles_us: u64,
    misc_us: u64,
) {
    with_perf(|s| {
        s.client_tick.total.record(total_us);
        s.client_tick.deltas.record(deltas_us);
        s.client_tick.entity_pool.record(entity_pool_us);
        s.client_tick.poses.record(poses_us);
        s.client_tick.particles.record(particles_us);
        s.client_tick.misc.record(misc_us);
    });
}

pub fn record_frame_render_perf(
    total_us: u64,
    frame_lookup_us: u64,
    frame_clone_us: u64,
    mob_lerp_us: u64,
    particles_sim_us: u64,
    block_rewind_us: u64,
    be_anims_us: u64,
) {
    with_perf(|s| {
        s.frame_render.total.record(total_us);
        s.frame_render.frame_lookup.record(frame_lookup_us);
        s.frame_render.frame_clone.record(frame_clone_us);
        s.frame_render.mob_lerp.record(mob_lerp_us);
        s.frame_render.particles_sim.record(particles_sim_us);
        s.frame_render.block_rewind.record(block_rewind_us);
        s.frame_render.be_anims.record(be_anims_us);
    });
}

pub fn record_commit_perf(
    total_us: u64,
    mat_us: u64,
    server_rb_us: u64,
    client_rb_us: u64,
) {
    with_perf(|s| {
        s.commit.total.record(total_us);
        s.commit.materialize.record(mat_us);
        s.commit.server_rollback.record(server_rb_us);
        s.commit.client_rollback.record(client_rb_us);
    });
}

pub fn reset_perf() {
    if let Ok(mut guard) = PERF_STATE.lock() {
        *guard = Some(ScrubPerfState::default());
    }
}

pub fn get_perf_report_tree() -> String {
    let guard = match PERF_STATE.lock() {
        Ok(g) => g,
        Err(_) => return "Error locking perf state".to_owned(),
    };
    let s = match guard.as_ref() {
        Some(s) => s,
        None => return "No performance samples recorded yet".to_owned(),
    };

    let mut out = String::new();
    out.push_str("=== RReturn Scrubbing & Timeline Performance Trace Tree ===\n\n");

    // 1. Server Tick
    let s_tot = s.server_tick.total.avg_ms();
    let s_budget_pct = (s_tot / 50.0) * 100.0;
    out.push_str(&format!(
        "[Phase 1] Live Server Tick (Rate: 20 Hz, Budget: 50.00ms)\n\
         ├── Total Server Tick Overhead: {:.3}ms avg (max: {:.2}ms, min: {:.3}ms) [samples: {}, {:.1}% of tick budget]\n",
        s_tot,
        s.server_tick.total.max_ms(),
        s.server_tick.total.min_ms(),
        s.server_tick.total.count,
        s_budget_pct
    ));

    let pct = |sub: f64, tot: f64| if tot > 0.0 { (sub / tot) * 100.0 } else { 0.0 };

    let raw_ms = s.server_tick.raw_capture.avg_ms();
    out.push_str(&format!(
        "│   ├── raw_capture (Full LevelSnapshot capture): {:.3}ms ({:.1}%)\n",
        raw_ms,
        pct(raw_ms, s_tot)
    ));
    if s.server_tick.raw_chunks.count > 0 {
        let ch_ms = s.server_tick.raw_chunks.avg_ms();
        let mb_ms = s.server_tick.raw_mobs.avg_ms();
        let ent_ms = s.server_tick.raw_entities.avg_ms();
        out.push_str(&format!(
            "│   │   ├── chunks & block_entities clone: {:.3}ms ({:.1}%)\n\
             │   │   ├── mobs deep clone: {:.3}ms ({:.1}%)\n\
             │   │   └── items/orbs/tnts/projectiles clone: {:.3}ms ({:.1}%)\n",
            ch_ms,
            pct(ch_ms, raw_ms),
            mb_ms,
            pct(mb_ms, raw_ms),
            ent_ms,
            pct(ent_ms, raw_ms),
        ));
    }
    let cow_ms = s.server_tick.cow_process.avg_ms();
    out.push_str(&format!(
        "│   ├── cow_process (Chunk diffing, Arc sharing, segment write): {:.3}ms ({:.1}%)\n",
        cow_ms,
        pct(cow_ms, s_tot)
    ));
    let glob_ms = s.server_tick.globals_pack.avg_ms();
    out.push_str(&format!(
        "│   ├── globals_pack (Bossbars, Scoreboard, Sculk, NBT packs): {:.3}ms ({:.1}%)\n",
        glob_ms,
        pct(glob_ms, s_tot)
    ));
    let unl_ms = s.server_tick.unloaded_scan.avg_ms();
    out.push_str(&format!(
        "│   └── unloaded_scan (Unloaded chunks scan & tracking): {:.3}ms ({:.1}%)\n\n",
        unl_ms,
        pct(unl_ms, s_tot)
    ));

    // 2. Client Tick
    let c_tot = s.client_tick.total.avg_ms();
    out.push_str(&format!(
        "[Phase 2] Live Client Tick (Rate: 20 Hz)\n\
         ├── Total Client Tick Overhead: {:.3}ms avg (max: {:.2}ms, min: {:.3}ms) [samples: {}]\n",
        c_tot,
        s.client_tick.total.max_ms(),
        s.client_tick.total.min_ms(),
        s.client_tick.total.count
    ));
    let pool_ms = s.client_tick.entity_pool.avg_ms();
    out.push_str(&format!(
        "│   ├── entity_pool.observe (PreviewEntityPool cloning): {:.3}ms ({:.1}%)\n",
        pool_ms,
        pct(pool_ms, c_tot)
    ));
    let deltas_ms = s.client_tick.deltas.avg_ms();
    out.push_str(&format!(
        "│   ├── chunk_deltas & dim_snapshots clone: {:.3}ms ({:.1}%)\n",
        deltas_ms,
        pct(deltas_ms, c_tot)
    ));
    let poses_ms = s.client_tick.poses.avg_ms();
    out.push_str(&format!(
        "│   ├── entity_poses (MobPoseRecord & EntityPoseRecord collection): {:.3}ms ({:.1}%)\n",
        poses_ms,
        pct(poses_ms, c_tot)
    ));
    let part_ms = s.client_tick.particles.avg_ms();
    out.push_str(&format!(
        "│   ├── particles (Particle tracking & LZ4/Zlib sealing): {:.3}ms ({:.1}%)\n",
        part_ms,
        pct(part_ms, c_tot)
    ));
    let misc_ms = s.client_tick.misc.avg_ms();
    out.push_str(&format!(
        "│   └── misc (Spawner spins, arm swing, sounds prewarm): {:.3}ms ({:.1}%)\n\n",
        misc_ms,
        pct(misc_ms, c_tot)
    ));

    // 3. Frame Render
    let f_tot = s.frame_render.total.avg_ms();
    let f_budget_144 = (f_tot / 6.94) * 100.0;
    out.push_str(&format!(
        "[Phase 3] Scrubbing Frame Render (Rate: 60-144 Hz, Budget: 6.94ms @ 144 FPS / 16.6ms @ 60 FPS)\n\
         ├── Total Frame Render Time: {:.3}ms avg (max: {:.2}ms, min: {:.3}ms) [samples: {}, {:.1}% of 144FPS frame budget]\n",
        f_tot,
        s.frame_render.total.max_ms(),
        s.frame_render.total.min_ms(),
        s.frame_render.total.count,
        f_budget_144
    ));
    let f_lookup = s.frame_render.frame_lookup.avg_ms();
    out.push_str(&format!(
        "│   ├── frame_lookup (find_frame_pair & particle decode): {:.3}ms ({:.1}%)\n",
        f_lookup,
        pct(f_lookup, f_tot)
    ));
    let f_clone = s.frame_render.frame_clone.avg_ms();
    out.push_str(&format!(
        "│   ├── frame_cloning (TickHistoryFrame deep clone): {:.3}ms ({:.1}%)\n",
        f_clone,
        pct(f_clone, f_tot)
    ));
    let f_part = s.frame_render.particles_sim.avg_ms();
    out.push_str(&format!(
        "│   ├── particles_sim (interpolate_lossless_particles + particles.tick): {:.3}ms ({:.1}%)\n",
        f_part,
        pct(f_part, f_tot)
    ));
    let f_mob = s.frame_render.mob_lerp.avg_ms();
    out.push_str(&format!(
        "│   ├── mob_lerp (Interpolate poses & apply to live Game.mobs): {:.3}ms ({:.1}%)\n",
        f_mob,
        pct(f_mob, f_tot)
    ));
    let f_block = s.frame_render.block_rewind.avg_ms();
    out.push_str(&format!(
        "│   ├── block_rewind (Coalesced block/section rewind): {:.3}ms ({:.1}%)\n",
        f_block,
        pct(f_block, f_tot)
    ));
    let f_be = s.frame_render.be_anims.avg_ms();
    out.push_str(&format!(
        "│   └── be_anims (Chest lids, spawner spins, moving pistons): {:.3}ms ({:.1}%)\n\n",
        f_be,
        pct(f_be, f_tot)
    ));

    // 4. Commit
    let com_tot = s.commit.total.avg_ms();
    out.push_str(&format!(
        "[Phase 4] Scrub Commit Latency (On releasing U / confirming rewind)\n\
         ├── Total Commit Time: {:.3}ms avg (max: {:.2}ms, min: {:.3}ms) [samples: {}]\n",
        com_tot,
        s.commit.total.max_ms(),
        s.commit.total.min_ms(),
        s.commit.total.count
    ));
    let com_mat = s.commit.materialize.avg_ms();
    out.push_str(&format!(
        "│   ├── materialize_server_scrub (Segment CoW replay): {:.3}ms ({:.1}%)\n",
        com_mat,
        pct(com_mat, com_tot)
    ));
    let com_s_rb = s.commit.server_rollback.avg_ms();
    out.push_str(&format!(
        "│   ├── server_rollback (apply_timeline_snapshot_on_server): {:.3}ms ({:.1}%)\n",
        com_s_rb,
        pct(com_s_rb, com_tot)
    ));
    let com_c_rb = s.commit.client_rollback.avg_ms();
    out.push_str(&format!(
        "│   └── client_rollback (apply_client_level_rollback section diffs): {:.3}ms ({:.1}%)\n",
        com_c_rb,
        pct(com_c_rb, com_tot)
    ));

    out
}

pub fn get_perf_report_json() -> String {
    let guard = match PERF_STATE.lock() {
        Ok(g) => g,
        Err(_) => return "{}".to_owned(),
    };
    let s = match guard.as_ref() {
        Some(s) => s,
        None => return "{}".to_owned(),
    };

    serde_json::json!({
        "server_tick": {
            "count": s.server_tick.total.count,
            "avg_ms": s.server_tick.total.avg_ms(),
            "max_ms": s.server_tick.total.max_ms(),
            "min_ms": s.server_tick.total.min_ms(),
            "unloaded_scan_ms": s.server_tick.unloaded_scan.avg_ms(),
            "raw_capture_ms": s.server_tick.raw_capture.avg_ms(),
            "raw_chunks_ms": s.server_tick.raw_chunks.avg_ms(),
            "raw_mobs_ms": s.server_tick.raw_mobs.avg_ms(),
            "raw_entities_ms": s.server_tick.raw_entities.avg_ms(),
            "globals_pack_ms": s.server_tick.globals_pack.avg_ms(),
            "cow_process_ms": s.server_tick.cow_process.avg_ms(),
        },
        "client_tick": {
            "count": s.client_tick.total.count,
            "avg_ms": s.client_tick.total.avg_ms(),
            "max_ms": s.client_tick.total.max_ms(),
            "min_ms": s.client_tick.total.min_ms(),
            "deltas_ms": s.client_tick.deltas.avg_ms(),
            "entity_pool_ms": s.client_tick.entity_pool.avg_ms(),
            "poses_ms": s.client_tick.poses.avg_ms(),
            "particles_ms": s.client_tick.particles.avg_ms(),
            "misc_ms": s.client_tick.misc.avg_ms(),
        },
        "frame_render": {
            "count": s.frame_render.total.count,
            "avg_ms": s.frame_render.total.avg_ms(),
            "max_ms": s.frame_render.total.max_ms(),
            "min_ms": s.frame_render.total.min_ms(),
            "frame_lookup_ms": s.frame_render.frame_lookup.avg_ms(),
            "frame_clone_ms": s.frame_render.frame_clone.avg_ms(),
            "mob_lerp_ms": s.frame_render.mob_lerp.avg_ms(),
            "particles_sim_ms": s.frame_render.particles_sim.avg_ms(),
            "block_rewind_ms": s.frame_render.block_rewind.avg_ms(),
            "be_anims_ms": s.frame_render.be_anims.avg_ms(),
        },
        "commit": {
            "count": s.commit.total.count,
            "avg_ms": s.commit.total.avg_ms(),
            "max_ms": s.commit.total.max_ms(),
            "min_ms": s.commit.total.min_ms(),
            "materialize_ms": s.commit.materialize.avg_ms(),
            "server_rollback_ms": s.commit.server_rollback.avg_ms(),
            "client_rollback_ms": s.commit.client_rollback.avg_ms(),
        }
    }).to_string()
}
