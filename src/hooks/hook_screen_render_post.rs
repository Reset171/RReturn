fn hook_screen_render_post(
    game: &Game<'_>,
    draw: &mut horizon_client::hud::HudBatch,
    screen: &mod_api::ScreenView<'_>,
) {
    if screen.id == "mod" {
        crate::game::autosave::rreturn::render_settings_screen(game, draw);
    }
    crate::game::autosave::rreturn::handle_screen_render_post(game, draw, screen.id);
}
