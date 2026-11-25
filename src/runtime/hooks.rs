use super::super::{
    native_lifecycle::WorldPhase,
    Game, LanHost,
};
use super::adaptive::tick_autosave_on_server;
use super::capture::create_checkpoint_on_server;
use super::hydration::hydrate_slot_from_disk_on_server;
use super::load_disk::load_checkpoint_from_disk;
use super::restore::rollback_checkpoint_on_server;
use super::restore_client::{apply_client_level_rollback, apply_client_rollback_with_ui};
use super::scrub_hud::{
    draw_scrub_hud, handle_scrub_hud_mouse_pressed, handle_scrub_hud_mouse_released,
};
use super::settings_ui::{
    flush_settings_ui_actions, handle_screen_char_typed, is_settings_ui_open,
    request_open_settings_ui, reset_settings_ui_state,
};
use super::snapshots::*;
use super::storage::has_disk_checkpoint;
use super::types::*;
use super::ui::{load_saved_settings, save_current_settings};
use super::ui_snapshot::*;
use std::cell::RefCell;
use std::ops::ControlFlow;

#[derive(Clone, Copy)]
pub enum ScreenHotkeyAction {
    SaveCheckpoint,
    LoadCheckpoint,
    ToggleScrub,
    CommitScrub,
    CancelScrub,
    StepScrub(f64),
}

thread_local! {
    static PENDING_SCREEN_HOTKEY: RefCell<Option<(String, ScreenHotkeyAction)>> = const { RefCell::new(None) };
    static SUPPRESS_SCREEN_CHAR: RefCell<bool> = const { RefCell::new(false) };
}

fn mod_key_binding(game: &Game<'_>, id: &str, default_key: &str) -> (String, u32) {
    game.mod_key(MOD_ID, id)
        .map(|(k, m)| (k, m.bits & 7))
        .unwrap_or_else(|| (default_key.to_owned(), 0))
}

fn is_bound_key_match(key: &str, cur_mods: u32, bound_key: &str, bound_mods: u32) -> bool {
    !bound_key.is_empty()
        && bound_key != "key.keyboard.unknown"
        && key == bound_key
        && (bound_mods == 0 || (cur_mods & bound_mods) == bound_mods)
}

fn is_scrub_key_match(key: &str, cur_mods: u32, scrub_key: &str, scrub_mods: u32) -> bool {
    if scrub_key.is_empty() || scrub_key == "key.keyboard.unknown" {
        return false;
    }
    let key_ok = key == scrub_key
        || (scrub_key.eq_ignore_ascii_case("key.keyboard.u") && key.eq_ignore_ascii_case("u"));
    key_ok && (scrub_mods == 0 || (cur_mods & scrub_mods) == scrub_mods)
}

fn execute_screen_hotkey_action(game: &mut Game<'_>, act: ScreenHotkeyAction) {
    match act {
        ScreenHotkeyAction::SaveCheckpoint => execute_checkpoint_save_hotkey(game),
        ScreenHotkeyAction::LoadCheckpoint => execute_checkpoint_rollback_hotkey(game),
        ScreenHotkeyAction::ToggleScrub => {
            if !with_state(|st| st.scrub_enabled) {
                mod_chat(game, &tr("command.rreturn.scrub.disabled"));
                return;
            }
            if is_scrubbing() {
                commit_scrub(game);
            } else {
                start_scrub(game);
            }
        }
        ScreenHotkeyAction::CommitScrub => commit_scrub(game),
        ScreenHotkeyAction::CancelScrub => cancel_scrub(game),
        ScreenHotkeyAction::StepScrub(delta) => step_scrub_ticks(delta),
    }
}

fn flush_pending_screen_hotkey(game: &mut Game<'_>) {
    let pending = PENDING_SCREEN_HOTKEY.with(|c| c.borrow_mut().take().map(|(_, a)| a));
    if let Some(act) = pending {
        execute_screen_hotkey_action(game, act);
    }
}

pub fn handle_screen_key_pressed_any(
    game: &Game<'_>,
    key: &str,
    repeat: bool,
    modifiers: u32,
    canceled: &mut bool,
) -> bool {
    if is_settings_ui_open(game) && !is_scrubbing() {
        return false;
    }
    if game.title.is_some()
        || game.player.is_none()
        || game.screen == super::super::Screen::Dead
    {
        return false;
    }
    let (scrub_key, scrub_mods) = mod_key_binding(game, "scrub", "key.keyboard.u");
    let cur_mods = modifiers & 7;

    if is_scrubbing() {
        if !repeat && is_scrub_key_match(key, cur_mods, &scrub_key, scrub_mods) {
            SUPPRESS_SCREEN_CHAR.with(|c| *c.borrow_mut() = true);
            PENDING_SCREEN_HOTKEY.with(|c| {
                *c.borrow_mut() = Some((key.to_owned(), ScreenHotkeyAction::CommitScrub))
            });
            *canceled = true;
            return true;
        }
        if !repeat && (key == "key.keyboard.enter" || key == "key.keyboard.keypad_enter") {
            PENDING_SCREEN_HOTKEY.with(|c| {
                *c.borrow_mut() = Some((key.to_owned(), ScreenHotkeyAction::CommitScrub))
            });
            *canceled = true;
            return true;
        }
        if !repeat && key == "key.keyboard.escape" {
            PENDING_SCREEN_HOTKEY.with(|c| {
                *c.borrow_mut() = Some((key.to_owned(), ScreenHotkeyAction::CancelScrub))
            });
            *canceled = true;
            return true;
        }
        if key == "key.keyboard.left" || key == "key.keyboard.a" {
            SUPPRESS_SCREEN_CHAR.with(|c| *c.borrow_mut() = true);
            PENDING_SCREEN_HOTKEY.with(|c| {
                *c.borrow_mut() = Some((key.to_owned(), ScreenHotkeyAction::StepScrub(-1.0)))
            });
            *canceled = true;
            return true;
        }
        if key == "key.keyboard.right" || key == "key.keyboard.d" {
            SUPPRESS_SCREEN_CHAR.with(|c| *c.borrow_mut() = true);
            PENDING_SCREEN_HOTKEY.with(|c| {
                *c.borrow_mut() = Some((key.to_owned(), ScreenHotkeyAction::StepScrub(1.0)))
            });
            *canceled = true;
            return true;
        }
        *canceled = true;
        return true;
    }

    if repeat {
        return false;
    }
    let (save_key, save_mods) = mod_key_binding(game, "save", "key.keyboard.f6");
    let (load_key, load_mods) = mod_key_binding(game, "load", "key.keyboard.f7");

    if is_bound_key_match(key, cur_mods, &save_key, save_mods) {
        PENDING_SCREEN_HOTKEY.with(|c| {
            *c.borrow_mut() = Some((key.to_owned(), ScreenHotkeyAction::SaveCheckpoint))
        });
        *canceled = true;
        return true;
    }
    if is_bound_key_match(key, cur_mods, &load_key, load_mods) {
        PENDING_SCREEN_HOTKEY.with(|c| {
            *c.borrow_mut() = Some((key.to_owned(), ScreenHotkeyAction::LoadCheckpoint))
        });
        *canceled = true;
        return true;
    }
    if is_scrub_key_match(key, cur_mods, &scrub_key, scrub_mods) {
        let ctrl = (modifiers & 2) != 0;
        let focused = is_text_input_focused(game);
        if !focused || ctrl {
            if !ctrl {
                SUPPRESS_SCREEN_CHAR.with(|c| *c.borrow_mut() = true);
            }
            PENDING_SCREEN_HOTKEY.with(|c| {
                *c.borrow_mut() = Some((key.to_owned(), ScreenHotkeyAction::ToggleScrub))
            });
            *canceled = true;
            return true;
        }
    }
    false
}

pub fn handle_screen_char_typed_dispatch(
    game: &Game<'_>,
    screen_id: &str,
    ch: char,
    modifiers: u32,
    canceled: &mut bool,
) {
    let suppressed = SUPPRESS_SCREEN_CHAR.with(|c| std::mem::replace(&mut *c.borrow_mut(), false));
    if suppressed || is_scrubbing() {
        *canceled = true;
        return;
    }
    if screen_id == "mod" {
        handle_screen_char_typed(game, ch, modifiers, canceled);
    }
}

pub fn handle_play_mouse_button(game: &Game<'_>, button: i32, pressed: bool) -> bool {
    if !is_scrubbing() {
        return false;
    }
    if pressed {
        let [mx, my] = game.cursor();
        let _ = handle_scrub_hud_mouse_pressed(button, mx, my);
    } else {
        handle_scrub_hud_mouse_released(button);
    }
    true
}

pub fn handle_screen_render_post(
    game: &Game<'_>,
    draw: &mut horizon_client::hud::HudBatch,
    screen_id: &str,
) {
    if with_state(|st| st.scrub_enabled) && is_scrubbing() && screen_id != "dead" {
        draw_scrub_hud(game, draw);
    }
}

pub fn execute_checkpoint_save_hotkey(game: &mut Game<'_>) {
    let ui_snap = capture_client_ui_snapshot(game);
    if let Some(server) = &game.integrated {
        let res = server.call(|srv| {
            let root = srv.world_root.clone();
            if !has_cheat_permission(srv, None) {
                return (false, root, tr("command.rreturn.no_permission"));
            }
            if create_checkpoint_on_server(srv, None) {
                with_state_mut(|st| st.current_slot = "default".to_owned());
                (true, root, tr("command.rreturn.set.success"))
            } else {
                (false, root, tr("command.rreturn.set.failed"))
            }
        });
        if let Some((ok, root, text)) = res {
            set_active_ui_world_root(&root);
            if ok {
                save_client_ui_slot(&root, "default", ui_snap);
            }
            mod_chat(game, &text);
        }
    } else if !has_cheat_permission(game, None) {
        mod_chat(game, &tr("command.rreturn.no_permission"));
    } else {
        let root = resolve_ui_world_root(&game.world_root);
        save_client_ui_slot(&root, "default", ui_snap);
        with_state_mut(|st| st.pending_save_request = true);
    }
}

pub fn execute_checkpoint_rollback_hotkey(game: &mut Game<'_>) {
    if let Some(server) = &game.integrated {
        let res = server.call(|srv| {
            let root = srv.world_root.clone();
            if !has_level_4_permission(srv, None) {
                return (false, root, tr("command.rreturn.no_permission"));
            }
            if rollback_checkpoint_on_server(srv) {
                (
                    true,
                    root,
                    tr_args(
                        "command.rreturn.load.success",
                        &[&srv.world.game_time.to_string()],
                    ),
                )
            } else {
                (false, root, tr("command.rreturn.load.failed"))
            }
        });
        if let Some((did_rollback, root, text)) = res {
            set_active_ui_world_root(&root);
            if did_rollback {
                close_client_container_local(game);
                game.screen = super::super::Screen::Playing;
                let slot = with_state(|st| st.current_slot.clone());
                apply_client_rollback_with_ui(game, &slot, true);
            }
            mod_chat(game, &text);
        }
    } else if !has_level_4_permission(game, None) {
        mod_chat(game, &tr("command.rreturn.no_permission"));
    } else {
        close_client_container_local(game);
        game.screen = super::super::Screen::Playing;
        with_state_mut(|st| st.pending_load_request = true);
    }
}

pub fn handle_key(game: &mut Game<'_>, key: &str, action: mod_api::KeyAction) {
    let queued = PENDING_SCREEN_HOTKEY.with(|c| {
        let mut cur = c.borrow_mut();
        if matches!(action, mod_api::KeyAction::Press | mod_api::KeyAction::Repeat)
            && cur.as_ref().is_some_and(|(k, _)| k == key)
        {
            cur.take().map(|(_, a)| a)
        } else {
            None
        }
    });
    if let Some(act) = queued {
        execute_screen_hotkey_action(game, act);
        return;
    }

    if game.title.is_some()
        || game.player.is_none()
        || game.screen == super::super::Screen::Dead
    {
        return;
    }

    let (scrub_key, scrub_mods) = mod_key_binding(game, "scrub", "key.keyboard.u");
    let cur_mods = game.modifiers().bits & 7;

    if is_scrub_key_match(key, cur_mods, &scrub_key, scrub_mods) && matches!(action, mod_api::KeyAction::Release) {
        if is_scrubbing() && is_held_scrub(300) {
            commit_scrub(game);
        }
        return;
    }

    let in_playing = game.playing();
    let overlay_over_play = game.screen == super::super::Screen::Playing && !in_playing;
    if !in_playing && !overlay_over_play {
        return;
    }

    if is_scrub_key_match(key, cur_mods, &scrub_key, scrub_mods) {
        let ctrl = game.has_control_down_with_quirk();
        if !with_state(|st| st.scrub_enabled) {
            if matches!(action, mod_api::KeyAction::Press)
                && (in_playing || !is_text_input_focused(game) || ctrl)
            {
                mod_chat(game, &tr("command.rreturn.scrub.disabled"));
            }
            return;
        }
        if matches!(action, mod_api::KeyAction::Press) {
            if is_scrubbing() {
                commit_scrub(game);
            } else if in_playing || !is_text_input_focused(game) || ctrl {
                start_scrub(game);
            }
        }
        return;
    }

    if is_scrubbing() {
        if (key == "key.keyboard.enter" || key == "key.keyboard.keypad_enter")
            && matches!(action, mod_api::KeyAction::Press)
        {
            commit_scrub(game);
            return;
        }
        if key == "key.keyboard.escape" && matches!(action, mod_api::KeyAction::Press) {
            cancel_scrub(game);
            return;
        }
        if (key == "key.keyboard.left" || key == "key.keyboard.a")
            && matches!(action, mod_api::KeyAction::Press)
        {
            step_scrub_ticks(-1.0);
            return;
        }
        if (key == "key.keyboard.right" || key == "key.keyboard.d")
            && matches!(action, mod_api::KeyAction::Press)
        {
            step_scrub_ticks(1.0);
            return;
        }
        // Block all other keys from interacting with the game during active scrub
        return;
    }

    let pressed = matches!(action, mod_api::KeyAction::Press);
    if !pressed {
        return;
    }
    let (save_key, save_mods) = mod_key_binding(game, "save", "key.keyboard.f6");
    let (load_key, load_mods) = mod_key_binding(game, "load", "key.keyboard.f7");
    let (settings_key, settings_mods) = mod_key_binding(game, "settings_menu", "key.keyboard.k");

    if is_bound_key_match(key, cur_mods, &settings_key, settings_mods) {
        if in_playing && !is_settings_ui_open(game) {
            request_open_settings_ui(game);
        }
        return;
    }

    if is_bound_key_match(key, cur_mods, &save_key, save_mods) {
        execute_checkpoint_save_hotkey(game);
    } else if is_bound_key_match(key, cur_mods, &load_key, load_mods) {
        execute_checkpoint_rollback_hotkey(game);
    }
}

pub fn handle_server_started(game: &mut Game<'_>) {
    set_active_ui_world_root(&game.world_root);
    load_saved_settings(game);
    clear_all_slots();
    clear_client_snapshots();
    with_scrub_ring_mut(|r| r.clear());
    clear_server_scrub();
    clear_reverse_audio_cache();
    let scrub_en = with_state(|st| st.scrub_enabled);
    game.audio.record_heard = scrub_en;
    if scrub_en {
        if game.audio.capture.is_none() {
            game.audio.capture = Some(Vec::new());
        }
    } else {
        game.audio.capture = None;
    }
    let has_disk_cp = has_disk_checkpoint(&game.world_root);
    let mut restored_anchors = HashSet::new();
    let mut restored_save_tick = game.world.game_time;
    if has_disk_cp {
        if let Some(snap) = load_checkpoint_from_disk(&game.world_root) {
            restored_anchors = snap.anchor_ids.clone();
            restored_save_tick = snap.game_time;
            set_snapshot(Some(Arc::new(snap)));
        }
        with_state_mut(|st| {
            st.has_checkpoint = true;
        });
    } else {
        with_state_mut(|st| {
            st.has_checkpoint = false;
        });
    }
    let players = collect_online_players(game);
    with_state_mut(|st| {
        st.online_players = players;
        st.current_slot = "default".to_owned();
        st.anchor_ids = restored_anchors;
        st.pending_death_rollback = None;
        st.last_save_tick = restored_save_tick;
        st.current_interval_ticks = i64::from(st.fixed_interval_sec.max(5)) * 20;
        st.last_rollback_tick = None;
        st.last_boss_death_tick = -9999;
        st.current_intensity = 0.0;
        st.peak_intensity = 0.0;
        st.relax_ticks = 0;
    });
}

pub fn handle_lifecycle(game: &mut Game<'_>, phase: WorldPhase) {
    match phase {
        WorldPhase::WorldCloseFinal
        | WorldPhase::WorldCloseFailed
        | WorldPhase::WorldOpenFailed => {
            clear_active_ui_world_root();
            reset_settings_ui_state();
            clear_all_slots();
            clear_client_snapshots();
            with_scrub_ring_mut(|r| r.clear());
            clear_server_scrub();
            clear_reverse_audio_cache();
            game.audio.capture = None;
            game.audio.record_heard = false;
            with_state_mut(|st| {
                st.pending_death_rollback = None;
            });
        }
        _ => {}
    }
}

pub fn handle_tick(game: &mut Game<'_>) {
    tick_ui_mode_guard();
    flush_pending_screen_hotkey(game);
    flush_settings_ui_actions(game);
    let scrub_en = with_state(|st| st.scrub_enabled);
    if scrub_en {
        if game.audio.capture.is_none() {
            game.audio.capture = Some(Vec::new());
        }
        if !game.audio.record_heard {
            game.audio.record_heard = true;
        }
    } else {
        if game.audio.capture.is_some() {
            game.audio.capture = None;
        }
        if game.audio.record_heard {
            game.audio.record_heard = false;
        }
    }

    if scrub_en
        && !is_scrubbing()
        && game.native_window()
        && is_scrub_recordable_client(game)
    {
        with_scrub_ring_mut(|r| {
            r.record_tick(game);
        });
    }

    if game.screen == super::super::Screen::Dead {
        let has_cp = with_state(|st| st.has_checkpoint);
        if has_cp {
            game.screen = super::super::Screen::Playing;
            game.ui.death_time = 0;
            game.ui.died_at = None;
            game.ui.death_confirm = None;
            game.boss_overlay.reset();
            if let Some(p) = game.player.as_mut() {
                let _ = p.set_health(20.0, false);
                p.health.dead = false;
                p.invulnerable = false;
                p.may_fly = false;
                p.spectator = false;
                p.flying = false;
                p.fall_distance = 0.0;
                p.health.hurt.no_action_time = 0;
                p.health.hurt.cooldown = 0;
                p.health.hurt.last_hurt = 0.0;
                p.health.hurt.hurt_duration = 0;
                p.health.hurt.hurt_time = 0;
                p.health.hurt.last_source = None;
                p.health.hurt.last_stamp = 0;
                p.health.hurt_animation_time = 0;
                p.health.hurt_dir = 0.0;
                p.health.indicated_dir = None;
            }
            if let Some(server) = &game.integrated {
                server.call(|srv| {
                    rollback_checkpoint_on_server(srv);
                });
            } else {
                rollback_checkpoint_on_server(game);
            }
            let slot = with_state(|st| st.current_slot.clone());
            apply_client_rollback_with_ui(game, &slot, true);
            return;
        }
    }

    if game.remote.is_some() {
        if client_epoch_needs_sync() && get_rollback_epoch() > 0 {
            let slot = with_state(|st| st.current_slot.clone());
            apply_client_rollback_with_ui(game, &slot, true);
        }
    }
}

pub fn handle_server_tick(game: &mut Game<'_>, world_running: bool) {
    if game.world.game_time % 20 == 0 || with_state(|st| st.online_players.is_empty()) {
        let players = collect_online_players(game);
        if !players.is_empty() {
            with_state_mut(|st| {
                if st.online_players != players {
                    st.online_players = players;
                }
            });
        }
    }
    if with_state(|st| st.scrub_enabled) && !is_scrubbing() && world_running {
        record_server_tick(game);
    }

    let (do_save, do_load, dead_rollback) = with_state_mut(|st| {
        (
            std::mem::replace(&mut st.pending_save_request, false),
            std::mem::replace(&mut st.pending_load_request, false),
            st.pending_death_rollback.take(),
        )
    });

    if do_save {
        create_checkpoint_on_server(game, None);
    }
    if do_load || dead_rollback.is_some() {
        rollback_checkpoint_on_server(game);
        if !game.serving() {
            let slot = with_state(|st| st.current_slot.clone());
            apply_client_rollback_with_ui(game, &slot, true);
        }
        return;
    }

    if !world_running {
        return;
    }

    if with_state(|st| st.has_checkpoint) {
        if !game.world.chunks.is_empty() {
            let active_slot = with_state(|st| st.current_slot.clone());
            let needs_hydration = get_slot_snapshot_clone(&active_slot)
                .is_none_or(|s| !s.levels.contains_key(&game.dim));
            if needs_hydration {
                let _ = hydrate_slot_from_disk_on_server(game, &active_slot);
            }
        }
        if let Ok(mut tr) = loop_tracker().write() {
            tr.record_tick(game.world.game_time, &game.mobs);
        }
    }

    tick_autosave_on_server(game);
}

pub fn intercept_guest_die(
    game: &mut Game<'_>,
    gi: usize,
    _conn: u32,
) -> ControlFlow<()> {
    let now = game.world.game_time;
    let Some(host) = game.lan.as_mut() else {
        return ControlFlow::Continue(());
    };
    if gi >= host.hub.guests.len() || host.hub.guests[gi].health > 0.0 || host.hub.guests[gi].dead {
        return ControlFlow::Continue(());
    }
    let player_uuid = host.hub.guests[gi].uuid;
    let online: Vec<u128> = host
        .hub
        .guests
        .iter()
        .filter(|g| g.placed)
        .map(|g| g.uuid)
        .collect();

    let should_intercept = with_state_mut(|st| {
        if !st.has_checkpoint {
            return false;
        }
        let active_anchors = resolve_anchors(&st.anchor, &online, now, None);
        let is_anchor =
            active_anchors.contains(&player_uuid) || st.anchor_ids.contains(&player_uuid);
        if !is_anchor {
            return false;
        }
        let cd = st.anchor.rollback_cooldown_seconds;
        if cd > 0
            && let Some(last) = st.last_rollback_tick
            && now >= last
            && now - last < i64::from(cd) * 20
        {
            return false;
        }
        st.last_rollback_tick = Some(now);
        st.pending_death_rollback = Some(player_uuid);
        true
    });

    if !should_intercept {
        return ControlFlow::Continue(());
    }

    let Some(host) = game.lan.as_mut() else {
        return ControlFlow::Continue(());
    };
    if gi >= host.hub.guests.len() {
        return ControlFlow::Continue(());
    }
    host.hub.guests.update(gi, |g| {
        g.health = 20.0;
        g.dead = false;
        g.death_time = 0;
        g.sent_health = None;
        if let Some(m) = g.motion.as_mut() {
            m.health.dead = false;
            m.invulnerable = false;
            m.may_fly = false;
            m.spectator = false;
            m.flying = false;
            m.fall_distance = 0.0;
            m.health.hurt.no_action_time = 0;
            m.health.hurt.cooldown = 0;
            m.health.hurt.last_hurt = 0.0;
            m.health.hurt.hurt_duration = 0;
            m.health.hurt.hurt_time = 0;
            m.health.hurt.last_source = None;
            m.health.hurt.last_stamp = 0;
            m.health.hurt_animation_time = 0;
            m.health.hurt_dir = 0.0;
            m.health.indicated_dir = None;
            let _ = m.set_health(20.0, false);
        }
    });
    ControlFlow::Break(())
}

pub fn handle_mouse_scrolling(
    game: &mut Game<'_>,
    scroll: [f64; 2],
    accumulated: [i32; 2],
) -> bool {
    if !with_state(|st| st.scrub_enabled) {
        return false;
    }
    on_mouse_scroll(game, scroll, accumulated)
}

pub fn handle_frame_render_pre(
    game: &mut Game<'_>,
    world_partial: f32,
    partial: f32,
) {
    flush_pending_screen_hotkey(game);
    flush_settings_ui_actions(game);
    if is_settings_ui_open(game) {
        let _ = game.apply_screen_blur();
    }
    if !with_state(|st| st.scrub_enabled) {
        return;
    }
    on_frame_render_pre(game, world_partial, partial);
}

pub fn handle_render_gui_post(
    game: &Game<'_>,
    draw: &mut horizon_client::hud::HudBatch,
) {
    if !with_state(|st| st.scrub_enabled) {
        return;
    }
    render_hud(game, draw);
}

