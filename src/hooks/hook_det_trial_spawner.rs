fn hook_det_trial_spawner(
    game: &mut Game<'_>,
    key: (i32, i32, i32),
) -> std::ops::ControlFlow<Result<()>> {
    use crate::game::trial_spawner::rreturn_trial_spawner::TrialSpawnerDetExt;
    match game.rreturn_det_tick_trial_spawner(key) {
        Some(res) => std::ops::ControlFlow::Break(res),
        None => std::ops::ControlFlow::Continue(()),
    }
}
