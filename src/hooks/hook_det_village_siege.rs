fn hook_det_village_siege(game: &mut Game<'_>) -> std::ops::ControlFlow<()> {
    if crate::game::autosave::rreturn::det_tick_village_siege_if_active(game) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
