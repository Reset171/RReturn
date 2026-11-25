fn hook_screen_key_pressed(
    game: &Game<'_>,
    screen: &mod_api::ScreenView<'_>,
    key: &str,
    repeat: bool,
    modifiers: u32,
    canceled: &mut bool,
) {
    let _ = screen;
    crate::game::autosave::rreturn::handle_screen_key_pressed(
        game,
        key,
        repeat,
        modifiers,
        canceled,
    );
}
