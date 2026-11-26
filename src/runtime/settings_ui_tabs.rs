use super::settings_catalog::{
    format_slider_value, read_bool_setting, read_slider_setting, ControlKind, SettingEntry,
    SliderUnit, UiGroup, UiTab, ROLLBACK_GROUPS, SCRUB_GROUPS, SETTINGS_CATALOG,
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

#[derive(Clone, Copy)]
pub struct SliderSpec {
    pub id: &'static str,
    pub label_key: &'static str,
    pub desc_key: &'static str,
    pub min: i32,
    pub max: i32,
    pub step: i32,
    pub unit: SliderUnit,
}

impl SliderSpec {
    pub fn from_entry(e: &SettingEntry) -> Option<Self> {
        let ControlKind::Slider { min, max, step, unit } = e.kind else {
            return None;
        };
        Some(Self {
            id: e.id,
            label_key: e.label_key,
            desc_key: e.desc_key,
            min,
            max,
            step,
            unit,
        })
    }
}

fn find_slider_spec(id: &str) -> Option<SliderSpec> {
    SETTINGS_CATALOG
        .iter()
        .find(|e| e.id == id)
        .and_then(SliderSpec::from_entry)
}

pub fn draw_compact_slider_row<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    mode: SettingsUiMode,
    spec: SliderSpec,
    [x, y, w, h]: [f32; 4],
    list_bounds: [f32; 4],
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) {
    let [mx, my] = mouse;
    let in_list = hit_in_rect(mx, my, list_bounds);
    let rrect = [x, y, w, h];
    if in_list && hit_in_rect(mx, my, rrect) {
        draw_rounded_rect(canvas, draw, [x + 4.0, y + 2.0, w - 8.0, h - 4.0], 5.0, UI_ROW_HOV);
    }

    let val = with_mode_state(mode, |st| read_slider_setting(st, spec.id));
    let val_str = format_slider_value(val, spec.unit);
    let vw = canvas.text_width(draw, &val_str).max(24.0);
    let val_x = (x + w - 12.0 - vw).floor();
    let plus_rect = [val_x - 18.0, y + (h - 14.0) * 0.5, 14.0, 14.0];
    let track_w = (w * 0.24).clamp(58.0, 112.0).min((w - 180.0 - vw).max(46.0));
    let track_x = (plus_rect[0] - 6.0 - track_w).floor();
    let minus_rect = [track_x - 19.0, plus_rect[1], 14.0, 14.0];

    let tmax = (minus_rect[0] - x - 20.0).max(40.0);
    canvas.text(draw, &fit_text(canvas, draw, &tr(spec.label_key), tmax), [x + 12.0, y + 5.5], UI_TEXT_PRI, false);
    if !spec.desc_key.is_empty() {
        canvas.text(draw, &fit_text(canvas, draw, &tr(spec.desc_key), tmax), [x + 12.0, y + 18.0], UI_TEXT_MUTED, false);
    }

    let m_hov = in_list && hit_in_rect(mx, my, minus_rect);
    let p_hov = in_list && hit_in_rect(mx, my, plus_rect);
    draw_pill(canvas, draw, minus_rect, "-", if m_hov { UI_PILL_HOV } else { UI_PILL_BG }, UI_TEXT_PRI);

    let track_y = (y + h * 0.5 - 2.0).floor();
    let frac = ((val - spec.min) as f32 / (spec.max - spec.min).max(1) as f32).clamp(0.0, 1.0);
    draw_rounded_rect(canvas, draw, [track_x, track_y, track_w, 4.0], 2.0, UI_TRACK_BG);
    draw_rounded_rect(canvas, draw, [track_x, track_y, (track_w * frac).max(4.0), 4.0], 2.0, UI_BLUE);
    let knob_x = (track_x + frac * track_w - 5.0).round();
    let knob_y = (track_y - 3.0).round();
    draw_rounded_rect(canvas, draw, [knob_x, knob_y, 10.0, 10.0], 5.0, UI_THUMB);

    draw_pill(canvas, draw, plus_rect, "+", if p_hov { UI_PILL_HOV } else { UI_PILL_BG }, UI_TEXT_PRI);
    canvas.text(draw, &val_str, [val_x, y + (h - 8.0) * 0.5], UI_BLUE, false);

    let track_hit = [track_x - 3.0, y + 4.0, track_w + 6.0, h - 8.0];
    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("slider.{}", spec.id), rect: track_hit, action: HitAction::SliderDrag { id: spec.id, track_x, track_w, min: spec.min, max: spec.max, step: spec.step } });
    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("minus.{}", spec.id), rect: minus_rect, action: HitAction::SliderStep(spec.id, -spec.step) });
    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("plus.{}", spec.id), rect: plus_rect, action: HitAction::SliderStep(spec.id, spec.step) });
}

pub fn draw_compact_slider_cell<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    mode: SettingsUiMode,
    spec: SliderSpec,
    [x, y, w, h]: [f32; 4],
    list_bounds: [f32; 4],
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) {
    let [mx, my] = mouse;
    let in_list = hit_in_rect(mx, my, list_bounds);
    if in_list && hit_in_rect(mx, my, [x, y, w, h]) {
        draw_rounded_rect(canvas, draw, [x + 4.0, y + 2.0, w - 8.0, h - 4.0], 5.0, UI_ROW_HOV);
    }

    let val = with_mode_state(mode, |st| read_slider_setting(st, spec.id));
    let val_str = format_slider_value(val, spec.unit);
    let vw = canvas.text_width(draw, &val_str);
    let plus_rect = [x + w - 24.0, y + 5.0, 13.0, 13.0];
    let val_x = plus_rect[0] - vw - 5.0;
    let minus_rect = [val_x - 18.0, y + 5.0, 13.0, 13.0];
    let tmax = (minus_rect[0] - x - 16.0).max(20.0);

    canvas.text(draw, &fit_text(canvas, draw, &tr(spec.label_key), tmax), [x + 10.0, y + 7.5], UI_TEXT_PRI, false);
    draw_pill(canvas, draw, minus_rect, "-", if in_list && hit_in_rect(mx, my, minus_rect) { UI_PILL_HOV } else { UI_PILL_BG }, UI_TEXT_PRI);
    canvas.text(draw, &val_str, [val_x, y + 7.5], UI_BLUE, false);
    draw_pill(canvas, draw, plus_rect, "+", if in_list && hit_in_rect(mx, my, plus_rect) { UI_PILL_HOV } else { UI_PILL_BG }, UI_TEXT_PRI);

    let track_x = x + 10.0;
    let track_w = (w - 20.0).max(36.0);
    let track_y = (y + 25.0).floor();
    let frac = ((val - spec.min) as f32 / (spec.max - spec.min).max(1) as f32).clamp(0.0, 1.0);
    draw_rounded_rect(canvas, draw, [track_x, track_y, track_w, 4.0], 2.0, UI_TRACK_BG);
    draw_rounded_rect(canvas, draw, [track_x, track_y, (track_w * frac).max(4.0), 4.0], 2.0, UI_BLUE);
    let knob_x = (track_x + frac * track_w - 5.0).round();
    draw_rounded_rect(canvas, draw, [knob_x, track_y - 3.0, 10.0, 10.0], 5.0, UI_THUMB);

    let track_hit = [track_x - 3.0, track_y - 6.0, track_w + 6.0, 15.0];
    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("slider.{}", spec.id), rect: track_hit, action: HitAction::SliderDrag { id: spec.id, track_x, track_w, min: spec.min, max: spec.max, step: spec.step } });
    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("minus.{}", spec.id), rect: minus_rect, action: HitAction::SliderStep(spec.id, -spec.step) });
    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("plus.{}", spec.id), rect: plus_rect, action: HitAction::SliderStep(spec.id, spec.step) });
}

fn draw_bool_row<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    ui: &mut SettingsUiState,
    id: &'static str,
    label_key: &str,
    desc_key: &str,
    [x, y, w, h]: [f32; 4],
    list_bounds: [f32; 4],
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) {
    let [mx, my] = mouse;
    let in_list = hit_in_rect(mx, my, list_bounds);
    let rrect = [x, y, w, h];
    if in_list && hit_in_rect(mx, my, rrect) {
        draw_rounded_rect(canvas, draw, [x + 4.0, y + 2.0, w - 8.0, h - 4.0], 5.0, UI_ROW_HOV);
    }
    let val = with_mode_state(ui.mode, |st| read_bool_setting(st, id));
    let anim = anim_for_key(ui, id, val);
    let sw_x = x + w - 40.0;
    draw_switch(canvas, draw, sw_x, (y + (h - 14.0) * 0.5).round(), anim);
    let tmax = (sw_x - x - 20.0).max(20.0);
    canvas.text(draw, &fit_text(canvas, draw, &tr(label_key), tmax), [x + 12.0, y + 5.0], UI_TEXT_PRI, false);
    canvas.text(draw, &fit_text(canvas, draw, &tr(desc_key), tmax), [x + 12.0, y + 17.5], UI_TEXT_MUTED, false);
    push_clipped_region(regions, list_bounds, HitRegion { tag: format!("opt.{}", id), rect: rrect, action: HitAction::ToggleBool(id) });
}

fn draw_autosave_card<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    ui: &mut SettingsUiState,
    x: f32,
    mut y: f32,
    w: f32,
    list_bounds: [f32; 4],
    mouse: [f32; 2],
    regions: &mut Vec<HitRegion>,
) -> f32 {
    let (auto_on, adapt, random_iv) = with_mode_state(ui.mode, |st| (st.auto_save, st.adaptive_mode, st.use_random_interval));
    let mode = if adapt {
        AutoSaveScheduleMode::Adaptive
    } else if random_iv {
        AutoSaveScheduleMode::Random
    } else {
        AutoSaveScheduleMode::Fixed
    };
    let grid = w >= 330.0;
    let mode_rows_h = if !auto_on {
        0.0
    } else {
        let sched_h = 34.0;
        let sliders_h = match mode {
            AutoSaveScheduleMode::Fixed => 34.0,
            AutoSaveScheduleMode::Random => if grid { 38.0 } else { 68.0 },
            AutoSaveScheduleMode::Adaptive => if grid { 76.0 } else { 136.0 },
        };
        sched_h + sliders_h
    };
    let total_h = 93.0 + mode_rows_h;
    let visible = y + total_h >= list_bounds[1] - 4.0 && y <= list_bounds[1] + list_bounds[3] + 4.0;
    if !visible {
        return total_h;
    }

    draw_rounded_rect(canvas, draw, [x, y, w, total_h], 8.0, UI_CARD_BG);
    let [mx, my] = mouse;
    let in_list = hit_in_rect(mx, my, list_bounds);

    draw_bool_row(
        canvas, draw, ui,
        "death_rollback", "rreturn.ui.opt.death_rollback", "rreturn.ui.opt.death_rollback.desc",
        [x, y, w, 31.0], list_bounds, mouse, regions,
    );
    y += 31.0;

    canvas.fill(draw, [x + 12.0, y, w - 24.0, 1.0], UI_DIVIDER);
    draw_bool_row(
        canvas, draw, ui,
        "auto_save", "rreturn.ui.opt.auto_save", "rreturn.ui.opt.auto_save.desc",
        [x, y, w, 31.0], list_bounds, mouse, regions,
    );
    y += 31.0;

    canvas.fill(draw, [x + 12.0, y, w - 24.0, 1.0], UI_DIVIDER);
    draw_bool_row(
        canvas, draw, ui,
        "save_on_boss_defeat", "rreturn.ui.opt.save_on_boss_defeat", "rreturn.ui.opt.save_on_boss_defeat.desc",
        [x, y, w, 31.0], list_bounds, mouse, regions,
    );
    y += 31.0;

    if !auto_on {
        return total_h;
    }

    // Schedule mode selector row
    canvas.fill(draw, [x + 12.0, y, w - 24.0, 1.0], UI_DIVIDER);
    let dock_w = if w < 330.0 { 150.0 } else { (w * 0.46).clamp(164.0, 224.0) };
    let dock = [x + w - dock_w - 12.0, y + 8.0, dock_w, 18.0];
    let tmax = (dock[0] - x - 20.0).max(30.0);
    let mode_desc = match mode {
        AutoSaveScheduleMode::Fixed => tr("rreturn.ui.autosave.mode_fixed.desc"),
        AutoSaveScheduleMode::Random => tr("rreturn.ui.autosave.mode_random.desc"),
        AutoSaveScheduleMode::Adaptive => tr("rreturn.ui.autosave.mode_adaptive.desc"),
    };
    canvas.text(draw, &fit_text(canvas, draw, &tr("rreturn.ui.opt.autosave_mode"), tmax), [x + 12.0, y + 5.5], UI_TEXT_PRI, false);
    canvas.text(draw, &fit_text(canvas, draw, &mode_desc, tmax), [x + 12.0, y + 18.0], UI_TEXT_MUTED, false);

    draw_rounded_rect(canvas, draw, dock, 6.0, UI_SEG_DOCK);
    let segs = [
        (AutoSaveScheduleMode::Fixed, "fixed", "rreturn.ui.autosave.mode_fixed"),
        (AutoSaveScheduleMode::Random, "random", "rreturn.ui.autosave.mode_random"),
        (AutoSaveScheduleMode::Adaptive, "adaptive", "rreturn.ui.autosave.mode_adaptive"),
    ];
    let sw = (dock[2] - 4.0) / 3.0;
    for (i, &(m, tag_s, lbl_k)) in segs.iter().enumerate() {
        let sr = [dock[0] + 2.0 + i as f32 * sw, dock[1] + 2.0, sw - 1.0, dock[3] - 4.0];
        let active = mode == m;
        let hov = in_list && hit_in_rect(mx, my, sr);
        let bg = if active { UI_BLUE } else if hov { UI_PILL_BG } else { 0 };
        draw_pill(canvas, draw, sr, &fit_text(canvas, draw, &tr(lbl_k), sr[2] - 4.0), bg, if active { UI_TEXT_PRI } else { UI_TEXT_SEC });
        push_clipped_region(regions, list_bounds, HitRegion { tag: format!("autosave.mode.{}", tag_s), rect: sr, action: HitAction::SetAutoSaveMode(m) });
    }
    y += 34.0;

    let ui_mode = ui.mode;
    let mut draw_pair_or_rows = |left_id: &str, right_id: &str, cur_y: f32, regs: &mut Vec<HitRegion>| -> f32 {
        let Some(ls) = find_slider_spec(left_id) else { return 0.0; };
        let Some(rs) = find_slider_spec(right_id) else { return 0.0; };
        canvas.fill(draw, [x + 12.0, cur_y, w - 24.0, 1.0], UI_DIVIDER);
        if grid {
            let half = (w * 0.5).floor();
            draw_compact_slider_cell(canvas, draw, ui_mode, ls, [x, cur_y, half, 38.0], list_bounds, mouse, regs);
            canvas.fill(draw, [x + half, cur_y + 6.0, 1.0, 26.0], UI_DIVIDER);
            draw_compact_slider_cell(canvas, draw, ui_mode, rs, [x + half, cur_y, w - half, 38.0], list_bounds, mouse, regs);
            38.0
        } else {
            draw_compact_slider_row(canvas, draw, ui_mode, ls, [x, cur_y, w, 34.0], list_bounds, mouse, regs);
            canvas.fill(draw, [x + 12.0, cur_y + 34.0, w - 24.0, 1.0], UI_DIVIDER);
            draw_compact_slider_row(canvas, draw, ui_mode, rs, [x, cur_y + 34.0, w, 34.0], list_bounds, mouse, regs);
            68.0
        }
    };

    match mode {
        AutoSaveScheduleMode::Fixed => {
            if let Some(sp) = find_slider_spec("fixed_interval_sec") {
                canvas.fill(draw, [x + 12.0, y, w - 24.0, 1.0], UI_DIVIDER);
                draw_compact_slider_row(canvas, draw, ui_mode, sp, [x, y, w, 34.0], list_bounds, mouse, regions);
            }
        }
        AutoSaveScheduleMode::Random => {
            let _ = draw_pair_or_rows("random_min_sec", "random_max_sec", y, regions);
        }
        AutoSaveScheduleMode::Adaptive => {
            let h1 = draw_pair_or_rows("adaptive_interval_sec", "min_save_gap_sec", y, regions);
            let _ = draw_pair_or_rows("relax_time_sec", "post_combat_save_pct", y + h1, regions);
        }
    }
    total_h
}

fn row_height(entry: &SettingEntry) -> f32 {
    match entry.kind {
        ControlKind::Bool => 31.0,
        ControlKind::Slider { .. } => 34.0,
    }
}

fn draw_grouped_card_rows<C: UiCanvas>(
    canvas: &C,
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
    if y + total_h >= list_bounds[1] - 4.0 && y <= list_bounds[1] + list_bounds[3] + 4.0 {
        draw_rounded_rect(canvas, draw, [x, y, w, total_h], 8.0, UI_CARD_BG);
    }

    let mut off = 0.0f32;
    let ui_mode = ui.mode;
    for (idx, entry) in entries.iter().enumerate() {
        let rh = row_height(entry);
        let visible = y + rh >= list_bounds[1] - 2.0 && y <= list_bounds[1] + list_bounds[3] + 2.0;
        if visible {
            if idx > 0 {
                canvas.fill(draw, [x + 12.0, y, w - 24.0, 1.0], UI_DIVIDER);
            }
            match entry.kind {
                ControlKind::Bool => {
                    draw_bool_row(
                        canvas, draw, ui,
                        entry.id, entry.label_key, entry.desc_key,
                        [x, y, w, rh], list_bounds, mouse, regions,
                    );
                }
                ControlKind::Slider { min, max, step, unit } => {
                    let spec = SliderSpec {
                        id: entry.id,
                        label_key: entry.label_key,
                        desc_key: entry.desc_key,
                        min,
                        max,
                        step,
                        unit,
                    };
                    draw_compact_slider_row(canvas, draw, ui_mode, spec, [x, y, w, rh], list_bounds, mouse, regions);
                }
            }
        }
        y += rh;
        off += rh;
    }
    off
}

fn draw_group_header<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    mode: SettingsUiMode,
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
            let dis_w = canvas.text_width(draw, &dis_lbl) + 6.0;
            let en_w = canvas.text_width(draw, &en_lbl) + 6.0;
            right_used = dis_w + en_w + 10.0;
            let dis_rect = [x + w - dis_w - 4.0, y + 1.0, dis_w, 13.0];
            let en_rect = [dis_rect[0] - en_w - 6.0, y + 1.0, en_w, 13.0];
            let en_hov = in_list && hit_in_rect(mx, my, en_rect);
            let dis_hov = in_list && hit_in_rect(mx, my, dis_rect);
            canvas.text(draw, &en_lbl, [en_rect[0] + 3.0, y + 3.0], if en_hov { UI_TEXT_PRI } else { UI_BLUE }, false);
            canvas.text(draw, &dis_lbl, [dis_rect[0] + 3.0, y + 3.0], if dis_hov { UI_RED } else { UI_TEXT_SEC }, false);
            push_clipped_region(regions, list_bounds, HitRegion { tag: format!("batch.en.{}", group.title_key()), rect: en_rect, action: HitAction::BatchGroup(group, true) });
            push_clipped_region(regions, list_bounds, HitRegion { tag: format!("batch.dis.{}", group.title_key()), rect: dis_rect, action: HitAction::BatchGroup(group, false) });

            if mode == SettingsUiMode::World && matches!(group, UiGroup::RbPlayer | UiGroup::ScrubPlayer) {
                let pp_lbl = tr("rreturn.ui.per_player_link");
                let pp_w = canvas.text_width(draw, &pp_lbl) + 8.0;
                let pp_rect = [en_rect[0] - pp_w - 6.0, y + 1.0, pp_w, 13.0];
                if pp_rect[0] > x + 65.0 {
                    right_used += pp_w + 6.0;
                    let pp_hov = in_list && hit_in_rect(mx, my, pp_rect);
                    draw_rounded_rect(canvas, draw, pp_rect, 4.0, if pp_hov { UI_BLUE } else { UI_BLUE_SOFT });
                    canvas.text(draw, &pp_lbl, [pp_rect[0] + 4.0, y + 3.0], if pp_hov { UI_TEXT_PRI } else { UI_BLUE }, false);
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
        let title = fit_text(canvas, draw, &tr(group.title_key()), (w - right_used - 10.0).max(20.0));
        canvas.text(draw, &title, [x + 6.0, y + 3.0], UI_TEXT_SEC, false);
    }
    hdr_h
}

pub fn draw_grouped_tab<C: UiCanvas>(
    canvas: &C,
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
    let ui_mode = ui.mode;
    for &grp in groups {
        if grp == UiGroup::RbAuto {
            ui.section_offsets
                .insert(grp.jump_id().to_owned(), (y - start_y).max(0.0));
            y += draw_group_header(canvas, draw, ui_mode, grp, list_bounds[0], y, list_bounds[2], list_bounds, mouse, regions);
            y += draw_autosave_card(canvas, draw, ui, list_bounds[0], y, list_bounds[2], list_bounds, mouse, regions);
            y += 8.0;
            continue;
        }
        let entries: Vec<&SettingEntry> = SETTINGS_CATALOG.iter().filter(|e| e.group == grp).collect();
        if entries.is_empty() {
            continue;
        }
        ui.section_offsets
            .insert(grp.jump_id().to_owned(), (y - start_y).max(0.0));
        y += draw_group_header(canvas, draw, ui_mode, grp, list_bounds[0], y, list_bounds[2], list_bounds, mouse, regions);
        y += draw_grouped_card_rows(canvas, draw, ui, &entries, list_bounds[0], y, list_bounds[2], list_bounds, mouse, regions);
        y += 8.0;
    }
    (y - start_y).max(0.0)
}

pub fn draw_search_tab<C: UiCanvas>(
    canvas: &C,
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
    let allow_players = ui.mode == SettingsUiMode::World;
    let matched: Vec<&SettingEntry> = SETTINGS_CATALOG
        .iter()
        .filter(|e| {
            (allow_players || e.tab != UiTab::Players)
                && (tr(e.label_key).to_lowercase().contains(&n)
                    || tr(e.desc_key).to_lowercase().contains(&n)
                    || e.id.to_lowercase().contains(&n))
        })
        .collect();
    if matched.is_empty() {
        canvas.text(draw, &fit_text(canvas, draw, &tr("rreturn.ui.no_results"), list_bounds[2] - 12.0), [list_bounds[0] + 6.0, y + 10.0], UI_TEXT_MUTED, false);
        return 32.0;
    }
    let mut cur = y;
    let tabs: &[UiTab] = if allow_players { &UiTab::ALL } else { &UiTab::GLOBAL };
    for &tab in tabs {
        let sub: Vec<&SettingEntry> = matched.iter().copied().filter(|e| e.tab == tab).collect();
        if sub.is_empty() {
            continue;
        }
        let sec_lbl = tr(tab.title_key()).to_uppercase();
        if cur + 16.0 >= list_bounds[1] - 2.0 && cur <= list_bounds[1] + list_bounds[3] + 2.0 {
            canvas.text(draw, &sec_lbl, [list_bounds[0] + 6.0, cur + 3.0], UI_TEXT_SEC, false);
        }
        cur += 16.0;
        cur += draw_grouped_card_rows(canvas, draw, ui, &sub, list_bounds[0], cur, list_bounds[2], list_bounds, mouse, regions);
        cur += 8.0;
    }
    (cur - start_y).max(0.0)
}

pub fn draw_system_tab<C: UiCanvas>(
    canvas: &C,
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
    let ui_mode = ui.mode;
    y += draw_group_header(canvas, draw, ui_mode, UiGroup::SysDiag, x, y, w, list_bounds, mouse, regions);
    let entries: Vec<&SettingEntry> = SETTINGS_CATALOG.iter().filter(|e| e.group == UiGroup::SysDiag).collect();
    y += draw_grouped_card_rows(canvas, draw, ui, &entries, x, y, w, list_bounds, mouse, regions);
    y += 8.0;

    // Keybindings reference group (resolved from live mod key mappings)
    ui.section_offsets
        .insert("sys_keys".to_owned(), (y - start_y).max(0.0));
    if y + 16.0 >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
        canvas.text(draw, &tr("rreturn.ui.group.sys_keys"), [x + 6.0, y + 3.0], UI_TEXT_SEC, false);
    }
    y += 16.0;
    let keys_list = [
        ("scrub", "key.rreturn.scrub", "key.keyboard.u"),
        ("save", "key.rreturn.save", "key.keyboard.f6"),
        ("load", "key.rreturn.load", "key.keyboard.f7"),
        ("settings_menu", "key.rreturn.settings_menu", "key.keyboard.k"),
    ];
    let resolved_keys: Vec<(&'static str, String, bool)> = keys_list
        .iter()
        .map(|&(id, label_key, default_key)| {
            let (bind_str, is_bound) = format_mod_binding(canvas, id, default_key);
            ui.bound_hotkeys.insert(id.to_owned(), bind_str.clone());
            (label_key, bind_str, is_bound)
        })
        .collect();
    let kh = 22.0;
    let kt_h = resolved_keys.len() as f32 * kh;
    if y + kt_h >= ly - 4.0 && y <= ly + list_bounds[3] + 4.0 {
        draw_rounded_rect(canvas, draw, [x, y, w, kt_h], 8.0, UI_CARD_BG);
        for (i, (klabel, kbind, is_bound)) in resolved_keys.iter().enumerate() {
            let ky = y + i as f32 * kh;
            if ky + kh >= ly - 2.0 && ky <= ly + list_bounds[3] + 2.0 {
                if i > 0 {
                    canvas.fill(draw, [x + 12.0, ky, w - 24.0, 1.0], UI_DIVIDER);
                }
                let max_kw = (w * 0.45).max(48.0);
                let fit_bind = fit_text(canvas, draw, kbind, max_kw - 10.0);
                let kw = (canvas.text_width(draw, &fit_bind) + 12.0).clamp(24.0, max_kw);
                let kr = [x + w - kw - 12.0, ky + 4.0, kw, 14.0];
                draw_pill(
                    canvas,
                    draw,
                    kr,
                    &fit_bind,
                    UI_PILL_BG,
                    if *is_bound { UI_TEXT_PRI } else { UI_TEXT_MUTED },
                );
                canvas.text(draw, &fit_text(canvas, draw, &tr(klabel), (kr[0] - x - 18.0).max(20.0)), [x + 12.0, ky + 7.0], UI_TEXT_PRI, false);
            }
        }
    }
    y += kt_h + 8.0;

    // Reset card
    ui.section_offsets
        .insert("sys_reset".to_owned(), (y - start_y).max(0.0));
    if y + 16.0 >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
        canvas.text(draw, &tr("rreturn.ui.group.sys_reset"), [x + 6.0, y + 3.0], UI_TEXT_SEC, false);
    }
    y += 16.0;
    let rr = [x, y, w, 30.0];
    if y + 30.0 >= ly - 2.0 && y <= ly + list_bounds[3] + 2.0 {
        let hov = in_list && hit_in_rect(mx, my, rr);
        draw_rounded_rect(canvas, draw, rr, 8.0, if hov { UI_RED_SOFT } else { UI_CARD_BG });
        let lbl = fit_text(canvas, draw, &tr("rreturn.ui.reset_defaults"), w - 24.0);
        let tw = canvas.text_width(draw, &lbl);
        canvas.text(draw, &lbl, [x + (w - tw) * 0.5, y + 11.0], UI_RED, false);
        push_clipped_region(regions, list_bounds, HitRegion { tag: "reset_defaults".to_owned(), rect: rr, action: HitAction::ResetDefaults });
    }
    y += 30.0;
    (y - start_y).max(0.0)
}
