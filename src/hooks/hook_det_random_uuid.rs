fn hook_det_random_uuid(op: impl FnOnce() -> [i32; 4]) -> [i32; 4] {
    match crate::game::autosave::rreturn::det_uuid() {
        Some(uuid) => uuid,
        None => op(),
    }
}
