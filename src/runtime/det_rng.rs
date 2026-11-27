use super::super::{Game, Mob};
use super::VillageSiegeSnapshot;
use horizon_client::{
    mobs::{animal_brain, armadillo, bee, warden, Species},
    save::insecure_uuid,
};
use horizon_core::random::{EntityRandom, LegacyRandom, RandomSource};
use std::cell::{Cell, RefCell};

const DET_UNIQUIFIER_START: i64 = 8_682_522_807_148_012;
const DET_UNIQUIFIER_STEP: i64 = 1_181_783_497_276_652_981;

#[derive(Clone, Copy)]
struct DetLoadMobMark {
    prev_next_entity: u32,
    uuid_missing: bool,
    armadillo_has_brain: bool,
    zv_redraw_prof: bool,
}

std::thread_local! {
    static DET_RNG_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static DET_RNG_UNIQUIFIER: Cell<i64> = const { Cell::new(0) };
    static DET_RNG_STEPS: Cell<u64> = const { Cell::new(0) };
    static DET_MOB_MARK_STACK: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    static DET_LOAD_MOB_STACK: RefCell<Vec<DetLoadMobMark>> = const { RefCell::new(Vec::new()) };
    static DET_TRADER_TICK_DELAY: Cell<i32> = const { Cell::new(1200) };
    static DET_VILLAGE_SIEGE: Cell<VillageSiegeSnapshot> = const {
        Cell::new(VillageSiegeSnapshot {
            tonight: false,
            set_up: false,
            zombies_to_spawn: 0,
            next_spawn_time: 0,
            spot: [0; 3],
        })
    };
}

pub fn det_rng_active() -> bool {
    DET_RNG_ACTIVE.with(|c| c.get())
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

pub fn det_rng_step_count() -> u64 {
    DET_RNG_STEPS.with(|c| c.get())
}

/// Clears transient per-run entity caches/stacks so rolled-back runs never inherit stale IDs.
pub fn clear_det_rng_transient_caches() {
    DET_MOB_MARK_STACK.with(|s| s.borrow_mut().clear());
    DET_LOAD_MOB_STACK.with(|s| s.borrow_mut().clear());
    DET_PRESEEDED_MOBS.with(|s| s.borrow_mut().clear());
    DET_UUID_ASSIGNED.with(|s| s.borrow_mut().clear());
    DET_PENDING_CREAKING_IDS.with(|s| s.borrow_mut().clear());
}

pub fn restore_det_rng_raw(active: bool, u: i64) {
    DET_RNG_ACTIVE.with(|c| c.set(active));
    DET_RNG_UNIQUIFIER.with(|c| c.set(u));
    clear_det_rng_transient_caches();
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
    clear_det_rng_transient_caches();
    DET_TRADER_TICK_DELAY.with(|c| c.set(1200));
    DET_VILLAGE_SIEGE.with(|c| c.set(VillageSiegeSnapshot::default()));
}

pub fn current_trader_tick_delay() -> i32 {
    DET_TRADER_TICK_DELAY.with(|c| c.get())
}

pub fn set_trader_tick_delay(delay: i32) {
    DET_TRADER_TICK_DELAY.with(|c| c.set(delay.clamp(1, 1200)));
}

pub fn step_trader_tick_delay_shadow() {
    DET_TRADER_TICK_DELAY.with(|c| {
        let cur = c.get();
        let next = if cur <= 1 { 1200 } else { cur - 1 };
        c.set(next);
    });
}

pub fn current_village_siege() -> VillageSiegeSnapshot {
    DET_VILLAGE_SIEGE.with(|c| c.get())
}

pub fn set_village_siege(siege: VillageSiegeSnapshot) {
    DET_VILLAGE_SIEGE.with(|c| c.set(siege));
}

pub fn det_tick_village_siege_if_active(game: &mut Game<'_>) -> bool {
    if !det_rng_active() {
        return false;
    }
    let mut siege = current_village_siege();
    game.rreturn_tick_village_siege(&mut siege);
    set_village_siege(siege);
    true
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
    clear_det_rng_transient_caches();
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
        DET_RNG_STEPS.with(|s| s.set(s.get().wrapping_add(1)));
        next
    }))
}

pub fn det_unique_seed() -> Option<i64> {
    step_det_rng_uniquifier()
}

pub fn det_mob_brain_seed(fallback_id: u32) -> i64 {
    step_det_rng_uniquifier().unwrap_or_else(|| i64::from(fallback_id))
}

pub fn det_uuid() -> Option<[i32; 4]> {
    let seed = step_det_rng_uniquifier()?;
    let mut random = LegacyRandom::new(seed);
    Some(insecure_uuid(&mut random))
}

pub fn det_entity_random_state() -> Option<EntityRandom> {
    let seed = step_det_rng_uniquifier()?;
    Some(EntityRandom::own(LegacyRandom::new(seed)))
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

pub fn det_living_random_with_seed() -> Option<(EntityRandom, [i32; 4], i64)> {
    let seed = step_det_rng_uniquifier()?;
    let mut random = LegacyRandom::new(seed);
    let uuid = insecure_uuid(&mut random);
    Some((EntityRandom::own(random), uuid, seed))
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

thread_local! {
    static DET_PRESEEDED_MOBS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    static DET_UUID_ASSIGNED: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    static DET_PENDING_CREAKING_IDS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

/// Records a newly created Creaking mob ID so its heart link receives a deterministic UUID.
pub fn mark_det_creaking_mob(id: u32) {
    DET_PENDING_CREAKING_IDS.with(|s| s.borrow_mut().push(id));
}

/// Replaces non-deterministic `HeartHost::uuid_for` UUIDs on newly spawned Creakings.
pub fn flush_pending_creaking_uuids(game: &mut Game<'_>) {
    let pending = DET_PENDING_CREAKING_IDS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    if pending.is_empty() || !det_rng_active() {
        return;
    }
    for mob_id in pending {
        let Some(uuid) = det_uuid() else {
            break;
        };
        game.persist.uuids.insert(mob_id, uuid);
        mark_det_mob_uuid_assigned(mob_id);
        let heart_key = game.world.block_entities.iter().find_map(|(&pos, be)| {
            matches!(
                be,
                horizon_client::containers::BlockEntity::CreakingHeart(h)
                    if h.creaking.is_some_and(|(id, _)| id == Some(mob_id))
            )
            .then_some(pos)
        });
        if let Some(pos) = heart_key
            && let Some(horizon_client::containers::BlockEntity::CreakingHeart(h)) =
                game.world.block_entities.get_mut(&pos)
        {
            h.creaking = Some((Some(mob_id), uuid));
        }
    }
}

pub fn mark_det_mob_preseeded(id: u32) {
    DET_PRESEEDED_MOBS.with(|s| {
        let mut v = s.borrow_mut();
        if v.len() >= 64 {
            v.drain(..32);
        }
        if !v.contains(&id) {
            v.push(id);
        }
    });
}

/// Captures pre-load NBT flags before `load_mob_transferred` moves `tag`.
pub fn push_det_load_mob_mark(game: &Game<'_>, tag: &horizon_core::nbt::Tag) {
    use horizon_core::nbt::Tag;
    let uuid_missing = horizon_client::entity_nbt::uuid(tag).is_none();
    let armadillo_has_brain = tag.get("Brain").is_some();
    let zv_finalized = matches!(tag.get("VillagerDataFinalized"), Some(Tag::Byte(v)) if *v != 0)
        || matches!(tag.get("VillagerDataFinalized"), Some(Tag::Int(v)) if *v != 0);
    let zv_redraw_prof = zv_finalized && tag.get("VillagerData").is_none();
    DET_LOAD_MOB_STACK.with(|s| {
        s.borrow_mut().push(DetLoadMobMark {
            prev_next_entity: game.next_entity,
            uuid_missing,
            armadillo_has_brain,
            zv_redraw_prof,
        });
    });
}

/// Reseeds any newly constructed mob from `load_mob_transferred` deterministically.
pub fn pop_det_load_mob_and_reseed(res: Option<u32>, game: &mut Game<'_>) -> Option<u32> {
    let Some(pre) = DET_LOAD_MOB_STACK.with(|s| s.borrow_mut().pop()) else {
        return res;
    };
    let Some(id) = res else {
        return res;
    };
    if id != pre.prev_next_entity.wrapping_add(1) || !det_rng_active() {
        return res;
    }
    let Some(seed) = step_det_rng_uniquifier() else {
        return res;
    };
    let mut random = LegacyRandom::new(seed);
    let uuid = insecure_uuid(&mut random);
    if let Some(mob) = game.mob_object_mut(id) {
        reset_mob_rng_with_constructor_draws(mob, random);
        if pre.zv_redraw_prof && mob.species() == Species::ZombieVillager {
            mob.extra.profession = horizon_client::mobs::villager::ProfessionState::from_index(
                horizon_client::mobs::zombie_villager::constructor_profession(mob),
            );
        }
        if pre.armadillo_has_brain && mob.species() == Species::Armadillo {
            armadillo::delay_sensors(mob);
        }
    }
    if pre.uuid_missing {
        game.persist.uuids.insert(id, uuid);
        mark_det_mob_uuid_assigned(id);
    }
    mark_det_mob_preseeded(id);
    res
}

fn is_det_mob_preseeded(id: u32) -> bool {
    DET_PRESEEDED_MOBS.with(|s| s.borrow().contains(&id))
}

pub fn mark_det_mob_uuid_assigned(id: u32) {
    DET_UUID_ASSIGNED.with(|s| {
        let mut v = s.borrow_mut();
        if v.len() >= 64 {
            v.drain(..32);
        }
        if !v.contains(&id) {
            v.push(id);
        }
    });
}

fn is_det_mob_uuid_assigned(id: u32) -> bool {
    DET_UUID_ASSIGNED.with(|s| s.borrow().contains(&id))
}

/// Applies deterministic `EntityRandom`, species brain seed, and returns deterministic UUID.
pub fn det_seed_new_mob(mob: &mut Mob) -> Option<[i32; 4]> {
    let seed = step_det_rng_uniquifier()?;
    let mut random = LegacyRandom::new(seed);
    let uuid = insecure_uuid(&mut random);
    reset_mob_rng_with_constructor_draws(mob, random);
    if matches!(mob.species(), Species::Axolotl | Species::Tadpole) {
        animal_brain::init(mob, seed);
    }
    Some(uuid)
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
        if is_det_mob_preseeded(mob.id) {
            continue;
        }
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
        if is_det_mob_preseeded(mob.id) {
            continue;
        }
        let Some(uuid) = det_seed_new_mob(mob) else {
            break;
        };
        mark_det_mob_preseeded(mob.id);
        uuid_updates.push((mob.id, uuid));
    }
    for (mob_id, uuid) in uuid_updates {
        if !is_det_mob_uuid_assigned(mob_id) {
            game.persist.uuids.insert(mob_id, uuid);
        }
    }
}
