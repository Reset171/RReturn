use super::*;
use std::ops::ControlFlow;

pub fn rreturn_intercept_game_native_settings(game: &mut Game<'_>, i: usize) -> ControlFlow<()> {
    let is_rreturn = game
        .settings
        .mods
        .installed
        .get(i)
        .is_some_and(|m| m.id == "rreturn");
    if !is_rreturn {
        return ControlFlow::Continue(());
    }
    if matches!(
        game.ui.options.page(),
        horizon_client::options_ui::Page::NativeMod(_)
    ) {
        let _ = game.ui.options.back(&mut game.settings);
    }
    crate::game::autosave::rreturn::open_global_settings_modal();
    ControlFlow::Break(())
}

pub fn rreturn_patch_menu_mods(mods: &mut horizon_client::mod_list::ModList) {
    for row in &mut mods.installed {
        if row.id == "rreturn" && row.native.is_none() {
            row.native = Some(horizon_client::mod_list::NativePage {
                mappings: Vec::new(),
                settings: vec![horizon_client::mod_list::NativeSetting {
                    key: "global_settings".to_owned(),
                    label: "RReturn".to_owned(),
                    tooltip: None,
                    value: horizon_client::mod_list::NativeSettingValue::Toggle(true),
                }],
            });
        }
    }
}

pub fn rreturn_intercept_loading_native_page(loading: &mut Loading) {
    if !loading.menu.options_open {
        return;
    }
    if matches!(
        loading.menu.options.page(),
        horizon_client::options_ui::Page::Mods
    ) {
        rreturn_patch_menu_mods(&mut loading.settings.mods);
    }
    let is_rreturn_native_page = matches!(
        loading.menu.options.page(),
        horizon_client::options_ui::Page::NativeMod(i)
            if loading
                .settings
                .mods
                .installed
                .get(i)
                .is_some_and(|m| m.id == "rreturn")
    );
    if is_rreturn_native_page {
        let _ = loading.menu.options.back(&mut loading.settings);
        crate::game::autosave::rreturn::open_global_settings_modal();
    }
}

pub fn rreturn_draw_loading_frame(loading: &mut Loading) -> ControlFlow<()> {
    rreturn_intercept_loading_native_page(loading);
    if !crate::game::autosave::rreturn::is_global_modal_open() {
        return ControlFlow::Continue(());
    }
    if !loading.menu.options_open || loading.menu.pending.is_some() || loading.menu.win.is_some() {
        crate::game::autosave::rreturn::close_global_settings_modal();
        return ControlFlow::Continue(());
    }
    let Some(size) = loading
        .renderer
        .as_ref()
        .map(|r| [r.config.width, r.config.height])
    else {
        return ControlFlow::Continue(());
    };
    let mut batch = HudBatch::new(size, loading.settings.gui_scale_inputs());
    loading.draw_menu(&mut batch);
    loading.menu.frame_drawn((batch.width, batch.height));
    if let Some(gui) = &loading.gui {
        crate::game::autosave::rreturn::render_global_settings_in_menu(
            &mut batch,
            &gui.atlas,
            loading.menu.cursor,
            &loading.settings.mods,
        );
    }
    let mut frame = super::loading_frame(&batch);
    frame.yaw = loading.panorama_rotation.update(
        loading.started.elapsed().as_secs_f32(),
        loading.settings.file.double("panoramaScrollSpeed") as f32,
    );
    if let Some(renderer) = &mut loading.renderer {
        match renderer.render(&frame) {
            Ok(true) if loading.gui.is_some() && loading.menu.pending.is_none() => {
                loading.title_drawn = true;
            }
            Ok(_) => {}
            Err(e) => log::warn!("Horizon: loading screen frame failed: {e}"),
        }
    }
    ControlFlow::Break(())
}

pub fn rreturn_intercept_loading_menu_event(
    loading: &mut Loading,
    id: winit::window::WindowId,
    event: &WindowEvent,
) -> ControlFlow<()> {
    if loading.window.as_ref().is_none_or(|w| w.id() != id) {
        return ControlFlow::Continue(());
    }
    rreturn_intercept_loading_native_page(loading);
    if !crate::game::autosave::rreturn::is_global_modal_open() {
        return ControlFlow::Continue(());
    }
    if !loading.menu.options_open || loading.menu.pending.is_some() || loading.menu.win.is_some() {
        crate::game::autosave::rreturn::close_global_settings_modal();
        return ControlFlow::Continue(());
    }
    loading.menu.dragging = false;
    match event {
        WindowEvent::MouseInput { state, button, .. } => {
            let btn = match button {
                MouseButton::Left => 0,
                MouseButton::Right => 1,
                MouseButton::Middle => 2,
                _ => -1,
            };
            if *state == ElementState::Pressed {
                crate::game::autosave::rreturn::handle_settings_mouse_pressed_core(
                    btn,
                    loading.menu.cursor.0,
                    loading.menu.cursor.1,
                );
            } else {
                crate::game::autosave::rreturn::handle_settings_mouse_released_core(btn);
            }
            ControlFlow::Break(())
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let raw = match delta {
                MouseScrollDelta::LineDelta(_, y) => f64::from(*y),
                MouseScrollDelta::PixelDelta(p) => p.y / 30.0,
            };
            let amount = if loading.settings.discrete_scroll {
                horizon_client::mouse::java_signum(raw)
            } else {
                raw
            } * f64::from(loading.settings.mouse_wheel_sensitivity);
            crate::game::autosave::rreturn::handle_settings_mouse_scrolled_core(amount);
            ControlFlow::Break(())
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let mut physical_key = event.physical_key;
            super::follow_named_keymap(&event.logical_key, &mut physical_key);
            if physical_key == PhysicalKey::Code(KeyCode::F11) {
                return ControlFlow::Continue(());
            }
            if event.state == ElementState::Pressed {
                let key_name = match physical_key {
                    PhysicalKey::Code(code) => {
                        horizon_client::keybinds::key_name(code).unwrap_or("")
                    }
                    PhysicalKey::Unidentified(_) => "",
                };
                let mods = glfw_modifiers(loading.menu.modifiers);
                let def = mod_api::__private::key_mappings()
                    .iter()
                    .find(|(owner, mapping)| *owner == "rreturn" && mapping.id == "settings_menu")
                    .map_or(("key.keyboard.k", 0u32), |(_, m)| {
                        (m.key.as_str(), m.modifiers.bits)
                    });
                let (close_key, close_mods) =
                    loading.settings.mods.binding("rreturn", "settings_menu", def);
                crate::game::autosave::rreturn::handle_settings_key_core(
                    key_name,
                    mods,
                    &close_key,
                    close_mods & 7,
                    true,
                );
                if (mods & 2) == 0
                    && let Some(text) = event.text.as_deref()
                {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        crate::game::autosave::rreturn::handle_settings_char_core(c, mods);
                    }
                }
            }
            ControlFlow::Break(())
        }
        _ => ControlFlow::Continue(()),
    }
}
