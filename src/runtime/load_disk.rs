use super::super::Dim;
use super::policy::*;
use super::snapshots::*;
use super::storage::*;
use super::types::*;
use horizon_client::{
    advancements::Progress,
    cooldowns::Cooldowns,
    effects::{PlayerEffects, SavedEffect},
    recipe_book::Settings as RecipeBookSettings,
    save::{item_from_nbt, read_dat, Level},
    scoreboard::Scoreboard,
    sculk::WardenSpawnTracker,
    world::HealthRules,
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::Vec3,
    position::BlockPos,
};
use horizon_world::{
    food::Difficulty,
    ground_support::GroundState,
    nbt::Tag,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
    sync::Arc,
};

pub fn load_slot_from_disk(world_root: &Path, slot: &str) -> Option<TimelineSnapshot> {
    if !is_valid_slot_name(slot.trim()) {
        return None;
    }
    let cp_dir = slot_dir(world_root, slot);
    let meta_file = cp_dir.join("meta.json");
    let meta_str = fs::read_to_string(meta_file).ok()?;
    let meta: serde_json::Value = serde_json::from_str(&meta_str).ok()?;

    let game_time = meta["game_time"].as_i64()?;
    let day_time = meta["day_time"].as_i64().unwrap_or(game_time);
    let difficulty = meta["difficulty"].as_u64().unwrap_or(2) as u8;
    let difficulty_locked = meta["difficulty_locked"].as_bool().unwrap_or(false);
    let next_entity = meta["next_entity"].as_u64().unwrap_or(100) as u32;

    let world_spawn = meta["world_spawn"].as_array().and_then(|arr| {
        if arr.len() == 3 {
            Some([
                arr[0].as_i64()? as i32,
                arr[1].as_i64()? as i32,
                arr[2].as_i64()? as i32,
            ])
        } else {
            None
        }
    });

    let world_spawn_angle = meta["world_spawn_angle"].as_array().map_or([0.0, 0.0], |arr| {
        if arr.len() == 2 {
            [
                arr[0].as_f64().unwrap_or(0.0) as f32,
                arr[1].as_f64().unwrap_or(0.0) as f32,
            ]
        } else {
            [0.0, 0.0]
        }
    });

    let mut anchor_ids = HashSet::new();
    if let Some(arr) = meta["anchor_ids"].as_array() {
        for v in arr {
            if let Some(s) = v.as_str() {
                if let Ok(u) = u128::from_str_radix(s, 16) {
                    anchor_ids.insert(u);
                }
            }
        }
    }

    let mut region_files = HashSet::new();
    if let Some(arr) = meta["region_files"].as_array() {
        for v in arr {
            if let Some(s) = v.as_str() {
                region_files.insert(world_root.join(s));
            }
        }
    }

    let mut offline_player_data = HashMap::new();
    let cp_players = cp_dir.join("players");
    collect_files_recursive(
        &cp_players,
        &mut offline_player_data,
        &world_root.join("players"),
        Path::new(""),
    );

    let mut guests = HashMap::new();
    if let Some(g_arr) = meta["guests"].as_array() {
        for g_val in g_arr {
            let uuid_str = g_val["uuid"].as_str().unwrap_or("");
            let uuid_clean = uuid_str.replace('-', "");
            let uuid = u128::from_str_radix(&uuid_clean, 16).unwrap_or(0);
            if uuid == 0 {
                continue;
            }

            let pos = g_val["position"].as_array().map_or([0.0, 65.0, 0.0], |arr| {
                [
                    arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
                    arr.get(1).and_then(|v| v.as_f64()).unwrap_or(65.0),
                    arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
                ]
            });

            let vel = g_val["velocity"].as_array().map_or(Vec3::ZERO, |arr| {
                Vec3::new(
                    arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
                    arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
                    arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
                )
            });

            let player_dat_hex = cp_players.join("data").join(format!("{:032x}.dat", uuid));
            let player_dat_std = cp_players
                .join("data")
                .join(format!("{}.dat", format_uuid_std(uuid)));
            let player_dat = if player_dat_hex.is_file() {
                player_dat_hex
            } else {
                player_dat_std
            };
            let player_tag = read_dat(&player_dat);

            fn decode_saved_effect(v: &serde_json::Value) -> Option<SavedEffect> {
                Some(SavedEffect {
                    id: v.get("id")?.as_str()?.to_owned(),
                    duration: v.get("duration")?.as_i64()? as i32,
                    amplifier: v.get("amplifier")?.as_i64().unwrap_or(0) as i32,
                    ambient: v.get("ambient").and_then(|b| b.as_bool()).unwrap_or(false),
                    particles: v.get("particles").and_then(|b| b.as_bool()).unwrap_or(true),
                    icon: v.get("icon").and_then(|b| b.as_bool()).unwrap_or(true),
                    hidden: v
                        .get("hidden")
                        .filter(|h| !h.is_null())
                        .and_then(decode_saved_effect)
                        .map(Box::new),
                })
            }

            let saved_fx: Vec<SavedEffect> = g_val["effects"]
                .as_array()
                .map(|arr| arr.iter().filter_map(decode_saved_effect).collect())
                .unwrap_or_default();
            let decoded_effects = PlayerEffects::from_saved(&saved_fx).list;

            let decoded_adv = if let Some(criteria) =
                serde_json::from_value::<BTreeMap<String, BTreeMap<String, String>>>(
                    g_val["advancements"].clone(),
                )
                .ok()
                .filter(|m| !m.is_empty())
            {
                Progress { criteria }
            } else {
                let adv_path = cp_players
                    .join("advancements")
                    .join(format!("{}.json", format_uuid_std(uuid)));
                fs::read_to_string(adv_path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .map(|v| Progress::from_json(&v))
                    .unwrap_or_default()
            };

            let decoded_stats: BTreeMap<String, BTreeMap<String, i32>> =
                serde_json::from_value(g_val["stats"].clone()).unwrap_or_default();

            let mut decoded_cooldowns = Cooldowns::default();
            if let Some(cd_arr) = g_val["cooldowns"].as_array() {
                for entry in cd_arr {
                    if let (Some(group), Some(ticks)) = (
                        entry.get("group").and_then(|v| v.as_str()),
                        entry.get("ticks").and_then(|v| v.as_i64()),
                    ) && ticks > 0
                    {
                        decoded_cooldowns.add(Arc::from(group), ticks as i32);
                    }
                }
            }

            let bed = g_val["bed"].as_array().and_then(|arr| {
                if arr.len() == 3 {
                    Some([
                        arr[0].as_i64()? as i32,
                        arr[1].as_i64()? as i32,
                        arr[2].as_i64()? as i32,
                    ])
                } else {
                    None
                }
            });

            let vehicle_good = g_val["vehicle_good"].as_array().and_then(|arr| {
                if arr.len() == 3 {
                    Some([
                        arr[0].as_f64()?,
                        arr[1].as_f64()?,
                        arr[2].as_f64()?,
                    ])
                } else {
                    None
                }
            });

            let supporting_block = g_val["supporting_block"].as_array().and_then(|arr| {
                if arr.len() == 3 {
                    Some(BlockPos::new(
                        arr[0].as_i64()? as i32,
                        arr[1].as_i64()? as i32,
                        arr[2].as_i64()? as i32,
                    ))
                } else {
                    None
                }
            });
            let on_ground_val = g_val["on_ground"].as_bool().unwrap_or(true);
            let ground = GroundState {
                on_ground: on_ground_val,
                horizontal_collision: g_val["ground_horizontal_collision"]
                    .as_bool()
                    .unwrap_or(false),
                on_ground_no_blocks: g_val["ground_no_blocks"].as_bool().unwrap_or(false),
                supporting_block,
            };

            let food_timer = g_val["food_timer"]
                .as_i64()
                .or_else(|| {
                    player_tag
                        .as_ref()
                        .and_then(|t| t.get("foodTickTimer"))
                        .and_then(|v| match v {
                            Tag::Int(i) => Some(i64::from(*i)),
                            Tag::Short(s) => Some(i64::from(*s)),
                            Tag::Byte(b) => Some(i64::from(*b)),
                            _ => None,
                        })
                })
                .unwrap_or(0) as i32;

            let shoulders = g_val["shoulders"]
                .as_array()
                .map(|arr| {
                    [
                        arr.first().and_then(|v| v.as_i64()).map(|v| v as i32),
                        arr.get(1).and_then(|v| v.as_i64()).map(|v| v as i32),
                    ]
                })
                .unwrap_or([None, None]);

            let stuck = g_val["stuck"]
                .as_array()
                .and_then(|arr| {
                    if arr.len() == 2 {
                        Some((arr[0].as_i64()? as i32, arr[1].as_i64()? as i32))
                    } else {
                        None
                    }
                })
                .unwrap_or((0, 0));

            let post_effects: Vec<String> = g_val["post_effects"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();

            let guest_attachments = match horizon_mod_runtime::attachments::attachment_compound(
                player_tag
                    .as_ref()
                    .and_then(|tag| tag.get(horizon_mod_runtime::attachments::ATTACHMENTS_KEY)),
            ) {
                Ok(Some(data)) => {
                    let mut a = horizon_mod_runtime::attachments::ResidentAttachments::default();
                    *a = horizon_mod_runtime::attachments::Attachments::from_nbt(data.clone());
                    a
                }
                _ => horizon_mod_runtime::attachments::ResidentAttachments::default(),
            };

            let mut guest_snap = GuestSnapshot {
                uuid,
                dimension: g_val["dimension"]
                    .as_str()
                    .unwrap_or("minecraft:overworld")
                    .to_owned(),
                position: pos,
                velocity: vel,
                yaw: g_val["yaw"].as_f64().unwrap_or(0.0) as f32,
                pitch: g_val["pitch"].as_f64().unwrap_or(0.0) as f32,
                head_yaw: g_val["head_yaw"].as_f64().unwrap_or(0.0) as f32,
                on_ground: on_ground_val,
                ground,
                game_mode: g_val["game_mode"].as_i64().unwrap_or(0) as i32,
                previous_game_mode: g_val["previous_game_mode"].as_i64().map(|v| v as i32),
                flying: g_val["flying"].as_bool().unwrap_or(false),
                flying_speed: g_val["flying_speed"]
                    .as_f64()
                    .map(|v| v as f32)
                    .filter(|s| s.is_finite())
                    .unwrap_or(0.05),
                allow_flight: g_val["allow_flight"].as_bool().unwrap_or(false),
                sprinting: g_val["sprinting"].as_bool().unwrap_or(false),
                sneaking: g_val["sneaking"].as_bool().unwrap_or(false),
                fall_flying: g_val["fall_flying"].as_bool().unwrap_or(false),
                sleeping: g_val["sleeping"].as_bool().unwrap_or(false),
                shoulders,
                living_flags: g_val["living_flags"].as_i64().unwrap_or(0) as i8,
                stuck,
                bed,
                sleep_timer: g_val["sleep_timer"].as_u64().unwrap_or(0) as u32,
                vehicle: g_val["vehicle"].as_u64().map(|v| v as u32),
                vehicle_order: g_val["vehicle_order"].as_i64().unwrap_or(0),
                vehicle_good,
                spawn: None,
                respawn_elsewhere: None,
                last_death: None,
                health: g_val["health"].as_f64().unwrap_or(20.0) as f32,
                absorption: g_val["absorption"].as_f64().unwrap_or(0.0) as f32,
                food: g_val["food"].as_i64().unwrap_or(20) as i32,
                saturation: g_val["saturation"].as_f64().unwrap_or(5.0) as f32,
                exhaustion: g_val["exhaustion"].as_f64().unwrap_or(0.0) as f32,
                food_timer,
                fire_ticks: g_val["fire_ticks"].as_i64().unwrap_or(-20) as i32,
                air: g_val["air"].as_i64().unwrap_or(300) as i32,
                ticks_frozen: g_val["ticks_frozen"].as_i64().unwrap_or(0) as i32,
                fall_distance: g_val["fall_distance"].as_f64().unwrap_or(0.0),
                xp_level: g_val["xp_level"].as_i64().unwrap_or(0) as i32,
                xp_progress: g_val["xp_progress"].as_f64().unwrap_or(0.0) as f32,
                xp_total: g_val["xp_total"].as_i64().unwrap_or(0) as i32,
                xp_seed: g_val["xp_seed"].as_i64().unwrap_or(0) as i32,
                inventory: vec![None; 36],
                armor: [None, None, None, None],
                offhand: None,
                carried: None,
                ender_items: vec![None; 27],
                selected: g_val["selected"].as_u64().unwrap_or(0) as usize,
                craft_grid: [None, None, None, None],
                craft_result: None,
                effects: decoded_effects,
                effects_tick_count: g_val["effects_tick_count"].as_i64().unwrap_or(0) as i32,
                effects_changes: g_val["effects_changes"].as_u64().unwrap_or(0) as u32,
                shield_cooldown: g_val["shield_cooldown"].as_u64().unwrap_or(0) as u32,
                cooldowns: decoded_cooldowns,
                recipe_book: RecipeBookSettings::default(),
                advancements: decoded_adv,
                stats: decoded_stats,
                seen_credits: g_val["seen_credits"].as_bool().unwrap_or(false),
                post_effects,
                attachments: guest_attachments,
                saved: player_tag.clone(),
                hook: g_val
                    .get("hook")
                    .filter(|v| !v.is_null())
                    .and_then(|v| {
                        let id = v.get("id")?.as_u64()? as u32;
                        let p = v.get("position")?.as_array()?;
                        let prev = v.get("previous")?.as_array()?;
                        let vel_arr = v.get("velocity").and_then(|vel| vel.as_array());
                        let vel = if let Some(va) = vel_arr
                            && va.len() == 3
                        {
                            [
                                va[0].as_f64().unwrap_or(0.0),
                                va[1].as_f64().unwrap_or(0.0),
                                va[2].as_f64().unwrap_or(0.0),
                            ]
                        } else {
                            [0.0, 0.0, 0.0]
                        };
                        let nibble = v.get("nibble").and_then(|n| n.as_i64()).unwrap_or(0) as i32;
                        let time_until_lured = v
                            .get("time_until_lured")
                            .and_then(|t| t.as_i64())
                            .unwrap_or(0) as i32;
                        let time_until_hooked = v
                            .get("time_until_hooked")
                            .and_then(|t| t.as_i64())
                            .unwrap_or(0) as i32;
                        let fish_angle = v
                            .get("fish_angle")
                            .and_then(|a| a.as_f64())
                            .unwrap_or(0.0) as f32;
                        let open_water = v
                            .get("open_water")
                            .and_then(|o| o.as_bool())
                            .unwrap_or(true);
                        let state_bobbing = v
                            .get("state_bobbing")
                            .and_then(|b| b.as_bool())
                            .unwrap_or(false);
                        if p.len() == 3 && prev.len() == 3 {
                            Some(HookSnapshot {
                                id,
                                position: [p[0].as_f64()?, p[1].as_f64()?, p[2].as_f64()?],
                                previous: [
                                    prev[0].as_f64()?,
                                    prev[1].as_f64()?,
                                    prev[2].as_f64()?,
                                ],
                                velocity: vel,
                                rng_state: v
                                    .get("rng_state")
                                    .and_then(|r| r.as_u64())
                                    .unwrap_or(0),
                                nibble,
                                time_until_lured,
                                time_until_hooked,
                                fish_angle,
                                open_water,
                                state_bobbing,
                            })
                        } else {
                            None
                        }
                    }),
                motion: None,
            };

            if let Some(tag) = &player_tag {
                let mut p_level = Level::default();
                p_level.read_player(tag);
                guest_snap.spawn = p_level.spawn;
                guest_snap.respawn_elsewhere = p_level.respawn_elsewhere;
                guest_snap.last_death = p_level.last_death;
                guest_snap.recipe_book = p_level.recipe_book;

                if let Some(Tag::List(items)) = tag.get("Inventory") {
                    for it in items {
                        let slot = it
                            .get("Slot")
                            .and_then(|v| match v {
                                Tag::Byte(b) => Some(*b as i16),
                                Tag::Short(s) => Some(*s),
                                Tag::Int(i) => Some(*i as i16),
                                _ => None,
                            })
                            .unwrap_or(0);
                        if let Some(stack) = item_from_nbt(it) {
                            match slot {
                                0..=35 => guest_snap.inventory[slot as usize] = Some(stack),
                                100 => guest_snap.armor[3] = Some(stack),
                                101 => guest_snap.armor[2] = Some(stack),
                                102 => guest_snap.armor[1] = Some(stack),
                                103 => guest_snap.armor[0] = Some(stack),
                                -106 | 150 => guest_snap.offhand = Some(stack),
                                _ => {}
                            }
                        }
                    }
                }
                if let Some(eq) = tag.get("equipment") {
                    if let Some(s) = eq.get("head").and_then(item_from_nbt) {
                        guest_snap.armor[0] = Some(s);
                    }
                    if let Some(s) = eq.get("chest").and_then(item_from_nbt) {
                        guest_snap.armor[1] = Some(s);
                    }
                    if let Some(s) = eq.get("legs").and_then(item_from_nbt) {
                        guest_snap.armor[2] = Some(s);
                    }
                    if let Some(s) = eq.get("feet").and_then(item_from_nbt) {
                        guest_snap.armor[3] = Some(s);
                    }
                    if let Some(s) = eq.get("offhand").and_then(item_from_nbt) {
                        guest_snap.offhand = Some(s);
                    }
                }
                if let Some(Tag::List(ender)) = tag.get("EnderItems") {
                    for it in ender {
                        let slot = it
                            .get("Slot")
                            .and_then(|v| match v {
                                Tag::Byte(b) => Some(*b as usize),
                                Tag::Short(s) => Some(*s as usize),
                                Tag::Int(i) => Some(*i as usize),
                                _ => None,
                            })
                            .unwrap_or(0);
                        if slot < guest_snap.ender_items.len() {
                            if let Some(stack) = item_from_nbt(it) {
                                guest_snap.ender_items[slot] = Some(stack);
                            }
                        }
                    }
                }
            }

            guests.insert(uuid, guest_snap);
        }
    }

    let mut policy = RollbackPolicy::default();
    if let Some(pol_obj) = meta["policy"].as_object() {
        for (k, v) in pol_obj {
            if let Some(b) = v.as_bool() {
                policy.set_field(k, b);
            }
        }
    }

    let disk_level = Level::read(&cp_dir);
    let weather = disk_level.as_ref().map(|l| l.weather).unwrap_or_default();
    let enabled_features = disk_level
        .as_ref()
        .map(|l| &l.enabled_features[..])
        .unwrap_or(&[]);
    let rules = horizon_client::gamerules::GameRules::load_with_features(&cp_dir, enabled_features);
    let dragon_fight_dat = fs::read(fight_dat_path(&cp_dir)).ok();
    let trader_dat = fs::read(trader_dat_path(&cp_dir)).ok();

    let globals_tag = read_dat(&cp_dir.join("rreturn_globals.dat"));
    let rand_sequences = globals_tag
        .as_ref()
        .and_then(|t| t.get("rand_sequences"))
        .map(horizon_client::random_sequences::RandomSequences::from_nbt)
        .or_else(|| {
            read_dat(&cp_dir.join("data/minecraft/random_sequences.dat"))
                .and_then(|t| t.get("data").cloned())
                .map(|d| horizon_client::random_sequences::RandomSequences::from_nbt(&d))
        })
        .unwrap_or_default();
    let border = globals_tag
        .as_ref()
        .and_then(|t| t.get("border"))
        .map(WorldBorder::unpack)
        .or_else(|| {
            read_dat(&cp_dir.join("dimensions/minecraft/overworld/data/minecraft/world_border.dat"))
                .or_else(|| read_dat(&cp_dir.join("data/minecraft/world_border.dat")))
                .and_then(|t| t.get("data").map(WorldBorder::unpack))
        })
        .unwrap_or_default();

    let scoreboard = globals_tag
        .as_ref()
        .and_then(|t| t.get("scoreboard"))
        .map(Scoreboard::unpack)
        .or_else(|| {
            read_dat(&cp_dir.join("data/minecraft/scoreboard.dat"))
                .and_then(|t| t.get("data").map(Scoreboard::unpack))
        })
        .unwrap_or_default();

    let bossbars = globals_tag
        .as_ref()
        .and_then(|t| t.get("bossbars").cloned())
        .unwrap_or_else(|| Tag::Compound(Default::default()));

    let sculk_tracker = globals_tag
        .as_ref()
        .and_then(|t| t.get("sculk_tracker"))
        .map(WardenSpawnTracker::from_nbt)
        .unwrap_or_default();

    let fight_tag = globals_tag
        .as_ref()
        .and_then(|t| t.get("fight_state").cloned());

    let player_extra = globals_tag
        .as_ref()
        .and_then(|t| t.get("player_extra").cloned())
        .or_else(|| disk_level.as_ref().and_then(|l| l.player_extra.clone()));

    let player_attachments = globals_tag
        .as_ref()
        .and_then(|t| match t.get("player_attachments") {
            Some(Tag::Compound(c)) => {
                let mut a = horizon_mod_runtime::attachments::ResidentAttachments::default();
                *a = horizon_mod_runtime::attachments::Attachments::from_nbt(c.clone());
                Some(a)
            }
            _ => None,
        })
        .unwrap_or_default();

    let mut guest_attachments = HashMap::new();
    if let Some(Tag::List(list)) = globals_tag.as_ref().and_then(|t| t.get("guest_attachments")) {
        for item in list {
            if let (Some(Tag::String(u_str)), Some(Tag::Compound(c))) =
                (item.get("uuid"), item.get("data"))
                && let Ok(s) = u_str.to_string()
                && let Ok(u) = u128::from_str_radix(&s, 16)
            {
                guest_attachments.insert(
                    u,
                    horizon_mod_runtime::attachments::Attachments::from_nbt(c.clone()),
                );
            }
        }
    }

    let mut entity_attachments = FxHashMap::default();
    if let Some(Tag::List(list)) = globals_tag
        .as_ref()
        .and_then(|t| t.get("entity_attachments"))
    {
        for item in list {
            if let (Some(Tag::Int(id)), Some(Tag::Compound(c))) =
                (item.get("id"), item.get("data"))
            {
                entity_attachments.insert(
                    *id as u32,
                    horizon_mod_runtime::attachments::Attachments::from_nbt(c.clone()),
                );
            }
        }
    }

    let sculk_darkness = meta["sculk_darkness"]
        .as_array()
        .and_then(|arr| {
            if arr.len() == 2 {
                Some((arr[0].as_f64()? as f32, arr[1].as_f64()? as f32))
            } else {
                None
            }
        })
        .unwrap_or((0.0, 0.0));

    let mut mob_players = horizon_client::fast_hash::FxHashMap::default();
    if let Some(arr) = meta["mob_players"].as_array() {
        for item in arr {
            if let Some(pair) = item.as_array()
                && pair.len() == 2
                && let (Some(k), Some(v)) = (pair[0].as_u64(), pair[1].as_u64())
            {
                mob_players.insert(k as u32, v as u32);
            }
        }
    }

    let ticked_entities_through = meta["ticked_entities_through"]
        .as_u64()
        .map(|v| v as u32)
        .unwrap_or(next_entity);
    let cat_tick = meta["cat_tick"].as_i64().map(|v| v as i32).unwrap_or(1200);
    let phantom_tick = meta["phantom_tick"]
        .as_i64()
        .map(|v| v as i32)
        .unwrap_or(1200);

    let mut shoulder_time = HashMap::new();
    if let Some(arr) = meta["shoulder_time"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 5
                && let (Some(u0), Some(u1), Some(u2), Some(u3), Some(t)) = (
                    a[0].as_i64(),
                    a[1].as_i64(),
                    a[2].as_i64(),
                    a[3].as_i64(),
                    a[4].as_i64(),
                )
            {
                shoulder_time.insert([u0 as i32, u1 as i32, u2 as i32, u3 as i32], t);
            }
        }
    }

    let mut portals_entities = HashMap::new();
    if let Some(arr) = meta["portals_entities"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 2
                && let (Some(id), Some(cd)) = (a[0].as_u64(), a[1].as_i64())
            {
                let mut st = horizon_client::nether_portal::PortalState::default();
                st.cooldown = cd as i32;
                portals_entities.insert(id as u32, st);
            }
        }
    }

    let mut end_gateway_cooldowns = HashMap::new();
    if let Some(arr) = meta["end_gateways"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 2
                && let (Some(id), Some(cd)) = (a[0].as_u64(), a[1].as_i64())
            {
                end_gateway_cooldowns.insert(id as u32, cd as i32);
            }
        }
    }

    let mut lectern_pulses = Vec::new();
    if let Some(arr) = meta["lectern_pulses"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 4
                && let (Some(x), Some(y), Some(z), Some(t)) =
                    (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
            {
                lectern_pulses.push(((x as i32, y as i32, z as i32), t as i32));
            }
        }
    }

    let mut player_beds = Vec::new();
    if let Some(arr) = meta["player_beds"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 4
                && let (Some(dim_str), Some(x), Some(y), Some(z)) =
                    (a[0].as_str(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
            {
                player_beds.push((Dim::from_id(dim_str), [x as i32, y as i32, z as i32]));
            }
        }
    }

    let mut lid_openers = HashMap::new();
    if let Some(arr) = meta["lid_openers"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 4
                && let (Some(x), Some(y), Some(z), Some(c)) =
                    (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
            {
                lid_openers.insert((x as i32, y as i32, z as i32), c as i32);
            }
        }
    }

    let mut scheduled_events = Vec::new();
    if let Some(arr) = meta["scheduled_events"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 3
                && let (Some(id), Some(time), Some(seq)) =
                    (a[0].as_str(), a[1].as_i64(), a[2].as_u64())
            {
                scheduled_events.push(super::super::functions::Scheduled {
                    id: id.to_owned(),
                    time,
                    seq,
                });
            }
        }
    }

    let mut command_clocks = BTreeMap::new();
    if let Some(arr) = meta["command_clocks"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 5
                && let (Some(k), Some(ticks), Some(partial), Some(rate), Some(paused)) = (
                    a[0].as_str(),
                    a[1].as_i64(),
                    a[2].as_f64(),
                    a[3].as_f64(),
                    a[4].as_bool(),
                )
            {
                command_clocks.insert(
                    k.to_owned(),
                    (
                        ticks,
                        super::super::chat::WorldClock {
                            partial: partial as f32,
                            rate: rate as f32,
                            paused,
                        },
                    ),
                );
            }
        }
    }

    let cmd_post_effects: Vec<String> = meta["cmd_post_effects"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    let mut lead_killers = HashMap::new();
    if let Some(arr) = meta["lead_killers"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 3
                && let (Some(id), Some(u_str), Some(dim_str)) =
                    (a[0].as_u64(), a[1].as_str(), a[2].as_str())
                && let Ok(u) = u128::from_str_radix(u_str, 16)
            {
                lead_killers.insert(id as u32, (u, Dim::from_id(dim_str)));
            }
        }
    }

    let sleep_seen = meta["sleep_seen"]
        .as_array()
        .and_then(|a| {
            if a.len() == 2 {
                Some((a[0].as_i64()?, a[1].as_i64()?))
            } else {
                None
            }
        })
        .unwrap_or((0, 0));

    let hr_json = &meta["health_rules"];
    let diff_enum = match difficulty {
        0 => Difficulty::Peaceful,
        1 => Difficulty::Easy,
        3 => Difficulty::Hard,
        _ => Difficulty::Normal,
    };
    let rule_bool = |name: &str, def: bool| -> bool {
        match rules.get(name) {
            Some(horizon_client::gamerules::Value::Bool(b)) => b,
            _ => def,
        }
    };
    let health_rules = HealthRules {
        difficulty: diff_enum,
        natural_regeneration: hr_json["natural_regeneration"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("natural_health_regeneration", true)),
        drowning_damage: hr_json["drowning_damage"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("drowning_damage", true)),
        fall_damage: hr_json["fall_damage"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("fall_damage", true)),
        fire_damage: hr_json["fire_damage"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("fire_damage", true)),
        freeze_damage: hr_json["freeze_damage"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("freeze_damage", true)),
    };

    Some(TimelineSnapshot {
        anchor_ids,
        policy,
        game_time,
        day_time,
        weather,
        health_rules,
        rules,
        border,
        bossbars,
        sculk_tracker,
        sculk_darkness,
        dragon_fight_dat,
        fight_tag,
        trader_dat,
        trader_tag: None,
        rand_sequences,
        region_files,
        difficulty,
        difficulty_locked,
        clock_rate: meta["clock_rate"].as_f64().map(|v| v as f32).unwrap_or(20.0),
        clock_frozen: meta["clock_frozen"].as_bool().unwrap_or(false),
        time_rate: meta["time_rate"].as_f64().map(|v| v as f32).unwrap_or(1.0),
        time_partial: meta["time_partial"].as_f64().map(|v| v as f32).unwrap_or(0.0),
        time_paused: meta["time_paused"].as_bool().unwrap_or(false),
        command_clocks,
        world_spawn,
        world_spawn_angle,
        next_entity,
        ticked_entities_through,
        ordered_announced: horizon_client::join_seq::JoinSeq::upcoming(),
        cat_tick,
        phantom_tick,
        player_extra,
        shoulder_time,
        fangs: Default::default(),
        ominous_spawners: Vec::new(),
        portals_entities,
        end_gateway_cooldowns,
        lectern_pulses,
        player_beds,
        sleep_seen,
        lead_killers,
        lid_openers,
        entity_attachments,
        player_attachments,
        guest_attachments,
        scheduled_events,
        scoreboard,
        command_storage: None,
        stopwatches_elapsed: None,
        sound_seed_state: meta["sound_seed_state"].as_u64(),
        cmd_post_effects,
        guests,
        offline_player_data,
        levels: HashMap::new(),
        mob_players,
        disk_restored: false,
        client_using: None,
        client_use_duration: 0,
        client_use_ticks_set: false,
        client_crossbow_sounds: [false; 2],
    })
}

pub fn load_checkpoint_from_disk(world_root: &Path) -> Option<TimelineSnapshot> {
    load_slot_from_disk(world_root, "default")
}