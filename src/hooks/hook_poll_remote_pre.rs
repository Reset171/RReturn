fn hook_poll_remote_pre() -> std::ops::ControlFlow<Result<()>> {
    if crate::game::autosave::rreturn::is_scrubbing() {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    std::ops::ControlFlow::Continue(())
}
