fn hook_det_unique_seed(op: impl FnOnce() -> i64) -> i64 {
    match crate::game::autosave::rreturn::det_unique_seed() {
        Some(seed) => seed,
        None => op(),
    }
}
