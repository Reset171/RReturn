fn hook_screen_mouse_button_pressed(
    game: &Game<'_>,
    screen: &mod_api::ScreenView<'_>,
    button: i32,
    modifiers: u32,
    canceled: &mut bool,
) {
    let _ = modifiers;
    crate::game::autosave::rreturn::handle_screen_mouse_pressed(
        game,
        button,
        screen.mouse[0],
        screen.mouse[1],
        canceled,
    );
}
