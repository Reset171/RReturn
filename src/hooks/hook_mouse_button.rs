fn hook_mouse_button(
    game: &mut Game<'_>,
    button: i32,
    pressed: bool,
    modifiers: u32,
    canceled: &mut bool,
) {
    let _ = (game, button, pressed, modifiers);
    if crate::game::autosave::rreturn::is_scrubbing() {
        *canceled = true;
    }
}
