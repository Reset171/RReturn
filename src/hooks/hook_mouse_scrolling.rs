fn hook_mouse_scrolling(
    game: &mut Game<'_>,
    scroll: [f64; 2],
    accumulated: [i32; 2],
    canceled: &mut bool,
) {
    if crate::game::autosave::rreturn::handle_mouse_scrolling(game, scroll, accumulated) {
        *canceled = true;
    }
}
