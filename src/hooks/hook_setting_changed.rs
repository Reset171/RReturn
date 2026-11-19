fn hook_setting_changed(game: &mut Game<'_>, mod_id: &str, key: &str) {
    let _ = key;
    if mod_id == crate::game::autosave::rreturn::MOD_ID {
        crate::game::autosave::rreturn::load_saved_settings(game);
    }
}
