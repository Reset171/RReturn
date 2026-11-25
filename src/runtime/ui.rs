use super::super::Game;
use super::scrub_audio::clear_reverse_audio_cache;
use super::scrub_server::clear_server_scrub;
use super::scrub_timeline::with_scrub_ring_mut;
use super::policy::RollbackPolicy;
use super::types::*;
use std::{
    fs,
    path::Path,
};

pub fn sync_scrub_runtime(game: &mut Game<'_>, enabled: bool) {
    if !enabled {
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
            with_state_mut(|st| st.scrub_enabled = enabled);
            if !enabled {
                clear_server_scrub();
            }
        });
    }
}

fn apply_nested_policy_json(rb: &serde_json::Value, p: &mut RollbackPolicy) {
    if !rb.is_object() {
        return;
    }
    let b = |val: &serde_json::Value, d: bool| val.as_bool().unwrap_or(d);
    p.blocks = b(&rb["blocks"], p.blocks);
    p.biomes = b(&rb["biomes"], p.biomes);
    p.block_entities = b(&rb["block_entities"], p.block_entities);
    p.block_ticks = b(&rb["block_ticks"], p.block_ticks);
    p.fluid_ticks = b(&rb["fluid_ticks"], p.fluid_ticks);
    p.block_events = b(&rb["block_events"], p.block_events);
    let ent = &rb["entities"];
    if ent.is_object() {
        p.entities_presence = b(&ent["presence"], p.entities_presence);
        p.dropped_items = b(&ent["dropped_items"], p.dropped_items);
        p.experience_orbs = b(&ent["experience_orbs"], p.experience_orbs);
    }
    let pl = &rb["players"];
    if pl.is_object() {
        p.player_position = b(&pl["position"], p.player_position);
        p.player_motion = b(&pl["motion"], p.player_motion);
        p.player_health = b(&pl["health"], p.player_health);
        p.player_hunger = b(&pl["hunger"], p.player_hunger);
        p.player_experience = b(&pl["experience"], p.player_experience);
        p.player_inventory = b(&pl["inventory"], p.player_inventory);
        p.player_ender_chest = b(&pl["ender_chest"], p.player_ender_chest);
        p.player_potion_effects = b(&pl["potion_effects"], p.player_potion_effects);
        p.player_cooldowns = b(&pl["cooldowns"], p.player_cooldowns);
        p.player_abilities = b(&pl["abilities"], p.player_abilities);
        p.player_recipe_book = b(&pl["recipe_book"], p.player_recipe_book);
        p.player_spawn_point = b(&pl["spawn_point"], p.player_spawn_point);
        p.player_stats = b(&pl["stats"], p.player_stats);
        if let Some(v) = pl.get("reset_new_player_inventory").and_then(|v| v.as_bool()) {
            p.reset_new_player_inventory = v;
        } else if let Some(v) = pl
            .get("preserve_new_player_inventory")
            .and_then(|v| v.as_bool())
        {
            p.reset_new_player_inventory = !v;
        }
    }
    let w = &rb["world"];
    if w.is_object() {
        p.world_time = b(&w["time"], p.world_time);
        p.world_weather = b(&w["weather"], p.world_weather);
        p.world_raids = b(&w["raids"], p.world_raids);
        p.world_dragon_fight = b(&w["dragon_fight"], p.world_dragon_fight);
        p.world_scoreboard = b(&w["scoreboard"], p.world_scoreboard);
        p.world_level_rng = b(&w["level_rng"], p.world_level_rng);
        p.world_difficulty = b(&w["difficulty"], p.world_difficulty);
        p.world_spawn = b(&w["world_spawn"], p.world_spawn);
        let sg = &w["server_globals"];
        if sg.is_object() {
            p.server_bossbars = b(&sg["bossbars"], p.server_bossbars);
            p.server_forced_chunks = b(&sg["forced_chunks"], p.server_forced_chunks);
            p.server_world_border = b(&sg["world_border"], p.server_world_border);
            p.server_game_rules = b(&sg["game_rules"], p.server_game_rules);
        }
    }
}

pub fn load_saved_settings(game: &Game<'_>) {
    let parse_num = |s: &str| -> Option<i32> {
        s.parse::<f64>().ok().map(|v| v as i32).or_else(|| s.parse::<i32>().ok())
    };
    let auto_save = game
        .setting(MOD_ID, "auto_save")
        .map(|v| v != "false");
    let adaptive = game
        .setting(MOD_ID, "adaptive_mode")
        .map(|v| v != "false");
    let interval = game
        .setting(MOD_ID, "fixed_interval_sec")
        .and_then(|v| parse_num(&v));
    let mode = game
        .setting(MOD_ID, "anchor_mode")
        .map(|v| AnchorMode::by_id(&v));
    let cooldown = game
        .setting(MOD_ID, "rollback_cooldown_sec")
        .and_then(|v| parse_num(&v));
    let boss_save = game
        .setting(MOD_ID, "save_on_boss_defeat")
        .map(|v| v != "false");
    let logs = game.setting(MOD_ID, "logs").map(|v| v == "true");
    let f3_hitboxes = game
        .setting(MOD_ID, "f3_loop_hitboxes")
        .map(|v| v != "false");
    let dev_mode = game
        .setting(MOD_ID, "dev_mode")
        .map(|v| v == "true");
    let scrub_enabled = game
        .setting(MOD_ID, "scrub_enabled")
        .map(|v| v == "true" || v == "1");

    let json_cfg = fs::read_to_string(Path::new("config").join("rreturn.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

    with_state_mut(|st| {
        if let Some(v) = &json_cfg {
            let b = |val: &serde_json::Value, d: bool| val.as_bool().unwrap_or(d);
            apply_nested_policy_json(&v["checkpointPolicy"]["rollback"], &mut st.policy);
            let av = &v["autosave"];
            if av.is_object() {
                st.auto_save = b(&av["auto_save"], st.auto_save);
                st.adaptive_mode = b(&av["adaptive_mode"], st.adaptive_mode);
                st.use_random_interval = b(&av["use_random_interval"], st.use_random_interval);
                if let Some(s) = av["fixed_interval_sec"].as_i64() {
                    st.fixed_interval_sec = (s as i32).max(5);
                    st.current_interval_ticks = i64::from(st.fixed_interval_sec) * 20;
                }
                if let Some(s) = av["random_min_sec"].as_i64() {
                    st.random_min_sec = (s as i32).max(5);
                }
                if let Some(s) = av["random_max_sec"].as_i64() {
                    st.random_max_sec = (s as i32).max(st.random_min_sec + 1);
                }
                if let Some(s) = av["adaptive_interval_sec"].as_i64() {
                    st.adaptive_interval_sec = (s as i32).max(60);
                }
            }
            let ad = &v["adaptive"];
            if ad.is_object() {
                if let Some(gap) = ad["min_save_gap_seconds"].as_i64() {
                    st.min_save_gap_sec = (gap as i32).max(5);
                }
                if let Some(rel) = ad["relax_time_seconds"].as_i64() {
                    st.relax_time_sec = (rel as i32).max(1);
                }
                if let Some(pct) = ad["post_combat_save_percent"].as_i64() {
                    st.post_combat_save_pct = (pct as i32).clamp(0, 100);
                }
                st.save_on_boss_defeat = b(&ad["save_on_boss_defeat"], st.save_on_boss_defeat);
            }
            let anc = &v["anchor"];
            if anc.is_object() {
                if let Some(m) = anc["mode"].as_str() {
                    st.anchor.mode = AnchorMode::by_id(m);
                }
                if let Some(rot) = anc["rotation_seconds"].as_i64() {
                    st.anchor.rotation_seconds = (rot as i32).max(10);
                }
                if let Some(cd) = anc["rollback_cooldown_seconds"].as_i64() {
                    st.anchor.rollback_cooldown_seconds = (cd as i32).max(0);
                }
                if let Some(pins) = anc["pinned"].as_array() {
                    st.anchor.pinned = pins
                        .iter()
                        .filter_map(|x| x.as_str())
                        .filter_map(|s| u128::from_str_radix(s.trim(), 16).ok())
                        .collect();
                }
            }
            let sc = &v["scrub"];
            if sc.is_object() {
                if let Some(en) = sc["enabled"].as_bool() {
                    st.scrub_enabled = en;
                }
                if let Some(hist) = sc["history_seconds"].as_i64() {
                    st.scrub_history_seconds = (hist as i32).clamp(30, 1200);
                }
                if let Some(sens) = sc["wheel_sensitivity"].as_i64() {
                    st.scrub_wheel_sensitivity = (sens as i32).clamp(5, 40);
                }
                st.scrub_reverse_audio = b(&sc["reverse_audio"], st.scrub_reverse_audio);
                st.scrub_show_hud = b(&sc["show_hud"], st.scrub_show_hud);
                apply_nested_policy_json(&sc["policy"], &mut st.scrub_policy);
            } else if let Some(en) = v["scrub_enabled"].as_bool() {
                st.scrub_enabled = en;
            }
            let dbg = &v["debug"];
            if dbg.is_object() {
                st.f3_loop_hitboxes = b(&dbg["f3_loop_hitboxes"], st.f3_loop_hitboxes);
                st.logs = b(&dbg["logs"], st.logs);
                st.dev_mode = b(&dbg["dev_mode"], st.dev_mode);
            }
            if let Some(po) = v["playerOverrides"].as_object() {
                st.player_overrides.clear();
                for (uuid_str, entry) in po {
                    let clean_hex = uuid_str.trim().replace('-', "");
                    let Ok(uuid) = u128::from_str_radix(&clean_hex, 16) else {
                        continue;
                    };
                    let name = entry["name"].as_str().unwrap_or("").to_owned();
                    let rollback = PlayerPolicyOverrides::from_json(&entry["rollback"]);
                    let scrub = PlayerPolicyOverrides::from_json(&entry["scrub"]);
                    let profile = PlayerProfileOverrides { name, rollback, scrub };
                    if !profile.is_empty() {
                        st.player_overrides.insert(uuid, profile);
                    }
                }
            }
        }
        if let Some(v) = auto_save {
            st.auto_save = v;
        }
        if let Some(v) = adaptive {
            st.adaptive_mode = v;
        }
        if let Some(v) = interval {
            st.fixed_interval_sec = v.max(5);
            st.current_interval_ticks = i64::from(st.fixed_interval_sec) * 20;
        }
        if let Some(v) = mode {
            st.anchor.mode = v;
        }
        if let Some(v) = cooldown {
            st.anchor.rollback_cooldown_seconds = v.max(0);
        }
        if let Some(v) = boss_save {
            st.save_on_boss_defeat = v;
        }
        if let Some(v) = logs {
            st.logs = v;
        }
        if let Some(v) = f3_hitboxes {
            st.f3_loop_hitboxes = v;
        }
        if let Some(v) = dev_mode {
            st.dev_mode = v;
        } else if std::env::var_os("RRETURN_DEV").is_some_and(|v| v == "1" || v == "true") {
            st.dev_mode = true;
        }
        if let Some(v) = scrub_enabled {
            st.scrub_enabled = v;
        }
        sync_scrub_atomics_from_state(st);
    });

    if !with_state(|st| st.scrub_enabled) {
        with_scrub_ring_mut(|r| r.clear());
        clear_server_scrub();
        clear_reverse_audio_cache();
    }
}

fn policy_to_nested_json(p: &RollbackPolicy) -> serde_json::Value {
    serde_json::json!({
        "blocks": p.blocks,
        "biomes": p.biomes,
        "block_entities": p.block_entities,
        "block_ticks": p.block_ticks,
        "fluid_ticks": p.fluid_ticks,
        "block_events": p.block_events,
        "entities": {
            "presence": p.entities_presence,
            "dropped_items": p.dropped_items,
            "experience_orbs": p.experience_orbs,
        },
        "players": {
            "position": p.player_position,
            "motion": p.player_motion,
            "health": p.player_health,
            "hunger": p.player_hunger,
            "experience": p.player_experience,
            "inventory": p.player_inventory,
            "ender_chest": p.player_ender_chest,
            "potion_effects": p.player_potion_effects,
            "cooldowns": p.player_cooldowns,
            "abilities": p.player_abilities,
            "recipe_book": p.player_recipe_book,
            "spawn_point": p.player_spawn_point,
            "stats": p.player_stats,
            "reset_new_player_inventory": p.reset_new_player_inventory,
        },
        "world": {
            "time": p.world_time,
            "weather": p.world_weather,
            "raids": p.world_raids,
            "dragon_fight": p.world_dragon_fight,
            "scoreboard": p.world_scoreboard,
            "level_rng": p.world_level_rng,
            "difficulty": p.world_difficulty,
            "world_spawn": p.world_spawn,
            "server_globals": {
                "bossbars": p.server_bossbars,
                "forced_chunks": p.server_forced_chunks,
                "world_border": p.server_world_border,
                "game_rules": p.server_game_rules,
            }
        }
    })
}

pub fn save_current_settings(game: &Game<'_>) {
    let snapshot = with_state(|st| {
        sync_scrub_atomics_from_state(st);
        let pinned_hex: Vec<String> = st.anchor.pinned.iter().map(|u| format!("{:032x}", u)).collect();
        let mut overrides_map = serde_json::Map::new();
        for (&uuid, prof) in &st.player_overrides {
            if prof.is_empty() {
                continue;
            }
            overrides_map.insert(
                format!("{:032x}", uuid),
                serde_json::json!({
                    "name": prof.name,
                    "rollback": prof.rollback.to_sparse_json(),
                    "scrub": prof.scrub.to_sparse_json(),
                }),
            );
        }
        let json = serde_json::json!({
            "checkpointPolicy": {
                "rollback": policy_to_nested_json(&st.policy),
            },
            "autosave": {
                "auto_save": st.auto_save,
                "adaptive_mode": st.adaptive_mode,
                "use_random_interval": st.use_random_interval,
                "fixed_interval_sec": st.fixed_interval_sec,
                "random_min_sec": st.random_min_sec,
                "random_max_sec": st.random_max_sec,
                "adaptive_interval_sec": st.adaptive_interval_sec,
            },
            "adaptive": {
                "min_save_gap_seconds": st.min_save_gap_sec,
                "relax_time_seconds": st.relax_time_sec,
                "post_combat_save_percent": st.post_combat_save_pct,
                "save_on_boss_defeat": st.save_on_boss_defeat,
            },
            "anchor": {
                "mode": st.anchor.mode.id(),
                "rotation_seconds": st.anchor.rotation_seconds,
                "rollback_cooldown_seconds": st.anchor.rollback_cooldown_seconds,
                "pinned": pinned_hex,
            },
            "scrub": {
                "enabled": st.scrub_enabled,
                "history_seconds": st.scrub_history_seconds,
                "wheel_sensitivity": st.scrub_wheel_sensitivity,
                "reverse_audio": st.scrub_reverse_audio,
                "show_hud": st.scrub_show_hud,
                "policy": policy_to_nested_json(&st.scrub_policy),
            },
            "debug": {
                "f3_loop_hitboxes": st.f3_loop_hitboxes,
                "logs": st.logs,
                "dev_mode": st.dev_mode,
            },
            "playerOverrides": serde_json::Value::Object(overrides_map),
        });
        (
            st.auto_save,
            st.adaptive_mode,
            st.fixed_interval_sec,
            st.anchor.mode,
            st.anchor.rollback_cooldown_seconds,
            st.save_on_boss_defeat,
            st.logs,
            st.f3_loop_hitboxes,
            st.dev_mode,
            st.scrub_enabled,
            json,
        )
    });

    let (
        auto_save,
        adaptive_mode,
        fixed_interval_sec,
        anchor_mode,
        cooldown,
        boss_defeat,
        logs,
        f3_hitboxes,
        dev_mode,
        scrub_enabled,
        json,
    ) = snapshot;

    let cfg_dir = Path::new("config");
    let _ = fs::create_dir_all(cfg_dir);
    if let Ok(bytes) = serde_json::to_vec_pretty(&json) {
        let tmp = cfg_dir.join(".rreturn.json.tmp");
        if fs::write(&tmp, &bytes).is_ok() {
            let _ = fs::rename(&tmp, cfg_dir.join("rreturn.json"));
        }
    }

    game.set_setting(MOD_ID, "auto_save", if auto_save { "true" } else { "false" });
    game.set_setting(
        MOD_ID,
        "adaptive_mode",
        if adaptive_mode { "true" } else { "false" },
    );
    game.set_setting(MOD_ID, "fixed_interval_sec", &fixed_interval_sec.to_string());
    let mode_idx = match anchor_mode {
        AnchorMode::Fixed => "0",
        AnchorMode::Multi => "1",
        AnchorMode::Everyone => "2",
        AnchorMode::Rotating => "3",
    };
    game.set_setting(MOD_ID, "anchor_mode", mode_idx);
    game.set_setting(MOD_ID, "rollback_cooldown_sec", &cooldown.to_string());
    game.set_setting(
        MOD_ID,
        "save_on_boss_defeat",
        if boss_defeat { "true" } else { "false" },
    );
    game.set_setting(MOD_ID, "logs", if logs { "true" } else { "false" });
    game.set_setting(
        MOD_ID,
        "f3_loop_hitboxes",
        if f3_hitboxes { "true" } else { "false" },
    );
    game.set_setting(MOD_ID, "dev_mode", if dev_mode { "true" } else { "false" });
    game.set_setting(
        MOD_ID,
        "scrub_enabled",
        if scrub_enabled { "true" } else { "false" },
    );
}
