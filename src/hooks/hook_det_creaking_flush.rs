fn hook_det_creaking_flush(game: &mut Game<'_>) {
    crate::game::autosave::rreturn::flush_pending_creaking_uuids(game);
}
