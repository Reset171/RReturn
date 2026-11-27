fn hook_det_load_mob_post(res: Option<u32>, game: &mut Game<'_>) -> Option<u32> {
    crate::game::autosave::rreturn::pop_det_load_mob_and_reseed(res, game)
}
