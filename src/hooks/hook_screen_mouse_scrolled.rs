fn hook_screen_mouse_scrolled(
    game: &Game<'_>,
    screen: &mod_api::ScreenView<'_>,
    scroll: [f64; 2],
    modifiers: u32,
    canceled: &mut bool,
) {
    let _ = (screen, modifiers);
    crate::game::autosave::rreturn::handle_screen_mouse_scrolled(game, scroll[1], canceled);
}
