fn hook_mode_changed_pre(_game: &mut Game<'_>) -> std::ops::ControlFlow<()> {
    if crate::game::autosave::rreturn::should_suppress_mode_inventory_reset() {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
