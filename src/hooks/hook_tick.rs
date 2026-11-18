fn hook_tick(game: &mut Game<'_>) {
    crate::game::autosave::rreturn::handle_tick(game);
}
