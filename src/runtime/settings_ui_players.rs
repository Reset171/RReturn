use super::policy::*;
use super::settings_catalog::{policy_field, SliderUnit, PLAYER_OVERRIDE_SPECS};
use super::settings_ui_draw::*;
use super::settings_ui_icons::{draw_ui_icon, UiIcon};
use super::settings_ui_tabs::{draw_compact_slider_cell, draw_compact_slider_row, push_clipped_region, SliderSpec};
use super::types::*;
use horizon_client::hud::HudBatch;

struct ListedPlayer {
    uuid: u128,
    name: String,
    dimension: String,
    is_owner: bool,
    online: bool,
    rb_custom: usize,
    sc_custom: usize,
}

fn collect_listed_players(st: &RReturnState) -> Vec<ListedPlayer> {
    let mut out = Vec::new();
    for p in &st.online_players {
        let prof = st.player_overrides.get(&p.uuid);
        out.push(ListedPlayer {
            uuid: p.uuid,
            name: p.name.clone(),
            dimension: p.dimension.clone(),
            is_owner: p.is_owner,
            online: true,
            rb_custom: prof.map_or(0, |pr| pr.rollback.count_custom()),
            sc_custom: prof.map_or(0, |pr| pr.scrub.count_custom()),
        });
    }
    let mut offline: Vec<(u128, &PlayerProfileOverrides)> = st
        .player_overrides
        .iter()
        .filter(|(u, _)| !st.online_players.iter().any(|p| p.uuid == **u))
        .map(|(u, p)| (*u, p))
        .collect();
    offline.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()));
    for (uuid, prof) in offline {
        out.push(ListedPlayer {
            uuid,
            name: if prof.name.is_empty() {
                format!("{:08x}", (uuid >> 96) as u32)
            } else {
                prof.name.clone()
            },
            dimension: "offline".to_owned(),
            is_owner: false,
            online: false,
            rb_custom: prof.rollback.count_custom(),
            sc_custom: prof.scrub.count_custom(),
        });
    }
    out
}

fn sw_anim(ui: &mut SettingsUiState, key: &str, val: bool) -> f32 {
    let target = if val { 1.0f32 } else { 0.0f32 };
    let e = ui.toggle_anim.entry(key.to_owned()).or_insert(target);
    *e += (target - *e) * 0.28;
    if (*e - target).abs() < 0.01 {
        *e = target;
    }
    *e
}

fn draw_anchor_bar<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    anchor_cfg: &AnchorSettings,
    x: f32,
    mut y: f32,
    w: f32,
    list_bounds: [f32; 4],
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    let start_y = y;
    let [_, ly, _, lh] = list_bounds;
    let [mx, my] = mouse;
    let in_list = hit_in_rect(mx, my, list_bounds);

    if y + 14.0 >= ly - 2.0 && y <= ly + lh + 2.0 {
        let mut right_w = 0.0f32;
        if !anchor_cfg.pinned.is_empty() {
            let clr_lbl = tr("rreturn.ui.clear_pins");
            let cw = canvas.text_width(draw, &clr_lbl) + 6.0;
            right_w = cw + 8.0;
            let crect = [x + w - cw - 4.0, y, cw, 13.0];
            let chov = in_list && hit_in_rect(mx, my, crect);
            canvas.text(draw, &clr_lbl, [crect[0] + 3.0, y + 2.0], if chov { UI_RED } else { UI_BLUE }, false);
            push_clipped_region(regions, list_bounds, HitRegion { tag: "anchor.clear_pins".to_owned(), rect: crect, action: HitAction::ClearPins });
        }
        canvas.text(
            draw,
            &fit_text(canvas, draw, &tr("rreturn.ui.group.anchor_mode"), (w - right_w - 10.0).max(20.0)),
            [x + 6.0, y + 2.0],
            UI_TEXT_SEC,
            false,
        );
    }
    y += 14.0;

    let rotating = anchor_cfg.mode == AnchorMode::Rotating;
    let two_cols = rotating && w >= 330.0;
    let mode_row_h = 28.0;
    let slider_h = if two_cols { 36.0 } else if rotating { 60.0 } else { 30.0 };
    let card_h = mode_row_h + slider_h;

    if y + card_h >= ly - 4.0 && y <= ly + lh + 4.0 {
        draw_rounded_rect(canvas, draw, [x, y, w, card_h], 8.0, UI_CARD_BG);

        // Row 1: Anchor mode label + segmented dock on right
        let dock_w = if w < 330.0 { 146.0 } else { (w * 0.48).clamp(160.0, 224.0) };
        let dock = [x + w - dock_w - 10.0, y + 5.0, dock_w, 18.0];
        let tmax = (dock[0] - x - 18.0).max(30.0);
        canvas.text(draw, &fit_text(canvas, draw, &tr("rreturn.ui.opt.anchor_mode"), tmax), [x + 12.0, y + 10.0], UI_TEXT_PRI, false);

        draw_rounded_rect(canvas, draw, dock, 6.0, UI_SEG_DOCK);
        let segs = [
            (AnchorMode::Fixed, "rreturn.ui.seg.fixed"),
            (AnchorMode::Multi, "rreturn.ui.seg.multi"),
            (AnchorMode::Everyone, "rreturn.ui.seg.everyone"),
            (AnchorMode::Rotating, "rreturn.ui.seg.rotating"),
        ];
        let seg_w = (dock[2] - 4.0) / 4.0;
        for (i, &(m, key)) in segs.iter().enumerate() {
            let sr = [dock[0] + 2.0 + i as f32 * seg_w, dock[1] + 2.0, seg_w - 1.0, dock[3] - 4.0];
            let active = anchor_cfg.mode == m;
            let hov = in_list && hit_in_rect(mx, my, sr);
            let bg = if active { UI_BLUE } else if hov { UI_PILL_BG } else { 0 };
            draw_pill(canvas, draw, sr, &fit_text(canvas, draw, &tr(key), sr[2] - 4.0), bg, if active { UI_TEXT_PRI } else { UI_TEXT_SEC });
            push_clipped_region(regions, list_bounds, HitRegion { tag: format!("anchor.mode.{}", m.id()), rect: sr, action: HitAction::SetAnchorMode(m) });
        }

        // Row 2: Death rollback cooldown + optional rotation interval
        let ry = y + mode_row_h;
        canvas.fill(draw, [x + 12.0, ry, w - 24.0, 1.0], UI_DIVIDER);
        let cd_spec = SliderSpec {
            id: "rollback_cooldown_sec",
            label_key: "rreturn.ui.opt.rollback_cooldown_sec",
            desc_key: "",
            min: 0,
            max: 60,
            step: 1,
            unit: SliderUnit::Sec,
        };
        let rot_spec = SliderSpec {
            id: "anchor_rotation_sec",
            label_key: "rreturn.ui.opt.anchor_rotation_sec",
            desc_key: "",
            min: 10,
            max: 900,
            step: 10,
            unit: SliderUnit::Sec,
        };
        if two_cols {
            let half = (w * 0.5).floor();
            draw_compact_slider_cell(canvas, draw, SettingsUiMode::World, cd_spec, [x, ry, half, 36.0], list_bounds, mouse, regions);
            canvas.fill(draw, [x + half, ry + 6.0, 1.0, 24.0], UI_DIVIDER);
            draw_compact_slider_cell(canvas, draw, SettingsUiMode::World, rot_spec, [x + half, ry, w - half, 36.0], list_bounds, mouse, regions);
        } else {
            draw_compact_slider_row(canvas, draw, SettingsUiMode::World, cd_spec, [x, ry, w, 30.0], list_bounds, mouse, regions);
            if rotating {
                canvas.fill(draw, [x + 12.0, ry + 30.0, w - 24.0, 1.0], UI_DIVIDER);
                draw_compact_slider_row(canvas, draw, SettingsUiMode::World, rot_spec, [x, ry + 30.0, w, 30.0], list_bounds, mouse, regions);
            }
        }
    }
    y += card_h + 6.0;
    y - start_y
}

fn draw_players_column<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    ui: &mut SettingsUiState,
    players: &[ListedPlayer],
    anchor_cfg: &AnchorSettings,
    active_anchors: &[u128],
    x: f32,
    mut y: f32,
    w: f32,
    list_bounds: [f32; 4],
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    let start_y = y;
    let [_, ly, _, lh] = list_bounds;
    let [mx, my] = mouse;
    let in_list = hit_in_rect(mx, my, list_bounds);

    if y + 14.0 >= ly - 2.0 && y <= ly + lh + 2.0 {
        let hdr = format!("{} ({})", tr("rreturn.ui.group.anchor_players"), players.len());
        canvas.text(draw, &fit_text(canvas, draw, &hdr, w - 12.0), [x + 6.0, y + 2.0], UI_TEXT_SEC, false);
    }
    y += 14.0;

    if players.is_empty() {
        let eh = 34.0;
        if y + eh >= ly - 4.0 && y <= ly + lh + 4.0 {
            draw_rounded_rect(canvas, draw, [x, y, w, eh], 8.0, UI_CARD_BG);
            canvas.text(draw, &fit_text(canvas, draw, &tr("rreturn.ui.no_players"), w - 20.0), [x + 10.0, y + 13.0], UI_TEXT_MUTED, false);
        }
        return (y + eh) - start_y;
    }

    let row_h = 32.0;
    let total_h = players.len() as f32 * row_h;
    if y + total_h >= ly - 4.0 && y <= ly + lh + 4.0 {
        draw_rounded_rect(canvas, draw, [x, y, w, total_h], 8.0, UI_CARD_BG);
    }

    for (i, p) in players.iter().enumerate() {
        let py = y + i as f32 * row_h;
        if py + row_h >= ly - 2.0 && py <= ly + lh + 2.0 {
            let pr = [x, py, w, row_h];
            let selected = ui.selected_player == Some(p.uuid);
            let hov = in_list && hit_in_rect(mx, my, pr);
            if selected {
                draw_rounded_rect(canvas, draw, [x + 4.0, py + 2.0, w - 8.0, row_h - 4.0], 5.0, UI_BLUE_SOFT);
                draw_rounded_rect(canvas, draw, [x + 4.0, py + 7.0, 2.5, row_h - 14.0], 1.0, UI_BLUE);
            } else if hov {
                draw_rounded_rect(canvas, draw, [x + 4.0, py + 2.0, w - 8.0, row_h - 4.0], 5.0, UI_ROW_HOV);
            }
            if i > 0 {
                canvas.fill(draw, [x + 10.0, py, w - 20.0, 1.0], UI_DIVIDER);
            }

            let is_pinned = anchor_cfg.pinned.contains(&p.uuid);
            let is_active = active_anchors.contains(&p.uuid);
            let key = format!("pin.{:032x}", p.uuid);
            let anim = sw_anim(ui, &key, is_pinned);
            let pin_rect = [x + w - 38.0, py + 6.0, 34.0, 24.0];
            draw_switch(canvas, draw, x + w - 35.0, py + 11.0, anim);

            let dot_col = if p.online { UI_GREEN } else { UI_TEXT_MUTED };
            draw_rounded_rect(canvas, draw, [x + 11.0, py + 9.0, 5.0, 5.0], 2.5, dot_col);

            let mut name_x = x + 19.0;
            if p.is_owner {
                draw_ui_icon(canvas, draw, UiIcon::Crown, name_x, py + 6.5, UI_BLUE);
                name_x += 13.0;
            }
            let custom_total = p.rb_custom + p.sc_custom;
            let mut badge_w = 0.0f32;
            if custom_total > 0 && w >= 155.0 {
                let b_str = format!("{}", custom_total);
                let bw = (canvas.text_width(draw, &b_str) + 10.0).max(16.0);
                draw_pill(canvas, draw, [pin_rect[0] - bw - 3.0, py + 11.0, bw, 13.0], &b_str, UI_BLUE_SOFT, UI_BLUE);
                badge_w = bw + 6.0;
            }

            let tmax = (pin_rect[0] - name_x - 4.0 - badge_w).max(20.0);
            canvas.text(draw, &fit_text(canvas, draw, &p.name, tmax), [name_x, py + 7.5], UI_TEXT_PRI, false);

            let status_str = if is_pinned {
                tr("rreturn.ui.player.pinned")
            } else if is_active {
                tr("rreturn.ui.player.active")
            } else if !p.online {
                tr("rreturn.ui.player.offline")
            } else {
                tr("rreturn.ui.player.observer")
            };
            canvas.text(
                draw,
                &fit_text(canvas, draw, &status_str, (pin_rect[0] - x - 18.0 - badge_w).max(20.0)),
                [x + 19.0, py + 19.5],
                if is_pinned { UI_BLUE } else if is_active { UI_GREEN } else { UI_TEXT_MUTED },
                false,
            );

            push_clipped_region(regions, list_bounds, HitRegion {
                tag: format!("select_player.{}", i),
                rect: [x, py, (w - 38.0).max(20.0), row_h],
                action: HitAction::SelectPlayer(p.uuid, p.name.clone()),
            });
            push_clipped_region(regions, list_bounds, HitRegion {
                tag: format!("pin.{}", i),
                rect: pin_rect,
                action: HitAction::TogglePinPlayer(p.uuid),
            });
        }
    }
    y += total_h;
    y - start_y
}

fn draw_player_rules_column<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    sel_player: &ListedPlayer,
    profile_map: &std::collections::HashMap<u128, PlayerProfileOverrides>,
    global_rb: &RollbackPolicy,
    global_sc: &RollbackPolicy,
    scrub_scope: bool,
    x: f32,
    mut y: f32,
    w: f32,
    list_bounds: [f32; 4],
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    let start_y = y;
    let [_, ly, _, lh] = list_bounds;
    let [mx, my] = mouse;
    let in_list = hit_in_rect(mx, my, list_bounds);

    let sel_uuid = sel_player.uuid;
    let sel_name = sel_player.name.clone();
    let base_pol = if scrub_scope { global_sc } else { global_rb };
    let empty_ov = PlayerPolicyOverrides::default();
    let prof_opt = profile_map.get(&sel_uuid);
    let cur_ov = prof_opt
        .map(|pr| if scrub_scope { &pr.scrub } else { &pr.rollback })
        .unwrap_or(&empty_ov);

    if y + 14.0 >= ly - 2.0 && y <= ly + lh + 2.0 {
        let mut right_used = 0.0f32;
        if !cur_ov.is_empty() {
            let rst_lbl = tr("rreturn.ui.player.reset_rules");
            let rw = canvas.text_width(draw, &rst_lbl) + 6.0;
            right_used = rw + 8.0;
            let rrect = [x + w - rw - 4.0, y, rw, 13.0];
            let rhov = in_list && hit_in_rect(mx, my, rrect);
            canvas.text(draw, &rst_lbl, [rrect[0] + 3.0, y + 2.0], if rhov { UI_RED } else { UI_BLUE }, false);
            push_clipped_region(regions, list_bounds, HitRegion {
                tag: "player.reset_rules".to_owned(),
                rect: rrect,
                action: HitAction::ResetPlayerOverrides(sel_uuid, scrub_scope),
            });
        }
        let hdr_lbl = tr_args("rreturn.ui.group.player_rules", &[&sel_name]);
        canvas.text(draw, &fit_text(canvas, draw, &hdr_lbl, (w - right_used - 10.0).max(20.0)), [x + 6.0, y + 2.0], UI_TEXT_SEC, false);
    }
    y += 14.0;

    let hdr_card_h = 26.0;
    let row_h = 30.0;
    let total_h = hdr_card_h + (PLAYER_OVERRIDE_SPECS.len() as f32) * row_h;
    if y + total_h >= ly - 4.0 && y <= ly + lh + 4.0 {
        draw_rounded_rect(canvas, draw, [x, y, w, total_h], 8.0, UI_CARD_BG);
    }

    // Compact Scope switcher + batch pills on single header bar
    if y + hdr_card_h >= ly - 2.0 && y <= ly + lh + 2.0 {
        let deny_lbl = tr("rreturn.ui.disable_all");
        let allow_lbl = tr("rreturn.ui.enable_all");
        let dw = canvas.text_width(draw, &deny_lbl) + 8.0;
        let aw = canvas.text_width(draw, &allow_lbl) + 8.0;
        let deny_r = [x + w - dw - 8.0, y + 6.0, dw, 14.0];
        let allow_r = [deny_r[0] - aw - 4.0, y + 6.0, aw, 14.0];
        let show_batch = allow_r[0] > x + 150.0;
        let dock_max_w = if show_batch { (allow_r[0] - x - 14.0).max(120.0) } else { (w - 16.0).max(120.0) };
        let dock = [x + 8.0, y + 4.0, dock_max_w.min(220.0), 18.0];
        draw_rounded_rect(canvas, draw, dock, 6.0, UI_SEG_DOCK);
        let sw = (dock[2] - 4.0) * 0.5;
        let rb_cnt = prof_opt.map_or(0, |pr| pr.rollback.count_custom());
        let sc_cnt = prof_opt.map_or(0, |pr| pr.scrub.count_custom());
        let rb_lbl = if rb_cnt > 0 { format!("{} ({})", tr("rreturn.ui.player.scope_rollback"), rb_cnt) } else { tr("rreturn.ui.player.scope_rollback") };
        let sc_lbl = if sc_cnt > 0 { format!("{} ({})", tr("rreturn.ui.player.scope_scrub"), sc_cnt) } else { tr("rreturn.ui.player.scope_scrub") };
        let rb_r = [dock[0] + 2.0, dock[1] + 2.0, sw - 1.0, dock[3] - 4.0];
        let sc_r = [dock[0] + 2.0 + sw, dock[1] + 2.0, sw - 1.0, dock[3] - 4.0];
        draw_pill(canvas, draw, rb_r, &fit_text(canvas, draw, &rb_lbl, rb_r[2] - 4.0), if !scrub_scope { UI_BLUE } else if in_list && hit_in_rect(mx, my, rb_r) { UI_PILL_BG } else { 0 }, if !scrub_scope { UI_TEXT_PRI } else { UI_TEXT_SEC });
        draw_pill(canvas, draw, sc_r, &fit_text(canvas, draw, &sc_lbl, sc_r[2] - 4.0), if scrub_scope { UI_BLUE } else if in_list && hit_in_rect(mx, my, sc_r) { UI_PILL_BG } else { 0 }, if scrub_scope { UI_TEXT_PRI } else { UI_TEXT_SEC });
        push_clipped_region(regions, list_bounds, HitRegion { tag: "p_scope.rollback".to_owned(), rect: rb_r, action: HitAction::SetPlayerScope(false) });
        push_clipped_region(regions, list_bounds, HitRegion { tag: "p_scope.scrub".to_owned(), rect: sc_r, action: HitAction::SetPlayerScope(true) });

        if show_batch {
            let ahov = in_list && hit_in_rect(mx, my, allow_r);
            let dhov = in_list && hit_in_rect(mx, my, deny_r);
            draw_pill(canvas, draw, allow_r, &allow_lbl, if ahov { UI_BLUE } else { UI_BLUE_SOFT }, if ahov { UI_TEXT_PRI } else { UI_BLUE });
            draw_pill(canvas, draw, deny_r, &deny_lbl, if dhov { UI_RED } else { UI_PILL_BG }, if dhov { UI_TEXT_PRI } else { UI_TEXT_SEC });
            push_clipped_region(regions, list_bounds, HitRegion {
                tag: "player.batch_allow".to_owned(),
                rect: allow_r,
                action: HitAction::BatchPlayerOverrides(sel_uuid, sel_name.clone(), scrub_scope, OverrideValue::Allow),
            });
            push_clipped_region(regions, list_bounds, HitRegion {
                tag: "player.batch_deny".to_owned(),
                rect: deny_r,
                action: HitAction::BatchPlayerOverrides(sel_uuid, sel_name.clone(), scrub_scope, OverrideValue::Deny),
            });
        }
    }
    y += hdr_card_h;

    let dock_w = if w < 260.0 { 110.0 } else { 130.0 };
    for &(field, label_key, desc_key) in PLAYER_OVERRIDE_SPECS {
        if y + row_h >= ly - 2.0 && y <= ly + lh + 2.0 {
            if in_list && hit_in_rect(mx, my, [x, y, w, row_h]) {
                draw_rounded_rect(canvas, draw, [x + 4.0, y + 2.0, w - 8.0, row_h - 4.0], 5.0, UI_ROW_HOV);
            }
            canvas.fill(draw, [x + 12.0, y, w - 24.0, 1.0], UI_DIVIDER);

            let dock = [x + w - dock_w - 10.0, y + 6.0, dock_w, 18.0];
            draw_rounded_rect(canvas, draw, dock, 6.0, UI_SEG_DOCK);

            let ov_val = cur_ov.get(field);
            let global_val = policy_field(base_pol, field);
            let glob_lbl = if global_val { tr("rreturn.ui.tri.global_on") } else { tr("rreturn.ui.tri.global_off") };
            let yes_lbl = tr("rreturn.ui.tri.yes");
            let no_lbl = tr("rreturn.ui.tri.no");

            let inner_w = dock[2] - 4.0;
            let w_glob = (inner_w * 0.48).floor();
            let w_yes = (inner_w * 0.26).floor();
            let w_no = inner_w - w_glob - w_yes;

            let rg = [dock[0] + 2.0, dock[1] + 2.0, w_glob - 1.0, dock[3] - 4.0];
            let ry = [dock[0] + 2.0 + w_glob, dock[1] + 2.0, w_yes - 1.0, dock[3] - 4.0];
            let rn = [dock[0] + 2.0 + w_glob + w_yes, dock[1] + 2.0, w_no, dock[3] - 4.0];

            let mut draw_seg = |rect: [f32; 4], txt: &str, active: bool, active_bg: u32, active_fg: u32, inactive_fg: u32| {
                let hov = in_list && hit_in_rect(mx, my, rect);
                let bg = if active { active_bg } else if hov { UI_PILL_BG } else { 0 };
                let fg = if active { active_fg } else { inactive_fg };
                draw_pill(canvas, draw, rect, &fit_text(canvas, draw, txt, rect[2] - 4.0), bg, fg);
            };
            draw_seg(rg, &glob_lbl, ov_val == OverrideValue::Global, UI_PILL_HOV, if global_val { UI_BLUE } else { UI_TEXT_PRI }, UI_TEXT_MUTED);
            draw_seg(ry, &yes_lbl, ov_val == OverrideValue::Allow, UI_GREEN, 0xFF0A140D, UI_TEXT_SEC);
            draw_seg(rn, &no_lbl, ov_val == OverrideValue::Deny, UI_RED, UI_TEXT_PRI, UI_TEXT_SEC);

            let tmax = (dock[0] - x - 18.0).max(20.0);
            canvas.text(draw, &fit_text(canvas, draw, &tr(label_key), tmax), [x + 12.0, y + 5.0], UI_TEXT_PRI, false);
            canvas.text(draw, &fit_text(canvas, draw, &tr(desc_key), tmax), [x + 12.0, y + 17.0], UI_TEXT_MUTED, false);

            push_clipped_region(regions, list_bounds, HitRegion {
                tag: format!("p_rule.{}.global", field),
                rect: rg,
                action: HitAction::SetPlayerOverride(sel_uuid, sel_name.clone(), scrub_scope, field, OverrideValue::Global),
            });
            push_clipped_region(regions, list_bounds, HitRegion {
                tag: format!("p_rule.{}.allow", field),
                rect: ry,
                action: HitAction::SetPlayerOverride(sel_uuid, sel_name.clone(), scrub_scope, field, OverrideValue::Allow),
            });
            push_clipped_region(regions, list_bounds, HitRegion {
                tag: format!("p_rule.{}.deny", field),
                rect: rn,
                action: HitAction::SetPlayerOverride(sel_uuid, sel_name.clone(), scrub_scope, field, OverrideValue::Deny),
            });
        }
        y += row_h;
    }

    y - start_y
}

pub fn draw_players_tab<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    ui: &mut SettingsUiState,
    game_time: i64,
    list_bounds: [f32; 4],
    scroll_y: f32,
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    let [x, ly, w, _] = list_bounds;
    let mut y = ly - scroll_y;
    let start_y = y;

    let (anchor_cfg, players, global_rb, global_sc, profile_map) = with_state(|st| {
        (
            st.anchor.clone(),
            collect_listed_players(st),
            st.policy,
            st.scrub_policy,
            st.player_overrides.clone(),
        )
    });

    if let Some(first) = players.first() {
        let valid = ui
            .selected_player
            .is_some_and(|sel| players.iter().any(|p| p.uuid == sel));
        if !valid {
            ui.selected_player = Some(first.uuid);
        }
    } else {
        ui.selected_player = None;
    }

    ui.section_offsets.insert("players_anchor".to_owned(), 0.0);
    y += draw_anchor_bar(canvas, draw, &anchor_cfg, x, y, w, list_bounds, mouse, regions);

    let online_uuids: Vec<u128> = players.iter().filter(|p| p.online).map(|p| p.uuid).collect();
    let active_anchors = resolve_anchors(&anchor_cfg, &online_uuids, game_time, None);

    ui.section_offsets
        .insert("players_list".to_owned(), (y - start_y).max(0.0));
    ui.section_offsets
        .insert("players_rules".to_owned(), (y - start_y).max(0.0));

    let sel_player = ui
        .selected_player
        .and_then(|sel| players.iter().find(|p| p.uuid == sel));

    if w >= 370.0 && sel_player.is_some() {
        let left_w = (w * 0.35).clamp(144.0, 196.0).floor();
        let gap = 8.0;
        let right_x = x + left_w + gap;
        let right_w = (w - left_w - gap).max(160.0);

        let right_h = draw_player_rules_column(
            canvas,
            draw,
            sel_player.unwrap(),
            &profile_map,
            &global_rb,
            &global_sc,
            ui.player_scope_scrub,
            right_x,
            y,
            right_w,
            list_bounds,
            mouse,
            regions,
        );

        let left_est_h = 14.0 + (players.len().max(1) as f32) * 32.0;
        let max_sticky = y + (right_h - left_est_h).max(0.0);
        let sticky_y = y.max(ly).min(max_sticky);
        let left_h = draw_players_column(
            canvas,
            draw,
            ui,
            &players,
            &anchor_cfg,
            &active_anchors,
            x,
            sticky_y,
            left_w,
            list_bounds,
            mouse,
            regions,
        );
        return (y - start_y) + right_h.max(left_h);
    }

    let left_h = draw_players_column(
        canvas,
        draw,
        ui,
        &players,
        &anchor_cfg,
        &active_anchors,
        x,
        y,
        w,
        list_bounds,
        mouse,
        regions,
    );
    y += left_h + 6.0;

    if let Some(sp) = sel_player {
        ui.section_offsets
            .insert("players_rules".to_owned(), (y - start_y).max(0.0));
        y += draw_player_rules_column(
            canvas,
            draw,
            sp,
            &profile_map,
            &global_rb,
            &global_sc,
            ui.player_scope_scrub,
            x,
            y,
            w,
            list_bounds,
            mouse,
            regions,
        );
    }

    (y - start_y).max(0.0)
}
