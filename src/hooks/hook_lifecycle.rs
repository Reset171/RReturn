fn hook_lifecycle(game: &mut Game<'_>, phase: WorldPhase) {
    crate::game::levels::rreturn::handle_lifecycle(game, phase);
}