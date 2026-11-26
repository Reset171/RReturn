fn hook_hotbar_takes_wheel(game: &Game<'_>) -> std::ops::ControlFlow<bool> {
    if crate::game::autosave::rreturn::should_override_wheel_for_scrub(game) {
        return std::ops::ControlFlow::Break(true);
    }
    std::ops::ControlFlow::Continue(())
}
