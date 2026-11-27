use super::*;

#[derive(Clone, Default)]
pub struct OrderedStateSnapshot {
    pub restarts: TickRestarts,
    pub born: JoinSeq,
    pub done: Option<JoinSeq>,
    pub announced: JoinSeq,
}

/// Extension helpers to snapshot and restore entity tick-restart order (`OrderedTicks`).
pub trait OrderedExt {
    /// Captures the active dimension's `OrderedTicks` restart/order state.
    fn rreturn_capture_ordered_state(&self) -> OrderedStateSnapshot;
    /// Restores the active dimension's `OrderedTicks` restart/order state cleanly.
    fn rreturn_restore_ordered_state(&mut self, snap: &OrderedStateSnapshot);
    /// Clears stale `TickRestarts` and per-tick `OrderedTicks` progress across rollback.
    fn rreturn_clear_ordered_restarts(&mut self);
}

impl OrderedExt for Game<'_> {
    fn rreturn_capture_ordered_state(&self) -> OrderedStateSnapshot {
        OrderedStateSnapshot {
            restarts: self.ordered.restarts.clone(),
            born: self.ordered.born,
            done: self.ordered.done,
            announced: self.ordered.announced,
        }
    }

    fn rreturn_restore_ordered_state(&mut self, snap: &OrderedStateSnapshot) {
        self.ordered.restarts = snap.restarts.clone();
        self.ordered.born = snap.born;
        self.ordered.done = snap.done;
        self.ordered.announced = snap.announced;
        self.ordered.lava_random = None;
        self.ordered.falling_ticked.clear();
    }

    fn rreturn_clear_ordered_restarts(&mut self) {
        self.ordered.restarts = TickRestarts::default();
        self.ordered.done = None;
        self.ordered.lava_random = None;
        self.ordered.falling_ticked.clear();
    }
}
