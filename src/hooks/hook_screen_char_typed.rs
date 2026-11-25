fn hook_screen_char_typed(
    game: &Game<'_>,
    screen: &mod_api::ScreenView<'_>,
    c: char,
    modifiers: u32,
    canceled: &mut bool,
) {
    crate::game::autosave::rreturn::handle_screen_char_typed_dispatch(
        game,
        screen.id,
        c,
        modifiers,
        canceled,
    );
}
