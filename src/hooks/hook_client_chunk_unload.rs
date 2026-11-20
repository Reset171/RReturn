fn hook_client_chunk_unload(game: &mut Game<'_>, dimension: &str, chunk: [i32; 2]) {
    crate::game::autosave::rreturn::handle_client_chunk_unload(game, dimension, chunk);
}