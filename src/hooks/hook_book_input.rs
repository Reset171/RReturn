fn hook_book_input(game: &Game<'_>) -> std::ops::ControlFlow<()> {
    if crate::game::autosave::rreturn::should_block_text_input_during_scrub(game) {
        return std::ops::ControlFlow::Break(());
    }
    std::ops::ControlFlow::Continue(())
}
