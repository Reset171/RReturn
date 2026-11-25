fn hook_key_mapping_pre(
    game: &mut Game<'_>,
    mapping: &str,
    pressed: bool,
    canceled: &mut bool,
) {
    let _ = game;
    if pressed && crate::game::autosave::rreturn::is_scrubbing() {
        // Block gameplay actions that mutate inventory, combat, or containers,
        // plus chat/command keys, without blocking navigation, Esc, Enter, or scrub keys.
        let block = mapping.starts_with("key.hotbar.")
            || matches!(
                mapping,
                "key.inventory"
                    | "key.drop"
                    | "key.swapOffhand"
                    | "key.attack"
                    | "key.use"
                    | "key.pickItem"
                    | "key.chat"
                    | "key.command"
                    | "key.advancements"
                    | "key.socialInteractions"
                    | "key.quickActions"
            );
        if block {
            *canceled = true;
        }
    }
}
