fn hook_settings_click(game: &mut Game<'_>, mod_id: &str, button: usize) {
    crate::game::levels::rreturn::handle_settings_click(game, mod_id, button);
}