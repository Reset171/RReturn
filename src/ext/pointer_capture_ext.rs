use super::*;

/// Extension helpers to inspect and control pointer capture during timeline scrub.
pub trait PointerCaptureExt {
    /// Returns true when the window pointer is currently grabbed/captured.
    fn rreturn_pointer_captured(&self) -> bool;
    /// Clears any queued focus-regrab flag while scrubbing keeps the pointer free.
    fn rreturn_clear_regrab_on_refocus(&mut self);
    /// Returns true while the primary left mouse button is physically held.
    fn rreturn_left_button_held(&self) -> bool;
    /// Updates `game.cursor` to GUI coordinates and warps the OS pointer when uncaptured.
    fn rreturn_warp_cursor(&mut self, gui_x: f32, gui_y: f32);
}

impl PointerCaptureExt for Game<'_> {
    fn rreturn_pointer_captured(&self) -> bool {
        self.captured
    }

    fn rreturn_clear_regrab_on_refocus(&mut self) {
        self.regrab_on_refocus = false;
    }

    fn rreturn_left_button_held(&self) -> bool {
        self.physical_buttons.contains(&winit::event::MouseButton::Left) || self.left
    }

    fn rreturn_warp_cursor(&mut self, gui_x: f32, gui_y: f32) {
        if !gui_x.is_finite() || !gui_y.is_finite() {
            return;
        }
        let size = self.renderer.as_ref().map_or_else(
            || {
                self.window
                    .as_ref()
                    .map_or([0, 0], |w| w.inner_size().into())
            },
            |r| [r.config.width, r.config.height],
        );
        let scale = HudBatch::new(size, self.settings.gui_scale_inputs()).scale;
        let max_x = if size[0] > 0 && scale > 0.0 {
            size[0] as f32 / scale
        } else {
            f32::MAX
        };
        let max_y = if size[1] > 0 && scale > 0.0 {
            size[1] as f32 / scale
        } else {
            f32::MAX
        };
        let cx = gui_x.clamp(0.0, max_x);
        let cy = gui_y.clamp(0.0, max_y);
        self.cursor = (cx, cy);
        if !self.internal_grab()
            && !self.captured
            && self.keyboard_mouse
            && self.focused
            && scale > 0.0
            && let Some(window) = &self.window
        {
            let [vx, vy] = self.view_at.map(f64::from);
            let pos = winit::dpi::PhysicalPosition::new(
                vx + f64::from(cx * scale),
                vy + f64::from(cy * scale),
            );
            let _ = window.set_cursor_position(pos);
        }
    }
}
