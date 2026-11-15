use super::*;

impl Stats {
    pub fn rreturn_phantom_tick(&self) -> i32 {
        self.phantom_tick
    }

    pub fn rreturn_set_phantom_tick(&mut self, value: i32) {
        self.phantom_tick = value;
    }
}