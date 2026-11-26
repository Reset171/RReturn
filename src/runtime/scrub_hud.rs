use super::super::Game;
use super::scrub::*;
use super::scrub_timeline::*;
use super::settings_ui_draw::*;
use super::types::*;
use horizon_client::hud::HudBatch;
use std::cell::RefCell;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScrubHudAction {
    Apply,
    Cancel,
}

#[derive(Default)]
struct ScrubHudState {
    capsule_rect: [f32; 4],
    track_rect: [f32; 4],
    apply_rect: [f32; 4],
    cancel_rect: [f32; 4],
    dragging_track: bool,
    pending_action: Option<ScrubHudAction>,
    suppress_next_release: bool,
}

thread_local! {
    static SCRUB_HUD_STATE: RefCell<ScrubHudState> = RefCell::new(ScrubHudState::default());
}

pub fn reset_scrub_hud_state() {
    SCRUB_HUD_STATE.with(|c| *c.borrow_mut() = ScrubHudState::default());
}

pub fn take_pending_scrub_hud_action() -> Option<ScrubHudAction> {
    SCRUB_HUD_STATE.with(|c| c.borrow_mut().pending_action.take())
}

pub fn is_scrub_hud_dragging() -> bool {
    SCRUB_HUD_STATE.with(|c| c.borrow().dragging_track)
}

pub fn consume_scrub_hud_release() -> bool {
    SCRUB_HUD_STATE.with(|c| {
        let mut st = c.borrow_mut();
        std::mem::replace(&mut st.suppress_next_release, false)
    })
}

pub fn scrub_hud_bounds_json() -> String {
    SCRUB_HUD_STATE.with(|c| {
        let st = c.borrow();
        let center = |r: [f32; 4]| [
            (r[0] + r[2] * 0.5).round() as i32,
            (r[1] + r[3] * 0.5).round() as i32,
        ];
        serde_json::json!({
            "capsule": st.capsule_rect,
            "track": st.track_rect,
            "apply_rect": st.apply_rect,
            "cancel_rect": st.cancel_rect,
            "apply_center": center(st.apply_rect),
            "cancel_center": center(st.cancel_rect),
            "track_center": center(st.track_rect),
        })
        .to_string()
    })
}

fn hud_update_track_at(mx: f32, snap_now: bool) {
    let track = SCRUB_HUD_STATE.with(|c| c.borrow().track_rect);
    if track[2] <= 1.0 {
        return;
    }
    let frac = f64::from(((mx - track[0]) / track[2]).clamp(0.0, 1.0));
    set_scrub_target_fraction(frac, snap_now);
}

pub fn handle_scrub_hud_mouse_pressed(button: i32, mx: f32, my: f32) -> bool {
    if !is_scrubbing() || !with_state(|st| st.scrub_enabled) || !scrub_show_hud_enabled() {
        return false;
    }
    if button != 0 {
        return true;
    }
    let (apply_r, cancel_r, track_r, capsule_r) = SCRUB_HUD_STATE.with(|c| {
        let st = c.borrow();
        (st.apply_rect, st.cancel_rect, st.track_rect, st.capsule_rect)
    });
    if hit_in_rect(mx, my, apply_r) {
        SCRUB_HUD_STATE.with(|c| {
            let mut st = c.borrow_mut();
            st.dragging_track = false;
            st.suppress_next_release = true;
            st.pending_action = Some(ScrubHudAction::Apply);
        });
        return true;
    }
    if hit_in_rect(mx, my, cancel_r) {
        SCRUB_HUD_STATE.with(|c| {
            let mut st = c.borrow_mut();
            st.dragging_track = false;
            st.suppress_next_release = true;
            st.pending_action = Some(ScrubHudAction::Cancel);
        });
        return true;
    }
    let track_hit = [
        track_r[0] - 6.0,
        track_r[1] - 6.0,
        track_r[2] + 12.0,
        track_r[3] + 12.0,
    ];
    if track_r[2] > 1.0 && hit_in_rect(mx, my, track_hit) {
        SCRUB_HUD_STATE.with(|c| c.borrow_mut().dragging_track = true);
        hud_update_track_at(mx, true);
        return true;
    }
    hit_in_rect(mx, my, capsule_r)
}

pub fn handle_scrub_hud_mouse_released(button: i32) {
    if button == 0 {
        SCRUB_HUD_STATE.with(|c| c.borrow_mut().dragging_track = false);
    }
}

pub fn sync_scrub_hud_drag(game: &Game<'_>) {
    let dragging = SCRUB_HUD_STATE.with(|c| c.borrow().dragging_track);
    if !dragging {
        return;
    }
    let [mx, _] = game.cursor();
    hud_update_track_at(mx, true);
}

fn hud_context_label(screen_id: &str, has_container: bool, has_station: bool) -> Option<String> {
    let key = if has_container || has_station {
        "rreturn.scrub_hud.ctx_container"
    } else {
        match screen_id {
            "creative" => "rreturn.scrub_hud.ctx_creative",
            "inventory" => "rreturn.scrub_hud.ctx_inventory",
            "chat" => "rreturn.scrub_hud.ctx_chat",
            "command_block" => "rreturn.scrub_hud.ctx_editor",
            _ => return None,
        }
    };
    Some(tr(key))
}

fn scrub_hud_layout(game: &Game<'_>, draw: &HudBatch, _sw: f32, sh: f32) -> (f32, f32, bool) {
    if game.screen == super::super::Screen::Playing {
        let bar_h = 56.0;
        let bar_y = (sh - bar_h - 64.0).max(12.0).floor();
        return (bar_y, bar_h, false);
    }
    let bar_h = 22.0;
    let margin = if matches!(game.screen, super::super::Screen::Creative) && sh <= 250.0 {
        2.0
    } else {
        4.0
    };
    let bottom_y = (sh - bar_h - margin).max(4.0).floor();
    let bar_y = if game.screen == super::super::Screen::Chat {
        4.0
    } else if game.screen == super::super::Screen::Inventory {
        let (_, oy) = game.inventory_origin(draw);
        let gui_bottom = oy + game.screen_height();
        if bottom_y < gui_bottom && oy >= bar_h + 6.0 {
            4.0
        } else {
            bottom_y
        }
    } else {
        bottom_y
    };
    (bar_y, bar_h, true)
}

pub fn draw_scrub_hud(game: &Game<'_>, draw: &mut HudBatch) {
    if !with_state(|st| st.scrub_enabled) || !scrub_show_hud_enabled() {
        return;
    }
    let Some((curr, min_t, max_t)) = get_scrub_info() else {
        return;
    };
    let [sw, sh] = game.native_size(draw);
    if sw < 120.0 || sh < 90.0 {
        return;
    }
    let [mx, my] = game.cursor();
    let dragging = SCRUB_HUD_STATE.with(|c| c.borrow().dragging_track);

    let bar_w = (sw - 24.0).clamp(296.0, 520.0);
    let (bar_y, bar_h, compact) = scrub_hud_layout(game, draw, sw, sh);
    let bar_x = ((sw - bar_w) * 0.5).floor();
    let capsule = [bar_x, bar_y, bar_w, bar_h];
    let radius = if compact { 8.0 } else { 11.0 };

    draw_soft_shadow(game, draw, capsule, radius, 4);
    draw_rounded_rect(game, draw, capsule, radius, 0xF710_131C);
    draw_rounded_border(game, draw, capsule, radius, 0x24FF_FFFF);

    let is_live = (max_t as f64 - curr).abs() < 0.25;
    let badge_label = if is_live {
        tr("rreturn.scrub_hud.badge_live")
    } else {
        tr("rreturn.scrub_hud.badge_rewind")
    };
    let badge_bg = if is_live { 0xFF1E_8E5A } else { UI_BLUE };
    let badge_w = (game.native_text_width(draw, &badge_label) + 12.0).clamp(44.0, 96.0);
    let top_y = bar_y + if compact { 3.5 } else { 7.0 };
    let badge_rect = [bar_x + 10.0, top_y, badge_w, 15.0];
    draw_pill(game, draw, badge_rect, &badge_label, badge_bg, 0xFFFF_FFFF);

    let sec = ((curr - max_t as f64) / 20.0).min(0.0);
    let sec_unit = tr("rreturn.ui.sec_unit");
    let offset_text = if is_live {
        format!("0.0 {}", sec_unit)
    } else {
        format!("{:.1} {}", sec, sec_unit)
    };
    let mut cursor_x = badge_rect[0] + badge_rect[2] + 7.0;
    game.native_text(draw, &offset_text, [cursor_x, top_y + 3.5], UI_TEXT_PRI, false);
    cursor_x += game.native_text_width(draw, &offset_text) + 8.0;

    let cancel_label = tr("rreturn.scrub_hud.cancel");
    let apply_label = tr("rreturn.scrub_hud.apply");
    let cancel_w = (game.native_text_width(draw, &cancel_label) + 12.0).clamp(46.0, 92.0);
    let apply_w = (game.native_text_width(draw, &apply_label) + 12.0).clamp(50.0, 104.0);
    let apply_rect = [bar_x + bar_w - 10.0 - apply_w, top_y, apply_w, 15.0];
    let cancel_rect = [apply_rect[0] - 5.0 - cancel_w, top_y, cancel_w, 15.0];

    if !compact {
        let target_tick = curr.round() as i64;
        let frame_ctx = with_scrub_ring(|ring| {
            ring.frames
                .iter()
                .min_by_key(|f| (f.game_time - target_tick).abs())
                .and_then(|f| {
                    hud_context_label(
                        &f.ui.screen,
                        f.ui.container.is_some()
                            || f.ui.ender_open.is_some()
                            || f.ui.craft_table.is_some(),
                        f.ui.station.is_some(),
                    )
                })
        });
        if let Some(ctx_label) = frame_ctx {
            let avail = (cancel_rect[0] - 8.0) - cursor_x;
            if avail >= 36.0 {
                let fit = fit_text(game, draw, &ctx_label, avail - 12.0);
                let ctx_w = game.native_text_width(draw, &fit) + 12.0;
                draw_pill(game, draw, [cursor_x, top_y, ctx_w, 15.0], &fit, UI_PILL_BG, UI_TEXT_SEC);
            }
        }
    }

    let cancel_hov = hit_in_rect(mx, my, cancel_rect);
    let apply_hov = hit_in_rect(mx, my, apply_rect);
    draw_pill(
        game,
        draw,
        cancel_rect,
        &cancel_label,
        if cancel_hov { UI_PILL_HOV } else { UI_PILL_BG },
        UI_TEXT_PRI,
    );
    draw_pill(
        game,
        draw,
        apply_rect,
        &apply_label,
        if apply_hov { 0xFF54_9AFF } else { UI_BLUE },
        0xFFFF_FFFF,
    );

    let (track_x, track_y, track_w) = if compact {
        let tx = (cursor_x + 4.0).floor();
        let tw = ((cancel_rect[0] - 10.0) - tx).max(40.0);
        (tx, bar_y + 8.0, tw)
    } else {
        (bar_x + 14.0, bar_y + 29.0, bar_w - 28.0)
    };
    let track_h = 6.0f32;
    let track_rect = [track_x, track_y, track_w, track_h];
    draw_rounded_rect(game, draw, track_rect, 3.0, UI_TRACK_BG);

    let total_range = (max_t - min_t).max(1) as f64;
    let progress = ((curr - min_t as f64) / total_range).clamp(0.0, 1.0) as f32;
    let thumb_cx = track_x + track_w * progress;
    let fill_w = (track_x + track_w - thumb_cx).max(0.0);
    if fill_w > 1.0 {
        draw_rounded_rect(game, draw, [thumb_cx, track_y, fill_w, track_h], 3.0, 0xAA3E_8AFF);
    }

    with_scrub_ring(|ring| {
        for &cp_t in &ring.checkpoint_markers {
            if cp_t >= min_t && cp_t <= max_t {
                let frac = ((cp_t - min_t) as f64 / total_range).clamp(0.0, 1.0) as f32;
                let mx = track_x + track_w * frac;
                game.native_fill(draw, [mx - 1.0, track_y - 2.0, 2.0, track_h + 4.0], 0xFFF5_B841);
            }
        }
        for &(dim_t, dim) in &ring.dim_markers {
            if dim_t >= min_t && dim_t <= max_t {
                let frac = ((dim_t - min_t) as f64 / total_range).clamp(0.0, 1.0) as f32;
                let mx = track_x + track_w * frac;
                let col = match dim {
                    Dim::Overworld => 0xFF34_C759,
                    Dim::Nether => 0xFFBF_5AF2,
                    Dim::End => 0xFFFF_D60A,
                    Dim::Extra(_) => 0xFF55_AAFF,
                };
                game.native_fill(draw, [mx - 1.0, track_y - 2.0, 2.0, track_h + 4.0], col);
            }
        }
        for &dmg_t in &ring.damage_markers {
            if dmg_t >= min_t && dmg_t <= max_t {
                let frac = ((dmg_t - min_t) as f64 / total_range).clamp(0.0, 1.0) as f32;
                let mx = track_x + track_w * frac;
                game.native_fill(draw, [mx - 1.0, track_y - 2.0, 2.0, track_h + 4.0], UI_RED);
            }
        }
    });

    let thumb_hov = dragging
        || hit_in_rect(
            mx,
            my,
            [track_x - 6.0, track_y - 6.0, track_w + 12.0, track_h + 12.0],
        );
    let thumb_size = if thumb_hov { 12.0 } else { 10.0 };
    let thumb_r = thumb_size * 0.5;
    let thumb_rect = [
        (thumb_cx - thumb_r).round(),
        (track_y + track_h * 0.5 - thumb_r).round(),
        thumb_size,
        thumb_size,
    ];
    draw_rounded_rect(game, draw, thumb_rect, thumb_r, UI_THUMB);
    if thumb_hov {
        draw_rounded_border(game, draw, thumb_rect, thumb_r, UI_BLUE);
    }

    if !compact {
        let (scrub_lbl, scrub_bound) = format_mod_binding(game, "scrub", "key.keyboard.u");
        let apply_keys = if scrub_bound {
            format!("Enter / {}", scrub_lbl)
        } else {
            "Enter".to_owned()
        };
        let hint = fit_text(game, draw, &tr_args("rreturn.scrub_hud.hint", &[&apply_keys]), bar_w - 24.0);
        let hint_w = game.native_text_width(draw, &hint);
        let hint_x = (bar_x + (bar_w - hint_w) * 0.5).floor();
        game.native_text(draw, &hint, [hint_x, bar_y + 41.0], UI_TEXT_MUTED, false);
    }

    SCRUB_HUD_STATE.with(|c| {
        let mut st = c.borrow_mut();
        st.capsule_rect = capsule;
        st.track_rect = track_rect;
        st.apply_rect = apply_rect;
        st.cancel_rect = cancel_rect;
    });
}
