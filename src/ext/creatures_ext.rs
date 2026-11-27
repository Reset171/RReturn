use super::*;
use horizon_client::mobs::conversion;

/// Extension helper to run `convert_event` with deterministic creation RNG.
pub trait CreaturesDetExt {
    fn rreturn_det_convert_event(
        &mut self,
        from: &Mob,
        kind: &str,
        params: conversion::Params,
        after: conversion::After,
    ) -> bool;
}

impl CreaturesDetExt for Game<'_> {
    fn rreturn_det_convert_event(
        &mut self,
        from: &Mob,
        kind: &str,
        params: conversion::Params,
        after: conversion::After,
    ) -> bool {
        if !crate::game::autosave::rreturn::det_rng_active() {
            return false;
        }
        let pre_checked = after.pre_checked();
        match self.mobs.iter_mut().find(|m| m.id == from.id) {
            Some(old) => old.removed = false,
            None => {
                let mut old = from.clone();
                old.removed = false;
                self.mobs.push(old);
            }
        }
        let Some(t) = self.res.mob_types().get(kind).cloned() else {
            return true;
        };
        let mut cancel = false;
        let from_kind = (mod_api::__private::listened(mod_api::HookId::LivingConversionPre)
            || mod_api::__private::listened(mod_api::HookId::LivingConversionPost))
        .then(|| crate::game::native_hooks::mob_kind(from).to_owned());
        if !pre_checked
            && let Some(from_kind) = &from_kind
        {
            self.native_living_conversion_pre(from.id, from_kind, kind, &mut cancel);
        }
        if cancel {
            return true;
        }
        let Some(from) = self.mobs.iter().find(|m| m.id == from.id).cloned() else {
            return true;
        };
        let Some((own, uuid, brain_seed)) =
            crate::game::autosave::rreturn::det_living_random_with_seed()
        else {
            return false;
        };
        self.next_entity += 1;
        self.persist.uuids.insert(self.next_entity, uuid);
        let mut mob = Mob::with_random(self.next_entity, t, from.position, from.yaw, own);
        if matches!(mob.species(), Species::Axolotl | Species::Tadpole) {
            horizon_client::mobs::animal_brain::init(&mut mob, brain_seed);
        }
        conversion::single(&from, &mut mob, params);
        self.transfer_conversion_identity(from.id, mob.id);
        if matches!(from.species(), Species::Zombie | Species::Husk) {
            let at = [from.position.x, from.position.y, from.position.z];
            let special = self.local_difficulty(at).special();
            horizon_client::mobs::finalize::zombie_attributes(
                &mut mob,
                special,
                horizon_client::mobs::spawn_rules::Reason::Conversion,
            );
        }
        match (from.species(), kind) {
            (Species::Tadpole, "minecraft:frog") | (Species::Villager, "minecraft:witch") => {
                self.finalize_converted(&mut mob);
            }
            (Species::Pig, "minecraft:zombified_piglin") => {
                if let Some(old) = self.mobs.iter_mut().find(|m| m.id == from.id) {
                    conversion::pig_thunder_hit(old, &mut mob);
                }
            }
            (Species::Hoglin, "minecraft:zoglin")
            | (Species::Piglin | Species::PiglinBrute, "minecraft:zombified_piglin") => {
                mob.add_effect("minecraft:nausea", 200, 0);
            }
            _ => {}
        }
        let id = mob.id;
        self.mobs.push(mob);
        self.seat_mob(id);
        self.copy_conversion_attachments(from.id, id, kind);
        crate::game::autosave::rreturn::mark_det_mob_preseeded(id);
        self.native_mobs_joined(self.mobs.len() - 1, false);
        let transformed_kind = self
            .mobs
            .iter()
            .find(|m| m.id == id)
            .and_then(|transformed| from_kind.as_ref().map(|_| transformed.kind.id.clone()));
        for rider in self.mobs.iter_mut() {
            if rider.extra.mount == Some(from.id) {
                rider.extra.mount = Some(id);
            }
        }
        if let (Some(from_kind), Some(transformed_kind)) = (from_kind, transformed_kind) {
            self.native_living_conversion_post(from.id, &from_kind, id, &transformed_kind);
        }
        if let Some(old) = self.mobs.iter_mut().find(|m| m.id == from.id) {
            old.removed = true;
        }
        true
    }
}

/// Extension helpers for deterministic mob creation through `creatures::Game::spawn_mob*`.
pub trait CreaturesSpawnDetExt {
    fn rreturn_det_spawn_mob(
        &mut self,
        kind: &str,
        at: [f64; 3],
        persistent: bool,
        site: SpawnSite,
        facing: Option<f32>,
    ) -> Option<Option<u32>>;
}

fn det_create_mob_reason(
    game: &mut Game<'_>,
    kind: &str,
    at: [f64; 3],
    persistent: bool,
    site: SpawnSite,
    fixed_yaw: Option<f32>,
) -> Option<Mob> {
    let kind = if kind.contains(':') {
        kind.to_owned()
    } else {
        format!("minecraft:{kind}")
    };
    let t = game.res.mob_types().get(&kind).cloned()?;
    let (own, uuid, brain_seed) = crate::game::autosave::rreturn::det_living_random_with_seed()?;
    game.next_entity += 1;
    let yaw = match (fixed_yaw, site) {
        (Some(yaw), _) => yaw,
        (None, SpawnSite::Finalizing(SpawnReason::Command)) => 0.0,
        (None, SpawnSite::Finalizing(SpawnReason::SpawnItemUse | SpawnReason::Dispenser)) => {
            horizon_client::camera::wrap_degrees(game.world.random.next_float() * 360.0)
        }
        _ => game.world.random.next_float() * 360.0,
    };
    game.persist.uuids.insert(game.next_entity, uuid);
    let mut mob = Mob::with_random(
        game.next_entity,
        t,
        Vec3::new(at[0], at[1], at[2]),
        yaw,
        own,
    );
    if matches!(
        mob.species(),
        horizon_client::mobs::Species::Axolotl | horizon_client::mobs::Species::Tadpole
    ) {
        horizon_client::mobs::animal_brain::init(&mut mob, brain_seed);
    }
    game.initialize_entity(mod_api::Holder::Entity(mob.id), &kind);
    mob.persistent = persistent;
    match site {
        SpawnSite::Raw => return Some(mob),
        SpawnSite::Finalizing(reason) => match game.finalize_with(&mut mob, reason, None) {
            Finalized::Cancelled => return game.discard_unjoined_mob(mob.id),
            Finalized::Skipped => return Some(mob),
            Finalized::Done => horizon_client::mobs::shulker::finalize_spawn(&mut mob),
        },
        SpawnSite::Plain => {
            let difficulty =
                game.local_difficulty([mob.position.x, mob.position.y, mob.position.z]);
            game.finalize_plain(
                &mut mob,
                &difficulty,
                horizon_client::mobs::spawn_rules::Reason::MobSummoned,
            );
            horizon_client::mobs::shulker::finalize_spawn(&mut mob);
        }
    }
    Some(mob)
}

impl CreaturesSpawnDetExt for Game<'_> {
    fn rreturn_det_spawn_mob(
        &mut self,
        kind: &str,
        at: [f64; 3],
        persistent: bool,
        site: SpawnSite,
        facing: Option<f32>,
    ) -> Option<Option<u32>> {
        if !crate::game::autosave::rreturn::det_rng_active() {
            return None;
        }
        let Some(mob) = det_create_mob_reason(self, kind, at, persistent, site, facing) else {
            return Some(None);
        };
        crate::game::autosave::rreturn::mark_det_mob_preseeded(mob.id);
        Some(self.insert_spawned_mob(mob))
    }
}
