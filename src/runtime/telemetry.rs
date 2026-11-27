use super::super::Game;
use super::types::*;

fn fnv_mix_u64(h: &mut u64, v: u64) {
    *h ^= v;
    *h = h.wrapping_mul(0x1000_0000_01b3);
}

fn fnv_mix_bytes(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h ^= u64::from(b);
        *h = h.wrapping_mul(0x1000_0000_01b3);
    }
}

/// Computes an entity-ID-independent FNV-1a digest over server mobs, drops, and primed TNT.
fn server_entities_digest(game: &Game<'_>) -> String {
    let mut item_hashes: Vec<u64> = Vec::with_capacity(game.mobs.len() + game.dropped.len() + game.tnts.len());
    for m in game.mobs.iter() {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        fnv_mix_u64(&mut h, 1);
        fnv_mix_bytes(&mut h, m.kind.id.as_bytes());
        fnv_mix_u64(&mut h, m.position.x.to_bits());
        fnv_mix_u64(&mut h, m.position.y.to_bits());
        fnv_mix_u64(&mut h, m.position.z.to_bits());
        fnv_mix_u64(&mut h, m.velocity.x.to_bits());
        fnv_mix_u64(&mut h, m.velocity.y.to_bits());
        fnv_mix_u64(&mut h, m.velocity.z.to_bits());
        fnv_mix_u64(&mut h, u64::from(m.health.to_bits()));
        fnv_mix_u64(&mut h, m.age as u64);
        fnv_mix_u64(&mut h, m.random.state());
        fnv_mix_u64(&mut h, u64::from(m.removed));
        item_hashes.push(h);
    }
    for d in game.dropped.iter() {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        fnv_mix_u64(&mut h, 2);
        fnv_mix_bytes(&mut h, d.stack.item.as_bytes());
        fnv_mix_u64(&mut h, u64::from(d.stack.count));
        fnv_mix_u64(&mut h, d.position.x.to_bits());
        fnv_mix_u64(&mut h, d.position.y.to_bits());
        fnv_mix_u64(&mut h, d.position.z.to_bits());
        fnv_mix_u64(&mut h, d.ticks as u64);
        fnv_mix_u64(&mut h, d.age as u64);
        fnv_mix_u64(&mut h, d.random.state());
        item_hashes.push(h);
    }
    for t in game.tnts.iter() {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        fnv_mix_u64(&mut h, 3);
        fnv_mix_u64(&mut h, t.position.x.to_bits());
        fnv_mix_u64(&mut h, t.position.y.to_bits());
        fnv_mix_u64(&mut h, t.position.z.to_bits());
        fnv_mix_u64(&mut h, t.velocity.x.to_bits());
        fnv_mix_u64(&mut h, t.velocity.y.to_bits());
        fnv_mix_u64(&mut h, t.velocity.z.to_bits());
        fnv_mix_u64(&mut h, t.fuse as u64);
        item_hashes.push(h);
    }
    item_hashes.sort_unstable();
    let mut out: u64 = 0xcbf2_9ce4_8422_2325;
    for sig in item_hashes {
        fnv_mix_u64(&mut out, sig);
    }
    format!("{out:016x}")
}

pub fn handle_bench_subcommand() -> String {
    let bench = with_state(|st| st.last_benchmark);
    match bench {
        Some(b) => {
            format!(
                "[RReturn Benchmark] Last Rollback: {:.2} ms\n  ├── Blocks & Chunks:     {:.2} ms ({:.1}%)\n  ├── Entities & Mobs:     {:.2} ms ({:.1}%)\n  ├── Block Entities:      {:.2} ms ({:.1}%)\n  ├── Player & Network:    {:.2} ms ({:.1}%)\n  └── Globals & Weather:   {:.2} ms ({:.1}%)",
                b.total_ms,
                b.blocks_ms,
                if b.total_ms > 0.0 { (b.blocks_ms / b.total_ms) * 100.0 } else { 0.0 },
                b.entities_ms,
                if b.total_ms > 0.0 { (b.entities_ms / b.total_ms) * 100.0 } else { 0.0 },
                b.containers_ms,
                if b.total_ms > 0.0 { (b.containers_ms / b.total_ms) * 100.0 } else { 0.0 },
                b.player_ms,
                if b.total_ms > 0.0 { (b.player_ms / b.total_ms) * 100.0 } else { 0.0 },
                b.globals_ms,
                if b.total_ms > 0.0 { (b.globals_ms / b.total_ms) * 100.0 } else { 0.0 }
            )
        }
        None => "[RReturn Benchmark] No rollback has been executed yet.".to_owned(),
    }
}

pub fn report_metrics_json(game: &Game<'_>) -> String {
    let (intensity, peak, relax, last_boss) = with_state(|st| {
        (
            st.current_intensity,
            st.peak_intensity,
            st.relax_ticks,
            st.last_boss_death_tick,
        )
    });

    let (player_health, player_food) = game
        .lan
        .as_ref()
        .and_then(|h| h.hub.guests.iter().find(|g| g.placed))
        .map(|g| (g.health, g.food))
        .unwrap_or((20.0, 20));

    let threat_level = (intensity / 10.0).clamp(0.0, 1.0);
    let is_safe = threat_level < 0.25 && player_health >= 15.0;

    let rec = if threat_level > 0.7 || player_health < 6.0 {
        "critical_danger"
    } else if threat_level > 0.25 {
        "combat_active"
    } else {
        "safe_to_save"
    };

    serde_json::to_string_pretty(&serde_json::json!({
        "threat_level": threat_level,
        "combat_intensity": intensity,
        "peak_intensity": peak,
        "relax_ticks": relax,
        "ticks_since_boss_defeat": game.world.game_time - last_boss,
        "player_health": player_health,
        "player_food": player_food,
        "safe_to_save": is_safe,
        "recommended_action": rec,
    }))
    .unwrap_or_else(|_| "{}".to_owned())
}

pub fn report_status_json(game: &Game<'_>) -> String {
    let now = game.world.game_time;
    let online: Vec<u128> = game
        .lan
        .as_ref()
        .map(|h| {
            h.hub
                .guests
                .iter()
                .filter(|g| g.placed)
                .map(|g| g.uuid)
                .collect()
        })
        .unwrap_or_default();

    let (
        has_cp,
        death_rollback,
        last_save,
        anchor_mode,
        cooldown_sec,
        auto_save,
        adaptive,
        save_on_boss_defeat,
        interval_sec,
        intensity,
        peak,
        current_slot,
        scrub_enabled,
    ) = with_state(|st| {
        (
            st.has_checkpoint,
            st.death_rollback,
            st.last_save_tick,
            st.anchor.mode.id(),
            st.anchor.rollback_cooldown_seconds,
            st.auto_save,
            st.adaptive_mode,
            st.save_on_boss_defeat,
            st.fixed_interval_sec,
            st.current_intensity,
            st.peak_intensity,
            st.current_slot.clone(),
            st.scrub_enabled,
        )
    });

    let player_json = game
        .lan
        .as_ref()
        .and_then(|h| h.hub.guests.iter().find(|g| g.placed))
        .map(|g| {
            serde_json::json!({
                "uuid": format!("{:032x}", g.uuid),
                "name": g.name,
                "dimension": g.dimension,
                "position": g.position,
                "health": g.health,
                "food": g.food,
                "game_mode": match g.game_mode {
                    1 => "creative",
                    2 => "adventure",
                    3 => "spectator",
                    _ => "survival",
                },
                "on_ground": g.on_ground,
                "flying": g.abilities.flying,
            })
        });

    let anchors: Vec<String> = with_state(|st| {
        resolve_anchors(&st.anchor, &online, now, None)
            .into_iter()
            .map(|u| format!("{:032x}", u))
            .collect()
    });

    let entities_digest = server_entities_digest(game);
    serde_json::to_string_pretty(&serde_json::json!({
        "rreturn_version": MOD_VERSION,
        "has_checkpoint": has_cp,
        "death_rollback": death_rollback,
        "current_slot": current_slot,
        "game_time": now,
        "day_time": game.world.day_time,
        "clock_frozen": game.clock.frozen(),
        "clock_sprinting": game.clock.sprinting(),
        "det_rng_uniquifier": det_rng_raw_state().1,
        "det_rng_steps": det_rng_step_count(),
        "level_rng": game.world.random.state(),
        "level_rng_gaussian": game.world.random.cached_gaussian().map(f64::to_bits),
        "random_tick_value": game.block_ticks.random_tick_value,
        "loaded_chunks": game.world.chunks.len(),
        "streaming_busy": game.streaming.busy(),
        "view_arriving": game.view_arriving,
        "lit_chunks": game.world.chunks.values().filter(|c| c.lit).count(),
        "server_mobs": game.mobs.len(),
        "server_dropped": game.dropped.len(),
        "server_tnts": game.tnts.len(),
        "server_blocks_queued": game.block_ticks.blocks.all_queued().count(),
        "server_fluids_queued": game.block_ticks.fluids.all_queued().count(),
        "server_entities_digest": entities_digest,
        "last_save_tick": last_save,
        "checkpoint_age_ticks": if has_cp { now - last_save } else { -1 },
        "anchor_mode": anchor_mode,
        "active_anchors": anchors,
        "rollback_cooldown_seconds": cooldown_sec,
        "auto_save": auto_save,
        "adaptive_mode": adaptive,
        "save_on_boss_defeat": save_on_boss_defeat,
        "fixed_interval_seconds": interval_sec,
        "scrub_enabled": scrub_enabled,
        "combat_intensity": intensity,
        "peak_intensity": peak,
        "player": player_json,
        "last_benchmark": with_state(|st| st.last_benchmark),
    }))
    .unwrap_or_else(|_| "{}".to_owned())
}

pub fn report_status(game: &Game<'_>) -> String {
    let online: Vec<u128> = game
        .lan
        .as_ref()
        .map(|h| {
            h.hub
                .guests
                .iter()
                .filter(|g| g.placed)
                .map(|g| g.uuid)
                .collect()
        })
        .unwrap_or_default();
    let now = game.world.game_time;
    with_state(|st| {
        let anchors = resolve_anchors(&st.anchor, &online, now, None);
        let cd_text = if st.anchor.rollback_cooldown_seconds <= 0 {
            "disabled".to_owned()
        } else {
            let rem = st
                .last_rollback_tick
                .map(|last| {
                    (i64::from(st.anchor.rollback_cooldown_seconds) * 20 - (now - last)).max(0) / 20
                })
                .unwrap_or(0);
            format!(
                "{} s (ready in {} s)",
                st.anchor.rollback_cooldown_seconds, rem
            )
        };
        format!(
            "[RReturn] Slot: {} | Key player mode: {} | rollback cooldown: {} | active key players: {} | has_checkpoint: {}",
            st.current_slot,
            st.anchor.mode.id(),
            cd_text,
            anchors.len(),
            st.has_checkpoint
        )
    })
}