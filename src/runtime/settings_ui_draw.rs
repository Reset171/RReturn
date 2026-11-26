use super::super::Game;
use super::policy::OverrideValue;
use super::settings_catalog::{UiGroup, UiTab};
use super::types::{
    tr, with_global_state, with_global_state_mut, with_state, with_state_mut, AnchorMode,
    RReturnState, MOD_ID,
};
use horizon_client::hud::{rgba, HudAtlas, HudBatch};
use horizon_client::keybinds;
use horizon_client::mod_list::ModList;
use horizon_client::text::Text;
use std::collections::HashMap;
use std::time::Instant;

// Dark theme color tokens
pub const UI_SCRIM_TOP: u32 = 0x7A000000;
pub const UI_SCRIM_BOT: u32 = 0x9E000000;
pub const UI_WIN_BG: u32 = 0xFC0C0D13;
pub const UI_WIN_BORDER: u32 = 0x1EFFFFFF;
pub const UI_SIDEBAR_BG: u32 = 0xFF0E0F15;
pub const UI_CARD_BG: u32 = 0xFF171820;
pub const UI_ROW_HOV: u32 = 0x10FFFFFF;
pub const UI_DIVIDER: u32 = 0x16FFFFFF;
pub const UI_PILL_BG: u32 = 0xFF21232D;
pub const UI_PILL_HOV: u32 = 0xFF2C2F3D;
pub const UI_SEARCH_BG: u32 = 0xFF191B23;
pub const UI_BLUE: u32 = 0xFF3E8AFF;
pub const UI_BLUE_SOFT: u32 = 0x2B3E8AFF;
pub const UI_GREEN: u32 = 0xFF34C759;
pub const UI_RED: u32 = 0xFFFF5C5C;
pub const UI_RED_SOFT: u32 = 0x28FF5C5C;
pub const UI_SWITCH_OFF: u32 = 0xFF424552;
pub const UI_THUMB: u32 = 0xFFF8FAFC;
pub const UI_TRACK_BG: u32 = 0xFF333642;
pub const UI_SEG_DOCK: u32 = 0xFF101118;
pub const UI_TEXT_PRI: u32 = 0xFFF4F5F7;
pub const UI_TEXT_SEC: u32 = 0xFF989CAB;
pub const UI_TEXT_MUTED: u32 = 0xFF747888;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AutoSaveScheduleMode {
    Fixed,
    Random,
    Adaptive,
}

#[derive(Clone)]
pub enum HitAction {
    Close,
    FocusSearch,
    ClearSearch,
    SelectTab(UiTab),
    JumpToSection(UiTab, String, Option<bool>),
    ToggleBool(&'static str),
    SetAutoSaveMode(AutoSaveScheduleMode),
    SliderStep(&'static str, i32),
    SliderDrag { id: &'static str, track_x: f32, track_w: f32, min: i32, max: i32, step: i32 },
    SetAnchorMode(AnchorMode),
    SelectPlayer(u128, String),
    SetPlayerScope(bool),
    SetPlayerOverride(u128, String, bool, &'static str, OverrideValue),
    BatchPlayerOverrides(u128, String, bool, OverrideValue),
    ResetPlayerOverrides(u128, bool),
    TogglePinPlayer(u128),
    ClearPins,
    BatchGroup(UiGroup, bool),
    ResetDefaults,
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

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsUiMode {
    #[default]
    World,
    Global,
}

pub fn with_mode_state<R>(mode: SettingsUiMode, f: impl FnOnce(&RReturnState) -> R) -> R {
    if mode == SettingsUiMode::Global {
        with_global_state(f)
    } else {
        with_state(f)
    }
}

pub fn with_mode_state_mut<R>(mode: SettingsUiMode, f: impl FnOnce(&mut RReturnState) -> R) -> R {
    if mode == SettingsUiMode::Global {
        with_global_state_mut(f)
    } else {
        with_state_mut(f)
    }
}

fn argb_to_abgr(argb: u32) -> u32 {
    let [a, r, g, b] = argb.to_be_bytes();
    rgba(r, g, b, a)
}

fn canvas_picture(
    atlas: &HudAtlas,
    draw: &mut HudBatch,
    name: &str,
    [x, y, w, h]: [f32; 4],
    argb: u32,
) -> bool {
    let before = draw.vertices.len();
    draw.picture(atlas, name, x, y, w, h, argb_to_abgr(argb));
    draw.vertices.len() > before
}

fn canvas_fill_quad(atlas: &HudAtlas, draw: &mut HudBatch, corners: [[f32; 2]; 4], argb: u32) {
    draw.fill_quad(atlas, corners, argb_to_abgr(argb));
}

pub trait UiCanvas {
    fn screen_size(&self, draw: &HudBatch) -> [f32; 2];
    fn fill(&self, draw: &mut HudBatch, rect: [f32; 4], argb: u32);
    fn fill_quad(&self, draw: &mut HudBatch, corners: [[f32; 2]; 4], argb: u32);
    fn picture(&self, draw: &mut HudBatch, name: &str, rect: [f32; 4], argb: u32) -> bool;
    fn text(&self, draw: &mut HudBatch, text: &str, at: [f32; 2], argb: u32, shadow: bool);
    fn text_width(&self, draw: &HudBatch, text: &str) -> f32;
    fn cursor(&self) -> [f32; 2];
    fn mod_binding(&self, id: &str, default_key: &str) -> (String, u32);
}

impl UiCanvas for Game<'_> {
    fn screen_size(&self, draw: &HudBatch) -> [f32; 2] {
        self.native_size(draw)
    }

    fn fill(&self, draw: &mut HudBatch, rect: [f32; 4], argb: u32) {
        self.native_fill(draw, rect, argb);
    }

    fn fill_quad(&self, draw: &mut HudBatch, corners: [[f32; 2]; 4], argb: u32) {
        canvas_fill_quad(&self.gui.atlas, draw, corners, argb);
    }

    fn picture(&self, draw: &mut HudBatch, name: &str, rect: [f32; 4], argb: u32) -> bool {
        canvas_picture(&self.gui.atlas, draw, name, rect, argb)
    }

    fn text(&self, draw: &mut HudBatch, text: &str, at: [f32; 2], argb: u32, shadow: bool) {
        self.native_text(draw, text, at, argb, shadow);
    }

    fn text_width(&self, draw: &HudBatch, text: &str) -> f32 {
        self.native_text_width(draw, text)
    }

    fn cursor(&self) -> [f32; 2] {
        Game::cursor(self)
    }

    fn mod_binding(&self, id: &str, default_key: &str) -> (String, u32) {
        self.mod_key(MOD_ID, id)
            .map(|(k, m)| (k, m.bits))
            .unwrap_or_else(|| (default_key.to_owned(), 0))
    }
}

pub struct MenuUiCanvas<'a> {
    pub atlas: &'a HudAtlas,
    pub cursor_pos: [f32; 2],
    pub mods: &'a ModList,
}

impl UiCanvas for MenuUiCanvas<'_> {
    fn screen_size(&self, draw: &HudBatch) -> [f32; 2] {
        [draw.width, draw.height]
    }

    fn fill(&self, draw: &mut HudBatch, rect: [f32; 4], argb: u32) {
        let [x, y, w, h] = rect;
        draw.fill(self.atlas, x, y, w, h, argb_to_abgr(argb));
    }

    fn fill_quad(&self, draw: &mut HudBatch, corners: [[f32; 2]; 4], argb: u32) {
        canvas_fill_quad(self.atlas, draw, corners, argb);
    }

    fn picture(&self, draw: &mut HudBatch, name: &str, rect: [f32; 4], argb: u32) -> bool {
        canvas_picture(self.atlas, draw, name, rect, argb)
    }

    fn text(&self, draw: &mut HudBatch, text: &str, at: [f32; 2], argb: u32, shadow: bool) {
        let abgr = argb_to_abgr(argb);
        if (argb >> 24) == 0 {
            return;
        }
        draw.text_styled(
            self.atlas,
            &Text::literal(text),
            at[0],
            at[1],
            abgr,
            shadow,
        );
    }

    fn text_width(&self, _draw: &HudBatch, text: &str) -> f32 {
        HudBatch::text_width(self.atlas, text)
    }

    fn cursor(&self) -> [f32; 2] {
        self.cursor_pos
    }

    fn mod_binding(&self, id: &str, default_key: &str) -> (String, u32) {
        let def = mod_api::__private::key_mappings()
            .iter()
            .find(|(owner, mapping)| *owner == MOD_ID && mapping.id == id)
            .map_or((default_key, 0u32), |(_, m)| (m.key.as_str(), m.modifiers.bits));
        self.mods.binding(MOD_ID, id, def)
    }
}

pub struct SettingsUiState {
    pub mode: SettingsUiMode,
    pub global_modal_open: bool,
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
    pub bound_hotkeys: HashMap<String, String>,
    pub section_offsets: HashMap<String, f32>,
    pub pending_jump_section: Option<String>,
    pub pending_open: bool,
    pub pending_close: bool,
    pub pending_save: bool,
    pub pending_scrub_sync: Option<bool>,
}

impl Default for SettingsUiState {
    fn default() -> Self {
        Self {
            mode: SettingsUiMode::World,
            global_modal_open: false,
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
            bound_hotkeys: HashMap::new(),
            section_offsets: HashMap::new(),
            pending_jump_section: None,
            pending_open: false,
            pending_close: false,
            pending_save: false,
            pending_scrub_sync: None,
        }
    }
}

fn format_single_key_name(key: &str) -> String {
    let raw = keybinds::display_name(key).plain();
    if raw != key {
        return raw;
    }
    if key == "key.keyboard.unknown" {
        return "None".to_owned();
    }
    if let Some(rest) = key.strip_prefix("key.keyboard.") {
        return rest
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|seg| {
                let mut ch = seg.chars();
                match ch.next() {
                    Some(c) => format!("{}{}", c.to_uppercase(), ch.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    raw
}

pub fn format_binding_parts(key: &str, mods: u32) -> (String, bool) {
    if key.is_empty() || key == "key.keyboard.unknown" {
        return (format_single_key_name("key.keyboard.unknown"), false);
    }
    let mut parts: Vec<String> = Vec::with_capacity(4);
    for (bit, mod_key, fallback) in [
        (1u32, "key.keyboard.left.shift", "Shift"),
        (2u32, "key.keyboard.left.control", "Ctrl"),
        (4u32, "key.keyboard.left.alt", "Alt"),
        (8u32, "key.keyboard.left.super", "Super"),
    ] {
        if (mods & bit) != 0 {
            let lbl = keybinds::display_name(mod_key).plain();
            parts.push(if lbl == mod_key { fallback.to_owned() } else { lbl });
        }
    }
    parts.push(format_single_key_name(key));
    (parts.join(" + "), true)
}

pub fn format_mod_binding<C: UiCanvas>(canvas: &C, id: &str, default_key: &str) -> (String, bool) {
    let (key, mods) = canvas.mod_binding(id, default_key);
    format_binding_parts(&key, mods)
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

pub fn draw_rounded_rect<C: UiCanvas>(canvas: &C, draw: &mut HudBatch, [x, y, w, h]: [f32; 4], radius: f32, argb: u32) {
    if w <= 0.0 || h <= 0.0 || (argb >> 24) == 0 {
        return;
    }
    let x0 = x.round();
    let y0 = y.round();
    let x1 = (x + w).round();
    let y1 = (y + h).round();
    let rw = (x1 - x0).max(0.0);
    let rh = (y1 - y0).max(0.0);
    if rw <= 0.0 || rh <= 0.0 {
        return;
    }
    let ri = (radius.clamp(0.0, (rw * 0.5).min(rh * 0.5)).floor() as i32).max(0);
    if ri <= 0 {
        canvas.fill(draw, [x0, y0, rw, rh], argb);
        return;
    }
    let rf = ri as f32;
    if rh > rf * 2.0 {
        canvas.fill(draw, [x0, y0 + rf, rw, rh - rf * 2.0], argb);
    }
    for row in 0..ri {
        let dy = (rf - 0.5) - row as f32;
        let dx = (rf * rf - dy * dy).max(0.0).sqrt();
        let inset = (rf - dx).round().max(0.0);
        let row_w = (rw - inset * 2.0).max(0.0);
        if row_w > 0.0 {
            canvas.fill(draw, [x0 + inset, y0 + row as f32, row_w, 1.0], argb);
            canvas.fill(draw, [x0 + inset, y1 - 1.0 - row as f32, row_w, 1.0], argb);
        }
    }
}

pub fn draw_rounded_left_rect<C: UiCanvas>(canvas: &C, draw: &mut HudBatch, [x, y, w, h]: [f32; 4], radius: f32, argb: u32) {
    if w <= 0.0 || h <= 0.0 || (argb >> 24) == 0 {
        return;
    }
    let x0 = x.round();
    let y0 = y.round();
    let x1 = (x + w).round();
    let y1 = (y + h).round();
    let rw = (x1 - x0).max(0.0);
    let rh = (y1 - y0).max(0.0);
    let ri = (radius.clamp(0.0, rw.min(rh * 0.5)).floor() as i32).max(0);
    if ri <= 0 {
        canvas.fill(draw, [x0, y0, rw, rh], argb);
        return;
    }
    let rf = ri as f32;
    if rh > rf * 2.0 {
        canvas.fill(draw, [x0, y0 + rf, rw, rh - rf * 2.0], argb);
    }
    for row in 0..ri {
        let dy = (rf - 0.5) - row as f32;
        let dx = (rf * rf - dy * dy).max(0.0).sqrt();
        let inset = (rf - dx).round().max(0.0);
        let row_w = (rw - inset).max(0.0);
        if row_w > 0.0 {
            canvas.fill(draw, [x0 + inset, y0 + row as f32, row_w, 1.0], argb);
            canvas.fill(draw, [x0 + inset, y1 - 1.0 - row as f32, row_w, 1.0], argb);
        }
    }
}

pub fn draw_soft_shadow<C: UiCanvas>(canvas: &C, draw: &mut HudBatch, [x, y, w, h]: [f32; 4], radius: f32, layers: usize) {
    let alphas: &[u32] = &[0x1800_0000, 0x1100_0000, 0x0B00_0000, 0x0600_0000];
    let n = layers.min(alphas.len());
    for i in (0..n).rev() {
        let pad = (i + 1) as f32;
        let sr = if radius > 0.5 { radius + pad } else { 0.0 };
        draw_rounded_rect(
            canvas,
            draw,
            [x - pad, y - pad + 1.0, w + pad * 2.0, h + pad * 2.0],
            sr,
            alphas[i],
        );
    }
}

pub fn draw_rounded_border<C: UiCanvas>(canvas: &C, draw: &mut HudBatch, [x, y, w, h]: [f32; 4], radius: f32, argb: u32) {
    let x0 = x.round();
    let y0 = y.round();
    let rw = ((x + w).round() - x0).max(0.0);
    let rh = ((y + h).round() - y0).max(0.0);
    if rw <= 2.0 || rh <= 2.0 || (argb >> 24) == 0 {
        return;
    }
    let ri = (radius.clamp(0.0, (rw * 0.5).min(rh * 0.5)).floor() as i32).max(0);
    if ri <= 0 {
        canvas.fill(draw, [x0, y0, rw, 1.0], argb);
        canvas.fill(draw, [x0, y0 + rh - 1.0, rw, 1.0], argb);
        canvas.fill(draw, [x0, y0 + 1.0, 1.0, rh - 2.0], argb);
        canvas.fill(draw, [x0 + rw - 1.0, y0 + 1.0, 1.0, rh - 2.0], argb);
        return;
    }
    let rf = ri as f32;
    let flat_w = (rw - rf * 2.0).max(0.0);
    if flat_w > 0.0 {
        canvas.fill(draw, [x0 + rf, y0, flat_w, 1.0], argb);
        canvas.fill(draw, [x0 + rf, y0 + rh - 1.0, flat_w, 1.0], argb);
    }
    let flat_h = (rh - rf * 2.0).max(0.0);
    if flat_h > 0.0 {
        canvas.fill(draw, [x0, y0 + rf, 1.0, flat_h], argb);
        canvas.fill(draw, [x0 + rw - 1.0, y0 + rf, 1.0, flat_h], argb);
    }
    for row in 0..ri {
        let dy = (rf - 0.5) - row as f32;
        let dx = (rf * rf - dy * dy).max(0.0).sqrt();
        let inset = (rf - dx).round().max(0.0);
        canvas.fill(draw, [x0 + inset, y0 + row as f32, 1.0, 1.0], argb);
        canvas.fill(draw, [x0 + rw - 1.0 - inset, y0 + row as f32, 1.0, 1.0], argb);
        canvas.fill(draw, [x0 + inset, y0 + rh - 1.0 - row as f32, 1.0, 1.0], argb);
        canvas.fill(draw, [x0 + rw - 1.0 - inset, y0 + rh - 1.0 - row as f32, 1.0, 1.0], argb);
    }
}

pub fn fit_text<C: UiCanvas>(canvas: &C, draw: &HudBatch, s: &str, max_w: f32) -> String {
    if max_w <= 6.0 {
        return String::new();
    }
    if canvas.text_width(draw, s) <= max_w {
        return s.to_owned();
    }
    let ell = "...";
    let ell_w = canvas.text_width(draw, ell);
    if ell_w >= max_w {
        return String::new();
    }
    for (idx, _) in s.char_indices().rev() {
        if canvas.text_width(draw, &s[..idx]) + ell_w <= max_w {
            return format!("{}{}", &s[..idx], ell);
        }
    }
    String::new()
}

pub fn draw_pill<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    rect: [f32; 4],
    label: &str,
    bg: u32,
    text_col: u32,
) {
    draw_rounded_rect(canvas, draw, rect, (rect[3] * 0.5).min(8.0), bg);
    let tw = canvas.text_width(draw, label);
    let tx = (rect[0] + (rect[2] - tw) * 0.5).floor();
    let ty = (rect[1] + (rect[3] - 8.0) * 0.5).floor();
    canvas.text(draw, label, [tx, ty], text_col, false);
}

pub fn draw_switch<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    x: f32,
    y: f32,
    anim: f32,
) {
    let sw = [x, y, 28.0, 14.0];
    draw_rounded_rect(canvas, draw, sw, 7.0, lerp_argb(UI_SWITCH_OFF, UI_BLUE, anim));
    let knob_x = sw[0] + 2.0 + anim * 14.0;
    draw_rounded_rect(canvas, draw, [knob_x, sw[1] + 2.0, 10.0, 10.0], 5.0, UI_THUMB);
}

pub fn slider_snap(mx: f32, track_x: f32, track_w: f32, min: i32, max: i32, step: i32) -> i32 {
    let frac = ((mx - track_x) / track_w.max(1.0)).clamp(0.0, 1.0);
    let raw = min as f32 + frac * (max - min) as f32;
    let stepped = ((raw - min as f32) / step.max(1) as f32).round() as i32 * step.max(1) + min;
    stepped.clamp(min, max)
}
