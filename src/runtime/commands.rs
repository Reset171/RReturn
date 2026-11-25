use super::super::{Game, LanHost};
use super::capture::*;
use super::policy::*;
use super::restore::*;
use super::restore_client::*;
use super::scrub::*;
use super::scrub_audio::*;
use super::scrub_hud::scrub_hud_bounds_json;
use super::scrub_perf::*;
use super::scrub_server::*;
use super::scrub_timeline::*;
use super::settings_catalog::UiTab;
use super::settings_ui::{
    request_open_settings_tab, request_open_settings_ui, settings_ui_bounds_json,
};
use super::snapshots::*;
use super::storage::*;
use super::telemetry::*;
use super::types::*;
use super::ui::save_current_settings;
use super::ui_snapshot::*;
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
        "perf" => {
            if words.get(2).copied() == Some("reset") {
                reset_perf();
                "[RReturn] Performance metrics reset.".to_owned()
            } else if words.get(2).copied() == Some("json") {
                get_perf_report_json()
            } else {
                get_perf_report_tree()
            }
        }
        "purge_memory" => {
            clear_all_slots();
            clear_client_snapshots();
            "[RReturn] In-memory snapshots cleared.".to_owned()
        }
        "anchor" => handle_anchor_subcommand(game, &words[2..], requester),
        "player" | "players" => handle_player_subcommand(game, &words[2..], requester),
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
        "ui" | "settings" | "config" | "menu" => {
            match words.get(2).copied() {
                Some("bounds") => settings_ui_bounds_json(),
                Some("rollback") => {
                    request_open_settings_tab(game, Some(UiTab::Rollback));
                    tr("command.rreturn.ui.opened")
                }
                Some("scrub") => {
                    request_open_settings_tab(game, Some(UiTab::Scrub));
                    tr("command.rreturn.ui.opened")
                }
                Some("players" | "anchors") => {
                    request_open_settings_tab(game, Some(UiTab::Players));
                    tr("command.rreturn.ui.opened")
                }
                Some("system") => {
                    request_open_settings_tab(game, Some(UiTab::System));
                    tr("command.rreturn.ui.opened")
                }
                _ => {
                    request_open_settings_ui(game);
                    tr("command.rreturn.ui.opened")
                }
            }
        }
        _ => "[RReturn] Usage: /rreturn <set|load|json|status|slot|policy|metrics|anchor|autosave|scrub|settings|bench|f3|dev>".to_owned(),
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
                with_state_mut(|st| st.current_slot = slot.to_owned());
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
                with_state_mut(|st| st.current_slot = slot.to_owned());
                format!("[RReturn] Triggered checkpoint creation for slot '{}'.", slot)
            } else {
                tr_args("command.rreturn.set.slot_failed", &[slot])
            }
        }
        _ => "[RReturn] Usage: /rreturn trigger <rewind|checkpoint> [slot]".to_owned(),
    }
}

fn resolve_anchor_target(game: &Game<'_>, name: Option<&str>, requester: Option<u128>) -> Option<u128> {
    match name {
        Some(n) => game.lan.as_ref().and_then(|h| {
            h.hub
                .guests
                .iter()
                .find(|g| g.name.eq_ignore_ascii_case(n))
                .map(|g| g.uuid)
        }),
        None => requester.or_else(|| {
            game.lan.as_ref().and_then(|h| {
                h.owner
                    .as_ref()
                    .map(|o| o.uuid)
                    .or_else(|| h.hub.guests.iter().find(|g| g.placed).map(|g| g.uuid))
            })
        }),
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
            let target = resolve_anchor_target(game, args.get(1).copied(), requester);
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
            if args.get(1).is_some_and(|s| *s == "all" || *s == "*") {
                with_state_mut(|st| st.anchor.pinned.clear());
                save_current_settings(game);
                return "[RReturn] Cleared all pinned key players.".to_owned();
            }
            let target = resolve_anchor_target(game, args.get(1).copied(), requester);
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
        "clear" => {
            with_state_mut(|st| st.anchor.pinned.clear());
            save_current_settings(game);
            "[RReturn] Cleared all pinned key players.".to_owned()
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

fn resolve_player_uuid_and_name(
    game: &Game<'_>,
    token: &str,
    requester: Option<u128>,
) -> Option<(u128, String)> {
    if token.eq_ignore_ascii_case("@s") || token.eq_ignore_ascii_case("self") {
        if let Some(u) = resolve_anchor_target(game, None, requester) {
            let name = game
                .lan
                .as_ref()
                .and_then(|h| h.hub.guests.iter().find(|g| g.uuid == u).map(|g| g.name.clone()))
                .or_else(|| {
                    with_state(|st| {
                        st.online_players
                            .iter()
                            .find(|p| p.uuid == u)
                            .map(|p| p.name.clone())
                    })
                })
                .unwrap_or_default();
            return Some((u, name));
        }
    }
    if let Some(g) = game
        .lan
        .as_ref()
        .and_then(|h| h.hub.guests.iter().find(|g| g.name.eq_ignore_ascii_case(token)))
    {
        return Some((g.uuid, g.name.clone()));
    }
    let clean_hex = token.trim().replace('-', "");
    if clean_hex.len() == 32
        && let Ok(u) = u128::from_str_radix(&clean_hex, 16)
    {
        let name = game
            .lan
            .as_ref()
            .and_then(|h| h.hub.guests.iter().find(|g| g.uuid == u).map(|g| g.name.clone()))
            .or_else(|| {
                with_state(|st| {
                    st.player_overrides
                        .get(&u)
                        .map(|p| p.name.clone())
                        .or_else(|| st.online_players.iter().find(|p| p.uuid == u).map(|p| p.name.clone()))
                })
            })
            .unwrap_or_default();
        return Some((u, name));
    }
    with_state(|st| {
        st.online_players
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(token))
            .map(|p| (p.uuid, p.name.clone()))
            .or_else(|| {
                st.player_overrides
                    .iter()
                    .find(|(_, p)| p.name.eq_ignore_ascii_case(token))
                    .map(|(&u, p)| (u, p.name.clone()))
            })
    })
}

pub fn handle_player_subcommand(
    game: &mut Game<'_>,
    args: &[&str],
    requester: Option<u128>,
) -> String {
    let players_now = collect_online_players(game);
    if !players_now.is_empty() {
        with_state_mut(|st| st.online_players = players_now);
    }
    let sub = args.first().copied().unwrap_or("list");
    match sub {
        "list" => {
            let list = with_state(|st| {
                let mut rows = Vec::new();
                for p in &st.online_players {
                    let prof = st.player_overrides.get(&p.uuid);
                    rows.push(serde_json::json!({
                        "uuid": format!("{:032x}", p.uuid),
                        "name": p.name,
                        "online": true,
                        "is_owner": p.is_owner,
                        "rollback": prof.map(|pr| pr.rollback.to_sparse_json()).unwrap_or_else(|| serde_json::json!({})),
                        "scrub": prof.map(|pr| pr.scrub.to_sparse_json()).unwrap_or_else(|| serde_json::json!({})),
                    }));
                }
                for (&uuid, prof) in &st.player_overrides {
                    if st.online_players.iter().any(|p| p.uuid == uuid) {
                        continue;
                    }
                    rows.push(serde_json::json!({
                        "uuid": format!("{:032x}", uuid),
                        "name": prof.name,
                        "online": false,
                        "is_owner": false,
                        "rollback": prof.rollback.to_sparse_json(),
                        "scrub": prof.scrub.to_sparse_json(),
                    }));
                }
                serde_json::Value::Array(rows)
            });
            serde_json::to_string_pretty(&list).unwrap_or_else(|_| "[]".to_owned())
        }
        "get" => {
            let target_tok = args.get(1).copied().unwrap_or("@s");
            let scope = args.get(2).copied().unwrap_or("rollback").to_ascii_lowercase();
            let Some((uuid, name)) = resolve_player_uuid_and_name(game, target_tok, requester) else {
                return format!("[RReturn] Player '{}' not found.", target_tok);
            };
            let val = with_state(|st| {
                let is_scrub = scope == "scrub";
                let base = if is_scrub { st.scrub_policy } else { st.policy };
                let empty = PlayerPolicyOverrides::default();
                let ov = st
                    .player_overrides
                    .get(&uuid)
                    .map(|p| if is_scrub { &p.scrub } else { &p.rollback })
                    .unwrap_or(&empty);
                serde_json::json!({
                    "uuid": format!("{:032x}", uuid),
                    "name": name,
                    "scope": if is_scrub { "scrub" } else { "rollback" },
                    "data": ov.to_full_json(&base),
                })
            });
            serde_json::to_string_pretty(&val).unwrap_or_else(|_| "{}".to_owned())
        }
        "set" => {
            let target_tok = args.get(1).copied().unwrap_or("");
            let field = args.get(2).copied().unwrap_or("");
            let val_tok = args.get(3).copied().unwrap_or("");
            let scope = args.get(4).copied().unwrap_or("rollback").to_ascii_lowercase();
            if target_tok.is_empty() || field.is_empty() || val_tok.is_empty() {
                return "[RReturn] Usage: /rreturn player set <name|@s> <field|all> <global|true|false> [rollback|scrub|both]".to_owned();
            }
            let Some((uuid, name)) = resolve_player_uuid_and_name(game, target_tok, requester) else {
                return format!("[RReturn] Player '{}' not found.", target_tok);
            };
            let Some(ov_val) = OverrideValue::from_token(val_tok) else {
                return format!("[RReturn] Invalid override '{}': expected global, true, or false.", val_tok);
            };
            let set_rb = scope != "scrub";
            let set_sc = scope == "scrub" || scope == "both" || scope == "all";
            let ok = with_state_mut(|st| {
                let entry = st.player_overrides.entry(uuid).or_insert_with(|| PlayerProfileOverrides {
                    name: name.clone(),
                    ..Default::default()
                });
                if !name.is_empty() {
                    entry.name = name.clone();
                }
                let mut valid = true;
                if field.eq_ignore_ascii_case("all") {
                    if set_rb {
                        entry.rollback.set_all(ov_val);
                    }
                    if set_sc {
                        entry.scrub.set_all(ov_val);
                    }
                } else {
                    if set_rb {
                        valid &= entry.rollback.set(field, ov_val);
                    }
                    if set_sc {
                        valid &= entry.scrub.set(field, ov_val);
                    }
                }
                if entry.is_empty() {
                    st.player_overrides.remove(&uuid);
                }
                valid
            });
            if !ok {
                return format!("[RReturn] Unknown player policy field '{}'.", field);
            }
            save_current_settings(game);
            format!(
                "[RReturn] Player '{}' ({}) override '{}' set cleanly.",
                if name.is_empty() { target_tok } else { &name },
                scope,
                field
            )
        }
        "reset" => {
            let target_tok = args.get(1).copied().unwrap_or("all");
            let scope = args.get(2).copied().unwrap_or("both").to_ascii_lowercase();
            if target_tok == "all" || target_tok == "*" {
                with_state_mut(|st| {
                    if scope == "both" || scope == "all" {
                        st.player_overrides.clear();
                    } else {
                        for p in st.player_overrides.values_mut() {
                            if scope == "scrub" {
                                p.scrub.set_all(OverrideValue::Global);
                            } else {
                                p.rollback.set_all(OverrideValue::Global);
                            }
                        }
                        st.player_overrides.retain(|_, p| !p.is_empty());
                    }
                });
                save_current_settings(game);
                return "[RReturn] Player policy overrides cleared to global.".to_owned();
            }
            let Some((uuid, name)) = resolve_player_uuid_and_name(game, target_tok, requester) else {
                return format!("[RReturn] Player '{}' not found.", target_tok);
            };
            with_state_mut(|st| {
                if let Some(p) = st.player_overrides.get_mut(&uuid) {
                    if scope != "scrub" {
                        p.rollback.set_all(OverrideValue::Global);
                    }
                    if scope == "scrub" || scope == "both" || scope == "all" {
                        p.scrub.set_all(OverrideValue::Global);
                    }
                    if p.is_empty() {
                        st.player_overrides.remove(&uuid);
                    }
                }
            });
            save_current_settings(game);
            format!(
                "[RReturn] Cleared overrides for player '{}'.",
                if name.is_empty() { target_tok } else { &name }
            )
        }
        _ => "[RReturn] Usage: /rreturn player <list|get|set|reset> ...".to_owned(),
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

fn command_save_slot(trimmed: &str) -> Option<&str> {
    let mut it = trimmed.split_whitespace();
    if it.next()? != "rreturn" {
        return None;
    }
    match it.next()? {
        "set" | "save" => {
            let slot = it.next().unwrap_or("default");
            is_valid_slot_name(slot).then_some(slot)
        }
        "slot" | "slots" if it.next() == Some("save") => {
            let slot = it.next().unwrap_or("default");
            is_valid_slot_name(slot).then_some(slot)
        }
        "trigger" if matches!(it.next(), Some("checkpoint" | "save")) => {
            let slot = it.next().unwrap_or("default");
            is_valid_slot_name(slot).then_some(slot)
        }
        _ => None,
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

    let is_dev_cmd = trimmed.starts_with("rreturn dev");

    if matches!(
        trimmed.split_whitespace().nth(1),
        Some("ui" | "settings" | "config" | "menu")
    ) {
        match trimmed.split_whitespace().nth(2) {
            Some("bounds") => {
                let bounds = settings_ui_bounds_json();
                game.native_chat(&bounds);
            }
            Some("state") => {
                let json = serde_json::to_string(&ui_state_json(game))
                    .unwrap_or_else(|_| "{}".to_owned());
                game.native_chat(&json);
            }
            Some("scrub_hud") | Some("hud") => {
                let json = scrub_hud_bounds_json();
                game.native_chat(&json);
            }
            Some("rollback") => {
                request_open_settings_tab(game, Some(UiTab::Rollback));
                mod_chat(game, &tr("command.rreturn.ui.opened"));
            }
            Some("scrub") => {
                request_open_settings_tab(game, Some(UiTab::Scrub));
                mod_chat(game, &tr("command.rreturn.ui.opened"));
            }
            Some("players" | "anchors") => {
                request_open_settings_tab(game, Some(UiTab::Players));
                mod_chat(game, &tr("command.rreturn.ui.opened"));
            }
            Some("system") => {
                request_open_settings_tab(game, Some(UiTab::System));
                mod_chat(game, &tr("command.rreturn.ui.opened"));
            }
            _ => {
                request_open_settings_ui(game);
                mod_chat(game, &tr("command.rreturn.ui.opened"));
            }
        }
        return;
    }

    if trimmed.starts_with("rreturn scrub") {
        let permitted = has_level_4_permission(game, None);
        if !permitted {
            mod_chat(game, &tr("command.rreturn.no_permission"));
            return;
        }
        let lines = execute_rreturn_command(game, None, trimmed);
        for line in lines {
            mod_chat(game, &line);
        }
        return;
    }

    let is_rollback = trimmed == "rreturn load"
        || trimmed.starts_with("rreturn load ")
        || trimmed == "rreturn rollback"
        || trimmed.starts_with("rreturn rollback ")
        || trimmed == "rreturn rewind"
        || trimmed.starts_with("rreturn rewind ")
        || trimmed.starts_with("rreturn slot load ")
        || trimmed.starts_with("rreturn trigger rewind");
    let ui_save = command_save_slot(trimmed).map(|s| (s.to_owned(), capture_client_ui_snapshot(game)));
    let needs_level_4 = is_rollback
        || trimmed == "rreturn slot delete"
        || trimmed.starts_with("rreturn slot delete ")
        || trimmed == "rreturn slots delete"
        || trimmed.starts_with("rreturn slots delete ")
        || trimmed == "rreturn dev"
        || trimmed.starts_with("rreturn dev ");
    let cmd_owned = trimmed.to_owned();
    if let Some(server) = &game.integrated {
        if let Some((allowed, srv_root, lines)) = server.call(move |srv| {
            let root = srv.world_root.clone();
            let permitted = if needs_level_4 {
                has_level_4_permission(srv, None)
            } else {
                has_cheat_permission(srv, None)
            };
            if !permitted {
                return (false, root, vec![tr("command.rreturn.no_permission")]);
            }
            (true, root, execute_rreturn_command(srv, None, &cmd_owned))
        }) {
            set_active_ui_world_root(&srv_root);
            if allowed {
                if let Some((slot_name, ui_snap)) = ui_save
                    && !lines.iter().any(|l| {
                        l.contains("Failed")
                            || l.contains("Не удалось")
                            || l.contains("Invalid")
                            || l.contains("permission")
                            || l.contains("прав")
                    })
                {
                    save_client_ui_slot(&srv_root, &slot_name, ui_snap);
                }
                if is_rollback {
                    close_client_container_local(game);
                    game.screen = super::super::Screen::Playing;
                    let slot = with_state(|st| st.current_slot.clone());
                    apply_client_rollback_with_ui(game, &slot, true);
                }
            }
            for line in lines {
                if is_dev_cmd {
                    game.native_chat(&line);
                } else {
                    mod_chat(game, &line);
                }
            }
        }
    } else {
        let permitted = if needs_level_4 {
            has_level_4_permission(game, None)
        } else {
            has_cheat_permission(game, None)
        };
        if !permitted {
            if is_dev_cmd {
                game.native_chat(&tr("command.rreturn.no_permission"));
            } else {
                mod_chat(game, &tr("command.rreturn.no_permission"));
            }
            return;
        }
        if is_rollback {
            close_client_container_local(game);
            game.screen = super::super::Screen::Playing;
        }
        let lines = execute_rreturn_command(game, None, &cmd_owned);
        if let Some((slot_name, ui_snap)) = ui_save
            && !lines.iter().any(|l| {
                l.contains("Failed")
                    || l.contains("Не удалось")
                    || l.contains("Invalid")
                    || l.contains("permission")
                    || l.contains("прав")
            })
        {
            let root = resolve_ui_world_root(&game.world_root);
            save_client_ui_slot(&root, &slot_name, ui_snap);
        }
        if is_rollback {
            let slot = with_state(|st| st.current_slot.clone());
            apply_client_rollback_with_ui(game, &slot, true);
        }
        for line in lines {
            if is_dev_cmd {
                game.native_chat(&line);
            } else {
                mod_chat(game, &line);
            }
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
    let is_dev_cmd = trimmed.starts_with("rreturn dev");
    let should_send = with_state(|st| st.dev_mode) || is_dev_cmd;
    if let Some(host) = &game.lan
        && let Some(owner_uuid) = owner
        && let Some(g) = host.hub.guests.iter().find(|g| g.uuid == owner_uuid)
    {
        if should_send {
            for line in &lines {
                g.send(&C::SystemChat(play::SystemChat {
                    content: literal(line),
                    overlay: false,
                }));
            }
        }
    }
    ControlFlow::Break(lines)
}

fn handle_scrub_subcommand(game: &mut Game<'_>, args: &[&str], requester: Option<u128>) -> String {
    if !has_level_4_permission(game, requester) {
        return tr("command.rreturn.no_permission");
    }
    let sub = args.first().copied().unwrap_or("status");
    match sub {
        "on" | "enable" | "1" | "true" => {
            with_state_mut(|st| st.scrub_enabled = true);
            if game.audio.capture.is_none() {
                game.audio.capture = Some(Vec::new());
            }
            game.audio.record_heard = true;
            if let Some(server) = &game.integrated {
                let _ = server.call(|_| {
                    with_state_mut(|st| st.scrub_enabled = true);
                });
            }
            save_current_settings(game);
            let state_str = tr("ui.rreturn.on");
            tr_args("command.rreturn.scrub.status", &[&state_str])
        }
        "off" | "disable" | "0" | "false" => {
            if is_scrubbing() {
                cancel_scrub(game);
            }
            with_state_mut(|st| st.scrub_enabled = false);
            with_scrub_ring_mut(|r| r.clear());
            clear_server_scrub();
            clear_reverse_audio_cache();
            game.audio.capture = None;
            game.audio.record_heard = false;
            if let Some(server) = &game.integrated {
                let _ = server.call(|_| {
                    with_state_mut(|st| st.scrub_enabled = false);
                    clear_server_scrub();
                });
            }
            save_current_settings(game);
            let state_str = tr("ui.rreturn.off");
            tr_args("command.rreturn.scrub.status", &[&state_str])
        }
        "toggle" => {
            let next = with_state(|st| !st.scrub_enabled);
            if !next && is_scrubbing() {
                cancel_scrub(game);
            }
            with_state_mut(|st| st.scrub_enabled = next);
            if !next {
                with_scrub_ring_mut(|r| r.clear());
                clear_server_scrub();
                clear_reverse_audio_cache();
                game.audio.capture = None;
                game.audio.record_heard = false;
            } else {
                if game.audio.capture.is_none() {
                    game.audio.capture = Some(Vec::new());
                }
                game.audio.record_heard = true;
            }
            if let Some(server) = &game.integrated {
                let _ = server.call(move |_| {
                    with_state_mut(|st| st.scrub_enabled = next);
                    if !next {
                        clear_server_scrub();
                    }
                });
            }
            save_current_settings(game);
            let state_str = if next {
                tr("ui.rreturn.on")
            } else {
                tr("ui.rreturn.off")
            };
            tr_args("command.rreturn.scrub.status", &[&state_str])
        }
        "start" => {
            if !with_state(|st| st.scrub_enabled) {
                return tr("command.rreturn.scrub.disabled");
            }
            start_scrub(game);
            "[RReturn] Scrub mode started. Rotate mouse wheel to rewind/advance.".to_owned()
        }
        "step" => {
            if !with_state(|st| st.scrub_enabled) {
                return tr("command.rreturn.scrub.disabled");
            }
            let val: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(-1.0);
            on_mouse_scroll(game, [0.0, val], [0, val.round() as i32]);
            snap_scrub_time_to_target();
            on_frame_render_pre(game, 0.0, 0.0);
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
            let enabled = with_state(|st| st.scrub_enabled);
            let swinging = game.swing.current.is_some();
            let swing_anim = game.swing.animation;
            let p_count = game.particle_count();
            let s_count = with_scrub_ring(|r| r.frames.iter().map(|f| f.sounds.len()).sum::<usize>());
            let fov = game.view.fov_modifier;
            let walk_dist = game.avatar.walk_dist;
            let day_time = game.world.day_time;
            let rain_level = game.world.weather.rain_level.1;
            let srv_range = if let Some(server) = &game.integrated {
                server
                    .call(|_| server_scrub_time_range())
                    .flatten()
                    .unwrap_or((0, 0))
            } else {
                server_scrub_time_range().unwrap_or((0, 0))
            };
            if let Some((curr, min_t, max_t)) = get_scrub_info() {
                format!(
                    r#"{{"enabled":{},"active":true,"current_time":{:.2},"min_tick":{},"anchor_tick":{},"server_min_tick":{},"server_max_tick":{},"day_time":{},"rain_level":{:.3},"swinging":{},"swing_anim":{:.2},"particles":{},"sounds":{},"fov_modifier":{:.3},"walk_dist":{:.3}}}"#,
                    enabled, curr, min_t, max_t, srv_range.0, srv_range.1, day_time, rain_level, swinging, swing_anim, p_count, s_count, fov, walk_dist
                )
            } else {
                let range = with_scrub_ring(|r| r.time_range()).unwrap_or((0, 0));
                format!(
                    r#"{{"enabled":{},"active":false,"min_tick":{},"max_tick":{},"server_min_tick":{},"server_max_tick":{},"day_time":{},"rain_level":{:.3},"swinging":{},"swing_anim":{:.2},"particles":{},"sounds":{},"fov_modifier":{:.3},"walk_dist":{:.3}}}"#,
                    enabled, range.0, range.1, srv_range.0, srv_range.1, day_time, rain_level, swinging, swing_anim, p_count, s_count, fov, walk_dist
                )
            }
        }
        "seek" => {
            if !with_state(|st| st.scrub_enabled) {
                return tr("command.rreturn.scrub.disabled");
            }
            let secs: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(-1.0);
            if !is_scrubbing() {
                start_scrub(game);
            }
            step_scrub_ticks(secs * 20.0);
            snap_scrub_time_to_target();
            on_frame_render_pre(game, 0.0, 0.0);
            if let Some((curr, min_t, max_t)) = get_scrub_info() {
                format!(
                    "[RReturn] Scrubbed preview by {:.1}s to tick {:.2} ({}..{}).",
                    secs, curr, min_t, max_t
                )
            } else {
                "[RReturn] Scrub mode not active.".to_owned()
            }
        }
        "seek_ticks" => {
            if !with_state(|st| st.scrub_enabled) {
                return tr("command.rreturn.scrub.disabled");
            }
            let ticks: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(-20.0);
            if !is_scrubbing() {
                start_scrub(game);
            }
            step_scrub_ticks(ticks);
            snap_scrub_time_to_target();
            on_frame_render_pre(game, 0.0, 0.0);
            if let Some((curr, min_t, max_t)) = get_scrub_info() {
                format!(
                    "[RReturn] Scrubbed preview by {:.0} ticks to tick {:.2} ({}..{}).",
                    ticks, curr, min_t, max_t
                )
            } else {
                "[RReturn] Scrub mode not active.".to_owned()
            }
        }
        "rewind" => {
            if !with_state(|st| st.scrub_enabled) {
                return tr("command.rreturn.scrub.disabled");
            }
            let secs: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5.0);
            start_scrub(game);
            let ticks = secs * 20.0;
            step_scrub_ticks(-ticks);
            snap_scrub_time_to_target();
            on_frame_render_pre(game, 0.0, 0.0);
            commit_scrub(game);
            format!("[RReturn] Scrubbed back by {:.1} seconds.", secs)
        }
        "rewind_ticks" => {
            if !with_state(|st| st.scrub_enabled) {
                return tr("command.rreturn.scrub.disabled");
            }
            let ticks: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20.0);
            start_scrub(game);
            step_scrub_ticks(-ticks);
            snap_scrub_time_to_target();
            on_frame_render_pre(game, 0.0, 0.0);
            commit_scrub(game);
            format!("[RReturn] Scrubbed back by {:.0} ticks.", ticks)
        }
        "policy" => {
            let psub = args.get(1).copied().unwrap_or("get");
            match psub {
                "get" => {
                    let pol = with_state(|st| st.scrub_policy.to_json());
                    serde_json::to_string_pretty(&pol).unwrap_or_else(|_| "{}".to_owned())
                }
                "set" => {
                    let key = args.get(2).copied().unwrap_or("");
                    let val = match args.get(3).copied().unwrap_or("").to_ascii_lowercase().as_str() {
                        "true" | "1" | "on" => Some(true),
                        "false" | "0" | "off" => Some(false),
                        _ => None,
                    };
                    if key.is_empty() || val.is_none() {
                        return "[RReturn] Usage: /rreturn scrub policy set <field> <true|false>".to_owned();
                    }
                    let ok = with_state_mut(|st| st.scrub_policy.set_field(key, val.unwrap()));
                    if ok {
                        save_current_settings(game);
                        format!("[RReturn] Scrub policy '{}' set to {}.", key, val.unwrap())
                    } else {
                        format!("[RReturn] Unknown scrub policy field '{}'.", key)
                    }
                }
                "reset" => {
                    with_state_mut(|st| st.scrub_policy = RollbackPolicy::default());
                    save_current_settings(game);
                    "[RReturn] Scrub policy reset to deterministic defaults.".to_owned()
                }
                _ => "[RReturn] Usage: /rreturn scrub policy <get|set|reset> [field] [value]".to_owned(),
            }
        }
        _ => "[RReturn] Usage: /rreturn scrub [on|off|toggle|start|step <N>|commit|cancel|status|rewind <sec>|policy ...]".to_owned(),
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
    let is_dev_cmd = trimmed.starts_with("rreturn dev");
    let should_send = with_state(|st| st.dev_mode) || is_dev_cmd;
    let allowed = has_op || (is_owner && game.allow_commands());
    if !allowed {
        if should_send {
            if let Some(g) = host.hub.guest(conn) {
                let msg = tr("command.rreturn.no_permission");
                g.send(&C::SystemChat(play::SystemChat {
                    content: literal(&msg),
                    overlay: false,
                }));
            }
        }
        return ControlFlow::Break(());
    }
    let lines = execute_rreturn_command(game, requester, trimmed);
    if should_send {
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
    }
    ControlFlow::Break(())
}