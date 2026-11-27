fn hook_det_mob_new(
    id: u32,
    kind: std::sync::Arc<horizon_client::mobs::MobType>,
    pos: horizon_core::geometry::Vec3,
    yaw: f32,
    op: impl FnOnce(u32, std::sync::Arc<horizon_client::mobs::MobType>, horizon_core::geometry::Vec3, f32) -> horizon_client::mobs::Mob,
) -> horizon_client::mobs::Mob {
    match crate::game::autosave::rreturn::det_entity_random_state() {
        Some(own) => {
            let mut mob = horizon_client::mobs::Mob::with_random(id, kind, pos, yaw, own);
            if matches!(
                mob.species(),
                horizon_client::mobs::Species::Axolotl | horizon_client::mobs::Species::Tadpole
            ) {
                let seed = crate::game::autosave::rreturn::det_mob_brain_seed(id);
                horizon_client::mobs::animal_brain::init(&mut mob, seed);
            }
            mob
        }
        None => op(id, kind, pos, yaw),
    }
}
