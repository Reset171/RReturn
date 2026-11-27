fn hook_det_mobs_joined(game: &mut Game<'_>, from: usize) {
    crate::game::autosave::rreturn::reseed_joined_mobs_det_rng(game, from);
}
