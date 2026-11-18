fn hook_lifecycle(game: &mut Game<'_>, phase: mod_api::WorldPhase) {
    crate::game::autosave::rreturn::handle_lifecycle(game, phase);
}
