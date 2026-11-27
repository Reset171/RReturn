fn hook_det_load_dragon(
    game: &mut Game<'_>,
    tag: &horizon_core::nbt::Tag,
    uuid: Option<[i32; 4]>,
) -> std::ops::ControlFlow<()> {
    use crate::game::persist::rreturn_persist::PersistDetExt;
    if game.rreturn_det_load_dragon(tag, uuid) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
