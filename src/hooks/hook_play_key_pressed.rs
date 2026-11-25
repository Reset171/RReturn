fn hook_play_key_pressed(
    game: &mut Game<'_>,
    key: winit::keyboard::KeyCode,
    raw: winit::keyboard::KeyCode,
) -> std::ops::ControlFlow<Result<()>> {
    use winit::keyboard::KeyCode;
    let _ = raw;
    if crate::game::autosave::rreturn::is_scrubbing()
        && matches!(
            key,
            KeyCode::Escape
                | KeyCode::Enter
                | KeyCode::NumpadEnter
                | KeyCode::KeyT
                | KeyCode::Slash
                | KeyCode::KeyE
                | KeyCode::KeyL
                | KeyCode::KeyP
                | KeyCode::KeyG
                | KeyCode::KeyB
                | KeyCode::KeyA
                | KeyCode::KeyD
                | KeyCode::ArrowLeft
                | KeyCode::ArrowRight
                | KeyCode::F1
                | KeyCode::F5
        )
    {
        return std::ops::ControlFlow::Break(Ok(()));
    }
    let _ = game;
    std::ops::ControlFlow::Continue(())
}
