fn hook_loading_draw_head(loading: &mut Loading) -> std::ops::ControlFlow<()> {
    crate::game::startup_menu::rreturn_startup_menu::rreturn_draw_loading_frame(loading)
}
