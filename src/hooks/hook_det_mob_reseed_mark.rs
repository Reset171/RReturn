fn hook_det_mob_reseed_mark(_val: (), game: &mut Game<'_>) -> () {
    crate::game::autosave::rreturn::pop_det_rng_mob_mark_and_reseed(game);
}
