use super::super::{Game, Mob};
use horizon_client::{
    mobs::{armadillo, bee, warden, Species},
    save::insecure_uuid,
};
use horizon_core::random::{EntityRandom, LegacyRandom, RandomSource};
use std::cell::{Cell, RefCell};

const DET_UNIQUIFIER_START: i64 = 8_682_522_807_148_012;
const DET_UNIQUIFIER_STEP: i64 = 1_181_783_497_276_652_981;

std::thread_local! {
    static DET_RNG_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static DET_RNG_UNIQUIFIER: Cell<i64> = const { Cell::new(0) };
    static DET_MOB_MARK_STACK: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

pub fn mark_det_rng_server_thread(active: bool) {
    DET_RNG_ACTIVE.with(|c| c.set(active));
}

pub fn det_rng_raw_state() -> (bool, i64) {
    (
        DET_RNG_ACTIVE.with(|c| c.get()),
        DET_RNG_UNIQUIFIER.with(|c| c.get()),
    )
}

pub fn restore_det_rng_raw(active: bool, u: i64) {
    DET_RNG_ACTIVE.with(|c| c.set(active));
    DET_RNG_UNIQUIFIER.with(|c| c.set(u));
    DET_MOB_MARK_STACK.with(|s| s.borrow_mut().clear());
}

fn init_nonzero_uniquifier(seed: i64) -> i64 {
    let mixed = horizon_client::random_source::unique_seed()
        ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64);
    if mixed == 0 {
        DET_UNIQUIFIER_START
    } else {
        mixed
    }
}

pub fn reset_det_rng_for_world(seed: i64) {
    DET_RNG_ACTIVE.with(|c| c.set(true));
    DET_RNG_UNIQUIFIER.with(|c| c.set(init_nonzero_uniquifier(seed)));
    DET_MOB_MARK_STACK.with(|s| s.borrow_mut().clear());
}

pub fn current_det_rng_uniquifier() -> i64 {
    DET_RNG_ACTIVE.with(|c| c.set(true));
    DET_RNG_UNIQUIFIER.with(|c| {
        let u = c.get();
        if u != 0 {
            u
        } else {
            let init = init_nonzero_uniquifier(0);
            c.set(init);
            init
        }
    })
}

pub fn set_det_rng_uniquifier(u: i64) {
    if u != 0 {
        DET_RNG_ACTIVE.with(|c| c.set(true));
        DET_RNG_UNIQUIFIER.with(|c| c.set(u));
    }
}

pub fn step_det_rng_uniquifier() -> Option<i64> {
    if !DET_RNG_ACTIVE.with(|c| c.get()) {
        return None;
    }
    Some(DET_RNG_UNIQUIFIER.with(|c| {
        let mut u = c.get();
        if u == 0 {
            u = init_nonzero_uniquifier(0);
        }
        let mut next = u.wrapping_mul(DET_UNIQUIFIER_STEP);
        if next == 0 {
            next = DET_UNIQUIFIER_START;
        }
        c.set(next);
        next
    }))
}

pub fn det_entity_random_or(orig: LegacyRandom) -> LegacyRandom {
    match step_det_rng_uniquifier() {
        Some(seed) => LegacyRandom::new(seed),
        None => orig,
    }
}

pub fn det_living_random_or(orig: (EntityRandom, [i32; 4])) -> (EntityRandom, [i32; 4]) {
    match step_det_rng_uniquifier() {
        Some(seed) => {
            let mut random = LegacyRandom::new(seed);
            let uuid = insecure_uuid(&mut random);
            (EntityRandom::own(random), uuid)
        }
        None => orig,
    }
}

fn reset_mob_rng_with_constructor_draws(mob: &mut Mob, mut random: LegacyRandom) {
    if mob.species() == Species::ZombieVillager {
        let _ = random.next_int_bound(15);
    }
    let _ = random.next_float();
    mob.random = EntityRandom::own(random);
    match mob.species() {
        Species::Bee => bee::draw_cooldowns(mob),
        Species::Armadillo => armadillo::delay_sensors(mob),
        Species::Warden => warden::delay_sensors(mob),
        _ => {}
    }
}

/// Records the current `game.mobs` length before a batch load/release call.
pub fn push_det_rng_mob_mark(from: usize) {
    DET_MOB_MARK_STACK.with(|s| s.borrow_mut().push(from));
}

/// Deterministically reseeds all newly appended mobs since the matching mark.
pub fn pop_det_rng_mob_mark_and_reseed(game: &mut Game<'_>) {
    let Some(from) = DET_MOB_MARK_STACK.with(|s| s.borrow_mut().pop()) else {
        return;
    };
    if !DET_RNG_ACTIVE.with(|c| c.get()) {
        return;
    }
    for mob in game.mobs.iter_mut().skip(from) {
        let Some(seed) = step_det_rng_uniquifier() else {
            break;
        };
        let mut random = LegacyRandom::new(seed);
        let _ = insecure_uuid(&mut random);
        reset_mob_rng_with_constructor_draws(mob, random);
    }
}

pub fn reseed_joined_mobs_det_rng(game: &mut Game<'_>, from: usize) {
    if !DET_RNG_ACTIVE.with(|c| c.get()) {
        return;
    }
    let mut uuid_updates: Vec<(u32, [i32; 4])> = Vec::new();
    for mob in game.mobs.iter_mut().skip(from) {
        let Some(seed) = step_det_rng_uniquifier() else {
            break;
        };
        let mut random = LegacyRandom::new(seed);
        let uuid = insecure_uuid(&mut random);
        reset_mob_rng_with_constructor_draws(mob, random);
        uuid_updates.push((mob.id, uuid));
    }
    for (mob_id, uuid) in uuid_updates {
        game.persist.uuids.insert(mob_id, uuid);
    }
}
