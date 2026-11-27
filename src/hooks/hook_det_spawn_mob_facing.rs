fn hook_det_spawn_mob_facing(
    game: &mut Game<'_>,
    kind: &str,
    at: [f64; 3],
    persistent: bool,
    site: crate::game::creatures::SpawnSite,
    yaw: f32,
) -> std::ops::ControlFlow<Option<u32>> {
    use crate::game::creatures::rreturn_creatures::CreaturesSpawnDetExt;
    match game.rreturn_det_spawn_mob(kind, at, persistent, site, Some(yaw)) {
        Some(res) => std::ops::ControlFlow::Break(res),
        None => std::ops::ControlFlow::Continue(()),
    }
}
