use super::super::Game;
use super::policy::*;
use super::settings_catalog::{policy_field, PLAYER_OVERRIDE_SPECS};
use super::settings_ui_draw::*;
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

fn push_clipped_hit(out: &mut Vec<HitRegion>, bounds: [f32; 4], mut reg: HitRegion) {
    let x0 = reg.rect[0].max(bounds[0]);
    let y0 = reg.rect[1].max(bounds[1]);
    let x1 = (reg.rect[0] + reg.rect[2]).min(bounds[0] + bounds[2]);
    let y1 = (reg.rect[1] + reg.rect[3]).min(bounds[1] + bounds[3]);
    if x1 > x0 && y1 > y0 {
        reg.rect = [x0, y0, x1 - x0, y1 - y0];
        out.push(reg);
    }
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

pub fn draw_players_tab(
    game: &Game<'_>,
    draw: &mut HudBatch,
    ui: &mut SettingsUiState,
    list_bounds: [f32; 4],
    scroll_y: f32,
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    let [x, ly, w, _] = list_bounds;
    let [mx, my] = mouse;
    let in_list = hit_in_rect(mx, my, list_bounds);
    let mut y = ly - scroll_y;
    let start_y = y;

    let (anchor_cfg, players, game_time, global_rb, global_sc, profile_map) = with_state(|st| {
        (
            st.anchor.clone(),
            collect_listed_players(st),
            game.world.game_time,
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

    // Section 1: Anchor Mode & Rotation
    ui.section_offsets
        .insert("players_anchor".to_owned(), (y - start_y).max(0.0));
    if y + 16.0 >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
        game.native_text(
            draw,
            &tr("rreturn.ui.group.anchor_mode"),
            [x + 6.0, y + 3.0],
            UI_TEXT_SEC,
            false,
        );
    }
    y += 16.0;

    let card_h = 84.0;
    if y + card_h >= ly - 4.0 && y <= ly + list_bounds[3] + 4.0 {
        draw_rounded_rect(game, draw, [x, y, w, card_h], 8.0, UI_CARD_BG);
        game.native_text(
            draw,
            &fit_text(game, draw, &tr("rreturn.ui.opt.anchor_mode"), w - 24.0),
            [x + 12.0, y + 6.0],
            UI_TEXT_PRI,
            false,
        );
        let dock = [x + 12.0, y + 19.0, w - 24.0, 18.0];
        draw_rounded_rect(game, draw, dock, 6.0, UI_SEG_DOCK);
        let segs = [
            (AnchorMode::Fixed, "rreturn.ui.seg.fixed"),
            (AnchorMode::Multi, "rreturn.ui.seg.multi"),
            (AnchorMode::Everyone, "rreturn.ui.seg.everyone"),
            (AnchorMode::Rotating, "rreturn.ui.seg.rotating"),
        ];
        let seg_w = (dock[2] - 4.0) / 4.0;
        for (i, &(m, key)) in segs.iter().enumerate() {
            let sr = [
                dock[0] + 2.0 + i as f32 * seg_w,
                dock[1] + 2.0,
                seg_w - 1.0,
                dock[3] - 4.0,
            ];
            let active = anchor_cfg.mode == m;
            let hov = in_list && hit_in_rect(mx, my, sr);
            if active || hov {
                draw_rounded_rect(
                    game,
                    draw,
                    sr,
                    4.0,
                    if active { UI_BLUE } else { UI_PILL_BG },
                );
            }
            let lbl = fit_text(game, draw, &tr(key), sr[2] - 4.0);
            let tw = game.native_text_width(draw, &lbl);
            game.native_text(
                draw,
                &lbl,
                [sr[0] + (sr[2] - tw) * 0.5, sr[1] + 3.0],
                if active { UI_TEXT_PRI } else { UI_TEXT_SEC },
                false,
            );
            push_clipped_hit(
                regions,
                list_bounds,
                HitRegion {
                    tag: format!("anchor.mode.{}", m.id()),
                    rect: sr,
                    action: HitAction::SetAnchorMode(m),
                },
            );
        }

        game.native_fill(draw, [x + 12.0, y + 42.0, w - 24.0, 1.0], UI_DIVIDER);
        let ry = y + 43.0;
        let rot = anchor_cfg.rotation_seconds;
        let vstr = format!("{} {}", rot, tr("rreturn.ui.sec_unit"));
        let vw = game.native_text_width(draw, &vstr);
        let p_rect = [x + w - 25.0, ry + 5.5, 14.0, 13.0];
        let vx = p_rect[0] - vw - 5.0;
        let m_rect = [vx - 19.0, ry + 5.5, 14.0, 13.0];
        game.native_text(
            draw,
            &fit_text(
                game,
                draw,
                &tr("rreturn.ui.opt.anchor_rotation_sec"),
                (m_rect[0] - x - 18.0).max(20.0),
            ),
            [x + 12.0, ry + 8.0],
            UI_TEXT_PRI,
            false,
        );
        draw_pill(
            game,
            draw,
            m_rect,
            "-",
            if in_list && hit_in_rect(mx, my, m_rect) {
                UI_PILL_HOV
            } else {
                UI_PILL_BG
            },
            UI_TEXT_PRI,
        );
        game.native_text(draw, &vstr, [vx, ry + 8.0], UI_BLUE, false);
        draw_pill(
            game,
            draw,
            p_rect,
            "+",
            if in_list && hit_in_rect(mx, my, p_rect) {
                UI_PILL_HOV
            } else {
                UI_PILL_BG
            },
            UI_TEXT_PRI,
        );

        let tx = x + 12.0;
        let tw = (w - 24.0).max(40.0);
        let ty = ry + 28.0;
        let frac = ((rot - 10) as f32 / 890.0).clamp(0.0, 1.0);
        draw_rounded_rect(game, draw, [tx, ty, tw, 4.0], 2.0, UI_TRACK_BG);
        draw_rounded_rect(game, draw, [tx, ty, (tw * frac).max(4.0), 4.0], 2.0, UI_BLUE);
        draw_rounded_rect(
            game,
            draw,
            [tx + frac * tw - 4.5, ty - 2.5, 9.0, 9.0],
            4.5,
            UI_THUMB,
        );
        push_clipped_hit(
            regions,
            list_bounds,
            HitRegion {
                tag: "minus.anchor_rotation_sec".to_owned(),
                rect: m_rect,
                action: HitAction::SliderStep("anchor_rotation_sec", -10),
            },
        );
        push_clipped_hit(
            regions,
            list_bounds,
            HitRegion {
                tag: "plus.anchor_rotation_sec".to_owned(),
                rect: p_rect,
                action: HitAction::SliderStep("anchor_rotation_sec", 10),
            },
        );
        push_clipped_hit(
            regions,
            list_bounds,
            HitRegion {
                tag: "slider.anchor_rotation_sec".to_owned(),
                rect: [tx - 4.0, ty - 6.0, tw + 8.0, 16.0],
                action: HitAction::SliderDrag {
                    id: "anchor_rotation_sec",
                    track_x: tx,
                    track_w: tw,
                    min: 10,
                    max: 900,
                    step: 10,
                },
            },
        );
    }
    y += card_h + 8.0;

    // Section 2: Players List (click row to edit per-player rules; switch on right pins Anchor)
    ui.section_offsets
        .insert("players_list".to_owned(), (y - start_y).max(0.0));
    if y + 16.0 >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
        let mut right_w = 0.0f32;
        if !anchor_cfg.pinned.is_empty() {
            let clr_lbl = tr("rreturn.ui.clear_pins");
            let cw = game.native_text_width(draw, &clr_lbl) + 6.0;
            right_w = cw + 8.0;
            let crect = [x + w - cw - 4.0, y + 1.0, cw, 13.0];
            let chov = in_list && hit_in_rect(mx, my, crect);
            game.native_text(
                draw,
                &clr_lbl,
                [crect[0] + 3.0, y + 3.0],
                if chov { UI_RED } else { UI_BLUE },
                false,
            );
            push_clipped_hit(
                regions,
                list_bounds,
                HitRegion {
                    tag: "anchor.clear_pins".to_owned(),
                    rect: crect,
                    action: HitAction::ClearPins,
                },
            );
        }
        game.native_text(
            draw,
            &fit_text(
                game,
                draw,
                &tr("rreturn.ui.group.anchor_players"),
                (w - right_w - 10.0).max(20.0),
            ),
            [x + 6.0, y + 3.0],
            UI_TEXT_SEC,
            false,
        );
    }
    y += 16.0;

    let online_uuids: Vec<u128> = players
        .iter()
        .filter(|p| p.online)
        .map(|p| p.uuid)
        .collect();
    let active_anchors = resolve_anchors(&anchor_cfg, &online_uuids, game_time, None);

    if players.is_empty() {
        let eh = 34.0;
        if y + eh >= ly - 4.0 && y <= ly + list_bounds[3] + 4.0 {
            draw_rounded_rect(game, draw, [x, y, w, eh], 8.0, UI_CARD_BG);
            game.native_text(
                draw,
                &fit_text(game, draw, &tr("rreturn.ui.no_players"), w - 24.0),
                [x + 12.0, y + 13.0],
                UI_TEXT_MUTED,
                false,
            );
        }
        y += eh + 8.0;
    } else {
        let row_h = 34.0;
        let total_h = players.len() as f32 * row_h;
        if y + total_h >= ly - 4.0 && y <= ly + list_bounds[3] + 4.0 {
            draw_rounded_rect(game, draw, [x, y, w, total_h], 8.0, UI_CARD_BG);
        }
        for (i, p) in players.iter().enumerate() {
            let py = y + i as f32 * row_h;
            let pr = [x, py, w, row_h];
            if py + row_h >= ly - 2.0 && py <= ly + list_bounds[3] + 2.0 {
                let selected = ui.selected_player == Some(p.uuid);
                let hov = in_list && hit_in_rect(mx, my, pr);
                if selected {
                    draw_rounded_rect(
                        game,
                        draw,
                        [x + 2.0, py + 1.0, w - 4.0, row_h - 2.0],
                        6.0,
                        UI_BLUE_SOFT,
                    );
                    draw_rounded_rect(
                        game,
                        draw,
                        [x + 3.0, py + 6.0, 2.5, row_h - 12.0],
                        1.0,
                        UI_BLUE,
                    );
                } else if hov {
                    draw_rounded_rect(
                        game,
                        draw,
                        [x + 2.0, py + 1.0, w - 4.0, row_h - 2.0],
                        6.0,
                        UI_ROW_HOV,
                    );
                }
                if i > 0 {
                    game.native_fill(draw, [x + 12.0, py, w - 24.0, 1.0], UI_DIVIDER);
                }

                let is_pinned = anchor_cfg.pinned.contains(&p.uuid);
                let is_active = active_anchors.contains(&p.uuid);
                let key = format!("pin.{:032x}", p.uuid);
                let anim = sw_anim(ui, &key, is_pinned);
                let pin_rect = [x + w - 46.0, py + 6.0, 38.0, 22.0];
                draw_switch(game, draw, x + w - 40.0, py + 10.0, anim);

                let custom_total = p.rb_custom + p.sc_custom;
                let mut badge_w = 0.0f32;
                if custom_total > 0 {
                    let b_str = tr_args("rreturn.ui.player.custom_badge", &[&custom_total.to_string()]);
                    let bw = game.native_text_width(draw, &b_str) + 10.0;
                    let bx = pin_rect[0] - bw - 4.0;
                    draw_pill(
                        game,
                        draw,
                        [bx, py + 10.0, bw, 14.0],
                        &b_str,
                        UI_BLUE_SOFT,
                        UI_BLUE,
                    );
                    badge_w = bw + 8.0;
                }

                let dot_col = if p.online { UI_GREEN } else { UI_TEXT_MUTED };
                draw_rounded_rect(game, draw, [x + 11.0, py + 8.5, 5.0, 5.0], 2.5, dot_col);

                let status_str = if is_pinned {
                    tr("rreturn.ui.player.pinned")
                } else if is_active {
                    tr("rreturn.ui.player.active")
                } else if !p.online {
                    tr("rreturn.ui.player.offline")
                } else {
                    tr("rreturn.ui.player.observer")
                };
                let dim_short = p.dimension.strip_prefix("minecraft:").unwrap_or(&p.dimension);
                let sub = if p.is_owner {
                    format!("{} • {} • {}", tr("rreturn.ui.player.owner"), dim_short, status_str)
                } else {
                    format!("{} • {}", dim_short, status_str)
                };
                let tmax = (pin_rect[0] - x - 24.0 - badge_w).max(20.0);
                game.native_text(
                    draw,
                    &fit_text(game, draw, &p.name, tmax),
                    [x + 20.0, py + 6.0],
                    UI_TEXT_PRI,
                    false,
                );
                game.native_text(
                    draw,
                    &fit_text(game, draw, &sub, (pin_rect[0] - x - 24.0).max(20.0)),
                    [x + 20.0, py + 18.5],
                    if is_pinned {
                        UI_BLUE
                    } else if is_active {
                        UI_GREEN
                    } else {
                        UI_TEXT_MUTED
                    },
                    false,
                );
                push_clipped_hit(
                    regions,
                    list_bounds,
                    HitRegion {
                        tag: format!("select_player.{}", i),
                        rect: [x, py, (w - 46.0).max(20.0), row_h],
                        action: HitAction::SelectPlayer(p.uuid, p.name.clone()),
                    },
                );
                push_clipped_hit(
                    regions,
                    list_bounds,
                    HitRegion {
                        tag: format!("pin.{}", i),
                        rect: pin_rect,
                        action: HitAction::TogglePinPlayer(p.uuid),
                    },
                );
            }
        }
        y += total_h + 8.0;
    }

    // Section 3: Per-Player 3-State Overrides (Global / Yes / No)
    ui.section_offsets
        .insert("players_rules".to_owned(), (y - start_y).max(0.0));
    if let Some(sel_uuid) = ui.selected_player {
        let sel_player = players.iter().find(|p| p.uuid == sel_uuid);
        let sel_name = sel_player.map_or_else(|| "Player".to_owned(), |p| p.name.clone());
        let scrub_scope = ui.player_scope_scrub;
        let base_pol = if scrub_scope { &global_sc } else { &global_rb };
        let empty_ov = PlayerPolicyOverrides::default();
        let cur_ov = profile_map
            .get(&sel_uuid)
            .map(|pr| if scrub_scope { &pr.scrub } else { &pr.rollback })
            .unwrap_or(&empty_ov);

        if y + 16.0 >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
            let mut right_used = 0.0f32;
            if !cur_ov.is_empty() {
                let rst_lbl = tr("rreturn.ui.player.reset_rules");
                let rw = game.native_text_width(draw, &rst_lbl) + 6.0;
                right_used = rw + 8.0;
                let rrect = [x + w - rw - 4.0, y + 1.0, rw, 13.0];
                let rhov = in_list && hit_in_rect(mx, my, rrect);
                game.native_text(
                    draw,
                    &rst_lbl,
                    [rrect[0] + 3.0, y + 3.0],
                    if rhov { UI_RED } else { UI_BLUE },
                    false,
                );
                push_clipped_hit(
                    regions,
                    list_bounds,
                    HitRegion {
                        tag: "player.reset_rules".to_owned(),
                        rect: rrect,
                        action: HitAction::ResetPlayerOverrides(sel_uuid, scrub_scope),
                    },
                );
            }
            let hdr_lbl = tr_args("rreturn.ui.group.player_rules", &[&sel_name]);
            game.native_text(
                draw,
                &fit_text(game, draw, &hdr_lbl, (w - right_used - 10.0).max(20.0)),
                [x + 6.0, y + 3.0],
                UI_TEXT_SEC,
                false,
            );
        }
        y += 16.0;

        let scope_bar_h = 28.0;
        let row_h = 31.0;
        let total_rules_h = scope_bar_h + (PLAYER_OVERRIDE_SPECS.len() as f32) * row_h;
        if y + total_rules_h >= ly - 4.0 && y <= ly + list_bounds[3] + 4.0 {
            draw_rounded_rect(game, draw, [x, y, w, total_rules_h], 8.0, UI_CARD_BG);
        }

        // Scope selector header row inside the rules card
        if y + scope_bar_h >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
            let dock = [x + 10.0, y + 5.0, (w - 20.0).min(210.0), 18.0];
            draw_rounded_rect(game, draw, dock, 6.0, UI_SEG_DOCK);
            let sw = (dock[2] - 4.0) * 0.5;
            let rb_cnt = profile_map
                .get(&sel_uuid)
                .map_or(0, |pr| pr.rollback.count_custom());
            let sc_cnt = profile_map
                .get(&sel_uuid)
                .map_or(0, |pr| pr.scrub.count_custom());
            let rb_lbl = if rb_cnt > 0 {
                format!("{} ({})", tr("rreturn.ui.player.scope_rollback"), rb_cnt)
            } else {
                tr("rreturn.ui.player.scope_rollback")
            };
            let sc_lbl = if sc_cnt > 0 {
                format!("{} ({})", tr("rreturn.ui.player.scope_scrub"), sc_cnt)
            } else {
                tr("rreturn.ui.player.scope_scrub")
            };
            let rb_r = [dock[0] + 2.0, dock[1] + 2.0, sw - 1.0, dock[3] - 4.0];
            let sc_r = [dock[0] + 2.0 + sw, dock[1] + 2.0, sw - 1.0, dock[3] - 4.0];
            draw_pill(
                game,
                draw,
                rb_r,
                &fit_text(game, draw, &rb_lbl, rb_r[2] - 4.0),
                if !scrub_scope {
                    UI_BLUE
                } else if in_list && hit_in_rect(mx, my, rb_r) {
                    UI_PILL_BG
                } else {
                    0
                },
                if !scrub_scope { UI_TEXT_PRI } else { UI_TEXT_SEC },
            );
            draw_pill(
                game,
                draw,
                sc_r,
                &fit_text(game, draw, &sc_lbl, sc_r[2] - 4.0),
                if scrub_scope {
                    UI_BLUE
                } else if in_list && hit_in_rect(mx, my, sc_r) {
                    UI_PILL_BG
                } else {
                    0
                },
                if scrub_scope { UI_TEXT_PRI } else { UI_TEXT_SEC },
            );
            push_clipped_hit(
                regions,
                list_bounds,
                HitRegion {
                    tag: "p_scope.rollback".to_owned(),
                    rect: rb_r,
                    action: HitAction::SetPlayerScope(false),
                },
            );
            push_clipped_hit(
                regions,
                list_bounds,
                HitRegion {
                    tag: "p_scope.scrub".to_owned(),
                    rect: sc_r,
                    action: HitAction::SetPlayerScope(true),
                },
            );
        }
        y += scope_bar_h;

        let dock_w = if w < 310.0 { 114.0 } else { 132.0 };
        for &(field, label_key, desc_key) in PLAYER_OVERRIDE_SPECS {
            let rrect = [x, y, w, row_h];
            if y + row_h >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
                if in_list && hit_in_rect(mx, my, rrect) {
                    draw_rounded_rect(
                        game,
                        draw,
                        [x + 2.0, y + 1.0, w - 4.0, row_h - 2.0],
                        6.0,
                        UI_ROW_HOV,
                    );
                }
                game.native_fill(draw, [x + 12.0, y, w - 24.0, 1.0], UI_DIVIDER);

                let dock = [x + w - dock_w - 10.0, y + 6.5, dock_w, 18.0];
                draw_rounded_rect(game, draw, dock, 6.0, UI_SEG_DOCK);

                let ov_val = cur_ov.get(field);
                let global_val = policy_field(base_pol, field);
                let glob_lbl = if global_val {
                    tr("rreturn.ui.tri.global_on")
                } else {
                    tr("rreturn.ui.tri.global_off")
                };
                let yes_lbl = tr("rreturn.ui.tri.yes");
                let no_lbl = tr("rreturn.ui.tri.no");

                let inner_w = dock[2] - 4.0;
                let w_glob = (inner_w * 0.48).floor();
                let w_yes = (inner_w * 0.26).floor();
                let w_no = inner_w - w_glob - w_yes;

                let rg = [dock[0] + 2.0, dock[1] + 2.0, w_glob - 1.0, dock[3] - 4.0];
                let ry = [dock[0] + 2.0 + w_glob, dock[1] + 2.0, w_yes - 1.0, dock[3] - 4.0];
                let rn = [dock[0] + 2.0 + w_glob + w_yes, dock[1] + 2.0, w_no, dock[3] - 4.0];

                let mut draw_seg = |rect: [f32; 4],
                                    txt: &str,
                                    active: bool,
                                    active_bg: u32,
                                    active_fg: u32,
                                    inactive_fg: u32| {
                    let hov = in_list && hit_in_rect(mx, my, rect);
                    let bg = if active {
                        active_bg
                    } else if hov {
                        UI_PILL_BG
                    } else {
                        0
                    };
                    let fg = if active { active_fg } else { inactive_fg };
                    draw_pill(game, draw, rect, &fit_text(game, draw, txt, rect[2] - 4.0), bg, fg);
                };

                draw_seg(
                    rg,
                    &glob_lbl,
                    ov_val == OverrideValue::Global,
                    UI_PILL_HOV,
                    if global_val { UI_BLUE } else { UI_TEXT_PRI },
                    UI_TEXT_MUTED,
                );
                draw_seg(
                    ry,
                    &yes_lbl,
                    ov_val == OverrideValue::Allow,
                    UI_GREEN,
                    0xFF0A140D,
                    UI_TEXT_SEC,
                );
                draw_seg(
                    rn,
                    &no_lbl,
                    ov_val == OverrideValue::Deny,
                    UI_RED,
                    UI_TEXT_PRI,
                    UI_TEXT_SEC,
                );

                let tmax = (dock[0] - x - 18.0).max(20.0);
                game.native_text(
                    draw,
                    &fit_text(game, draw, &tr(label_key), tmax),
                    [x + 12.0, y + 5.0],
                    UI_TEXT_PRI,
                    false,
                );
                game.native_text(
                    draw,
                    &fit_text(game, draw, &tr(desc_key), tmax),
                    [x + 12.0, y + 17.5],
                    UI_TEXT_MUTED,
                    false,
                );

                push_clipped_hit(
                    regions,
                    list_bounds,
                    HitRegion {
                        tag: format!("p_rule.{}.global", field),
                        rect: rg,
                        action: HitAction::SetPlayerOverride(
                            sel_uuid,
                            sel_name.clone(),
                            scrub_scope,
                            field,
                            OverrideValue::Global,
                        ),
                    },
                );
                push_clipped_hit(
                    regions,
                    list_bounds,
                    HitRegion {
                        tag: format!("p_rule.{}.allow", field),
                        rect: ry,
                        action: HitAction::SetPlayerOverride(
                            sel_uuid,
                            sel_name.clone(),
                            scrub_scope,
                            field,
                            OverrideValue::Allow,
                        ),
                    },
                );
                push_clipped_hit(
                    regions,
                    list_bounds,
                    HitRegion {
                        tag: format!("p_rule.{}.deny", field),
                        rect: rn,
                        action: HitAction::SetPlayerOverride(
                            sel_uuid,
                            sel_name.clone(),
                            scrub_scope,
                            field,
                            OverrideValue::Deny,
                        ),
                    },
                );
            }
            y += row_h;
        }
    }

    (y - start_y).max(0.0)
}
