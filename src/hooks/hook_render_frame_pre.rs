fn hook_render_frame_pre(
    game: &mut Game<'_>,
    world_partial: f32,
    partial: f32,
) {
    crate::game::autosave::rreturn::handle_frame_render_pre(game, world_partial, partial);
}
