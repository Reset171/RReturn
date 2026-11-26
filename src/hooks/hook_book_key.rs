fn hook_book_key(
    game: &mut Game<'_>,
    event: &winit::event::KeyEvent,
) -> std::ops::ControlFlow<()> {
    let pressed = matches!(event.state, winit::event::ElementState::Pressed);
    let released = matches!(event.state, winit::event::ElementState::Released);
    let key_name = match event.physical_key {
        winit::keyboard::PhysicalKey::Code(code) => horizon_client::keybinds::key_name(code),
        _ => None,
    };
    if crate::game::autosave::rreturn::handle_overlay_key_during_scrub(
        game,
        key_name,
        pressed,
        released,
        event.repeat,
    ) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
