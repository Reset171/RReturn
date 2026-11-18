fn hook_guest_die(
    game: &mut Game<'_>,
    gi: usize,
    conn: u32,
) -> std::ops::ControlFlow<bool> {
    match crate::game::autosave::rreturn::intercept_guest_die(game, gi, conn) {
        std::ops::ControlFlow::Break(()) => std::ops::ControlFlow::Break(true),
        std::ops::ControlFlow::Continue(()) => std::ops::ControlFlow::Continue(()),
    }
}
