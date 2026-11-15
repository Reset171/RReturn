fn hook_mods(mods: &mut Vec<NativeMod>) {
    crate::game::levels::rreturn::register_native_mod(mods);
}