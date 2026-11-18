use super::*;

impl Hook {
    pub fn rreturn_create(id: u32, pos: [f64; 3], prev: [f64; 3], rng_state: u64) -> Self {
        Self::rreturn_create_full(
            id,
            pos,
            prev,
            [0.0, 0.0, 0.0],
            rng_state,
            0,
            0,
            0,
            0.0,
            true,
            false,
        )
    }

    pub fn rreturn_create_full(
        id: u32,
        pos: [f64; 3],
        prev: [f64; 3],
        vel: [f64; 3],
        rng_state: u64,
        nibble: i32,
        time_until_lured: i32,
        time_until_hooked: i32,
        fish_angle: f32,
        open_water: bool,
        state_bobbing: bool,
    ) -> Self {
        let p = Vec3::new(pos[0], pos[1], pos[2]);
        let mut hook = Hook::remote(id, p);
        hook.previous = Vec3::new(prev[0], prev[1], prev[2]);
        hook.velocity = Vec3::new(vel[0], vel[1], vel[2]);
        if rng_state != 0 {
            hook.random = LegacyRandom::from_state(rng_state);
        }
        hook.nibble = nibble;
        hook.time_until_lured = time_until_lured;
        hook.time_until_hooked = time_until_hooked;
        hook.fish_angle = fish_angle;
        hook.open_water = open_water;
        if state_bobbing {
            hook.state = State::Bobbing;
        }
        hook
    }

    pub fn rreturn_velocity(&self) -> [f64; 3] {
        [self.velocity.x, self.velocity.y, self.velocity.z]
    }

    pub fn rreturn_fishing_state(&self) -> (i32, i32, i32, f32, bool, bool) {
        (
            self.nibble,
            self.time_until_lured,
            self.time_until_hooked,
            self.fish_angle,
            self.open_water,
            matches!(self.state, State::Bobbing),
        )
    }
}

impl Game<'_> {
    pub fn rreturn_restore_hook(
        &mut self,
        id: u32,
        pos: [f64; 3],
        prev: [f64; 3],
        rng_state: u64,
    ) {
        self.rreturn_restore_hook_full(
            id,
            pos,
            prev,
            [0.0, 0.0, 0.0],
            rng_state,
            0,
            0,
            0,
            0.0,
            true,
            false,
        );
    }

    pub fn rreturn_restore_hook_full(
        &mut self,
        id: u32,
        pos: [f64; 3],
        prev: [f64; 3],
        vel: [f64; 3],
        rng_state: u64,
        nibble: i32,
        time_until_lured: i32,
        time_until_hooked: i32,
        fish_angle: f32,
        open_water: bool,
        state_bobbing: bool,
    ) {
        let p = Vec3::new(pos[0], pos[1], pos[2]);
        let v = Vec3::new(vel[0], vel[1], vel[2]);
        if let Some(h) = self.hook.as_mut() {
            h.position = p;
            h.previous = Vec3::new(prev[0], prev[1], prev[2]);
            h.velocity = v;
            h.nibble = nibble;
            h.time_until_lured = time_until_lured;
            h.time_until_hooked = time_until_hooked;
            h.fish_angle = fish_angle;
            h.open_water = open_water;
            if state_bobbing {
                h.state = State::Bobbing;
            }
            if rng_state != 0 {
                h.random = LegacyRandom::from_state(rng_state);
            }
        } else {
            let mut hook = Hook::rreturn_create_full(
                id,
                pos,
                prev,
                vel,
                rng_state,
                nibble,
                time_until_lured,
                time_until_hooked,
                fish_angle,
                open_water,
                state_bobbing,
            );
            let b = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
            let liquid = self.water_height(b[0], b[1], b[2]).map_or(0.0, |w| w.0);
            if liquid > 0.0 {
                hook.state = State::Bobbing;
            }
            self.hook = Some(hook);
        }
    }
}
