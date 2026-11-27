fn hook_det_trader_tick(game: &Game<'_>) {
    use crate::game::wandering_trader::rreturn_trader::WanderingTraderExt;
    if game.rreturn_should_tick_trader() {
        crate::game::autosave::rreturn::step_trader_tick_delay_shadow();
    }
}
