fn hook_det_living_rng(
    op: impl FnOnce() -> (horizon_core::random::EntityRandom, [i32; 4]),
) -> (horizon_core::random::EntityRandom, [i32; 4]) {
    match crate::game::autosave::rreturn::step_det_rng_uniquifier() {
        Some(seed) => {
            let mut random = horizon_core::random::LegacyRandom::new(seed);
            let uuid = horizon_client::save::insecure_uuid(&mut random);
            (horizon_core::random::EntityRandom::own(random), uuid)
        }
        None => op(),
    }
}
