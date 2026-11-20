fn hook_movement_input(
    game: &Game<'_>,
    mut call: mod_api::__private::MovementInputCall<'_>,
) {
    let _ = game;
    if crate::game::autosave::rreturn::is_scrubbing() {
        *call.forward = 0.0;
        *call.strafe = 0.0;
        *call.jump = false;
        *call.sneak = false;
        *call.sprint = false;
        *call.forward_key = false;
        *call.backward_key = false;
        *call.left_key = false;
        *call.right_key = false;
    }
}
