use super::*;

impl Hub {
    pub fn rreturn_reset_tracked(&mut self) {
        self.entities.clear();
        self.joined.clear();
        self.fresh_joined.clear();
        self.native_tracking.clear();
        self.attachment_snapshots.clear();
        self.level_sync.clear();
        self.last_clock = i64::MIN;
        if let Ok(mut order) = self.sound_order.lock() {
            *order = Default::default();
        }
        if let Some(far) = self.far.as_mut() {
            far.forget_level();
        }
        let _ = super::super::handoff::take_changed();
    }

    pub fn rreturn_sound_seed_state(&self) -> u64 {
        self.sound_seeds.lock().map(|r| r.state()).unwrap_or(0)
    }

    pub fn rreturn_set_sound_seed_state(&self, state: u64) {
        if let Ok(mut r) = self.sound_seeds.lock() {
            *r = LegacyRandom::from_state(state);
        }
    }

    pub fn rreturn_sync_time(&mut self, world: &World, rate: f32) {
        let packet = self.time_packet(world, rate);
        self.broadcast_level(&packet);
        self.last_clock = world.day_time;
        self.last_rate = rate;
    }
}