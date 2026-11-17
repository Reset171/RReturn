use super::*;

impl Hook {
    pub fn rreturn_create(id: u32, pos: [f64; 3], prev: [f64; 3], rng_state: u64) -> Self {
        let p = Vec3::new(pos[0], pos[1], pos[2]);
        let mut hook = Hook::remote(id, p);
        hook.previous = Vec3::new(prev[0], prev[1], prev[2]);
        if rng_state != 0 {
            hook.random = LegacyRandom::from_state(rng_state);
        }
        hook
    }
}

impl Game<'_> {
    pub fn rreturn_restore_hook(&mut self, id: u32, pos: [f64; 3], prev: [f64; 3], rng_state: u64) {
        let p = Vec3::new(pos[0], pos[1], pos[2]);
        if let Some(h) = self.hook.as_mut() {
            h.position = p;
            h.previous = Vec3::new(prev[0], prev[1], prev[2]);
            if rng_state != 0 {
                h.random = LegacyRandom::from_state(rng_state);
            }
        } else {
            let mut hook = Hook::rreturn_create(id, pos, prev, rng_state);
            let b = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
            let liquid = self.water_height(b[0], b[1], b[2]).map_or(0.0, |w| w.0);
            if liquid > 0.0 {
                hook.state = State::Bobbing;
            }
            self.hook = Some(hook);
        }
    }
}
