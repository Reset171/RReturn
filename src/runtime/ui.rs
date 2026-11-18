use super::super::Game;
use super::types::*;
use std::{
    fs,
    path::Path,
};

pub fn load_saved_settings(game: &Game<'_>) {
    let auto_save = game
        .setting(MOD_ID, "auto_save")
        .map(|v| v != "false");
    let adaptive = game
        .setting(MOD_ID, "adaptive_mode")
        .map(|v| v != "false");
    let interval = game
        .setting(MOD_ID, "fixed_interval_sec")
        .and_then(|v| v.parse::<i32>().ok());
    let mode = game
        .setting(MOD_ID, "anchor_mode")
        .map(|v| AnchorMode::by_id(&v));
    let cooldown = game
        .setting(MOD_ID, "rollback_cooldown_sec")
        .and_then(|v| v.parse::<i32>().ok());
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

    let json_cfg = fs::read_to_string(Path::new("config").join("rreturn.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

    with_state_mut(|st| {
        if let Some(v) = &json_cfg {
            let b = |val: &serde_json::Value, d: bool| val.as_bool().unwrap_or(d);
            let rb = &v["checkpointPolicy"]["rollback"];
            if rb.is_object() {
                st.policy.blocks = b(&rb["blocks"], true);
                st.policy.biomes = b(&rb["biomes"], true);
                st.policy.block_entities = b(&rb["block_entities"], true);
                st.policy.block_ticks = b(&rb["block_ticks"], true);
                st.policy.fluid_ticks = b(&rb["fluid_ticks"], true);
                st.policy.block_events = b(&rb["block_events"], true);
                let ent = &rb["entities"];
                if ent.is_object() {
                    st.policy.entities_presence = b(&ent["presence"], true);
                    st.policy.dropped_items = b(&ent["dropped_items"], true);
                    st.policy.experience_orbs = b(&ent["experience_orbs"], true);
                }
                let pl = &rb["players"];
                if pl.is_object() {
                    st.policy.player_position = b(&pl["position"], true);
                    st.policy.player_motion = b(&pl["motion"], true);
                    st.policy.player_health = b(&pl["health"], true);
                    st.policy.player_hunger = b(&pl["hunger"], true);
                    st.policy.player_experience = b(&pl["experience"], true);
                    st.policy.player_inventory = b(&pl["inventory"], true);
                    st.policy.player_ender_chest = b(&pl["ender_chest"], true);
                    st.policy.player_potion_effects = b(&pl["potion_effects"], true);
                    st.policy.player_cooldowns = b(&pl["cooldowns"], true);
                    st.policy.player_abilities = b(&pl["abilities"], true);
                    st.policy.player_recipe_book = b(&pl["recipe_book"], true);
                    st.policy.player_spawn_point = b(&pl["spawn_point"], true);
                    st.policy.player_stats = b(&pl["stats"], true);
                    st.policy.preserve_new_player_inventory =
                        b(&pl["preserve_new_player_inventory"], false);
                }
                let w = &rb["world"];
                if w.is_object() {
                    st.policy.world_time = b(&w["time"], true);
                    st.policy.world_weather = b(&w["weather"], true);
                    st.policy.world_raids = b(&w["raids"], true);
                    st.policy.world_dragon_fight = b(&w["dragon_fight"], true);
                    st.policy.world_scoreboard = b(&w["scoreboard"], true);
                    st.policy.world_level_rng = b(&w["level_rng"], true);
                    st.policy.world_difficulty = b(&w["difficulty"], true);
                    st.policy.world_spawn = b(&w["world_spawn"], true);
                    let sg = &w["server_globals"];
                    if sg.is_object() {
                        st.policy.server_bossbars = b(&sg["bossbars"], true);
                        st.policy.server_forced_chunks = b(&sg["forced_chunks"], true);
                        st.policy.server_world_border = b(&sg["world_border"], true);
                        st.policy.server_game_rules = b(&sg["game_rules"], true);
                    }
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
    });
}

pub fn save_current_settings(game: &Game<'_>) {
    with_state(|st| {
        game.set_setting(
            MOD_ID,
            "auto_save",
            if st.auto_save { "true" } else { "false" },
        );
        game.set_setting(
            MOD_ID,
            "adaptive_mode",
            if st.adaptive_mode { "true" } else { "false" },
        );
        game.set_setting(
            MOD_ID,
            "fixed_interval_sec",
            &st.fixed_interval_sec.to_string(),
        );
        game.set_setting(MOD_ID, "anchor_mode", st.anchor.mode.id());
        game.set_setting(
            MOD_ID,
            "rollback_cooldown_sec",
            &st.anchor.rollback_cooldown_seconds.to_string(),
        );
        game.set_setting(
            MOD_ID,
            "save_on_boss_defeat",
            if st.save_on_boss_defeat {
                "true"
            } else {
                "false"
            },
        );
        game.set_setting(MOD_ID, "logs", if st.logs { "true" } else { "false" });
        game.set_setting(
            MOD_ID,
            "f3_loop_hitboxes",
            if st.f3_loop_hitboxes { "true" } else { "false" },
        );
        game.set_setting(
            MOD_ID,
            "dev_mode",
            if st.dev_mode { "true" } else { "false" },
        );
    });
}
