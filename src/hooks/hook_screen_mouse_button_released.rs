fn hook_screen_mouse_button_released(
    game: &Game<'_>,
    screen: &mod_api::ScreenView<'_>,
    button: i32,
    modifiers: u32,
    canceled: &mut bool,
) {
    let _ = (screen, modifiers);
    crate::game::autosave::rreturn::handle_screen_mouse_released(game, button, canceled);
}
