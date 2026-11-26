use super::*;

/// Extension helpers to inspect and control pointer capture during timeline scrub.
pub trait PointerCaptureExt {
    /// Returns true when the window pointer is currently grabbed/captured.
    fn rreturn_pointer_captured(&self) -> bool;
    /// Clears any queued focus-regrab flag while scrubbing keeps the pointer free.
    fn rreturn_clear_regrab_on_refocus(&mut self);
}

impl PointerCaptureExt for Game<'_> {
    fn rreturn_pointer_captured(&self) -> bool {
        self.captured
    }

    fn rreturn_clear_regrab_on_refocus(&mut self) {
        self.regrab_on_refocus = false;
    }
}
