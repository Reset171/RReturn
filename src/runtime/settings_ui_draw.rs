use super::super::Game;
use super::policy::OverrideValue;
use super::settings_catalog::{UiGroup, UiTab};
use super::types::AnchorMode;
use horizon_client::hud::HudBatch;
use std::collections::HashMap;
use std::time::Instant;

// Dark theme color tokens
pub const UI_SCRIM_TOP: u32 = 0x7A000000;
pub const UI_SCRIM_BOT: u32 = 0x9E000000;
pub const UI_SHADOW: u32 = 0x4E000000;
pub const UI_WIN_BG: u32 = 0xF50B0B0F;
pub const UI_WIN_BORDER: u32 = 0x1AFFFFFF;
pub const UI_SIDEBAR_BG: u32 = 0xFF0E0F14;
pub const UI_CARD_BG: u32 = 0xFF17181F;
pub const UI_ROW_HOV: u32 = 0x0CFFFFFF;
pub const UI_DIVIDER: u32 = 0x16FFFFFF;
pub const UI_PILL_BG: u32 = 0xFF21232C;
pub const UI_PILL_HOV: u32 = 0xFF2B2E3A;
pub const UI_SEARCH_BG: u32 = 0xFF1A1B23;
pub const UI_BLUE: u32 = 0xFF3E8AFF;
pub const UI_BLUE_SOFT: u32 = 0x293E8AFF;
pub const UI_GREEN: u32 = 0xFF34C759;
pub const UI_RED: u32 = 0xFFFF5C5C;
pub const UI_RED_SOFT: u32 = 0x26FF5C5C;
pub const UI_SWITCH_OFF: u32 = 0xFF424552;
pub const UI_THUMB: u32 = 0xFFF8FAFC;
pub const UI_TRACK_BG: u32 = 0xFF333642;
pub const UI_SEG_DOCK: u32 = 0xFF101117;
pub const UI_TEXT_PRI: u32 = 0xFFF4F5F7;
pub const UI_TEXT_SEC: u32 = 0xFF9497A6;
pub const UI_TEXT_MUTED: u32 = 0xFF727585;

#[derive(Clone)]
pub enum HitAction {
    Close,
    FocusSearch,
    ClearSearch,
    SelectTab(UiTab),
    JumpToSection(UiTab, String, Option<bool>),
    ToggleBool(&'static str),
    ToggleScrubQuick,
    SliderStep(&'static str, i32),
    SliderDrag { id: &'static str, track_x: f32, track_w: f32, min: i32, max: i32, step: i32 },
    SetAnchorMode(AnchorMode),
    SelectPlayer(u128, String),
    SetPlayerScope(bool),
    SetPlayerOverride(u128, String, bool, &'static str, OverrideValue),
    ResetPlayerOverrides(u128, bool),
    TogglePinPlayer(u128),
    ClearPins,
    BatchGroup(UiGroup, bool),
    ResetDefaults,
    SaveCheckpointNow,
    RollbackNow,
    ScrollTrack { track_y: f32, track_h: f32, max_scroll: f32 },
}

#[derive(Clone)]
pub struct HitRegion {
    pub tag: String,
    pub rect: [f32; 4],
    pub action: HitAction,
}

pub struct SliderDragState {
    pub id: &'static str,
    pub track_x: f32,
    pub track_w: f32,
    pub min: i32,
    pub max: i32,
    pub step: i32,
}

pub struct SettingsUiState {
    pub active_tab: UiTab,
    pub search_query: String,
    pub search_focused: bool,
    pub scroll_y: f32,
    pub target_scroll_y: f32,
    pub max_scroll_y: f32,
    pub last_frame: Instant,
    pub toggle_anim: HashMap<String, f32>,
    pub hit_regions: Vec<HitRegion>,
    pub dragging_slider: Option<SliderDragState>,
    pub dragging_scroll: Option<(f32, f32, f32)>,
    pub window_bounds: [f32; 4],
    pub selected_player: Option<u128>,
    pub player_scope_scrub: bool,
    pub section_offsets: HashMap<String, f32>,
    pub pending_jump_section: Option<String>,
    pub pending_open: bool,
    pub pending_close: bool,
    pub pending_save: bool,
    pub pending_scrub_sync: Option<bool>,
    pub pending_checkpoint_now: bool,
    pub pending_rollback_now: bool,
}

impl Default for SettingsUiState {
    fn default() -> Self {
        Self {
            active_tab: UiTab::Rollback,
            search_query: String::new(),
            search_focused: false,
            scroll_y: 0.0,
            target_scroll_y: 0.0,
            max_scroll_y: 0.0,
            last_frame: Instant::now(),
            toggle_anim: HashMap::new(),
            hit_regions: Vec::new(),
            dragging_slider: None,
            dragging_scroll: None,
            window_bounds: [0.0; 4],
            selected_player: None,
            player_scope_scrub: false,
            section_offsets: HashMap::new(),
            pending_jump_section: None,
            pending_open: false,
            pending_close: false,
            pending_save: false,
            pending_scrub_sync: None,
            pending_checkpoint_now: false,
            pending_rollback_now: false,
        }
    }
}

pub fn hit_in_rect(mx: f32, my: f32, r: [f32; 4]) -> bool {
    r[2] > 0.0 && r[3] > 0.0 && mx >= r[0] && mx <= r[0] + r[2] && my >= r[1] && my <= r[1] + r[3]
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

pub fn lerp_argb(c0: u32, c1: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let [a0, r0, g0, b0] = c0.to_be_bytes();
    let [a1, r1, g1, b1] = c1.to_be_bytes();
    u32::from_be_bytes([
        lerp_u8(a0, a1, t),
        lerp_u8(r0, r1, t),
        lerp_u8(g0, g1, t),
        lerp_u8(b0, b1, t),
    ])
}

pub fn draw_rounded_rect(game: &Game<'_>, draw: &mut HudBatch, [x, y, w, h]: [f32; 4], radius: f32, argb: u32) {
    if w <= 0.0 || h <= 0.0 || (argb >> 24) == 0 {
        return;
    }
    let r = radius.clamp(0.0, (w * 0.5).min(h * 0.5)).floor();
    if r <= 0.5 {
        game.native_fill(draw, [x, y, w, h], argb);
        return;
    }
    if h > r * 2.0 {
        game.native_fill(draw, [x, y + r, w, h - r * 2.0], argb);
    }
    let ri = r as i32;
    for row in 0..ri {
        let dy = (r - 0.5) - row as f32;
        let dx = (r * r - dy * dy).max(0.0).sqrt();
        let inset = (r - dx).max(0.0);
        let row_w = (w - inset * 2.0).max(0.0);
        if row_w > 0.0 {
            game.native_fill(draw, [x + inset, y + row as f32, row_w, 1.0], argb);
            game.native_fill(draw, [x + inset, y + h - 1.0 - row as f32, row_w, 1.0], argb);
        }
    }
}

pub fn draw_rounded_border(game: &Game<'_>, draw: &mut HudBatch, [x, y, w, h]: [f32; 4], radius: f32, argb: u32) {
    if w <= 2.0 || h <= 2.0 || (argb >> 24) == 0 {
        return;
    }
    let r = radius.clamp(0.0, (w * 0.5).min(h * 0.5)).floor();
    if r <= 0.5 {
        game.native_fill(draw, [x, y, w, 1.0], argb);
        game.native_fill(draw, [x, y + h - 1.0, w, 1.0], argb);
        game.native_fill(draw, [x, y + 1.0, 1.0, h - 2.0], argb);
        game.native_fill(draw, [x + w - 1.0, y + 1.0, 1.0, h - 2.0], argb);
        return;
    }
    let flat_w = (w - r * 2.0).max(0.0);
    if flat_w > 0.0 {
        game.native_fill(draw, [x + r, y, flat_w, 1.0], argb);
        game.native_fill(draw, [x + r, y + h - 1.0, flat_w, 1.0], argb);
    }
    let flat_h = (h - r * 2.0).max(0.0);
    if flat_h > 0.0 {
        game.native_fill(draw, [x, y + r, 1.0, flat_h], argb);
        game.native_fill(draw, [x + w - 1.0, y + r, 1.0, flat_h], argb);
    }
    let ri = r as i32;
    for row in 0..ri {
        let dy = (r - 0.5) - row as f32;
        let dx = (r * r - dy * dy).max(0.0).sqrt();
        let inset = (r - dx).round().max(0.0);
        game.native_fill(draw, [x + inset, y + row as f32, 1.0, 1.0], argb);
        game.native_fill(draw, [x + w - 1.0 - inset, y + row as f32, 1.0, 1.0], argb);
        game.native_fill(draw, [x + inset, y + h - 1.0 - row as f32, 1.0, 1.0], argb);
        game.native_fill(draw, [x + w - 1.0 - inset, y + h - 1.0 - row as f32, 1.0, 1.0], argb);
    }
}

pub fn fit_text(game: &Game<'_>, draw: &HudBatch, s: &str, max_w: f32) -> String {
    if max_w <= 6.0 {
        return String::new();
    }
    if game.native_text_width(draw, s) <= max_w {
        return s.to_owned();
    }
    let ell = "...";
    let ell_w = game.native_text_width(draw, ell);
    if ell_w >= max_w {
        return String::new();
    }
    for (idx, _) in s.char_indices().rev() {
        if game.native_text_width(draw, &s[..idx]) + ell_w <= max_w {
            return format!("{}{}", &s[..idx], ell);
        }
    }
    String::new()
}

pub fn draw_pill(
    game: &Game<'_>,
    draw: &mut HudBatch,
    rect: [f32; 4],
    label: &str,
    bg: u32,
    text_col: u32,
) {
    draw_rounded_rect(game, draw, rect, (rect[3] * 0.5).min(8.0), bg);
    let tw = game.native_text_width(draw, label);
    let tx = (rect[0] + (rect[2] - tw) * 0.5).floor();
    let ty = (rect[1] + (rect[3] - 8.0) * 0.5).floor();
    game.native_text(draw, label, [tx, ty], text_col, false);
}

pub fn draw_switch(
    game: &Game<'_>,
    draw: &mut HudBatch,
    x: f32,
    y: f32,
    anim: f32,
) {
    let sw = [x, y, 28.0, 14.0];
    draw_rounded_rect(game, draw, sw, 7.0, lerp_argb(UI_SWITCH_OFF, UI_BLUE, anim));
    let knob_x = sw[0] + 2.0 + anim * 14.0;
    draw_rounded_rect(game, draw, [knob_x, sw[1] + 2.0, 10.0, 10.0], 5.0, UI_THUMB);
}

pub fn slider_snap(mx: f32, track_x: f32, track_w: f32, min: i32, max: i32, step: i32) -> i32 {
    let frac = ((mx - track_x) / track_w.max(1.0)).clamp(0.0, 1.0);
    let raw = min as f32 + frac * (max - min) as f32;
    let stepped = ((raw - min as f32) / step.max(1) as f32).round() as i32 * step.max(1) + min;
    stepped.clamp(min, max)
}
