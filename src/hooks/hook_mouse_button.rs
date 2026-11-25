fn hook_mouse_button(
    game: &mut Game<'_>,
    button: i32,
    pressed: bool,
    modifiers: u32,
    canceled: &mut bool,
) {
    let _ = modifiers;
    if crate::game::autosave::rreturn::handle_play_mouse_button(game, button, pressed) {
        *canceled = true;
    }
}
