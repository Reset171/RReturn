fn hook_loading_menu_event(
    loading: &mut Loading,
    id: WindowId,
    event: &WindowEvent,
) -> std::ops::ControlFlow<()> {
    crate::game::startup_menu::rreturn_startup_menu::rreturn_intercept_loading_menu_event(
        loading,
        id,
        event,
    )
}
