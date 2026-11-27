fn hook_det_load_mob_pre(game: &Game<'_>, tag: &horizon_core::nbt::Tag) {
    crate::game::autosave::rreturn::push_det_load_mob_mark(game, tag);
}
