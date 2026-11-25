fn hook_open_chat(
    game: &mut Game<'_>,
    prefix: &str,
) -> std::ops::ControlFlow<()> {
    let _ = (game, prefix);
    if crate::game::autosave::rreturn::is_scrubbing() {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
