use super::super::Game;
use super::restore::rollback_slot_checkpoint_on_server;
use super::restore_client::apply_client_level_rollback;
use super::scrub_interpolate::apply_interpolated_poses_to_game;
use super::scrub_timeline::with_scrub_ring;
use super::types::*;
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

pub fn start_scrub(game: &mut Game<'_>) {
    if !has_cheat_permission(game, None) {
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

    // Stop any ongoing mining / block breaking
    game.stop_mining();

    // Freeze game clock so physics and mob ticking pause during scrub
    game.clock.set_frozen(true);
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
    drop(s);

    apply_interpolated_poses_to_game(game, target_t);
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
    s.active = false;
    s.start_time = None;
    s.jog_accel = 1.0;
    drop(s);

    if final_tick < anchor {
        let rolled_back = if let Some(server) = &game.integrated {
            server.call(|srv| rollback_slot_checkpoint_on_server(srv, "default")).unwrap_or(false)
        } else {
            rollback_slot_checkpoint_on_server(game, "default")
        };
        apply_interpolated_poses_to_game(game, final_time);
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
        if rolled_back {
            let active_slot = with_state(|st| st.current_slot.clone());
            apply_client_level_rollback(game, &active_slot);
            let dt = (anchor - final_tick) as f64 / 20.0;
            let dt_str = format!("{:.1}", dt);
            game.native_chat(&tr_args("message.rreturn.scrub.commit", &[&dt_str]));
        }
    }

    game.clock.set_frozen(false);
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
    s.active = false;
    s.start_time = None;
    s.current_time = anchor as f64;
    s.target_time = anchor as f64;
    drop(s);

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
    game.clock.set_frozen(false);
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
