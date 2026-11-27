fn hook_det_persist_uuid_for(
    game: &mut Game<'_>,
    id: u32,
) -> std::ops::ControlFlow<[i32; 4]> {
    use crate::game::persist::rreturn_persist::PersistDetExt;
    match game.rreturn_det_uuid_for(id) {
        Some(uuid) => std::ops::ControlFlow::Break(uuid),
        None => std::ops::ControlFlow::Continue(()),
    }
}
