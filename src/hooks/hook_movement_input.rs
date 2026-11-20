fn hook_movement_input(
    game: &Game<'_>,
) -> std::ops::ControlFlow<horizon_client::sprint::Keys> {
    let _ = game;
    if crate::game::autosave::rreturn::is_scrubbing() {
        return std::ops::ControlFlow::Break(horizon_client::sprint::Keys::default());
    }
    std::ops::ControlFlow::Continue(())
}