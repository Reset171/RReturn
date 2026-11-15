fn hook_key(game: &mut Game<'_>, key: &str, pressed: bool) {
    crate::game::levels::rreturn::handle_key(game, key, pressed);
}