use super::*;

pub trait StatsExt {
    fn rreturn_phantom_tick(&self) -> i32;
    fn rreturn_set_phantom_tick(&mut self, value: i32);
}

impl StatsExt for Stats {
    fn rreturn_phantom_tick(&self) -> i32 {
        self.phantom_tick
    }

    fn rreturn_set_phantom_tick(&mut self, value: i32) {
        self.phantom_tick = value;
    }
}
