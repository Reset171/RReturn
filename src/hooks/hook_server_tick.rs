fn hook_server_tick(game: &mut Game<'_>, phase: mod_api::ServerTickPhase) {
    if let mod_api::ServerTickPhase::ServerTickEnd { world_running } = phase {
        crate::game::autosave::rreturn::handle_server_tick(game, world_running);
    }
}