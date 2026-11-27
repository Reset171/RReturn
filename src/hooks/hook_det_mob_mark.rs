fn hook_det_mob_mark(game: &Game<'_>) {
    crate::game::autosave::rreturn::push_det_rng_mob_mark(game.mobs.len());
}
