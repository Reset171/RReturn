fn hook_book_click(
    game: &mut Game<'_>,
    size: [u32; 2],
) -> std::ops::ControlFlow<()> {
    let _ = size;
    if crate::game::autosave::rreturn::handle_overlay_click_during_scrub(game) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
