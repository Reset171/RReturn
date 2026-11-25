use super::super::Game;
use super::settings_catalog::{
    format_slider_value, read_bool_setting, read_slider_setting, ControlKind, SettingEntry,
    UiGroup, UiTab, ROLLBACK_GROUPS, SCRUB_GROUPS, SETTINGS_CATALOG,
};
use super::settings_ui_draw::*;
use super::types::*;
use horizon_client::hud::HudBatch;

pub fn push_clipped_region(
    out: &mut Vec<HitRegion>,
    bounds: [f32; 4],
    mut reg: HitRegion,
) {
    let x0 = reg.rect[0].max(bounds[0]);
    let y0 = reg.rect[1].max(bounds[1]);
    let x1 = (reg.rect[0] + reg.rect[2]).min(bounds[0] + bounds[2]);
    let y1 = (reg.rect[1] + reg.rect[3]).min(bounds[1] + bounds[3]);
    if x1 > x0 && y1 > y0 {
        reg.rect = [x0, y0, x1 - x0, y1 - y0];
        out.push(reg);
    }
}

pub fn anim_for_key(
    ui: &mut SettingsUiState,
    key: &str,
    val: bool,
) -> f32 {
    let target = if val { 1.0f32 } else { 0.0f32 };
    let e = ui.toggle_anim.entry(key.to_owned()).or_insert(target);
    *e += (target - *e) * 0.28;
    if (*e - target).abs() < 0.01 {
        *e = target;
    }
    *e
}

fn row_height(entry: &SettingEntry) -> f32 {
    match entry.kind {
        ControlKind::Bool => 31.0,
        ControlKind::Slider { .. } => 41.0,
    }
}

fn draw_grouped_card_rows(
    game: &Game<'_>,
    draw: &mut HudBatch,
    ui: &mut SettingsUiState,
    entries: &[&SettingEntry],
    x: f32,
    mut y: f32,
    w: f32,
    list_bounds: [f32; 4],
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    if entries.is_empty() {
        return 0.0;
    }
    let total_h: f32 = entries.iter().map(|e| row_height(e)).sum();
    let card_rect = [x, y, w, total_h];
    if y + total_h >= list_bounds[1] - 4.0 && y <= list_bounds[1] + list_bounds[3] + 4.0 {
        draw_rounded_rect(game, draw, card_rect, 8.0, UI_CARD_BG);
    }

    let [mx, my] = mouse;
    let in_list = hit_in_rect(mx, my, list_bounds);
    let mut off = 0.0f32;
    for (idx, entry) in entries.iter().enumerate() {
        let rh = row_height(entry);
        let rrect = [x, y, w, rh];
        let visible = y + rh >= list_bounds[1] - 2.0 && y <= list_bounds[1] + list_bounds[3] + 2.0;
        if visible {
            if in_list && hit_in_rect(mx, my, rrect) {
                draw_rounded_rect(game, draw, [x + 2.0, y + 1.0, w - 4.0, rh - 2.0], 6.0, UI_ROW_HOV);
            }
            if idx > 0 {
                game.native_fill(draw, [x + 12.0, y, w - 24.0, 1.0], UI_DIVIDER);
            }
            match entry.kind {
                ControlKind::Bool => {
                    let val = with_state(|st| read_bool_setting(st, entry.id));
                    let anim = anim_for_key(ui, entry.id, val);
                    let sw_x = x + w - 40.0;
                    draw_switch(game, draw, sw_x, y + 8.5, anim);
                    let tmax = (sw_x - x - 20.0).max(20.0);
                    game.native_text(draw, &fit_text(game, draw, &tr(entry.label_key), tmax), [x + 12.0, y + 5.0], UI_TEXT_PRI, false);
                    game.native_text(draw, &fit_text(game, draw, &tr(entry.desc_key), tmax), [x + 12.0, y + 17.5], UI_TEXT_MUTED, false);
                    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("opt.{}", entry.id), rect: rrect, action: HitAction::ToggleBool(entry.id) });
                }
                ControlKind::Slider { min, max, step, unit } => {
                    let val = with_state(|st| read_slider_setting(st, entry.id));
                    let val_str = format_slider_value(val, unit);
                    let vw = game.native_text_width(draw, &val_str);
                    let plus_rect = [x + w - 25.0, y + 5.5, 14.0, 13.0];
                    let val_x = plus_rect[0] - vw - 5.0;
                    let minus_rect = [val_x - 19.0, y + 5.5, 14.0, 13.0];
                    let tmax = (minus_rect[0] - x - 18.0).max(20.0);
                    game.native_text(draw, &fit_text(game, draw, &tr(entry.label_key), tmax), [x + 12.0, y + 8.0], UI_TEXT_PRI, false);

                    let m_hov = in_list && hit_in_rect(mx, my, minus_rect);
                    let p_hov = in_list && hit_in_rect(mx, my, plus_rect);
                    draw_pill(game, draw, minus_rect, "-", if m_hov { UI_PILL_HOV } else { UI_PILL_BG }, UI_TEXT_PRI);
                    game.native_text(draw, &val_str, [val_x, y + 8.0], UI_BLUE, false);
                    draw_pill(game, draw, plus_rect, "+", if p_hov { UI_PILL_HOV } else { UI_PILL_BG }, UI_TEXT_PRI);

                    let track_x = x + 12.0;
                    let track_w = (w - 24.0).max(40.0);
                    let track_y = y + 28.0;
                    let frac = ((val - min) as f32 / (max - min).max(1) as f32).clamp(0.0, 1.0);
                    draw_rounded_rect(game, draw, [track_x, track_y, track_w, 4.0], 2.0, UI_TRACK_BG);
                    draw_rounded_rect(game, draw, [track_x, track_y, (track_w * frac).max(4.0), 4.0], 2.0, UI_BLUE);
                    let knob_cx = track_x + frac * track_w;
                    draw_rounded_rect(game, draw, [knob_cx - 4.5, track_y - 2.5, 9.0, 9.0], 4.5, UI_THUMB);

                    let track_hit = [track_x - 4.0, track_y - 6.0, track_w + 8.0, 16.0];
                    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("slider.{}", entry.id), rect: track_hit, action: HitAction::SliderDrag { id: entry.id, track_x, track_w, min, max, step } });
                    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("minus.{}", entry.id), rect: minus_rect, action: HitAction::SliderStep(entry.id, -step) });
                    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("plus.{}", entry.id), rect: plus_rect, action: HitAction::SliderStep(entry.id, step) });
                }
            }
        }
        y += rh;
        off += rh;
    }
    off
}

fn draw_group_header(
    game: &Game<'_>,
    draw: &mut HudBatch,
    group: UiGroup,
    x: f32,
    y: f32,
    w: f32,
    list_bounds: [f32; 4],
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    let hdr_h = 16.0;
    if y + hdr_h >= list_bounds[1] - 2.0 && y <= list_bounds[1] + list_bounds[3] + 2.0 {
        let [mx, my] = mouse;
        let in_list = hit_in_rect(mx, my, list_bounds);
        let mut right_used = 0.0f32;
        if group.is_policy_group() {
            let dis_lbl = tr("rreturn.ui.disable_all");
            let en_lbl = tr("rreturn.ui.enable_all");
            let dis_w = game.native_text_width(draw, &dis_lbl) + 6.0;
            let en_w = game.native_text_width(draw, &en_lbl) + 6.0;
            right_used = dis_w + en_w + 10.0;
            let dis_rect = [x + w - dis_w - 4.0, y + 1.0, dis_w, 13.0];
            let en_rect = [dis_rect[0] - en_w - 6.0, y + 1.0, en_w, 13.0];
            let en_hov = in_list && hit_in_rect(mx, my, en_rect);
            let dis_hov = in_list && hit_in_rect(mx, my, dis_rect);
            game.native_text(draw, &en_lbl, [en_rect[0] + 3.0, y + 3.0], if en_hov { UI_TEXT_PRI } else { UI_BLUE }, false);
            game.native_text(draw, &dis_lbl, [dis_rect[0] + 3.0, y + 3.0], if dis_hov { UI_RED } else { UI_TEXT_SEC }, false);
            push_clipped_region(regions, list_bounds, HitRegion { tag: format!("batch.en.{}", group.title_key()), rect: en_rect, action: HitAction::BatchGroup(group, true) });
            push_clipped_region(regions, list_bounds, HitRegion { tag: format!("batch.dis.{}", group.title_key()), rect: dis_rect, action: HitAction::BatchGroup(group, false) });

            if matches!(group, UiGroup::RbPlayer | UiGroup::ScrubPlayer) {
                let pp_lbl = tr("rreturn.ui.per_player_link");
                let pp_w = game.native_text_width(draw, &pp_lbl) + 8.0;
                let pp_rect = [en_rect[0] - pp_w - 6.0, y + 1.0, pp_w, 13.0];
                if pp_rect[0] > x + 65.0 {
                    right_used += pp_w + 6.0;
                    let pp_hov = in_list && hit_in_rect(mx, my, pp_rect);
                    draw_rounded_rect(game, draw, pp_rect, 4.0, if pp_hov { UI_BLUE } else { UI_BLUE_SOFT });
                    game.native_text(draw, &pp_lbl, [pp_rect[0] + 4.0, y + 3.0], if pp_hov { UI_TEXT_PRI } else { UI_BLUE }, false);
                    let is_sc = matches!(group, UiGroup::ScrubPlayer);
                    push_clipped_region(
                        regions,
                        list_bounds,
                        HitRegion {
                            tag: format!("jump.per_player.{}", group.jump_id()),
                            rect: pp_rect,
                            action: HitAction::JumpToSection(UiTab::Players, "players_rules".to_owned(), Some(is_sc)),
                        },
                    );
                }
            }
        }
        let title = fit_text(game, draw, &tr(group.title_key()), (w - right_used - 10.0).max(20.0));
        game.native_text(draw, &title, [x + 6.0, y + 3.0], UI_TEXT_SEC, false);
    }
    hdr_h
}

pub fn draw_grouped_tab(
    game: &Game<'_>,
    draw: &mut HudBatch,
    ui: &mut SettingsUiState,
    tab: UiTab,
    list_bounds: [f32; 4],
    scroll_y: f32,
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    let groups = match tab {
        UiTab::Rollback => ROLLBACK_GROUPS,
        UiTab::Scrub => SCRUB_GROUPS,
        _ => &[],
    };
    let mut y = list_bounds[1] - scroll_y;
    let start_y = y;
    for &grp in groups {
        let entries: Vec<&SettingEntry> = SETTINGS_CATALOG.iter().filter(|e| e.group == grp).collect();
        if entries.is_empty() {
            continue;
        }
        ui.section_offsets
            .insert(grp.jump_id().to_owned(), (y - start_y).max(0.0));
        y += draw_group_header(game, draw, grp, list_bounds[0], y, list_bounds[2], list_bounds, mouse, regions);
        y += draw_grouped_card_rows(game, draw, ui, &entries, list_bounds[0], y, list_bounds[2], list_bounds, mouse, regions);
        y += 8.0;
    }
    (y - start_y).max(0.0)
}

pub fn draw_search_tab(
    game: &Game<'_>,
    draw: &mut HudBatch,
    ui: &mut SettingsUiState,
    needle: &str,
    list_bounds: [f32; 4],
    scroll_y: f32,
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    let n = needle.trim().to_lowercase();
    let y = list_bounds[1] - scroll_y;
    let start_y = y;
    let matched: Vec<&SettingEntry> = SETTINGS_CATALOG
        .iter()
        .filter(|e| {
            tr(e.label_key).to_lowercase().contains(&n)
                || tr(e.desc_key).to_lowercase().contains(&n)
                || e.id.to_lowercase().contains(&n)
        })
        .collect();
    if matched.is_empty() {
        game.native_text(draw, &fit_text(game, draw, &tr("rreturn.ui.no_results"), list_bounds[2] - 12.0), [list_bounds[0] + 6.0, y + 10.0], UI_TEXT_MUTED, false);
        return 32.0;
    }
    let mut cur = y;
    for tab in [UiTab::Rollback, UiTab::Scrub, UiTab::System] {
        let sub: Vec<&SettingEntry> = matched.iter().copied().filter(|e| e.tab == tab).collect();
        if sub.is_empty() {
            continue;
        }
        let sec_lbl = tr(tab.title_key()).to_uppercase();
        if cur + 16.0 >= list_bounds[1] - 2.0 && cur <= list_bounds[1] + list_bounds[3] + 2.0 {
            game.native_text(draw, &sec_lbl, [list_bounds[0] + 6.0, cur + 3.0], UI_TEXT_SEC, false);
        }
        cur += 16.0;
        cur += draw_grouped_card_rows(game, draw, ui, &sub, list_bounds[0], cur, list_bounds[2], list_bounds, mouse, regions);
        cur += 8.0;
    }
    (cur - start_y).max(0.0)
}

pub fn draw_system_tab(
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

    ui.section_offsets
        .insert("sys_diag".to_owned(), (y - start_y).max(0.0));
    y += draw_group_header(game, draw, UiGroup::SysDiag, x, y, w, list_bounds, mouse, regions);
    let entries: Vec<&SettingEntry> = SETTINGS_CATALOG.iter().filter(|e| e.group == UiGroup::SysDiag).collect();
    y += draw_grouped_card_rows(game, draw, ui, &entries, x, y, w, list_bounds, mouse, regions);
    y += 8.0;

    // Keybindings reference group
    ui.section_offsets
        .insert("sys_keys".to_owned(), (y - start_y).max(0.0));
    if y + 16.0 >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
        game.native_text(draw, &tr("rreturn.ui.group.sys_keys"), [x + 6.0, y + 3.0], UI_TEXT_SEC, false);
    }
    y += 16.0;
    let keys_list = [
        ("key.rreturn.scrub", "U"),
        ("key.rreturn.save", "F6"),
        ("key.rreturn.load", "F7"),
        ("key.rreturn.anchor_menu", "F8"),
        ("key.rreturn.settings_menu", "K"),
    ];
    let kh = 22.0;
    let kt_h = keys_list.len() as f32 * kh;
    if y + kt_h >= ly - 4.0 && y <= ly + list_bounds[3] + 4.0 {
        draw_rounded_rect(game, draw, [x, y, w, kt_h], 8.0, UI_CARD_BG);
        for (i, &(klabel, kbind)) in keys_list.iter().enumerate() {
            let ky = y + i as f32 * kh;
            if ky + kh >= ly - 2.0 && ky <= ly + list_bounds[3] + 2.0 {
                if i > 0 {
                    game.native_fill(draw, [x + 12.0, ky, w - 24.0, 1.0], UI_DIVIDER);
                }
                let kw = (game.native_text_width(draw, kbind) + 12.0).max(24.0);
                let kr = [x + w - kw - 12.0, ky + 4.0, kw, 14.0];
                draw_pill(game, draw, kr, kbind, UI_PILL_BG, UI_TEXT_PRI);
                game.native_text(draw, &fit_text(game, draw, &tr(klabel), (kr[0] - x - 18.0).max(20.0)), [x + 12.0, ky + 7.0], UI_TEXT_PRI, false);
            }
        }
    }
    y += kt_h + 8.0;

    // Reset card
    ui.section_offsets
        .insert("sys_reset".to_owned(), (y - start_y).max(0.0));
    if y + 16.0 >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
        game.native_text(draw, &tr("rreturn.ui.group.sys_reset"), [x + 6.0, y + 3.0], UI_TEXT_SEC, false);
    }
    y += 16.0;
    let rr = [x, y, w, 30.0];
    if y + 30.0 >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
        let hov = in_list && hit_in_rect(mx, my, rr);
        draw_rounded_rect(game, draw, rr, 8.0, if hov { UI_RED_SOFT } else { UI_CARD_BG });
        let lbl = fit_text(game, draw, &tr("rreturn.ui.reset_defaults"), w - 24.0);
        let tw = game.native_text_width(draw, &lbl);
        game.native_text(draw, &lbl, [x + (w - tw) * 0.5, y + 11.0], UI_RED, false);
        push_clipped_region(regions, list_bounds, HitRegion { tag: "reset_defaults".to_owned(), rect: rr, action: HitAction::ResetDefaults });
    }
    y += 30.0;
    (y - start_y).max(0.0)
}
