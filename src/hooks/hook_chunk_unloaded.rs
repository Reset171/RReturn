fn hook_chunk_unloaded(game: &mut Game<'_>, dimension: &str, chunk: [i32; 2]) {
    crate::game::autosave::rreturn::handle_server_chunk_unload(game, dimension, chunk);
}