fn hook_det_convert_event(
    game: &mut Game<'_>,
    from: &horizon_client::mobs::Mob,
    kind: &str,
    params: horizon_client::mobs::conversion::Params,
    after: horizon_client::mobs::conversion::After,
) -> std::ops::ControlFlow<()> {
    use crate::game::creatures::rreturn_creatures::CreaturesDetExt;
    if game.rreturn_det_convert_event(from, kind, params, after) {
        std::ops::ControlFlow::Break(())
    } else {
        std::ops::ControlFlow::Continue(())
    }
}
