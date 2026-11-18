fn hook_server_started(game: &mut Game<'_>) {
    crate::game::autosave::rreturn::handle_server_started(game);
}
