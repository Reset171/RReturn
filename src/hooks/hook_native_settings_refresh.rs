fn hook_native_settings_refresh(game: &mut Game<'_>, i: usize) -> std::ops::ControlFlow<()> {
    crate::game::startup_menu::rreturn_startup_menu::rreturn_intercept_game_native_settings(game, i)
}
