use super::*;
use horizon_gameplay::{
    custom_spawner::CustomSpawnerLevel,
    village_siege::SiegeLevel,
};

/// Serializable snapshot of `horizon_gameplay::village_siege::VillageSiege`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VillageSiegeSnapshot {
    pub tonight: bool,
    pub set_up: bool,
    pub zombies_to_spawn: i32,
    pub next_spawn_time: i32,
    pub spot: [i32; 3],
}

/// Lossless snapshot of Horizon's `Raiding` runtime caches & per-dimension load flags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RaidRuntimeSnapshot {
    pub present: bool,
    pub loaded: bool,
    pub omen_position: Option<[i32; 3]>,
    pub omen_loaded: bool,
    pub unseeded: Option<Vec<(i32, usize)>>,
    pub bar_hurts: Vec<(i32, u64)>,
    pub homes: Vec<(i32, i64, std::sync::Arc<Vec<[i32; 3]>>)>,
}

/// Captures private `Raiding` runtime fields from either live `game.raiding` or a parked level.
pub(in crate::game) fn capture_raiding_runtime(
    raiding: &super::Raiding,
    dim: Dim,
) -> RaidRuntimeSnapshot {
    let mut bar_hurts: Vec<(i32, u64)> =
        raiding.bar_hurts.iter().map(|(&k, &v)| (k, v)).collect();
    bar_hurts.sort_unstable_by_key(|&(k, _)| k);
    let mut homes: Vec<(i32, i64, std::sync::Arc<Vec<[i32; 3]>>)> = raiding
        .homes
        .iter()
        .map(|(&k, (t, v))| (k, *t, std::sync::Arc::clone(v)))
        .collect();
    homes.sort_unstable_by_key(|&(k, _, _)| k);
    RaidRuntimeSnapshot {
        present: true,
        loaded: raiding.loaded == Some(dim),
        omen_position: raiding.omen_position,
        omen_loaded: raiding.omen_loaded,
        unseeded: raiding.unseeded.clone(),
        bar_hurts,
        homes,
    }
}

/// Extension helpers for `PatrolSpawner`, `VillageSiege`, and `Raiding` runtime state.
pub trait RaidsExt {
    fn rreturn_patrol_next_tick(&self) -> i32;
    fn rreturn_set_patrol_next_tick(&mut self, value: i32);
    fn rreturn_tick_village_siege(&mut self, state: &mut VillageSiegeSnapshot);
    fn rreturn_restore_raid_runtime(&mut self, dim: Dim, meta: &RaidRuntimeSnapshot);
    fn rreturn_clear_raid_runtime(&mut self);
    fn rreturn_read_raids_only(&mut self);
    fn rreturn_restore_raid_runtime_after_disk_load(&mut self, meta: &RaidRuntimeSnapshot);
}

impl RaidsExt for Game<'_> {
    fn rreturn_patrol_next_tick(&self) -> i32 {
        self.raiding.patrol.next_tick
    }

    fn rreturn_set_patrol_next_tick(&mut self, value: i32) {
        self.raiding.patrol.next_tick = value;
    }

    fn rreturn_restore_raid_runtime(&mut self, dim: Dim, meta: &RaidRuntimeSnapshot) {
        self.raiding.loaded = meta.loaded.then_some(dim);
        self.raiding.omen_position = meta.omen_position;
        self.raiding.omen_loaded = meta.omen_loaded;
        self.raiding.unseeded = meta.unseeded.clone();
        self.raiding.bar_hurts = meta.bar_hurts.iter().copied().collect();
        self.raiding.homes = meta
            .homes
            .iter()
            .map(|(k, t, v)| (*k, (*t, std::sync::Arc::clone(v))))
            .collect();
    }

    fn rreturn_clear_raid_runtime(&mut self) {
        self.raiding.raids = Default::default();
        self.raiding.loaded = None;
        self.raiding.omen_position = None;
        self.raiding.omen_loaded = false;
        self.raiding.unseeded = None;
        self.raiding.bar_hurts.clear();
        self.raiding.homes.clear();
    }

    fn rreturn_read_raids_only(&mut self) {
        self.raiding.loaded = None;
        self.read_raids();
    }

    fn rreturn_restore_raid_runtime_after_disk_load(&mut self, meta: &RaidRuntimeSnapshot) {
        self.raiding.omen_position = meta.omen_position;
        self.raiding.omen_loaded = meta.omen_loaded;
        if meta.unseeded.is_some() {
            self.raiding.unseeded = meta.unseeded.clone();
        }
        self.raiding.bar_hurts = meta.bar_hurts.iter().copied().collect();
        self.raiding.homes = meta
            .homes
            .iter()
            .map(|(k, t, v)| (*k, (*t, std::sync::Arc::clone(v))))
            .collect();
    }

    fn rreturn_tick_village_siege(&mut self, state: &mut VillageSiegeSnapshot) {
        if !self.overworld() {
            return;
        }
        if self.sky_darken() < 4 || !self.spawn_mobs_and_monsters() {
            state.tonight = false;
            state.set_up = false;
            return;
        }
        if self.world.day_time.rem_euclid(24000) == 18000 {
            let tonight = self.world.random.next_int_bound(10).unwrap_or(1) == 0;
            state.tonight = tonight;
        }
        if !state.tonight {
            return;
        }
        if !state.set_up {
            if !setup_siege(self, state) {
                return;
            }
            state.set_up = true;
        }
        if state.next_spawn_time > 0 {
            state.next_spawn_time -= 1;
            return;
        }
        state.next_spawn_time = 2;
        if state.zombies_to_spawn > 0 {
            spawn_siege_zombie(self, state.spot);
            state.zombies_to_spawn -= 1;
        } else {
            state.tonight = false;
        }
    }
}

fn setup_siege(game: &mut Game<'_>, state: &mut VillageSiegeSnapshot) -> bool {
    let centers = game.village_centers();
    for player in game.players() {
        if player.spectator {
            continue;
        }
        let center = [
            player.position.x.floor() as i32,
            player.position.y.floor() as i32,
            player.position.z.floor() as i32,
        ];
        if !<Game<'_> as SiegeLevel>::is_village(&centers, center)
            || game.biome_tag("minecraft:without_zombie_sieges", center)
        {
            continue;
        }
        for _ in 0..10 {
            let angle = game.world.random.next_float() * std::f32::consts::TAU;
            let spot = [
                center[0] + (angle.cos() * 32.0).floor() as i32,
                center[1],
                center[2] + (angle.sin() * 32.0).floor() as i32,
            ];
            if let Some(attempted) = game.find_siege_spawn_pos(&centers, spot) {
                if game.village_siege_cancelled(player.uuid, attempted) {
                    return false;
                }
                state.spot = spot;
                state.next_spawn_time = 0;
                state.zombies_to_spawn = 20;
                break;
            }
        }
        return true;
    }
    false
}

fn spawn_siege_zombie(game: &mut Game<'_>, spot: [i32; 3]) {
    let centers = game.village_centers();
    let Some(at) = game.find_siege_spawn_pos(&centers, spot) else {
        return;
    };
    let Some(id) = game.spawn_event_mob(
        "minecraft:zombie",
        at,
        false,
        horizon_client::mobs::raider::Reason::Event,
    ) else {
        return;
    };
    let yaw = game.world.random.next_float() * 360.0;
    if let Some(m) = game.mobs.iter_mut().find(|m| m.id == id) {
        (m.yaw, m.previous_yaw, m.body_yaw, m.previous_body_yaw) = (yaw, yaw, yaw, yaw);
        (m.head_yaw, m.previous_head_yaw) = (yaw, yaw);
    }
}
