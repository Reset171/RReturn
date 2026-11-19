use super::super::{Game, LanHost};
use super::capture::*;
use super::policy::*;
use super::restore::*;
use super::restore_client::*;
use super::snapshots::*;
use super::storage::*;
use super::telemetry::*;
use super::types::*;
use super::ui::save_current_settings;
use horizon_protocol::{
    packets::play::{self, Clientbound as C},
    text::literal,
};
use std::ops::ControlFlow;

pub fn execute_rreturn_command(
    game: &mut Game<'_>,
    requester: Option<u128>,
    command: &str,
) -> Vec<String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    let sub = words.get(1).copied().unwrap_or("status");
    let reply = match sub {
        "json" => report_status_json(game),
        "status" => {
            if words.get(2).copied() == Some("--json") {
                report_status_json(game)
            } else {
                report_status(game)
            }
        }
        "set" | "save" => {
            let slot = words.get(2).copied().unwrap_or("default");
            if !is_valid_slot_name(slot) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if create_slot_checkpoint_on_server(game, requester, slot) {
                with_state_mut(|st| st.current_slot = slot.to_owned());
                tr_args("command.rreturn.set.slot_success", &[slot])
            } else {
                tr_args("command.rreturn.set.slot_failed", &[slot])
            }
        }
        "load" | "rollback" => {
            if !has_level_4_permission(game, requester) {
                tr("command.rreturn.no_permission")
            } else {
                let slot = words.get(2).copied().unwrap_or("default");
                if !is_valid_slot_name(slot) {
                    "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
                } else if rollback_slot_checkpoint_on_server(game, slot) {
                    with_state_mut(|st| st.current_slot = slot.to_owned());
                    tr_args(
                        "command.rreturn.load.slot_success",
                        &[slot, &game.world.game_time.to_string()],
                    )
                } else {
                    tr_args("command.rreturn.load.slot_failed", &[slot])
                }
            }
        }
        "slot" | "slots" => handle_slot_subcommand(game, &words[2..], requester),
        "policy" => handle_policy_subcommand(game, &words[2..]),
        "metrics" => report_metrics_json(game),
        "rewind" => {
            if !has_level_4_permission(game, requester) {
                tr("command.rreturn.no_permission")
            } else {
                let slot = words.get(2).copied().unwrap_or("default");
                if !is_valid_slot_name(slot) {
                    "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
                } else if rollback_slot_checkpoint_on_server(game, slot) {
                    format!("[RReturn] Immediate rewind executed to slot '{}'.", slot)
                } else {
                    format!("[RReturn] Rewind failed: Slot '{}' not found.", slot)
                }
            }
        }
        "trigger" => handle_trigger_subcommand(game, &words[2..], requester),
        "scrub" => handle_scrub_subcommand(game, &words[2..], requester),
        "purge_memory" => {
            clear_all_slots();
            clear_client_snapshots();
            "[RReturn] In-memory snapshots cleared.".to_owned()
        }
        "anchor" => handle_anchor_subcommand(game, &words[2..], requester),
        "autosave" => handle_autosave_subcommand(game, &words[2..]),
        "bench" | "benchmark" => handle_bench_subcommand(),
        "hitboxes" | "f3" => {
            let mode = words.get(2).copied();
            let val = match mode {
                Some("on" | "true" | "1") => true,
                Some("off" | "false" | "0") => false,
                _ => with_state(|st| !st.f3_loop_hitboxes),
            };
            with_state_mut(|st| st.f3_loop_hitboxes = val);
            save_current_settings(game);
            let state_str = if val {
                tr("ui.rreturn.on")
            } else {
                tr("ui.rreturn.off")
            };
            tr_args("command.rreturn.f3.status", &[&state_str])
        }
        "dev" => {
            let mode = words.get(2).copied();
            let val = match mode {
                Some("on" | "true" | "1") => true,
                Some("off" | "false" | "0") => false,
                _ => with_state(|st| !st.dev_mode),
            };
            with_state_mut(|st| st.dev_mode = val);
            save_current_settings(game);
            let state_str = if val {
                tr("ui.rreturn.on")
            } else {
                tr("ui.rreturn.off")
            };
            tr_args("command.rreturn.dev.status", &[&state_str])
        }
        _ => "[RReturn] Usage: /rreturn <set|load|json|status|slot|policy|metrics|anchor|autosave|bench|f3|dev>".to_owned(),
    };
    vec![reply]
}

pub fn handle_slot_subcommand(
    game: &mut Game<'_>,
    args: &[&str],
    requester: Option<u128>,
) -> String {
    let sub = args.first().copied().unwrap_or("list");
    match sub {
        "list" => {
            let disk_slots = list_disk_slots(&game.world_root);
            serde_json::to_string_pretty(&serde_json::json!({
                "current_slot": with_state(|st| st.current_slot.clone()),
                "slots": disk_slots,
            }))
            .unwrap_or_else(|_| "{}".to_owned())
        }
        "save" => {
            let name = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(name) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if create_slot_checkpoint_on_server(game, requester, name) {
                with_state_mut(|st| st.current_slot = name.to_owned());
                format!("[RReturn] Saved timeline to slot '{}'.", name)
            } else {
                tr_args("command.rreturn.set.slot_failed", &[name])
            }
        }
        "load" => {
            if !has_level_4_permission(game, requester) {
                return tr("command.rreturn.no_permission");
            }
            let name = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(name) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if rollback_slot_checkpoint_on_server(game, name) {
                with_state_mut(|st| st.current_slot = name.to_owned());
                format!("[RReturn] Rolled back to slot '{}'.", name)
            } else {
                format!("[RReturn] Slot '{}' not found.", name)
            }
        }
        "select" => {
            let name = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(name) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else {
                with_state_mut(|st| st.current_slot = name.to_owned());
                format!("[RReturn] Active slot set to '{}'.", name)
            }
        }
        "delete" => {
            if !has_level_4_permission(game, requester) {
                return tr("command.rreturn.no_permission");
            }
            let name = args.get(1).copied().unwrap_or("");
            if name.is_empty() || name == "default" {
                "[RReturn] Cannot delete default slot.".to_owned()
            } else if !is_valid_slot_name(name) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else {
                delete_slot_snapshot(name);
                let deleted = delete_disk_slot(&game.world_root, name);
                if deleted {
                    format!("[RReturn] Deleted slot '{}'.", name)
                } else {
                    format!("[RReturn] Slot '{}' not found.", name)
                }
            }
        }
        _ => "[RReturn] Usage: /rreturn slot <list|save|load|select|delete> [name]".to_owned(),
    }
}

pub fn handle_policy_subcommand(game: &mut Game<'_>, args: &[&str]) -> String {
    let sub = args.first().copied().unwrap_or("get");
    match sub {
        "get" => {
            let pol = with_state(|st| st.policy.to_json());
            serde_json::to_string_pretty(&pol).unwrap_or_else(|_| "{}".to_owned())
        }
        "set" => {
            let key = args.get(1).copied().unwrap_or("");
            let val = match args.get(2).copied().unwrap_or("").to_ascii_lowercase().as_str() {
                "true" | "1" | "on" => Some(true),
                "false" | "0" | "off" => Some(false),
                _ => None,
            };
            if key.is_empty() || val.is_none() {
                return "[RReturn] Usage: /rreturn policy set <field> <true|false>".to_owned();
            }
            let success = with_state_mut(|st| st.policy.set_field(key, val.unwrap()));
            if success {
                save_current_settings(game);
                format!("[RReturn] Policy '{}' set to {}.", key, val.unwrap())
            } else {
                format!("[RReturn] Unknown policy field '{}'.", key)
            }
        }
        "reset" => {
            with_state_mut(|st| st.policy = RollbackPolicy::default());
            save_current_settings(game);
            "[RReturn] Rollback policy reset to deterministic standard (100% rollback).".to_owned()
        }
        _ => "[RReturn] Usage: /rreturn policy <get|set|reset> [field] [value]".to_owned(),
    }
}

pub fn handle_trigger_subcommand(
    game: &mut Game<'_>,
    args: &[&str],
    requester: Option<u128>,
) -> String {
    match args.first().copied().unwrap_or("rewind") {
        "rewind" => {
            if !has_level_4_permission(game, requester) {
                return tr("command.rreturn.no_permission");
            }
            let slot = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(slot) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if rollback_slot_checkpoint_on_server(game, slot) {
                format!("[RReturn] Triggered rewind to slot '{}'.", slot)
            } else {
                format!("[RReturn] Trigger failed: Slot '{}' not found.", slot)
            }
        }
        "checkpoint" | "save" => {
            let slot = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(slot) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if create_slot_checkpoint_on_server(game, requester, slot) {
                format!("[RReturn] Triggered checkpoint creation for slot '{}'.", slot)
            } else {
                tr_args("command.rreturn.set.slot_failed", &[slot])
            }
        }
        _ => "[RReturn] Usage: /rreturn trigger <rewind|checkpoint> [slot]".to_owned(),
    }
}

pub fn handle_anchor_subcommand(
    game: &mut Game<'_>,
    args: &[&str],
    requester: Option<u128>,
) -> String {
    match args.first().copied().unwrap_or("status") {
        "cooldown" => {
            let seconds = args
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0)
                .max(0);
            with_state_mut(|st| {
                st.anchor.rollback_cooldown_seconds = seconds;
                st.last_rollback_tick = None;
            });
            save_current_settings(game);
            format!("[RReturn] Rollback cooldown set to {} s", seconds)
        }
        "rotate" | "interval" => {
            let seconds = args
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(300)
                .max(10);
            with_state_mut(|st| st.anchor.rotation_seconds = seconds);
            save_current_settings(game);
            format!("[RReturn] Key player rotation interval set to {} s", seconds)
        }
        "pin" => {
            let target = match args.get(1) {
                Some(name) => {
                    let host = game.lan.as_ref();
                    host.and_then(|h| {
                        h.hub
                            .guests
                            .iter()
                            .find(|g| g.name.eq_ignore_ascii_case(name))
                            .map(|g| g.uuid)
                    })
                }
                None => requester,
            };
            if let Some(uuid) = target {
                let added = with_state_mut(|st| {
                    if !st.anchor.pinned.contains(&uuid) {
                        st.anchor.pinned.push(uuid);
                        true
                    } else {
                        false
                    }
                });
                save_current_settings(game);
                if added {
                    format!("[RReturn] Pinned player {:032x} as key player.", uuid)
                } else {
                    format!("[RReturn] Player {:032x} is already pinned.", uuid)
                }
            } else {
                "[RReturn] Player not found.".to_owned()
            }
        }
        "unpin" => {
            let target = match args.get(1) {
                Some(name) => {
                    let host = game.lan.as_ref();
                    host.and_then(|h| {
                        h.hub
                            .guests
                            .iter()
                            .find(|g| g.name.eq_ignore_ascii_case(name))
                            .map(|g| g.uuid)
                    })
                }
                None => requester,
            };
            if let Some(uuid) = target {
                let removed = with_state_mut(|st| {
                    if let Some(idx) = st.anchor.pinned.iter().position(|u| *u == uuid) {
                        st.anchor.pinned.remove(idx);
                        true
                    } else {
                        false
                    }
                });
                save_current_settings(game);
                if removed {
                    format!("[RReturn] Unpinned player {:032x}.", uuid)
                } else {
                    format!("[RReturn] Player {:032x} was not pinned.", uuid)
                }
            } else {
                "[RReturn] Player not found.".to_owned()
            }
        }
        "mode" => {
            let mode_str = args.get(1).copied().unwrap_or("fixed");
            let mode = AnchorMode::by_id(mode_str);
            with_state_mut(|st| {
                st.anchor.mode = mode;
                if mode == AnchorMode::Fixed
                    && st.anchor.pinned.is_empty()
                    && let Some(req) = requester
                {
                    st.anchor.pinned = vec![req];
                }
            });
            save_current_settings(game);
            format!("[RReturn] Key player mode: {}", mode.id())
        }
        _ => report_status(game),
    }
}

pub fn handle_autosave_subcommand(game: &mut Game<'_>, args: &[&str]) -> String {
    match args.first().copied().unwrap_or("status") {
        "on" | "interval" => {
            let sec = args.get(1).and_then(|s| s.parse::<i32>().ok());
            with_state_mut(|st| {
                st.auto_save = true;
                st.adaptive_mode = false;
                if let Some(s) = sec {
                    st.fixed_interval_sec = s.max(5);
                }
                st.current_interval_ticks = i64::from(st.fixed_interval_sec) * 20;
            });
            save_current_settings(game);
            "[RReturn] Interval auto-save enabled.".to_owned()
        }
        "adaptive" => {
            let sec = args.get(1).and_then(|s| s.parse::<i32>().ok());
            with_state_mut(|st| {
                st.auto_save = true;
                st.adaptive_mode = true;
                if let Some(s) = sec {
                    st.adaptive_interval_sec = s.max(60);
                }
            });
            save_current_settings(game);
            "[RReturn] Adaptive auto-save enabled.".to_owned()
        }
        "off" => {
            with_state_mut(|st| st.auto_save = false);
            save_current_settings(game);
            "[RReturn] Auto-save disabled.".to_owned()
        }
        _ => with_state(|st| {
            format!(
                "[RReturn] Auto-save: {} | adaptive: {} | interval: {}s",
                st.auto_save, st.adaptive_mode, st.fixed_interval_sec
            )
        }),
    }
}

pub fn handle_client_command(game: &mut Game<'_>, command: &str, ran: &mut bool) {
    let trimmed = command.trim().trim_start_matches('/');
    let Some(first) = trimmed.split_whitespace().next() else {
        return;
    };
    if first != "rreturn" {
        return;
    }
    *ran = true;
    let is_rollback = trimmed == "rreturn load"
        || trimmed.starts_with("rreturn load ")
        || trimmed == "rreturn rollback"
        || trimmed.starts_with("rreturn rollback ")
        || trimmed == "rreturn rewind"
        || trimmed.starts_with("rreturn rewind ")
        || trimmed.starts_with("rreturn slot load ")
        || trimmed.starts_with("rreturn trigger rewind");
    let needs_level_4 = is_rollback
        || trimmed == "rreturn slot delete"
        || trimmed.starts_with("rreturn slot delete ")
        || trimmed == "rreturn slots delete"
        || trimmed.starts_with("rreturn slots delete ")
        || trimmed == "rreturn dev"
        || trimmed.starts_with("rreturn dev ");
    let cmd_owned = trimmed.to_owned();
    if let Some(server) = &game.integrated {
        if let Some((allowed, lines)) = server.call(move |srv| {
            let permitted = if needs_level_4 {
                has_level_4_permission(srv, None)
            } else {
                has_cheat_permission(srv, None)
            };
            if !permitted {
                return (false, vec![tr("command.rreturn.no_permission")]);
            }
            (true, execute_rreturn_command(srv, None, &cmd_owned))
        }) {
            if allowed && is_rollback {
                game.close_container();
                game.screen = super::super::Screen::Playing;
                let slot = with_state(|st| st.current_slot.clone());
                apply_client_level_rollback(game, &slot);
            }
            for line in lines {
                game.native_chat(&line);
            }
        }
    } else {
        let permitted = if needs_level_4 {
            has_level_4_permission(game, None)
        } else {
            has_cheat_permission(game, None)
        };
        if !permitted {
            game.native_chat(&tr("command.rreturn.no_permission"));
            return;
        }
        if is_rollback {
            game.close_container();
            game.screen = super::super::Screen::Playing;
        }
        let lines = execute_rreturn_command(game, None, &cmd_owned);
        if is_rollback {
            let slot = with_state(|st| st.current_slot.clone());
            apply_client_level_rollback(game, &slot);
        }
        for line in lines {
            game.native_chat(&line);
        }
    }
}

pub fn intercept_owner_console_command(
    game: &mut Game<'_>,
    command: &str,
) -> ControlFlow<Vec<String>> {
    let trimmed = command.trim().trim_start_matches('/');
    if trimmed.split_whitespace().next() != Some("rreturn") {
        return ControlFlow::Continue(());
    }
    let owner = game.serving.as_ref().map(|s| s.owner);
    let lines = execute_rreturn_command(game, owner, trimmed);
    if let Some(host) = &game.lan
        && let Some(owner_uuid) = owner
        && let Some(g) = host.hub.guests.iter().find(|g| g.uuid == owner_uuid)
    {
        for line in &lines {
            g.send(&C::SystemChat(play::SystemChat {
                content: literal(line),
                overlay: false,
            }));
        }
    }
    ControlFlow::Break(lines)
}

fn handle_scrub_subcommand(game: &mut Game<'_>, args: &[&str], requester: Option<u128>) -> String {
    if !has_cheat_permission(game, requester) {
        return tr("command.rreturn.no_permission");
    }
    let sub = args.first().copied().unwrap_or("status");
    match sub {
        "start" => {
            start_scrub(game);
            "[RReturn] Scrub mode started. Rotate mouse wheel to rewind/advance.".to_owned()
        }
        "step" => {
            let val: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(-1.0);
            on_mouse_scroll(game, [0.0, val], [0, val.round() as i32]);
            if let Some((curr, min_t, max_t)) = get_scrub_info() {
                format!("[RReturn] Scrub cursor at tick {:.2} (range: {}..{})", curr, min_t, max_t)
            } else {
                "[RReturn] Scrub mode not active.".to_owned()
            }
        }
        "commit" => {
            commit_scrub(game);
            "[RReturn] Scrub committed.".to_owned()
        }
        "cancel" => {
            cancel_scrub(game);
            "[RReturn] Scrub cancelled.".to_owned()
        }
        "status" => {
            if let Some((curr, min_t, max_t)) = get_scrub_info() {
                format!(r#"{{"active":true,"current_time":{:.2},"min_tick":{},"anchor_tick":{}}}"#, curr, min_t, max_t)
            } else {
                let range = with_scrub_ring(|r| r.time_range()).unwrap_or((0, 0));
                format!(r#"{{"active":false,"min_tick":{},"max_tick":{}}}"#, range.0, range.1)
            }
        }
        "rewind" => {
            let secs: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5.0);
            start_scrub(game);
            let ticks = secs * 20.0;
            on_mouse_scroll(game, [0.0, -ticks], [0, (-ticks).round() as i32]);
            commit_scrub(game);
            format!("[RReturn] Scrubbed back by {:.1} seconds.", secs)
        }
        _ => "[RReturn] Usage: /rreturn scrub [start|step <N>|commit|cancel|status|rewind <sec>]".to_owned(),
    }
}

pub fn intercept_guest_command(
    game: &mut Game<'_>,
    conn: u32,
    command: &str,
) -> ControlFlow<()> {
    let trimmed = command.trim().trim_start_matches('/');
    if trimmed.split_whitespace().next() != Some("rreturn") {
        return ControlFlow::Continue(());
    }
    let Some(host) = game.lan.as_ref() else {
        return ControlFlow::Continue(());
    };
    let requester = host.hub.guest(conn).map(|g| g.uuid);
    let is_owner = requester.is_some_and(|u| host.owner.as_ref().is_some_and(|o| o.uuid == u));
    let needs_level_4 = trimmed == "rreturn load"
        || trimmed.starts_with("rreturn load ")
        || trimmed == "rreturn rollback"
        || trimmed.starts_with("rreturn rollback ")
        || trimmed == "rreturn rewind"
        || trimmed.starts_with("rreturn rewind ")
        || trimmed == "rreturn slot load"
        || trimmed.starts_with("rreturn slot load ")
        || trimmed == "rreturn slots load"
        || trimmed.starts_with("rreturn slots load ")
        || trimmed == "rreturn slot delete"
        || trimmed.starts_with("rreturn slot delete ")
        || trimmed == "rreturn slots delete"
        || trimmed.starts_with("rreturn slots delete ")
        || trimmed.starts_with("rreturn trigger rewind")
        || trimmed == "rreturn dev"
        || trimmed.starts_with("rreturn dev ");
    let min_op = if needs_level_4 { 4 } else { 2 };
    let has_op = host.hub.guest(conn).is_some_and(|g| {
        g.permission >= min_op || host.hub.ops.get(&g.uuid).copied().unwrap_or(0) >= min_op
    });
    let allowed = has_op || (is_owner && game.allow_commands());
    if !allowed {
        if let Some(g) = host.hub.guest(conn) {
            let msg = tr("command.rreturn.no_permission");
            g.send(&C::SystemChat(play::SystemChat {
                content: literal(&msg),
                overlay: false,
            }));
        }
        return ControlFlow::Break(());
    }
    let lines = execute_rreturn_command(game, requester, trimmed);
    if let Some(host) = game.lan.as_ref()
        && let Some(g) = host.hub.guest(conn)
    {
        for line in lines {
            g.send(&C::SystemChat(play::SystemChat {
                content: literal(&line),
                overlay: false,
            }));
        }
    }
    ControlFlow::Break(())
}