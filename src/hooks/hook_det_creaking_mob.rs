fn hook_det_creaking_mob(
    id: u32,
    kind: std::sync::Arc<horizon_client::mobs::MobType>,
    pos: horizon_core::geometry::Vec3,
    yaw: f32,
    op: impl FnOnce(u32, std::sync::Arc<horizon_client::mobs::MobType>, horizon_core::geometry::Vec3, f32) -> horizon_client::mobs::Mob,
) -> horizon_client::mobs::Mob {
    match crate::game::autosave::rreturn::det_entity_random_state() {
        Some(own) => {
            crate::game::autosave::rreturn::mark_det_creaking_mob(id);
            horizon_client::mobs::Mob::with_random(id, kind, pos, yaw, own)
        }
        None => op(id, kind, pos, yaw),
    }
}
