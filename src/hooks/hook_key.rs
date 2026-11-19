fn hook_key(game: &mut Game<'_>, key: &str, action: mod_api::KeyAction) {
    crate::game::autosave::rreturn::handle_key(game, key, action);
}
