use super::super::Game;
use super::restore::{apply_timeline_snapshot_on_server, rollback_slot_checkpoint_on_server};
use super::restore_client::apply_client_level_rollback;
use super::scrub_interpolate::{
    apply_block_rewind_to_game, apply_interpolated_poses_to_game, apply_timeline_sounds_to_game,
    interpolate_mob_pose, lerp_angle, lerp_f32, lerp_f64, InterpolatedMobPose,
};
use super::scrub_server::get_server_snapshot_for_tick;
use super::scrub_timeline::{with_scrub_ring, with_scrub_ring_mut};
use super::types::*;
use horizon_client::fast_hash::FxHashMap;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub struct ScrubSession {
    pub active: bool,
    pub anchor_tick: i64,
    pub min_tick: i64,
    pub current_time: f64,
    pub target_time: f64,
    pub last_scroll_time: Option<Instant>,
    pub jog_accel: f64,
    pub start_time: Option<Instant>,
    pub applied_block_tick: i64,
    pub was_frozen_before_scrub: bool,
}

impl Default for ScrubSession {
    fn default() -> Self {
        Self {
            active: false,
            anchor_tick: 0,
            min_tick: 0,
            current_time: 0.0,
            target_time: 0.0,
            last_scroll_time: None,
            jog_accel: 1.0,
            start_time: None,
            applied_block_tick: 0,
            was_frozen_before_scrub: false,
        }
    }
}

static SESSION: OnceLock<Mutex<ScrubSession>> = OnceLock::new();

fn session_lock() -> &'static Mutex<ScrubSession> {
    SESSION.get_or_init(|| Mutex::new(ScrubSession::default()))
}

pub fn is_scrubbing() -> bool {
    session_lock().lock().map(|s| s.active).unwrap_or(false)
}

pub fn snap_scrub_time_to_target() {
    if let Ok(mut s) = session_lock().lock() {
        s.current_time = s.target_time;
    }
}

pub fn is_held_scrub(min_ms: u128) -> bool {
    let s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    if !s.active {
        return false;
    }
    s.start_time
        .map(|t| t.elapsed().as_millis() >= min_ms)
        .unwrap_or(false)
}

pub fn get_scrub_info() -> Option<(f64, i64, i64)> {
    let s = session_lock().lock().ok()?;
    if !s.active {
        return None;
    }
    Some((s.current_time, s.min_tick, s.anchor_tick))
}

pub fn step_scrub_ticks(ticks: f64) {
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if !s.active {
        return;
    }
    s.target_time = (s.target_time + ticks).clamp(s.min_tick as f64, s.anchor_tick as f64);
}

pub fn start_scrub(game: &mut Game<'_>) {
    if !has_level_4_permission(game, None) {
        game.native_chat(&tr("command.rreturn.no_permission"));
        return;
    }
    if game.remote.is_some() && game.integrated.is_none() {
        game.native_chat(&tr("command.rreturn.no_permission"));
        return;
    }
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if s.active {
        return;
    }

    let now_tick = game.world.game_time;
    let (min_tick, max_tick) = with_scrub_ring(|r| r.time_range()).unwrap_or((now_tick, now_tick));

    s.active = true;
    s.anchor_tick = now_tick.max(max_tick);
    s.min_tick = min_tick.min(s.anchor_tick);
    s.current_time = s.anchor_tick as f64;
    s.target_time = s.current_time;
    s.last_scroll_time = None;
    s.jog_accel = 1.0;
    s.start_time = Some(Instant::now());
    s.applied_block_tick = s.anchor_tick;
    s.was_frozen_before_scrub = game.clock.frozen();

    with_scrub_ring_mut(|r| {
        if r.reference_chunks.is_empty() {
            r.reference_chunks = (&*game.world.chunks).clone();
            r.last_world_revision = game.world.revision;
        }
    });

    // Stop any ongoing mining / block breaking
    game.stop_mining();

    // Freeze game clock so physics and mob ticking pause during scrub on client
    game.clock.set_frozen(true);

    // Freeze integrated server clock so server mob AI/physics pause immediately
    if let Some(server) = &game.integrated {
        server.call(|srv| {
            srv.clock.set_frozen(true);
        });
    }

    if let Some(r) = game.remote.as_mut() {
        r.stepped.clear();
        r.lerps.clear();
        r.cart_lerps.clear();
        r.head_lerps.clear();
    }

    let start_t = s.current_time;
    drop(s);
    apply_interpolated_poses_to_game(game, start_t);
}

pub fn on_mouse_scroll(_game: &mut Game<'_>, scroll: [f64; 2], accumulated: [i32; 2]) -> bool {
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    if !s.active {
        return false;
    }

    let wheel_y = if scroll[1].abs() > 1e-4 {
        scroll[1]
    } else if accumulated[1] != 0 {
        f64::from(accumulated[1])
    } else {
        return true;
    };

    let now = Instant::now();
    if let Some(prev) = s.last_scroll_time {
        let dt_ms = now.duration_since(prev).as_millis();
        if dt_ms < 60 {
            s.jog_accel = (s.jog_accel + 0.8).min(20.0);
        } else if dt_ms > 200 {
            s.jog_accel = 1.0;
        }
    } else {
        s.jog_accel = 1.0;
    }
    s.last_scroll_time = Some(now);

    let step = wheel_y * s.jog_accel * 1.5;
    s.target_time = (s.target_time + step).clamp(s.min_tick as f64, s.anchor_tick as f64);
    drop(s);

    true
}

pub fn on_frame_render_pre(game: &mut Game<'_>, _world_partial: f32, _partial: f32) {
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if !s.active {
        return;
    }

    let diff = s.target_time - s.current_time;
    if diff.abs() > 1e-3 {
        s.current_time += diff * 0.35;
    } else {
        s.current_time = s.target_time;
    }

    let target_t = s.current_time;
    let old_applied = s.applied_block_tick;
    let new_applied = target_t.round() as i64;
    s.applied_block_tick = new_applied;
    drop(s);

    apply_interpolated_poses_to_game(game, target_t);
    if old_applied != new_applied {
        apply_block_rewind_to_game(game, old_applied, new_applied);
        apply_timeline_sounds_to_game(game, old_applied, new_applied);
    }
}

pub fn commit_scrub(game: &mut Game<'_>) {
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if !s.active {
        return;
    }

    let final_tick = s.current_time.round() as i64;
    let anchor = s.anchor_tick;
    let final_time = s.current_time;
    let was_frozen = s.was_frozen_before_scrub;
    s.active = false;
    s.start_time = None;
    s.jog_accel = 1.0;
    s.applied_block_tick = final_tick;
    drop(s);

    if final_tick < anchor {
        let (mob_poses, _player_pos, _player_yaw, _player_pitch) = with_scrub_ring(|ring| {
            if let Some((left, right, alpha)) = ring.find_frame_pair(final_time) {
                let a64 = f64::from(alpha);
                let p_pos = [
                    lerp_f64(left.player_pos[0], right.player_pos[0], a64),
                    lerp_f64(left.player_pos[1], right.player_pos[1], a64),
                    lerp_f64(left.player_pos[2], right.player_pos[2], a64),
                ];
                let p_yaw = lerp_angle(left.player_yaw, right.player_yaw, alpha);
                let p_pitch = lerp_f32(left.player_pitch, right.player_pitch, alpha);

                let mut poses = Vec::new();
                let mut right_mobs_by_id = FxHashMap::default();
                for m in &right.mobs {
                    right_mobs_by_id.insert(m.id, m);
                }
                for left_mob in &left.mobs {
                    if let Some(right_mob) = right_mobs_by_id.get(&left_mob.id) {
                        poses.push((left_mob.id, interpolate_mob_pose(left_mob, right_mob, alpha)));
                    } else {
                        poses.push((left_mob.id, InterpolatedMobPose {
                            pos: left_mob.pos,
                            yaw: left_mob.yaw,
                            body_yaw: left_mob.body_yaw,
                            head_yaw: left_mob.head_yaw,
                            head_pitch: left_mob.head_pitch,
                            limb_position: left_mob.limb_position,
                            limb_speed: left_mob.limb_speed,
                            attack_anim: left_mob.attack_anim,
                            swell: left_mob.swell,
                            health: left_mob.health,
                            alive: left_mob.alive,
                            hurt_time: left_mob.hurt_time,
                            death_time: left_mob.death_time,
                        }));
                    }
                }
                (poses, p_pos, p_yaw, p_pitch)
            } else {
                (Vec::new(), [0.0; 3], 0.0, 0.0)
            }
        });

        apply_interpolated_poses_to_game(game, final_time);

        let valid_mob_ids: HashSet<u32> = mob_poses.iter().map(|(id, _)| *id).collect();
        if !valid_mob_ids.is_empty() {
            game.mobs.retain_mut(|m| valid_mob_ids.contains(&m.id));
        }

        let did_rollback = if let Some(server) = &game.integrated {
            server.call(move |srv| {
                if let Some(snap) = get_server_snapshot_for_tick(final_tick) {
                    apply_timeline_snapshot_on_server(
                        srv,
                        &snap,
                        RollbackOrigin::ScrubSnapshot {
                            target_tick: final_tick,
                            touched_unloaded: Vec::new(),
                            future_region_files: Vec::new(),
                        },
                    )
                } else {
                    false
                }
            }).unwrap_or(false)
        } else if let Some(snap) = get_server_snapshot_for_tick(final_tick) {
            apply_timeline_snapshot_on_server(
                game,
                &snap,
                RollbackOrigin::ScrubSnapshot {
                    target_tick: final_tick,
                    touched_unloaded: Vec::new(),
                    future_region_files: Vec::new(),
                },
            )
        } else {
            false
        };

        if did_rollback {
            game.close_container();
            game.screen = super::super::Screen::Playing;
            apply_client_level_rollback(game, "scrub");
        }

        let sound = mod_api::SoundPlay::ui("minecraft:block.respawn_anchor.deplete");
        game.play_local_sound(&sound, 0);

        let dt = (anchor - final_tick) as f64 / 20.0;
        let dt_str = format!("{:.1}", dt);
        game.native_chat(&tr_args("message.rreturn.scrub.commit", &[&dt_str]));
    }

    with_scrub_ring_mut(|r| {
        if final_tick < anchor {
            r.frames.retain(|f| f.game_time <= final_tick);
            r.keyframes.retain(|(t, _)| *t <= final_tick);
        }
        r.reference_chunks = (&*game.world.chunks).clone();
        r.last_world_revision = game.world.revision;
    });

    game.clock.set_frozen(was_frozen);
    if let Some(server) = &game.integrated {
        server.call(|srv| {
            srv.clock.set_frozen(was_frozen);
        });
    }
}

pub fn cancel_scrub(game: &mut Game<'_>) {
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if !s.active {
        return;
    }

    let anchor = s.anchor_tick;
    let applied = s.applied_block_tick;
    let was_frozen = s.was_frozen_before_scrub;
    s.active = false;
    s.start_time = None;
    s.current_time = anchor as f64;
    s.target_time = anchor as f64;
    s.applied_block_tick = anchor;
    drop(s);

    if applied != anchor {
        apply_block_rewind_to_game(game, applied, anchor);
        apply_timeline_sounds_to_game(game, applied, anchor);
    }
    apply_interpolated_poses_to_game(game, anchor as f64);
    let player_sync = game.player.as_ref().map(|p| (p.position, game.yaw, game.pitch));
    if let Some((pos, yaw, pitch)) = player_sync {
        if let Some(server) = &game.integrated {
            server.call(move |srv| {
                if let Some(sp) = srv.player.as_mut() {
                    sp.position = pos;
                    sp.old_position = pos;
                    sp.velocity = Default::default();
                    sp.yaw = yaw;
                    sp.head_yaw = yaw;
                    sp.pitch = pitch;
                    sp.teleported = true;
                }
                srv.yaw = yaw;
                srv.pitch = pitch;
            });
        }
    }

    with_scrub_ring_mut(|r| {
        r.reference_chunks = (&*game.world.chunks).clone();
        r.last_world_revision = game.world.revision;
    });

    let sound = mod_api::SoundPlay::ui("minecraft:block.iron_trapdoor.close");
    game.play_local_sound(&sound, 0);

    game.clock.set_frozen(was_frozen);
    if let Some(server) = &game.integrated {
        server.call(|srv| {
            srv.clock.set_frozen(was_frozen);
        });
    }
    game.native_chat(&tr("message.rreturn.scrub.cancel"));
}

pub fn render_hud(game: &Game<'_>, draw: &mut horizon_client::hud::HudBatch) {
    let Some((curr, min_t, max_t)) = get_scrub_info() else {
        return;
    };

    let w = draw.width;
    let h = draw.height;
    if w < 100.0 || h < 100.0 {
        return;
    }

    let box_w = 320.0f32;
    let box_h = 42.0f32;
    let x = (w - box_w) * 0.5;
    let y = h - box_h - 40.0;

    game.native_fill(draw, [x, y, box_w, box_h], 0xCC101018);
    game.native_fill(draw, [x, y, box_w, 2.0], 0xFF00D2FF);

    let bar_x = x + 12.0;
    let bar_y = y + 24.0;
    let bar_w = box_w - 24.0;
    let bar_h = 4.0;
    game.native_fill(draw, [bar_x, bar_y, bar_w, bar_h], 0x80303038);

    let total_range = (max_t - min_t).max(1) as f64;
    let progress = ((curr - min_t as f64) / total_range).clamp(0.0, 1.0) as f32;
    let fill_w = bar_w * progress;
    game.native_fill(draw, [bar_x, bar_y, fill_w, bar_h], 0xFF00E5FF);
    let pip_x = (bar_x + fill_w - 2.0).clamp(bar_x, bar_x + bar_w - 4.0);
    game.native_fill(draw, [pip_x, bar_y - 2.0, 4.0, 8.0], 0xFFFFFFFF);

    let delta_ticks = (curr - max_t as f64).round() as i64;
    let delta_sec = (delta_ticks as f64) / 20.0;
    let abs_sec = delta_sec.abs();
    let mins = (abs_sec / 60.0).floor() as i64;
    let secs = abs_sec % 60.0;

    let time_str = format!("{} {:02}:{:04.1}s", if delta_ticks < 0 { "REWIND" } else { "LIVE" }, mins, secs);
    let hint_str = "[Wheel] Scrub  [U / Enter] Apply  [Esc] Cancel";

    game.native_text(draw, &time_str, [x + 12.0, y + 8.0], 0xFFFFFFFF, true);
    game.native_text(draw, hint_str, [x + 120.0, y + 8.0], 0xFFAAAAAA, false);
}
