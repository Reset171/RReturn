use super::super::Game;
use super::hooks::handle_screen_key_pressed_any;
use super::policy::{OverrideValue, PlayerProfileOverrides, RollbackPolicy};
use super::scrub::{apply_scrub_wheel_delta, is_scrubbing};
use super::scrub_hud::{
    consume_scrub_hud_release, handle_scrub_hud_mouse_pressed, handle_scrub_hud_mouse_released,
};
use super::settings_catalog::{
    count_tab_policies, read_bool_setting, read_slider_setting, write_bool_setting,
    write_slider_setting, UiGroup, UiTab, SETTINGS_CATALOG,
};
use super::settings_ui_draw::*;
use super::settings_ui_icons::{draw_ui_icon, UiIcon};
use super::settings_ui_players::draw_players_tab;
use super::settings_ui_tabs::{draw_grouped_tab, draw_search_tab, draw_system_tab};
use super::types::*;
use super::ui::{save_current_settings, sync_scrub_runtime};
use horizon_client::hud::HudBatch;
use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    static UI_STATE: RefCell<SettingsUiState> = RefCell::new(SettingsUiState::default());
}

pub fn is_settings_ui_open(game: &Game<'_>) -> bool {
    game.mod_screen() == Some(MOD_ID)
}

pub fn request_open_settings_ui(game: &mut Game<'_>) {
    request_open_settings_tab(game, None);
}

pub fn request_open_settings_tab(game: &mut Game<'_>, tab: Option<UiTab>) {
    if let Some(server) = &game.integrated {
        let _ = server.call(|srv| {
            let players = collect_online_players(srv);
            if !players.is_empty() {
                with_state_mut(|st| st.online_players = players);
            }
        });
    } else {
        let players = collect_online_players(game);
        if !players.is_empty() {
            with_state_mut(|st| st.online_players = players);
        }
    }
    UI_STATE.with(|cell| {
        let mut ui = cell.borrow_mut();
        if let Some(t) = tab {
            ui.active_tab = t;
            ui.search_query.clear();
        }
        ui.scroll_y = 0.0;
        ui.target_scroll_y = 0.0;
        ui.search_focused = false;
        ui.dragging_slider = None;
        ui.dragging_scroll = None;
        ui.pending_jump_section = None;
        ui.pending_close = false;
        ui.pending_open = true;
    });
    if game.mod_screen_open(MOD_ID) {
        UI_STATE.with(|cell| cell.borrow_mut().pending_open = false);
    }
}

pub fn settings_ui_bounds_json() -> String {
    UI_STATE.with(|cell| {
        let ui = cell.borrow();
        let mut controls = serde_json::Map::new();
        for r in &ui.hit_regions {
            let cx = (r.rect[0] + r.rect[2] * 0.5).round() as i32;
            let cy = (r.rect[1] + r.rect[3] * 0.5).round() as i32;
            controls.insert(r.tag.clone(), serde_json::json!([cx, cy]));
        }
        let tab_name = match ui.active_tab {
            UiTab::Rollback => "rollback",
            UiTab::Scrub => "scrub",
            UiTab::Players => "players",
            UiTab::System => "system",
        };
        serde_json::json!({
            "window": ui.window_bounds,
            "active_tab": tab_name,
            "scroll_y": ui.scroll_y,
            "target_scroll_y": ui.target_scroll_y,
            "max_scroll_y": ui.max_scroll_y,
            "selected_player": ui.selected_player.map(|u| format!("{:032x}", u)),
            "player_scope_scrub": ui.player_scope_scrub,
            "hotkeys": ui.bound_hotkeys,
            "controls": controls,
        })
        .to_string()
    })
}

pub fn reset_settings_ui_state() {
    UI_STATE.with(|cell| *cell.borrow_mut() = SettingsUiState::default());
}

pub fn flush_settings_ui_actions(game: &mut Game<'_>) {
    let (open_req, close_req, save_req, scrub_sync) = UI_STATE.with(|cell| {
        let mut ui = cell.borrow_mut();
        let o = ui.pending_open;
        let c = ui.pending_close;
        ui.pending_close = false;
        let s = ui.pending_save;
        ui.pending_save = false;
        let sc = ui.pending_scrub_sync.take();
        (o, c, s, sc)
    });

    if open_req && game.mod_screen_open(MOD_ID) {
        UI_STATE.with(|cell| cell.borrow_mut().pending_open = false);
    }
    if close_req {
        game.mod_screen_close(MOD_ID);
    }
    if let Some(en) = scrub_sync {
        sync_scrub_runtime(game, en);
    }
    if save_req {
        save_current_settings(game);
    }
}

fn sync_drag_at_cursor(mx: f32, my: f32) {
    let drag_val = UI_STATE.with(|cell| {
        let mut ui = cell.borrow_mut();
        let now = Instant::now();
        let dt = now
            .saturating_duration_since(ui.last_frame)
            .as_secs_f32()
            .clamp(0.001, 0.05);
        ui.last_frame = now;
        ui.scroll_y += (ui.target_scroll_y - ui.scroll_y) * (dt * 20.0).min(1.0);
        if let Some((ty, th, max_s)) = ui.dragging_scroll {
            let frac = ((my - ty) / th.max(1.0)).clamp(0.0, 1.0);
            let t = (frac * max_s).clamp(0.0, max_s);
            ui.target_scroll_y = t;
            ui.scroll_y = t;
        }
        ui.dragging_slider
            .as_ref()
            .map(|d| (d.id, slider_snap(mx, d.track_x, d.track_w, d.min, d.max, d.step)))
    });
    if let Some((id, val)) = drag_val {
        let changed = with_state_mut(|st| {
            let prev = read_slider_setting(st, id);
            write_slider_setting(st, id, val);
            read_slider_setting(st, id) != prev
        });
        if changed {
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
    }
}

pub fn render_settings_screen(game: &Game<'_>, draw: &mut HudBatch) {
    if !is_settings_ui_open(game) {
        return;
    }
    let [sw, sh] = game.native_size(draw);
    if sw < 120.0 || sh < 80.0 {
        return;
    }
    let [mx, my] = game.cursor();
    sync_drag_at_cursor(mx, my);

    game.native_fill(draw, [0.0, 0.0, sw, sh * 0.5], UI_SCRIM_TOP);
    game.native_fill(draw, [0.0, sh * 0.5, sw, sh * 0.5], UI_SCRIM_BOT);

    let mx_pad = if sw < 480.0 { 6.0 } else { 20.0 };
    let my_pad = if sh < 280.0 { 5.0 } else { 12.0 };
    let win_w = (sw - mx_pad * 2.0).clamp(300.0, 760.0);
    let win_h = (sh - my_pad * 2.0).clamp(200.0, 440.0);
    let win_x = ((sw - win_w) * 0.5).floor();
    let win_y = ((sh - win_h) * 0.5).floor();

    draw_soft_shadow(game, draw, [win_x, win_y, win_w, win_h], 10.0, 4);
    draw_rounded_rect(game, draw, [win_x, win_y, win_w, win_h], 10.0, UI_WIN_BG);
    draw_rounded_border(game, draw, [win_x, win_y, win_w, win_h], 10.0, UI_WIN_BORDER);

    let side_w = if win_w < 460.0 { 132.0 } else { 142.0 };
    draw_rounded_left_rect(game, draw, [win_x + 1.0, win_y + 1.0, side_w, win_h - 2.0], 9.0, UI_SIDEBAR_BG);
    game.native_fill(draw, [win_x + side_w, win_y + 1.0, 1.0, win_h - 2.0], UI_WIN_BORDER);

    let (
        rb_counts,
        sc_counts,
        player_cnt,
        active_tab,
        search_q,
        search_focused,
        scroll_y,
    ) = with_state(|st| {
        UI_STATE.with(|cell| {
            let ui = cell.borrow();
            (
                count_tab_policies(st, UiTab::Rollback),
                count_tab_policies(st, UiTab::Scrub),
                st.online_players.len().max(st.player_overrides.len()),
                ui.active_tab,
                ui.search_query.clone(),
                ui.search_focused,
                ui.scroll_y,
            )
        })
    });

    let mut regions: Vec<HitRegion> = Vec::with_capacity(112);

    // Sidebar compact RReturn header + Search pill
    draw_ui_icon(game, draw, UiIcon::RReturnLogo, win_x + 10.0, win_y + 10.0, UI_BLUE);
    game.native_text(draw, "RReturn", [win_x + 25.0, win_y + 11.5], UI_TEXT_PRI, false);

    let search_rect = [win_x + 8.0, win_y + 28.0, side_w - 16.0, 17.0];
    let s_hov = hit_in_rect(mx, my, search_rect);
    draw_rounded_rect(game, draw, search_rect, 8.0, UI_SEARCH_BG);
    if search_focused || s_hov {
        draw_rounded_border(game, draw, search_rect, 8.0, if search_focused { UI_BLUE } else { UI_DIVIDER });
    }
    regions.push(HitRegion { tag: "search".to_owned(), rect: search_rect, action: HitAction::FocusSearch });
    let clr_rect = [search_rect[0] + search_rect[2] - 15.0, search_rect[1] + 2.0, 13.0, 13.0];
    if search_q.is_empty() {
        let ph = fit_text(game, draw, &tr("rreturn.ui.search_placeholder"), search_rect[2] - 14.0);
        game.native_text(draw, &ph, [search_rect[0] + 7.0, search_rect[1] + 4.5], UI_TEXT_MUTED, false);
    } else {
        let qf = fit_text(game, draw, &search_q, search_rect[2] - 24.0);
        game.native_text(draw, &qf, [search_rect[0] + 7.0, search_rect[1] + 4.5], UI_TEXT_PRI, false);
        if search_focused {
            let cw = game.native_text_width(draw, &qf);
            game.native_fill(draw, [search_rect[0] + 7.0 + cw + 1.0, search_rect[1] + 4.0, 1.0, 9.0], UI_BLUE);
        }
        let c_hov = hit_in_rect(mx, my, clr_rect);
        game.native_text(draw, "x", [clr_rect[0] + 3.0, clr_rect[1] + 2.0], if c_hov { UI_RED } else { UI_TEXT_SEC }, false);
        regions.push(HitRegion { tag: "clear_search".to_owned(), rect: clr_rect, action: HitAction::ClearSearch });
    }

    // Sidebar 4 menu tabs with vector icons
    let searching = !search_q.trim().is_empty();
    let compact_side = win_h < 225.0;
    let mut tab_y = win_y + 52.0;
    let tab_h = if compact_side { 20.0 } else { 22.0 };
    let tab_gap = if compact_side { 3.0 } else { 4.0 };
    for &tab in UiTab::ALL.iter() {
        let tr_rect = [win_x + 8.0, tab_y, side_w - 16.0, tab_h];
        let active = !searching && active_tab == tab;
        let hov = hit_in_rect(mx, my, tr_rect);
        if active || hov {
            draw_rounded_rect(game, draw, tr_rect, 7.0, if active { UI_PILL_BG } else { UI_ROW_HOV });
        }
        if active {
            draw_rounded_rect(game, draw, [tr_rect[0] + 2.0, tr_rect[1] + 5.0, 2.0, tab_h - 10.0], 1.0, UI_BLUE);
        }
        let icon = match tab {
            UiTab::Rollback => UiIcon::Rollback,
            UiTab::Scrub => UiIcon::Scrub,
            UiTab::Players => UiIcon::Players,
            UiTab::System => UiIcon::System,
        };
        let icon_y = tr_rect[1] + (tab_h - 11.0) * 0.5;
        draw_ui_icon(
            game,
            draw,
            icon,
            tr_rect[0] + 7.0,
            icon_y,
            if active { UI_BLUE } else { UI_TEXT_SEC },
        );

        let badge = match tab {
            UiTab::Rollback => format!("{}/{}", rb_counts.0, rb_counts.1),
            UiTab::Scrub => format!("{}/{}", sc_counts.0, sc_counts.1),
            UiTab::Players => format!("{}", player_cnt),
            UiTab::System => String::new(),
        };
        let bw = if badge.is_empty() { 0.0 } else { game.native_text_width(draw, &badge) + 7.0 };
        let lfit = fit_text(game, draw, &tr(tab.title_key()), tr_rect[2] - 25.0 - bw);
        let ty = tr_rect[1] + (tab_h - 8.0) * 0.5;
        game.native_text(draw, &lfit, [tr_rect[0] + 21.0, ty], if active { UI_TEXT_PRI } else { UI_TEXT_SEC }, false);
        if !badge.is_empty() {
            game.native_text(
                draw,
                &badge,
                [tr_rect[0] + tr_rect[2] - bw + 1.0, ty],
                if active { UI_BLUE } else { UI_TEXT_MUTED },
                false,
            );
        }
        let tab_tag = match tab {
            UiTab::Rollback => "tab.rollback",
            UiTab::Scrub => "tab.scrub",
            UiTab::Players => "tab.players",
            UiTab::System => "tab.system",
        };
        regions.push(HitRegion { tag: tab_tag.to_owned(), rect: tr_rect, action: HitAction::SelectTab(tab) });
        tab_y += tab_h + tab_gap;
    }

    // Right Viewing Header + Close X
    let right_x = win_x + side_w + 12.0;
    let right_w = (win_w - side_w - 22.0).max(80.0);
    let close_rect = [win_x + win_w - 26.0, win_y + 10.0, 16.0, 16.0];
    let c_hov = hit_in_rect(mx, my, close_rect);
    draw_pill(game, draw, close_rect, "X", if c_hov { UI_PILL_HOV } else { UI_PILL_BG }, if c_hov { UI_TEXT_PRI } else { UI_TEXT_SEC });
    regions.push(HitRegion { tag: "close".to_owned(), rect: close_rect, action: HitAction::Close });

    let hdr_title = tr(if searching { "rreturn.ui.search_results" } else { active_tab.title_key() });
    let hdr_sub = if searching { search_q.trim().to_owned() } else { tr(active_tab.subtitle_key()) };
    let hdr_text_max = (right_w - 28.0).max(30.0);
    game.native_text(draw, &fit_text(game, draw, &hdr_title, hdr_text_max), [right_x, win_y + 10.0], UI_TEXT_PRI, false);
    game.native_text(draw, &fit_text(game, draw, &hdr_sub, hdr_text_max), [right_x, win_y + 22.0], UI_TEXT_MUTED, false);

    // Quick-Jump Icon Bar only on multi-section Rollback / Scrub tabs
    let mut list_top_off = 38.0;
    let mut jump_tooltip: Option<([f32; 2], String)> = None;
    if !searching && matches!(active_tab, UiTab::Rollback | UiTab::Scrub) {
        let jbar = [right_x, win_y + 35.0, (right_w - 8.0).max(60.0), 22.0];
        jump_tooltip = draw_quick_jump_bar(
            game,
            draw,
            active_tab,
            jbar,
            scroll_y,
            [mx, my],
            &mut regions,
        );
        list_top_off = 61.0;
    }

    // Clipped settings list area
    let list_x = right_x;
    let list_y = win_y + list_top_off;
    let list_w = (right_w - 8.0).max(60.0);
    let list_h = (win_h - list_top_off - 8.0).max(30.0);
    let list_bounds = [list_x, list_y, list_w, list_h];

    let clip_start = draw.vertices.len();
    let mut list_regions: Vec<HitRegion> = Vec::new();
    let total_h = UI_STATE.with(|cell| {
        let mut ui = cell.borrow_mut();
        if searching {
            draw_search_tab(game, draw, &mut ui, &search_q, list_bounds, scroll_y, [mx, my], &mut list_regions)
        } else {
            ui.section_offsets.clear();
            match active_tab {
                UiTab::Rollback | UiTab::Scrub => {
                    draw_grouped_tab(game, draw, &mut ui, active_tab, list_bounds, scroll_y, [mx, my], &mut list_regions)
                }
                UiTab::Players => {
                    draw_players_tab(game, draw, &mut ui, list_bounds, scroll_y, [mx, my], &mut list_regions)
                }
                UiTab::System => {
                    draw_system_tab(game, draw, &mut ui, list_bounds, scroll_y, [mx, my], &mut list_regions)
                }
            }
        }
    });
    draw.clip_since(clip_start, list_x, list_y, list_x + list_w, list_y + list_h);

    let max_scroll = (total_h - list_h).max(0.0);
    if max_scroll > 0.5 {
        let sb_x = list_x + list_w + 3.0;
        draw_rounded_rect(game, draw, [sb_x, list_y, 3.0, list_h], 1.5, 0x24FFFFFF);
        let thumb_h = ((list_h / total_h) * list_h).clamp(16.0, list_h);
        let thumb_y = list_y + (scroll_y / max_scroll) * (list_h - thumb_h);
        let sb_rect = [sb_x - 2.0, list_y, 7.0, list_h];
        let sb_hov = hit_in_rect(mx, my, sb_rect);
        draw_rounded_rect(game, draw, [sb_x, thumb_y, 3.0, thumb_h], 1.5, if sb_hov { UI_BLUE } else { 0x99727585 });
        regions.push(HitRegion {
            tag: "scrollbar".to_owned(),
            rect: sb_rect,
            action: HitAction::ScrollTrack { track_y: list_y, track_h: list_h, max_scroll },
        });
    }

    regions.extend(list_regions);
    UI_STATE.with(|cell| {
        let mut ui = cell.borrow_mut();
        ui.max_scroll_y = max_scroll;
        if let Some(sec_id) = ui.pending_jump_section.take()
            && let Some(&off) = ui.section_offsets.get(&sec_id)
        {
            let clamped = off.clamp(0.0, max_scroll);
            ui.target_scroll_y = clamped;
            ui.scroll_y = clamped;
        }
        ui.target_scroll_y = ui.target_scroll_y.min(max_scroll);
        ui.scroll_y = ui.scroll_y.min(max_scroll);
        ui.window_bounds = [win_x, win_y, win_w, win_h];
        ui.hit_regions = regions;
    });

    // Draw tooltip last so it renders crisply above everything inside the quick-jump dock
    if let Some(([tx, ty], tip)) = jump_tooltip {
        let tw = game.native_text_width(draw, &tip) + 12.0;
        let rx = tx.min(win_x + win_w - tw - 6.0).max(win_x + 6.0);
        draw_rounded_rect(game, draw, [rx, ty, tw, 15.0], 5.0, 0xF01C1E26);
        draw_rounded_border(game, draw, [rx, ty, tw, 15.0], 5.0, UI_WIN_BORDER);
        game.native_text(draw, &tip, [rx + 6.0, ty + 3.5], UI_TEXT_PRI, false);
    }
}

struct JumpBtnSpec {
    tab: UiTab,
    section: &'static str,
    icon: UiIcon,
    label_key: &'static str,
    tag: &'static str,
    scope_scrub: Option<bool>,
}

fn quick_jump_specs(tab: UiTab) -> &'static [JumpBtnSpec] {
    const RB_JUMPS: &[JumpBtnSpec] = &[
        JumpBtnSpec { tab: UiTab::Rollback, section: "rb_auto", icon: UiIcon::Clock, label_key: "rreturn.ui.jump.auto", tag: "jump.rb_auto", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Rollback, section: "rb_world", icon: UiIcon::WorldCube, label_key: "rreturn.ui.jump.world", tag: "jump.rb_world", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Rollback, section: "rb_entities", icon: UiIcon::Entities, label_key: "rreturn.ui.jump.entities", tag: "jump.rb_entities", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Rollback, section: "rb_player", icon: UiIcon::PlayerSingle, label_key: "rreturn.ui.jump.player", tag: "jump.rb_player", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Rollback, section: "rb_rules", icon: UiIcon::RulesShield, label_key: "rreturn.ui.jump.rules", tag: "jump.rb_rules", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Players, section: "players_rules", icon: UiIcon::Players, label_key: "rreturn.ui.jump.players", tag: "jump.open_players", scope_scrub: Some(false) },
    ];
    const SC_JUMPS: &[JumpBtnSpec] = &[
        JumpBtnSpec { tab: UiTab::Scrub, section: "scrub_general", icon: UiIcon::Scrub, label_key: "rreturn.ui.jump.scrub_gen", tag: "jump.scrub_general", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Scrub, section: "scrub_world", icon: UiIcon::WorldCube, label_key: "rreturn.ui.jump.world", tag: "jump.scrub_world", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Scrub, section: "scrub_entities", icon: UiIcon::Entities, label_key: "rreturn.ui.jump.entities", tag: "jump.scrub_entities", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Scrub, section: "scrub_player", icon: UiIcon::PlayerSingle, label_key: "rreturn.ui.jump.player", tag: "jump.scrub_player", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Scrub, section: "scrub_rules", icon: UiIcon::RulesShield, label_key: "rreturn.ui.jump.rules", tag: "jump.scrub_rules", scope_scrub: None },
        JumpBtnSpec { tab: UiTab::Players, section: "players_rules", icon: UiIcon::Players, label_key: "rreturn.ui.jump.players", tag: "jump.open_players", scope_scrub: Some(true) },
    ];
    match tab {
        UiTab::Rollback => RB_JUMPS,
        UiTab::Scrub => SC_JUMPS,
        _ => &[],
    }
}

fn draw_quick_jump_bar(
    game: &Game<'_>,
    draw: &mut HudBatch,
    active_tab: UiTab,
    rect: [f32; 4],
    scroll_y: f32,
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> Option<([f32; 2], String)> {
    let specs = quick_jump_specs(active_tab);
    if specs.is_empty() {
        return None;
    }
    draw_rounded_rect(game, draw, rect, 6.0, UI_SEG_DOCK);
    let active_sec = UI_STATE.with(|cell| {
        let ui = cell.borrow();
        let mut best: Option<(&str, f32)> = None;
        for sp in specs {
            if sp.tab != active_tab {
                continue;
            }
            if let Some(&off) = ui.section_offsets.get(sp.section)
                && off <= scroll_y + 18.0
                && best.is_none_or(|(_, boff)| off >= boff)
            {
                best = Some((sp.section, off));
            }
        }
        best.map(|(s, _)| s).unwrap_or(specs[0].section)
    });

    let show_labels = rect[2] >= 400.0 || (specs.len() <= 3 && rect[2] >= 250.0);
    let [mx, my] = mouse;
    let mut cur_x = rect[0] + 3.0;
    let btn_h = rect[3] - 4.0;
    let by = rect[1] + 2.0;
    let mut tooltip: Option<([f32; 2], String)> = None;

    for (idx, sp) in specs.iter().enumerate() {
        let lbl = tr(sp.label_key);
        let is_cross = sp.tab != active_tab;
        let btn_w = if is_cross {
            let cw = if show_labels {
                20.0 + game.native_text_width(draw, &lbl) + 6.0
            } else {
                24.0
            };
            cw.min((rect[0] + rect[2] - cur_x - 3.0).max(20.0))
        } else if show_labels {
            (20.0 + game.native_text_width(draw, &lbl) + 6.0)
                .min(((rect[2] - 10.0) / specs.len() as f32).max(24.0))
        } else {
            24.0
        };
        let bx = if is_cross {
            (rect[0] + rect[2] - btn_w - 3.0).max(cur_x + 4.0)
        } else {
            cur_x
        };
        if is_cross && bx > cur_x + 2.0 {
            game.native_fill(draw, [bx - 3.0, by + 2.0, 1.0, btn_h - 4.0], UI_DIVIDER);
        }
        if bx + btn_w > rect[0] + rect[2] {
            break;
        }
        let br = [bx, by, btn_w, btn_h];
        let active = !is_cross && active_sec == sp.section;
        let hov = hit_in_rect(mx, my, br);
        let bg = if active {
            UI_BLUE
        } else if is_cross {
            if hov { UI_BLUE } else { UI_BLUE_SOFT }
        } else if hov {
            UI_PILL_HOV
        } else {
            UI_PILL_BG
        };
        draw_rounded_rect(game, draw, br, 4.5, bg);
        let fg = if active || (is_cross && hov) {
            UI_TEXT_PRI
        } else if is_cross {
            UI_BLUE
        } else if hov {
            UI_TEXT_PRI
        } else {
            UI_TEXT_SEC
        };
        let ix = if show_labels { br[0] + 5.0 } else { br[0] + (br[2] - 11.0) * 0.5 };
        let iy = br[1] + (br[3] - 11.0) * 0.5;
        draw_ui_icon(game, draw, sp.icon, ix, iy, fg);
        if show_labels {
            let lfit = fit_text(game, draw, &lbl, (br[2] - 21.0).max(10.0));
            game.native_text(draw, &lfit, [br[0] + 18.0, br[1] + 5.0], fg, false);
        } else if hov {
            tooltip = Some(([br[0], rect[1] + rect[3] + 3.0], lbl));
        }
        regions.push(HitRegion {
            tag: sp.tag.to_owned(),
            rect: br,
            action: HitAction::JumpToSection(sp.tab, sp.section.to_owned(), sp.scope_scrub),
        });
        if !is_cross && idx + 1 < specs.len() {
            cur_x += btn_w + 3.0;
        }
    }
    tooltip
}

pub fn handle_screen_mouse_pressed(game: &Game<'_>, button: i32, mx: f32, my: f32, canceled: &mut bool) {
    if handle_scrub_hud_mouse_pressed(button, mx, my) {
        *canceled = true;
        return;
    }
    if is_scrubbing() {
        *canceled = true;
        return;
    }
    if !is_settings_ui_open(game) {
        return;
    }
    *canceled = true;
    if button != 0 {
        return;
    }
    let (action_opt, win_rect) = UI_STATE.with(|cell| {
        let mut ui = cell.borrow_mut();
        ui.search_focused = false;
        let found = ui.hit_regions.iter().rev().find(|r| hit_in_rect(mx, my, r.rect)).map(|r| r.action.clone());
        (found, ui.window_bounds)
    });
    if action_opt.is_none() && !hit_in_rect(mx, my, win_rect) {
        UI_STATE.with(|cell| cell.borrow_mut().pending_close = true);
        return;
    }
    let Some(action) = action_opt else {
        return;
    };
    match action {
        HitAction::Close => UI_STATE.with(|cell| cell.borrow_mut().pending_close = true),
        HitAction::FocusSearch => UI_STATE.with(|cell| cell.borrow_mut().search_focused = true),
        HitAction::ClearSearch => UI_STATE.with(|cell| {
            let mut ui = cell.borrow_mut();
            ui.search_query.clear();
            ui.search_focused = true;
            ui.scroll_y = 0.0;
            ui.target_scroll_y = 0.0;
        }),
        HitAction::SelectTab(tab) => UI_STATE.with(|cell| {
            let mut ui = cell.borrow_mut();
            ui.active_tab = tab;
            ui.search_query.clear();
            ui.search_focused = false;
            ui.scroll_y = 0.0;
            ui.target_scroll_y = 0.0;
            ui.pending_jump_section = None;
        }),
        HitAction::JumpToSection(tab, sec_id, scope_opt) => UI_STATE.with(|cell| {
            let mut ui = cell.borrow_mut();
            let same_tab = ui.active_tab == tab && ui.search_query.trim().is_empty();
            ui.active_tab = tab;
            ui.search_query.clear();
            ui.search_focused = false;
            if let Some(sc) = scope_opt {
                ui.player_scope_scrub = sc;
            }
            if same_tab && let Some(&off) = ui.section_offsets.get(&sec_id) {
                let clamped = off.clamp(0.0, ui.max_scroll_y);
                ui.target_scroll_y = clamped;
                ui.scroll_y = clamped;
                ui.pending_jump_section = None;
            } else {
                ui.scroll_y = 0.0;
                ui.target_scroll_y = 0.0;
                ui.pending_jump_section = Some(sec_id);
            }
        }),
        HitAction::SetAutoSaveMode(mode) => {
            with_state_mut(|st| {
                st.auto_save = true;
                match mode {
                    AutoSaveScheduleMode::Fixed => {
                        st.adaptive_mode = false;
                        st.use_random_interval = false;
                    }
                    AutoSaveScheduleMode::Random => {
                        st.adaptive_mode = false;
                        st.use_random_interval = true;
                    }
                    AutoSaveScheduleMode::Adaptive => {
                        st.adaptive_mode = true;
                    }
                }
            });
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
        HitAction::ToggleBool(id) => {
            let next_val = with_state_mut(|st| {
                let next = !read_bool_setting(st, id);
                write_bool_setting(st, id, next);
                next
            });
            UI_STATE.with(|cell| {
                let mut ui = cell.borrow_mut();
                ui.pending_save = true;
                if id == "scrub_enabled" {
                    ui.pending_scrub_sync = Some(next_val);
                }
            });
        }
        HitAction::SliderStep(id, delta) => {
            with_state_mut(|st| {
                let cur = read_slider_setting(st, id);
                write_slider_setting(st, id, cur + delta);
            });
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
        HitAction::SliderDrag { id, track_x, track_w, min, max, step } => {
            let val = slider_snap(mx, track_x, track_w, min, max, step);
            with_state_mut(|st| write_slider_setting(st, id, val));
            UI_STATE.with(|cell| {
                let mut ui = cell.borrow_mut();
                ui.pending_save = true;
                ui.dragging_slider = Some(SliderDragState { id, track_x, track_w, min, max, step });
            });
        }
        HitAction::SetAnchorMode(mode) => {
            with_state_mut(|st| st.anchor.mode = mode);
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
        HitAction::SelectPlayer(uuid, _) => {
            UI_STATE.with(|cell| cell.borrow_mut().selected_player = Some(uuid));
        }
        HitAction::SetPlayerScope(scrub_scope) => {
            UI_STATE.with(|cell| cell.borrow_mut().player_scope_scrub = scrub_scope);
        }
        HitAction::SetPlayerOverride(uuid, name, scrub_scope, field, val) => {
            with_state_mut(|st| {
                let entry = st
                    .player_overrides
                    .entry(uuid)
                    .or_insert_with(|| PlayerProfileOverrides {
                        name: name.clone(),
                        ..Default::default()
                    });
                if !name.is_empty() {
                    entry.name = name;
                }
                if scrub_scope {
                    let _ = entry.scrub.set(field, val);
                } else {
                    let _ = entry.rollback.set(field, val);
                }
                if entry.is_empty() {
                    st.player_overrides.remove(&uuid);
                }
            });
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
        HitAction::BatchPlayerOverrides(uuid, name, scrub_scope, val) => {
            with_state_mut(|st| {
                let entry = st
                    .player_overrides
                    .entry(uuid)
                    .or_insert_with(|| PlayerProfileOverrides {
                        name: name.clone(),
                        ..Default::default()
                    });
                if !name.is_empty() {
                    entry.name = name;
                }
                if scrub_scope {
                    entry.scrub.set_all(val);
                } else {
                    entry.rollback.set_all(val);
                }
                if entry.is_empty() {
                    st.player_overrides.remove(&uuid);
                }
            });
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
        HitAction::ResetPlayerOverrides(uuid, scrub_scope) => {
            with_state_mut(|st| {
                if let Some(entry) = st.player_overrides.get_mut(&uuid) {
                    if scrub_scope {
                        entry.scrub.set_all(OverrideValue::Global);
                    } else {
                        entry.rollback.set_all(OverrideValue::Global);
                    }
                    if entry.is_empty() {
                        st.player_overrides.remove(&uuid);
                    }
                }
            });
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
        HitAction::TogglePinPlayer(uuid) => {
            with_state_mut(|st| {
                if let Some(pos) = st.anchor.pinned.iter().position(|u| *u == uuid) {
                    st.anchor.pinned.remove(pos);
                } else {
                    st.anchor.pinned.push(uuid);
                    if st.anchor.mode == AnchorMode::Everyone || st.anchor.mode == AnchorMode::Rotating {
                        st.anchor.mode = AnchorMode::Multi;
                    }
                }
            });
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
        HitAction::ClearPins => {
            with_state_mut(|st| st.anchor.pinned.clear());
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
        HitAction::BatchGroup(group, val) => {
            with_state_mut(|st| {
                for e in SETTINGS_CATALOG {
                    if e.group == group {
                        write_bool_setting(st, e.id, val);
                    }
                }
            });
            UI_STATE.with(|cell| cell.borrow_mut().pending_save = true);
        }
        HitAction::ResetDefaults => {
            with_state_mut(|st| {
                let slot = st.current_slot.clone();
                let has_cp = st.has_checkpoint;
                let last_save = st.last_save_tick;
                let players = st.online_players.clone();
                *st = RReturnState::default();
                st.current_slot = slot;
                st.has_checkpoint = has_cp;
                st.last_save_tick = last_save;
                st.online_players = players;
                sync_scrub_atomics_from_state(st);
            });
            UI_STATE.with(|cell| {
                let mut ui = cell.borrow_mut();
                ui.pending_save = true;
                ui.pending_scrub_sync = Some(false);
            });
        }
        HitAction::ScrollTrack { track_y, track_h, max_scroll } => {
            let frac = ((my - track_y) / track_h.max(1.0)).clamp(0.0, 1.0);
            let target = (frac * max_scroll).clamp(0.0, max_scroll);
            UI_STATE.with(|cell| {
                let mut ui = cell.borrow_mut();
                ui.target_scroll_y = target;
                ui.scroll_y = target;
                ui.dragging_scroll = Some((track_y, track_h, max_scroll));
            });
        }
    }
}

pub fn handle_screen_mouse_released(game: &Game<'_>, button: i32, canceled: &mut bool) {
    if is_scrubbing() {
        handle_scrub_hud_mouse_released(button);
        *canceled = true;
        return;
    }
    if consume_scrub_hud_release() {
        *canceled = true;
        return;
    }
    if !is_settings_ui_open(game) {
        return;
    }
    *canceled = true;
    if button == 0 {
        UI_STATE.with(|cell| {
            let mut ui = cell.borrow_mut();
            if ui.dragging_slider.take().is_some() {
                ui.pending_save = true;
            }
            ui.dragging_scroll = None;
        });
    }
}

pub fn handle_screen_mouse_scrolled(game: &Game<'_>, scroll_y: f64, canceled: &mut bool) {
    if is_scrubbing() && with_state(|st| st.scrub_enabled) {
        if apply_scrub_wheel_delta(scroll_y) {
            *canceled = true;
        }
        return;
    }
    if !is_settings_ui_open(game) {
        return;
    }
    *canceled = true;
    UI_STATE.with(|cell| {
        let mut ui = cell.borrow_mut();
        ui.target_scroll_y = (ui.target_scroll_y - scroll_y as f32 * 26.0).clamp(0.0, ui.max_scroll_y);
    });
}

pub fn handle_screen_key_pressed(
    game: &Game<'_>,
    key: &str,
    repeat: bool,
    modifiers: u32,
    canceled: &mut bool,
) {
    if handle_screen_key_pressed_any(game, key, repeat, modifiers, canceled) {
        return;
    }
    if !is_settings_ui_open(game) {
        return;
    }
    let (settings_key, settings_mods) = game
        .mod_key(MOD_ID, "settings_menu")
        .map(|(k, m)| (k, m.bits & 7))
        .unwrap_or_else(|| ("key.keyboard.k".to_owned(), 0));

    if key == "key.keyboard.escape" {
        let cleared = UI_STATE.with(|cell| {
            let mut ui = cell.borrow_mut();
            if ui.search_focused && !ui.search_query.is_empty() {
                ui.search_query.clear();
                ui.scroll_y = 0.0;
                ui.target_scroll_y = 0.0;
                true
            } else {
                ui.search_focused = false;
                false
            }
        });
        if cleared {
            *canceled = true;
        }
        return;
    }
    *canceled = true;

    if (modifiers & 2) != 0 && key == "key.keyboard.f" {
        UI_STATE.with(|cell| cell.borrow_mut().search_focused = true);
        return;
    }
    if UI_STATE.with(|cell| cell.borrow().search_focused) {
        if key == "key.keyboard.backspace" {
            UI_STATE.with(|cell| {
                let mut ui = cell.borrow_mut();
                ui.search_query.pop();
                ui.scroll_y = 0.0;
                ui.target_scroll_y = 0.0;
            });
        } else if key == "key.keyboard.enter" || key == "key.keyboard.keypad_enter" {
            UI_STATE.with(|cell| cell.borrow_mut().search_focused = false);
        }
        return;
    }
    if !settings_key.is_empty()
        && settings_key != "key.keyboard.unknown"
        && key == settings_key
        && (settings_mods == 0 || ((modifiers & 7) & settings_mods) == settings_mods)
    {
        UI_STATE.with(|cell| cell.borrow_mut().pending_close = true);
        return;
    }
    match key {
        "key.keyboard.up" => UI_STATE.with(|cell| {
            let mut ui = cell.borrow_mut();
            ui.target_scroll_y = (ui.target_scroll_y - 28.0).clamp(0.0, ui.max_scroll_y);
        }),
        "key.keyboard.down" => UI_STATE.with(|cell| {
            let mut ui = cell.borrow_mut();
            ui.target_scroll_y = (ui.target_scroll_y + 28.0).clamp(0.0, ui.max_scroll_y);
        }),
        "key.keyboard.page_up" => UI_STATE.with(|cell| {
            let mut ui = cell.borrow_mut();
            ui.target_scroll_y = (ui.target_scroll_y - 120.0).clamp(0.0, ui.max_scroll_y);
        }),
        "key.keyboard.page_down" => UI_STATE.with(|cell| {
            let mut ui = cell.borrow_mut();
            ui.target_scroll_y = (ui.target_scroll_y + 120.0).clamp(0.0, ui.max_scroll_y);
        }),
        "key.keyboard.slash" => UI_STATE.with(|cell| cell.borrow_mut().search_focused = true),
        _ => {}
    }
}

pub fn handle_screen_char_typed(game: &Game<'_>, ch: char, modifiers: u32, canceled: &mut bool) {
    if !is_settings_ui_open(game) {
        return;
    }
    *canceled = true;
    if (modifiers & 2) != 0 || ch.is_control() {
        return;
    }
    UI_STATE.with(|cell| {
        let mut ui = cell.borrow_mut();
        if !ui.search_focused {
            return;
        }
        if ui.search_query.chars().count() < 48 {
            ui.search_query.push(ch);
        }
        ui.scroll_y = 0.0;
        ui.target_scroll_y = 0.0;
    });
}
