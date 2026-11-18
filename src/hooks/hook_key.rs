fn hook_key(game: &mut Game<'_>, key: &str, action: mod_api::KeyAction) {
    let pressed = matches!(action, mod_api::KeyAction::Press);
    crate::game::autosave::rreturn::handle_key(game, key, pressed);
}
