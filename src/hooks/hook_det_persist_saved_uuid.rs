fn hook_det_persist_saved_uuid(
    game: &Game<'_>,
    id: u32,
) -> std::ops::ControlFlow<[i32; 4]> {
    use crate::game::persist::rreturn_persist::PersistDetExt;
    match game.rreturn_det_saved_uuid_or_random(id) {
        Some(uuid) => std::ops::ControlFlow::Break(uuid),
        None => std::ops::ControlFlow::Continue(()),
    }
}
