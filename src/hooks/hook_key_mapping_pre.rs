fn hook_key_mapping_pre(
    game: &mut Game<'_>,
    mapping: &str,
    pressed: bool,
    canceled: &mut bool,
) {
    let _ = game;
    if pressed && crate::game::autosave::rreturn::is_scrubbing() {
        // Block gameplay actions that mutate inventory, combat, or containers,
        // but never block navigation, escape, enter, scrub key, or functional keys.
        let block = mapping.starts_with("key.hotbar.")
            || matches!(
                mapping,
                "key.inventory"
                    | "key.drop"
                    | "key.swapOffhand"
                    | "key.attack"
                    | "key.use"
                    | "key.pickItem"
            );
        if block {
            *canceled = true;
        }
    }
}
