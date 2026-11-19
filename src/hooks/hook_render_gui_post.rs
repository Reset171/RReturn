fn hook_render_gui_post(
    game: &Game<'_>,
    draw: &mut horizon_client::hud::HudBatch,
) {
    crate::game::autosave::rreturn::handle_render_gui_post(game, draw);
}
