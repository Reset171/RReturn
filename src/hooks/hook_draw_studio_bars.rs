fn hook_draw_studio_bars(
    game: &Game<'_>,
    draw: &mut horizon_client::hud::HudBatch,
) {
    crate::game::autosave::rreturn::handle_draw_overlay_hud_post(game, draw);
}
