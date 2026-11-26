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
    Close,
}

impl UiIcon {
    fn picture_name(self) -> &'static str {
        match self {
            Self::RReturnLogo => "rreturn:rreturn_icons/rreturn_logo",
            Self::Rollback => "rreturn:rreturn_icons/rollback",
            Self::Scrub => "rreturn:rreturn_icons/scrub",
            Self::Players => "rreturn:rreturn_icons/players",
            Self::System => "rreturn:rreturn_icons/system",
            Self::Clock => "rreturn:rreturn_icons/clock",
            Self::WorldCube => "rreturn:rreturn_icons/world_cube",
            Self::Entities => "rreturn:rreturn_icons/entities",
            Self::PlayerSingle => "rreturn:rreturn_icons/player_single",
            Self::RulesShield => "rreturn:rreturn_icons/rules_shield",
            Self::Crown => "rreturn:rreturn_icons/crown",
            Self::Close => "rreturn:rreturn_icons/close",
        }
    }
}

pub fn draw_ui_icon<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    icon: UiIcon,
    x: f32,
    y: f32,
    col: u32,
) {
    draw_ui_icon_sized(canvas, draw, icon, x, y, 12.0, col);
}

pub fn draw_ui_icon_sized<C: UiCanvas>(
    canvas: &C,
    draw: &mut HudBatch,
    icon: UiIcon,
    x: f32,
    y: f32,
    size: f32,
    col: u32,
) {
    let ox = x.round();
    let oy = y.round();
    let sz = size.max(6.0);
    let glyph_col = if icon == UiIcon::RReturnLogo {
        draw_rounded_rect(canvas, draw, [ox, oy, sz, sz], (sz * 0.25).max(2.5), col);
        0xFFFF_FFFF
    } else {
        col
    };
    if canvas.picture(draw, icon.picture_name(), [ox, oy, sz, sz], glyph_col) {
        return;
    }
    let mut vc = VecCtx {
        canvas,
        draw,
        ox,
        oy,
        scale: sz / 24.0,
        col: glyph_col,
    };
    draw_vector_fallback(&mut vc, icon);
}

struct VecCtx<'a, C: UiCanvas> {
    canvas: &'a C,
    draw: &'a mut HudBatch,
    ox: f32,
    oy: f32,
    scale: f32,
    col: u32,
}

impl<C: UiCanvas> VecCtx<'_, C> {
    fn pt(&self, x: f32, y: f32) -> [f32; 2] {
        [self.ox + x * self.scale, self.oy + y * self.scale]
    }

    fn stroke(&mut self, p0: [f32; 2], p1: [f32; 2], width: f32) {
        let a = self.pt(p0[0], p0[1]);
        let b = self.pt(p1[0], p1[1]);
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.01 {
            return;
        }
        let half = (width * self.scale * 0.5).max(0.55);
        let nx = -dy / len * half;
        let ny = dx / len * half;
        self.canvas.fill_quad(
            self.draw,
            [
                [a[0] + nx, a[1] + ny],
                [b[0] + nx, b[1] + ny],
                [b[0] - nx, b[1] - ny],
                [a[0] - nx, a[1] - ny],
            ],
            self.col,
        );
    }

    fn poly(&mut self, pts: &[[f32; 2]], width: f32) {
        for pair in pts.windows(2) {
            self.stroke(pair[0], pair[1], width);
        }
    }

    fn arc(&mut self, center: [f32; 2], radius: f32, a0: f32, a1: f32, steps: usize, width: f32) {
        let n = steps.max(4);
        let mut prev = [
            center[0] + radius * a0.cos(),
            center[1] + radius * a0.sin(),
        ];
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let ang = a0 + (a1 - a0) * t;
            let next = [
                center[0] + radius * ang.cos(),
                center[1] + radius * ang.sin(),
            ];
            self.stroke(prev, next, width);
            prev = next;
        }
    }

    fn circle(&mut self, center: [f32; 2], radius: f32, steps: usize, width: f32) {
        self.arc(center, radius, 0.0, std::f32::consts::TAU, steps, width);
    }
}

/// Crisp geometric `fill_quad` fallback before `.hmod` picture assets mount on startup.
fn draw_vector_fallback<C: UiCanvas>(vc: &mut VecCtx<'_, C>, icon: UiIcon) {
    match icon {
        UiIcon::RReturnLogo => {
            vc.poly(&[[7.5, 18.5], [7.5, 5.5], [13.0, 5.5]], 2.6);
            vc.arc(
                [13.0, 9.5],
                4.0,
                -std::f32::consts::FRAC_PI_2,
                std::f32::consts::FRAC_PI_2,
                8,
                2.6,
            );
            vc.stroke([13.0, 13.5], [7.5, 13.5], 2.6);
            vc.stroke([12.8, 13.5], [16.8, 18.5], 2.6);
        }
        UiIcon::Rollback => {
            vc.arc([12.0, 12.0], 8.5, -2.35, std::f32::consts::PI, 14, 2.2);
            vc.poly(&[[3.5, 3.5], [3.5, 8.2], [8.2, 8.2]], 2.2);
        }
        UiIcon::Scrub => {
            for (x, y0, y1) in [
                (3.0, 10.0, 14.0),
                (7.5, 6.0, 18.0),
                (12.0, 3.5, 20.5),
                (16.5, 7.0, 17.0),
                (21.0, 10.0, 14.0),
            ] {
                vc.stroke([x, y0], [x, y1], 2.2);
            }
        }
        UiIcon::Players => {
            vc.circle([9.0, 7.5], 3.5, 12, 2.1);
            vc.arc([9.0, 20.0], 6.0, -std::f32::consts::PI, 0.0, 10, 2.1);
            vc.arc([16.0, 7.5], 3.5, -1.1, 1.1, 6, 2.1);
            vc.arc([16.5, 20.0], 5.5, -1.1, 0.0, 6, 2.1);
        }
        UiIcon::PlayerSingle => {
            vc.circle([12.0, 7.5], 3.7, 12, 2.2);
            vc.arc([12.0, 20.0], 6.5, -std::f32::consts::PI, 0.0, 10, 2.2);
        }
        UiIcon::System => {
            for (y, cx) in [(6.0, 14.5), (12.0, 9.5), (18.0, 14.5)] {
                vc.stroke([3.0, y], [cx - 2.4, y], 2.1);
                vc.stroke([cx + 2.4, y], [21.0, y], 2.1);
                vc.circle([cx, y], 2.1, 10, 2.1);
            }
        }
        UiIcon::Clock => {
            vc.circle([12.0, 12.0], 8.8, 16, 2.2);
            vc.poly(&[[12.0, 7.0], [12.0, 12.0], [15.5, 14.0]], 2.2);
        }
        UiIcon::WorldCube => {
            vc.poly(
                &[
                    [12.0, 2.8],
                    [20.2, 7.4],
                    [20.2, 16.6],
                    [12.0, 21.2],
                    [3.8, 16.6],
                    [3.8, 7.4],
                    [12.0, 2.8],
                ],
                2.1,
            );
            vc.poly(&[[3.8, 7.4], [12.0, 12.0], [20.2, 7.4]], 2.1);
            vc.stroke([12.0, 12.0], [12.0, 21.2], 2.1);
        }
        UiIcon::Entities => {
            for (cx, cy) in [(5.2, 10.2), (9.6, 5.8), (14.4, 5.8), (18.8, 10.2)] {
                vc.circle([cx, cy], 1.6, 8, 2.0);
            }
            vc.circle([12.0, 15.6], 4.2, 12, 2.1);
        }
        UiIcon::RulesShield => {
            vc.poly(
                &[
                    [12.0, 2.6],
                    [19.2, 5.5],
                    [19.2, 12.2],
                    [12.0, 21.2],
                    [4.8, 12.2],
                    [4.8, 5.5],
                    [12.0, 2.6],
                ],
                2.1,
            );
            vc.poly(&[[9.0, 12.0], [11.2, 14.2], [15.2, 9.8]], 2.1);
        }
        UiIcon::Crown => {
            vc.stroke([3.2, 19.0], [20.8, 19.0], 2.1);
            vc.poly(
                &[
                    [4.2, 15.2],
                    [3.2, 6.8],
                    [8.2, 10.6],
                    [12.0, 4.8],
                    [15.8, 10.6],
                    [20.8, 6.8],
                    [19.8, 15.2],
                    [4.2, 15.2],
                ],
                2.1,
            );
        }
        UiIcon::Close => {
            vc.stroke([6.5, 6.5], [17.5, 17.5], 2.4);
            vc.stroke([17.5, 6.5], [6.5, 17.5], 2.4);
        }
    }
}
