fn hook_det_entity_rng(
    op: impl FnOnce() -> horizon_core::random::LegacyRandom,
) -> horizon_core::random::LegacyRandom {
    match crate::game::autosave::rreturn::step_det_rng_uniquifier() {
        Some(seed) => horizon_core::random::LegacyRandom::new(seed),
        None => op(),
    }
}
