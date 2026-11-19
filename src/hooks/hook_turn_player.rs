fn hook_turn_player(
    game: &mut Game<'_>,
    xo: f64,
    yo: f64,
) -> std::ops::ControlFlow<()> {
    let _ = (game, xo, yo);
    if crate::game::autosave::rreturn::is_scrubbing() {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
