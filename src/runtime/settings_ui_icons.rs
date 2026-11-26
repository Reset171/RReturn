use super::settings_ui_draw::{draw_rounded_rect, UiCanvas};
use horizon_client::hud::HudBatch;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiIcon {
    RReturnLogo,
    Rollback,
    Scrub,
    Players,
    System,
    Clock,
    WorldCube,
    Entities,
    PlayerSingle,
    RulesShield,
    Crown,
}

fn px<C: UiCanvas>(canvas: &C, draw: &mut HudBatch, x: f32, y: f32, w: f32, h: f32, col: u32) {
    canvas.fill(draw, [x.floor(), y.floor(), w, h], col);
}

pub fn draw_ui_icon<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    icon: UiIcon,
    x: f32,
    y: f32,
    col: u32,
) {
    let ox = x.floor();
    let oy = y.floor();
    match icon {
        UiIcon::RReturnLogo => {
            draw_rounded_rect(canvas, draw, [ox, oy, 11.0, 11.0], 3.0, col);
            let fg = 0xFFFFFFFF;
            px(canvas, draw, ox + 3.0, oy + 3.0, 5.0, 1.5, fg);
            px(canvas, draw, ox + 2.5, oy + 4.5, 1.5, 2.5, fg);
            px(canvas, draw, ox + 7.0, oy + 4.5, 1.5, 3.0, fg);
            px(canvas, draw, ox + 3.5, oy + 7.0, 4.5, 1.5, fg);
            px(canvas, draw, ox + 2.0, oy + 2.0, 2.0, 2.0, fg);
        }
        UiIcon::Rollback => {
            // Rewind arrows (<<)
            px(canvas, draw, ox + 1.5, oy + 4.5, 1.5, 2.0, col);
            px(canvas, draw, ox + 3.0, oy + 3.0, 1.5, 5.0, col);
            px(canvas, draw, ox + 4.5, oy + 2.0, 1.0, 7.0, col);
            px(canvas, draw, ox + 6.5, oy + 4.5, 1.5, 2.0, col);
            px(canvas, draw, ox + 8.0, oy + 3.0, 1.5, 5.0, col);
            px(canvas, draw, ox + 9.5, oy + 2.0, 1.0, 7.0, col);
        }
        UiIcon::Scrub => {
            // Audio/timeline waveform + cursor line
            px(canvas, draw, ox + 1.0, oy + 4.0, 1.5, 3.0, col);
            px(canvas, draw, ox + 3.5, oy + 2.0, 1.5, 7.0, col);
            px(canvas, draw, ox + 6.0, oy + 1.0, 1.5, 9.0, col);
            px(canvas, draw, ox + 8.5, oy + 3.0, 1.5, 5.0, col);
        }
        UiIcon::Players => {
            // Two silhouettes side-by-side
            px(canvas, draw, ox + 2.0, oy + 1.5, 3.0, 3.0, col);
            px(canvas, draw, ox + 1.0, oy + 5.5, 5.0, 3.5, col);
            px(canvas, draw, ox + 6.5, oy + 2.5, 2.5, 2.5, col);
            px(canvas, draw, ox + 6.0, oy + 6.0, 4.0, 3.0, col);
        }
        UiIcon::PlayerSingle => {
            // Single user silhouette
            px(canvas, draw, ox + 3.5, oy + 1.5, 4.0, 3.5, col);
            px(canvas, draw, ox + 2.0, oy + 6.0, 7.0, 3.5, col);
        }
        UiIcon::System => {
            // Sliders / terminal tuning bars
            px(canvas, draw, ox + 1.0, oy + 2.0, 9.0, 1.5, col);
            px(canvas, draw, ox + 6.5, oy + 1.0, 2.0, 3.5, col);
            px(canvas, draw, ox + 1.0, oy + 5.0, 9.0, 1.5, col);
            px(canvas, draw, ox + 2.5, oy + 4.0, 2.0, 3.5, col);
            px(canvas, draw, ox + 1.0, oy + 8.0, 9.0, 1.5, col);
            px(canvas, draw, ox + 5.5, oy + 7.0, 2.0, 3.5, col);
        }
        UiIcon::Clock => {
            // Clock circle outline + hands
            px(canvas, draw, ox + 2.5, oy + 1.0, 6.0, 1.5, col);
            px(canvas, draw, ox + 2.5, oy + 8.5, 6.0, 1.5, col);
            px(canvas, draw, ox + 1.0, oy + 2.5, 1.5, 6.0, col);
            px(canvas, draw, ox + 8.5, oy + 2.5, 1.5, 6.0, col);
            px(canvas, draw, ox + 5.0, oy + 3.0, 1.5, 3.0, col);
            px(canvas, draw, ox + 5.0, oy + 5.0, 3.0, 1.5, col);
        }
        UiIcon::WorldCube => {
            // 3D cube outline
            px(canvas, draw, ox + 2.0, oy + 1.5, 7.0, 1.5, col);
            px(canvas, draw, ox + 1.5, oy + 3.0, 1.5, 6.0, col);
            px(canvas, draw, ox + 8.0, oy + 3.0, 1.5, 6.0, col);
            px(canvas, draw, ox + 2.0, oy + 8.0, 7.0, 1.5, col);
            px(canvas, draw, ox + 3.0, oy + 4.5, 5.0, 1.0, col);
            px(canvas, draw, ox + 5.0, oy + 5.0, 1.5, 3.0, col);
        }
        UiIcon::Entities => {
            // Creature / diamond spark
            px(canvas, draw, ox + 4.5, oy + 1.0, 2.0, 2.0, col);
            px(canvas, draw, ox + 2.5, oy + 3.0, 6.0, 2.0, col);
            px(canvas, draw, ox + 1.5, oy + 5.0, 8.0, 2.0, col);
            px(canvas, draw, ox + 3.0, oy + 7.0, 2.0, 2.5, col);
            px(canvas, draw, ox + 6.0, oy + 7.0, 2.0, 2.5, col);
        }
        UiIcon::RulesShield => {
            // Shield / checklist document
            px(canvas, draw, ox + 2.0, oy + 1.5, 7.0, 1.5, col);
            px(canvas, draw, ox + 2.0, oy + 3.0, 1.5, 4.5, col);
            px(canvas, draw, ox + 7.5, oy + 3.0, 1.5, 4.5, col);
            px(canvas, draw, ox + 3.5, oy + 7.5, 4.0, 1.5, col);
            px(canvas, draw, ox + 4.5, oy + 9.0, 2.0, 1.0, col);
            px(canvas, draw, ox + 4.0, oy + 4.5, 3.0, 1.5, col);
        }
        UiIcon::Crown => {
            // 3-peak crown
            px(canvas, draw, ox + 1.5, oy + 3.0, 1.5, 5.0, col);
            px(canvas, draw, ox + 4.7, oy + 2.0, 1.5, 6.0, col);
            px(canvas, draw, ox + 8.0, oy + 3.0, 1.5, 5.0, col);
            px(canvas, draw, ox + 3.0, oy + 5.0, 2.0, 2.0, col);
            px(canvas, draw, ox + 6.0, oy + 5.0, 2.0, 2.0, col);
            px(canvas, draw, ox + 1.5, oy + 7.5, 8.0, 1.5, col);
        }
    }
}
