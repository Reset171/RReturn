fn hook_key_mapping_pre(
    game: &mut Game<'_>,
    mapping: &str,
    pressed: bool,
    canceled: &mut bool,
) {
    let _ = (game, mapping);
    if pressed && crate::game::autosave::rreturn::is_scrubbing() {
        *canceled = true;
    }
}
