use super::super::{Dim, Game};
use super::policy::*;
use super::snapshots::*;
use horizon_client::lan::guest::SavedBody;
use horizon_core::{
    geometry::Vec3,
    random::LegacyRandom,
};
use horizon_protocol::packets::{
    play::{self, Clientbound as C},
    recipes,
};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
};

pub fn restore_player_motion(
    m: &mut horizon_client::motion::PlayerMotion,
    gs: &GuestSnapshot,
    policy: &RollbackPolicy,
    snap_game_time: i64,
) {
    let keep_pos = (
        m.position,
        m.old_position,
        m.yaw,
        m.head_yaw,
        m.pitch,
        m.ground,
        m.pose,
        m.dimensions,
        m.scale,
    );
    let keep_mot = (
        m.velocity,
        m.fall_distance,
        m.sprinting,
        m.sprint_input,
        m.shift_key_down,
        m.local_crouching,
        m.fall_flying,
        m.fall_fly_ticks,
        m.glide_ending,
        m.swimming,
        m.stuck_multiplier,
        m.piston,
        m.impulse,
    );
    let keep_hp = (
        m.health.health(),
        m.health.absorption(),
        m.fire_ticks,
        m.air,
        m.ticks_frozen,
    );
    let keep_food = m.health.food;
    let keep_abil = (
        m.invulnerable,
        m.may_fly,
        m.spectator,
        m.flying,
        m.flying_speed,
    );

    if let Some(saved_m) = &gs.motion {
        *m = saved_m.clone();
        if !policy.player_position {
            m.position = keep_pos.0;
            m.old_position = keep_pos.1;
            m.yaw = keep_pos.2;
            m.head_yaw = keep_pos.3;
            m.pitch = keep_pos.4;
            m.ground = keep_pos.5;
            m.pose = keep_pos.6;
            m.dimensions = keep_pos.7;
            m.scale = keep_pos.8;
        }
        if !policy.player_motion {
            m.velocity = keep_mot.0;
            m.fall_distance = keep_mot.1;
            m.sprinting = keep_mot.2;
            m.sprint_input = keep_mot.3;
            m.shift_key_down = keep_mot.4;
            m.local_crouching = keep_mot.5;
            m.fall_flying = keep_mot.6;
            m.fall_fly_ticks = keep_mot.7;
            m.glide_ending = keep_mot.8;
            m.swimming = keep_mot.9;
            m.stuck_multiplier = keep_mot.10;
            m.piston = keep_mot.11;
            m.impulse = keep_mot.12;
        }
        if !policy.player_health {
            let _ = m.set_health(keep_hp.0.max(1.0), false);
            let _ = m.internal_set_absorption(keep_hp.1);
            m.fire_ticks = keep_hp.2;
            m.air = keep_hp.3;
            m.ticks_frozen = keep_hp.4;
        } else {
            let _ = m.set_health(gs.health.max(1.0), false);
            let _ = m.internal_set_absorption(gs.absorption);
        }
        if !policy.player_hunger {
            m.health.food = keep_food;
        }
        if !policy.player_abilities {
            m.invulnerable = keep_abil.0;
            m.may_fly = keep_abil.1;
            m.spectator = keep_abil.2;
            m.flying = keep_abil.3;
            m.flying_speed = keep_abil.4;
        }
    } else {
        let is_invuln = gs.game_mode == 1 || gs.game_mode == 3;
        let is_spectator = gs.game_mode == 3;
        if let Some(tag) = &gs.saved {
            let _ = m.apply_saved_attributes(tag.get("attributes"));
        }
        if policy.player_abilities {
            m.invulnerable = is_invuln;
            m.may_fly = is_invuln || gs.allow_flight;
            m.spectator = is_spectator;
            m.flying = gs.flying;
            m.flying_speed = gs.flying_speed;
        }
        if policy.player_position {
            let pos = Vec3::new(gs.position[0], gs.position[1], gs.position[2]);
            m.position = pos;
            m.old_position = pos;
            m.yaw = gs.yaw;
            m.head_yaw = gs.head_yaw;
            m.pitch = gs.pitch;
            m.ground = gs.ground;
            m.sleeping = gs.sleeping;
        }
        if policy.player_motion {
            m.velocity = gs.velocity;
            m.fall_distance = gs.fall_distance;
            let _ = m.set_sprinting(gs.sprinting);
            m.shift_key_down = gs.sneaking;
            m.local_crouching = gs.sneaking;
            m.fall_flying = gs.fall_flying;
            if !gs.fall_flying {
                m.fall_fly_ticks = 0;
            }
            m.glide_ending = false;
            m.swimming = false;
            m.stuck_multiplier = Vec3::ZERO;
            m.piston = Default::default();
            m.impulse = Default::default();
            m.spawn_extra_particles_on_fall = false;
            m.auto_spin_attack_ticks = 0;
            m.auto_spin_attack_dmg = 0.0;
            m.fluids = Default::default();
            m.in_powder_snow = false;
            m.was_in_powder_snow = false;
        }
        if policy.player_health {
            m.fire_ticks = gs.fire_ticks;
            m.air = gs.air;
            m.ticks_frozen = gs.ticks_frozen;
            let _ = m.set_health(gs.health.max(1.0), false);
            let _ = m.internal_set_absorption(gs.absorption);
        }
        if policy.player_hunger {
            m.health.food.food = gs.food;
            m.health.food.saturation = gs.saturation;
            m.health.food.exhaustion = gs.exhaustion;
            m.health.food.timer = gs.food_timer;
        }
    }

    m.events.clear();
    m.health.dead = false;
    m.health.game_time = snap_game_time;
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
}

pub fn restore_offline_files(saved: &HashMap<PathBuf, Vec<u8>>, restore_stats: bool) {
    for (path, bytes) in saved {
        let is_stat = path.to_string_lossy().contains("stats");
        if is_stat && !restore_stats {
            continue;
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, bytes);
    }
}

pub fn restore_players_on_rollback(
    game: &mut Game<'_>,
    snap: &TimelineSnapshot,
    prev_dim: Dim,
    target_dim: Dim,
) {
    let policy = &snap.policy;
    let net_clock_rate = game.network_clock_rate();
    if let Some(mut host) = game.lan.take() {
        if policy.server_world_border && prev_dim != target_dim {
            host.border_sent = None;
        }
        if policy.entities_presence {
            host.mob_players = snap.mob_players.clone();
        }
        host.states.clear();
        host.states_next_entity = None;
        host.broken.clear();
        host.commands.clear();
        host.poses.clear();
        host.quiet.clear();
        host.waypoints.clear();
        host.waypoint_dead.clear();
        host.idle_states = Default::default();
        host.hub.native_advancement_grants.clear();
        for gi in 0..host.hub.guests.len() {
            if !host.hub.guests[gi].placed {
                continue;
            }
            let uuid = host.hub.guests[gi].uuid;
            let conn = host.hub.guests[gi].conn;
            let tracked = host.hub.guests[gi].tracker.ids();
            if !tracked.is_empty() {
                host.hub.guests[gi]
                    .send(&C::RemoveEntities(play::RemoveEntities { ids: tracked }));
                host.hub.guests.update(gi, |g| g.tracker.clear());
            }

            let mut close_windows = vec![0i32];
            if let Some(m) = &host.hub.guests[gi].menu {
                close_windows.push(m.window);
            }
            if policy.player_inventory {
                if let Some(play_state) = host.play.get_mut(&conn) {
                    if let Some(lec) = play_state.lectern.take() {
                        close_windows.push(lec.window);
                    }
                    if let Some(merch) = play_state.merchant.take() {
                        close_windows.push(merch.window);
                    }
                    if let Some(mw) = play_state.mount_window.take() {
                        close_windows.push(mw.window);
                    }
                }
                host.hub.close_container(conn);
            } else {
                host.hub.close_container(conn);
            }
            close_windows.sort_unstable();
            close_windows.dedup();
            for w in close_windows {
                host.hub.guests[gi]
                    .send(&C::ContainerClose(play::ContainerClose { container: w }));
            }
            if prev_dim != target_dim {
                host.hub.guests[gi].send(&C::StopSound(play::StopSound {
                    source: None,
                    sound: None,
                }));
            }
            host.hub.guests.update(gi, |g| g.landing = false);

            let is_singleplayer = host.hub.guests.len() == 1
                || host.owner.as_ref().is_some_and(|o| o.uuid == uuid);
            let guest_snap = snap.guests.get(&uuid).or_else(|| {
                if is_singleplayer {
                    snap.guests.values().next()
                } else {
                    None
                }
            });

            if let Some(gs) = guest_snap {
                let ent_id = host.hub.guests[gi].entity_id as i32;
                let dim_changed =
                    policy.player_position && host.hub.guests[gi].dimension != gs.dimension;
                let old_effects: Vec<&'static str> = if policy.player_potion_effects {
                    let mut ids = std::collections::HashSet::new();
                    for e in &host.hub.guests[gi].effects.list {
                        ids.insert(e.id);
                    }
                    for s in &host.hub.guests[gi].sent_effects {
                        ids.insert(s.id);
                    }
                    if is_singleplayer {
                        for e in &game.potions.effects.list {
                            ids.insert(e.id);
                        }
                    }
                    ids.into_iter().collect()
                } else {
                    Vec::new()
                };
                host.hub.guests.update(gi, |g| {
                    if policy.player_position {
                        g.dimension = gs.dimension.clone();
                        g.position = gs.position;
                        g.previous_position = gs.position;
                        g.first_good = gs.position;
                        g.yaw = gs.yaw;
                        g.pitch = gs.pitch;
                        g.head_yaw = gs.head_yaw;
                        g.on_ground = gs.on_ground;
                    }
                    if policy.player_abilities {
                        g.game_mode = gs.game_mode;
                        g.previous_game_mode = gs.previous_game_mode;
                        g.abilities.flying = gs.flying;
                        g.abilities.flying_speed = gs.flying_speed;
                        g.abilities.may_fly = gs.allow_flight;
                    }
                    if policy.player_motion {
                        g.sprinting = gs.sprinting;
                        g.client_sprinting = gs.sprinting;
                        g.sneaking = gs.sneaking;
                        g.fall_flying = gs.fall_flying;
                        g.client_is_floating = false;
                        g.above_ground_ticks = 0;
                        g.rests_on_ground = gs.on_ground;
                        g.moves.clear();
                        g.moves_from_ground = gs.on_ground;
                        g.known_movement = [0.0; 3];
                    }
                    g.sleeping = gs.sleeping;
                    g.shoulders = gs.shoulders;
                    g.living_flags = gs.living_flags;
                    g.stuck = gs.stuck;
                    g.bed = gs.bed;
                    g.woke_from = None;
                    g.sent_bed = None;
                    g.sleep_timer = gs.sleep_timer;

                    let prev_vehicle = g.vehicle;
                    g.vehicle = gs.vehicle;
                    g.vehicle_order = gs.vehicle_order;
                    g.vehicle_good = gs.vehicle_good;
                    g.vehicle_rests = false;
                    g.vehicle_floating = false;
                    g.above_ground_vehicle_ticks = 0;
                    if prev_vehicle != gs.vehicle {
                        if let Some(old_v) = prev_vehicle {
                            g.send(&C::SetPassengers(play::SetPassengers {
                                vehicle: old_v as i32,
                                passengers: Vec::new(),
                            }));
                        }
                        if let Some(new_v) = gs.vehicle {
                            g.send(&C::SetPassengers(play::SetPassengers {
                                vehicle: new_v as i32,
                                passengers: vec![g.entity_id],
                            }));
                        }
                    }

                    if policy.player_spawn_point {
                        g.spawn = gs.spawn;
                        g.respawn_elsewhere = gs.respawn_elsewhere.clone();
                    }
                    g.last_death = gs.last_death.clone();
                    if policy.player_health {
                        g.health = gs.health.max(1.0);
                        g.saved_absorption = Some(gs.absorption);
                    }
                    if policy.player_hunger {
                        g.food = gs.food;
                        g.saturation = gs.saturation;
                    }
                    if policy.player_experience {
                        g.xp_level = gs.xp_level;
                        g.xp_progress = gs.xp_progress;
                        g.xp_total = gs.xp_total;
                        g.xp_seed = gs.xp_seed;
                    }
                    if policy.player_inventory {
                        g.inventory = gs.inventory.clone();
                        g.armor = gs.armor.clone();
                        g.offhand = gs.offhand.clone();
                        g.selected = gs.selected;
                        g.craft_grid = gs.craft_grid.clone();
                        g.craft_result = gs.craft_result.clone();
                        g.carried = gs.carried.clone();
                    }
                    if policy.player_ender_chest {
                        g.ender_items = gs.ender_items.clone();
                    }
                    if policy.player_potion_effects {
                        g.effects.list = gs.effects.clone();
                        g.effects.tick_count = gs.effects_tick_count;
                        g.effects.changes = gs.effects_changes;
                    }
                    if policy.player_recipe_book {
                        g.recipe_book = gs.recipe_book.clone();
                        g.send(&C::RecipeBookSettings(recipes::RecipeBookSettings {
                            books: std::array::from_fn(|idx| {
                                (g.recipe_book.open[idx], g.recipe_book.filtering[idx])
                            }),
                        }));
                    }
                    g.advancements = gs.advancements.clone();
                    g.advancements_visible.clear();
                    g.advancements_changed.clear();
                    g.dead = false;
                    g.death_time = 0;
                    g.loading = false;
                    g.load_timeout = 0;
                    g.sent_health = None;
                    g.sent_xp = None;
                    g.awaiting_teleport = None;
                    g.send(&C::SetHealth(play::SetHealth {
                        health: g.health,
                        food: g.food,
                        saturation: g.saturation,
                    }));
                    g.send(&C::GameEvent(play::GameEvent {
                        event: 3,
                        param: gs.game_mode as f32,
                    }));
                    g.send(&g.abilities_packet());
                    g.seen_credits = gs.seen_credits;
                    g.post_effects = gs.post_effects.clone();
                    g.post_effects_dirty = true;
                    g.attachments = gs.attachments.clone();
                    g.saved = gs.saved.clone();
                    g.equipment = Default::default();
                    g.anticheat = Default::default();
                    g.record = None;
                    g.replay = None;
                    if let Ok(mut ch) = g.changes.lock() {
                        ch.clear();
                    }
                    g.sent_air = (-1, -1.0);
                    g.sent_frozen = -1;
                    if snap.policy.world_weather {
                        let rain_level = if snap.weather.raining { 1.0 } else { 0.0 };
                        let thunder_level = if snap.weather.thundering { 1.0 } else { 0.0 };
                        g.send(&C::GameEvent(play::GameEvent {
                            event: if snap.weather.raining { 2 } else { 1 },
                            param: 0.0,
                        }));
                        g.send(&C::GameEvent(play::GameEvent {
                            event: 7,
                            param: rain_level,
                        }));
                        g.send(&C::GameEvent(play::GameEvent {
                            event: 8,
                            param: thunder_level,
                        }));
                    }
                    g.sent_shared ^= -128;
                    g.sent_shoulders = [Some(-999), Some(-999)];
                    g.loaded_body = Some(SavedBody {
                        fire_ticks: gs.fire_ticks,
                        air: gs.air,
                        fall_distance: gs.fall_distance,
                        velocity: [gs.velocity.x, gs.velocity.y, gs.velocity.z],
                        food_exhaustion: gs.exhaustion,
                        food_tick_timer: gs.food_timer,
                        ticks_frozen: gs.ticks_frozen,
                        invulnerable: false,
                        invulnerable_time: 0,
                    });
                    let synced_attrs = g.motion.as_mut().map(|m| {
                        restore_player_motion(m, gs, policy, snap.game_time);
                        horizon_client::lan::attributes::syncable(&m.attributes)
                    });
                    if let Some(values) = synced_attrs
                        && !values.is_empty()
                    {
                        g.send(&C::UpdateAttributes(play::UpdateAttributes {
                            id: g.entity_id,
                            values,
                        }));
                    }
                    let pose_now = g.pose();
                    g.sent_pose = pose_now;
                    let mut shared_flags: i8 = 0;
                    if gs.fire_ticks > 0 {
                        shared_flags |= 0x01;
                    }
                    if gs.sneaking {
                        shared_flags |= 0x02;
                    }
                    if gs.sprinting {
                        shared_flags |= 0x08;
                    }
                    if gs.fall_flying {
                        shared_flags |= 0x80u8 as i8;
                    }

                    g.send(&C::SetEntityData(play::SetEntityData {
                        id: g.entity_id,
                        values: vec![
                            (
                                0,
                                horizon_protocol::entity_data::serializer::BYTE,
                                horizon_protocol::entity_data::Value::Byte(shared_flags),
                            ),
                            (
                                6,
                                horizon_protocol::entity_data::serializer::POSE,
                                horizon_protocol::entity_data::Value::Id(pose_now.id()),
                            ),
                            (
                                7,
                                horizon_protocol::entity_data::serializer::INT,
                                horizon_protocol::entity_data::Value::Int(gs.ticks_frozen),
                            ),
                            (
                                8,
                                horizon_protocol::entity_data::serializer::BYTE,
                                horizon_protocol::entity_data::Value::Byte(g.living_flags),
                            ),
                            (
                                12,
                                horizon_protocol::entity_data::serializer::INT,
                                horizon_protocol::entity_data::Value::Int(g.stuck.0),
                            ),
                            (
                                13,
                                horizon_protocol::entity_data::serializer::INT,
                                horizon_protocol::entity_data::Value::Int(g.stuck.1),
                            ),
                        ],
                    }));
                    if policy.player_inventory {
                        g.send(&C::SetHeldSlot(play::SetHeldSlot {
                            slot: gs.selected as i32,
                        }));
                    }

                    if let Some(active_snap_chunks) = snap.levels.get(&game.dim).map(|l| &l.chunks)
                    {
                        let forget: Vec<_> = g.sent_chunks.iter().copied().collect();
                        for (x, z) in forget {
                            let in_snap = active_snap_chunks.contains_key(&(x, z));
                            if !in_snap {
                                g.sent_chunks.remove(&(x, z));
                                g.send(&C::ForgetLevelChunk(play::ForgetLevelChunk {
                                    pos: [x, z],
                                }));
                            }
                        }
                    }

                    if policy.server_game_rules {
                        g.send(&C::TickingState(play::TickingState {
                            rate: snap.clock_rate,
                            frozen: snap.clock_frozen,
                        }));
                    }
                    if policy.world_difficulty {
                        g.send(&C::ChangeDifficulty(play::ChangeDifficulty {
                            difficulty: i32::from(snap.difficulty),
                            locked: snap.difficulty_locked,
                        }));
                    }
                    if policy.world_spawn
                        && let Some(pos) = snap.world_spawn
                    {
                        g.send(&C::SetDefaultSpawnPosition(play::SetDefaultSpawnPosition {
                            pos: play::GlobalPos {
                                dimension: "minecraft:overworld".to_owned(),
                                pos,
                            },
                            yaw: snap.world_spawn_angle[0],
                            pitch: snap.world_spawn_angle[1],
                        }));
                    }

                    if policy.server_bossbars
                        && dim_changed
                        && Dim::from_id(&gs.dimension) != Dim::End
                    {
                        let dragon_bar_id =
                            horizon_client::bossbar::stable_uuid("minecraft:ender_dragon");
                        g.send(&C::BossEvent(play::BossEvent {
                            id: dragon_bar_id,
                            action: play::BossAction::Remove,
                        }));
                    }
                });

                let mut cd_packets: Vec<(String, i32)> = Vec::new();
                if let Some(play_state) = host.play.get_mut(&conn) {
                    let prev_shield_cd = play_state.shield_cooldown;
                    let prev_cooldowns = std::mem::take(&mut play_state.cooldowns);
                    let prev_stats = std::mem::take(&mut play_state.stats);
                    let prev_hook = play_state.hook.extract();
                    let prev_left_game = play_state.left_game;
                    let prev_menus = if !policy.player_inventory {
                        Some((
                            play_state.lectern.take(),
                            play_state.merchant.take(),
                            play_state.mount_window.take(),
                        ))
                    } else {
                        None
                    };

                    *play_state = Default::default();
                    play_state.left_game = prev_left_game;

                    if let Some((lec, merch, mw)) = prev_menus {
                        play_state.lectern = lec;
                        play_state.merchant = merch;
                        play_state.mount_window = mw;
                    }

                    if policy.player_cooldowns {
                        let old_cds = extract_active_cooldowns(&prev_cooldowns);
                        let new_cds = extract_active_cooldowns(&gs.cooldowns);
                        for (old_grp, _) in &old_cds {
                            if !new_cds.iter().any(|(new_grp, _)| new_grp == old_grp) {
                                cd_packets.push((old_grp.to_string(), 0));
                            }
                        }
                        for (new_grp, ticks) in new_cds {
                            cd_packets.push((new_grp.to_string(), ticks));
                        }
                        play_state.shield_cooldown = gs.shield_cooldown;
                        play_state.cooldowns = gs.cooldowns.clone();
                    } else {
                        play_state.shield_cooldown = prev_shield_cd;
                        play_state.cooldowns = prev_cooldowns;
                    }

                    if policy.player_stats {
                        play_state.stats.values = gs.stats.clone();
                        play_state.stats.mark_all_dirty();
                        play_state.stats.events.clear();
                    } else {
                        play_state.stats = prev_stats;
                    }

                    if policy.entities_presence {
                        if let Some(hs) = &gs.hook {
                            let mut h = prev_hook.unwrap_or_else(|| {
                                fishing::Hook::rreturn_create_full(
                                    hs.id,
                                    hs.position,
                                    hs.previous,
                                    hs.velocity,
                                    hs.rng_state,
                                    hs.nibble,
                                    hs.time_until_lured,
                                    hs.time_until_hooked,
                                    hs.fish_angle,
                                    hs.open_water,
                                    hs.state_bobbing,
                                )
                            });
                            h.position = Vec3::new(hs.position[0], hs.position[1], hs.position[2]);
                            h.previous = Vec3::new(hs.previous[0], hs.previous[1], hs.previous[2]);
                            h.velocity = Vec3::new(hs.velocity[0], hs.velocity[1], hs.velocity[2]);
                            h.nibble = hs.nibble;
                            h.time_until_lured = hs.time_until_lured;
                            h.time_until_hooked = hs.time_until_hooked;
                            h.fish_angle = hs.fish_angle;
                            h.open_water = hs.open_water;
                            if hs.state_bobbing {
                                h.state = fishing::State::Bobbing;
                            }
                            if hs.rng_state != 0 {
                                h.random = horizon_core::random::EntityRandom::own(LegacyRandom::from_state(hs.rng_state));
                            }
                            play_state.hook.replace(h);
                        }
                    } else if let Some(h) = prev_hook {
                        play_state.hook.replace(h);
                    } else {
                        play_state.hook.discard();
                    }
                }
                for (group, duration) in cd_packets {
                    host.hub.guests[gi].send(&C::Cooldown(play::Cooldown { group, duration }));
                }

                if policy.player_abilities {
                    host.hub
                        .set_game_mode(conn, gs.game_mode, &game.world, Some(game.collision));
                }
                if policy.player_position {
                    if dim_changed {
                        let target_dim = Dim::from_id(&gs.dimension);
                        game.level_teleports
                            .push(super::super::server_travel::LevelTeleport {
                                conn,
                                dim: target_dim,
                                at: gs.position,
                                rotation: Some([gs.yaw, gs.pitch]),
                                vehicle: None,
                            });
                    } else {
                        host.hub.teleport(gi, Some(gs.position));
                    }
                }
                if policy.player_inventory {
                    host.hub.send_inventory(gi);
                }
                if policy.player_potion_effects {
                    for old_id in old_effects {
                        if !gs.effects.iter().any(|new_e| new_e.id == old_id) {
                            let full_id = if old_id.contains(':') {
                                old_id.to_string()
                            } else {
                                format!("minecraft:{}", old_id)
                            };
                            if let Some(effect_id) = host.hub.registry_id("minecraft:mob_effect", &full_id) {
                                let target_id = host.hub.guests[gi].entity_id;
                                host.hub.guests[gi].send(&C::RemoveMobEffect(play::RemoveMobEffect {
                                    id: target_id,
                                    effect: effect_id,
                                }));
                            }
                        }
                    }
                    host.hub.send_active_effects(gi);
                }
                if policy.entities_presence {
                    if let Some(hs) = &gs.hook {
                        if is_singleplayer || host.owner.as_ref().is_some_and(|o| o.uuid == uuid) {
                            game.rreturn_restore_hook_full(
                                hs.id,
                                hs.position,
                                hs.previous,
                                hs.velocity,
                                hs.rng_state,
                                hs.nibble,
                                hs.time_until_lured,
                                hs.time_until_hooked,
                                hs.fish_angle,
                                hs.open_water,
                                hs.state_bobbing,
                            );
                        } else if let Some(h) = game.hook.as_mut() {
                            h.position = Vec3::new(hs.position[0], hs.position[1], hs.position[2]);
                            h.previous = Vec3::new(hs.previous[0], hs.previous[1], hs.previous[2]);
                            h.velocity = Vec3::new(hs.velocity[0], hs.velocity[1], hs.velocity[2]);
                            h.nibble = hs.nibble;
                            h.time_until_lured = hs.time_until_lured;
                            h.time_until_hooked = hs.time_until_hooked;
                            h.fish_angle = hs.fish_angle;
                            h.open_water = hs.open_water;
                            if hs.state_bobbing {
                                h.state = fishing::State::Bobbing;
                            }
                            if hs.rng_state != 0 {
                                h.random = horizon_core::random::EntityRandom::own(LegacyRandom::from_state(hs.rng_state));
                            }
                        } else {
                            game.spawn_remote_hook(
                                hs.id,
                                [0; 4],
                                Vec3::new(hs.velocity[0], hs.velocity[1], hs.velocity[2]),
                                Vec3::new(hs.position[0], hs.position[1], hs.position[2]),
                                ent_id,
                            );
                        }
                    } else if let Some(h) = game.hook.extract() {
                        game.remove_remote_hook(h.id());
                    }
                }

                host.hub.flush_progress(conn);
            } else if !policy.preserve_new_player_inventory {
                if let Some(play_state) = host.play.get_mut(&conn) {
                    *play_state = Default::default();
                }
                host.hub.guests.update(gi, |g| {
                    g.inventory = vec![None; 36];
                    g.armor = [None, None, None, None];
                    g.offhand = None;
                    g.ender_items = vec![None; 27];
                    g.carried = None;
                });
                host.hub.send_inventory(gi);
            }
        }
        if !game.level_teleports.is_empty() {
            game.run_level_teleports();
        }
        game.server_trips.clear();
        game.teleport_notes.clear();
        game.spawn_wait.clear();
        for gi in 0..host.hub.guests.len() {
            let conn = host.hub.guests[gi].conn;
            host.hub.guests.update(gi, |g| {
                g.landing = false;
                g.loading = false;
                g.load_timeout = 0;
            });
            if let Some(play_state) = host.play.get_mut(&conn) {
                play_state.portal = Default::default();
            }
        }
        game.portals = Default::default();
        if policy.player_inventory {
            host.hub.native_menu_events.clear();
        }
        host.hub.native_tracking.clear();
        host.hub.attachment_snapshots.clear();
        if let Some(far) = host.hub.far.as_mut() {
            far.forget_level();
        }
        let _ = horizon_client::lan::handoff::take_changed();
        if policy.world_time {
            let overworld_clock = host
                .hub
                .sync_data()
                .id("minecraft:world_clock", "minecraft:overworld")
                .unwrap_or(0);
            let packet = horizon_protocol::packets::play::Clientbound::SetTime(
                horizon_protocol::packets::play::SetTime {
                    game_time: game.world.game_time,
                    clocks: vec![horizon_protocol::packets::play::ClockState {
                        clock: overworld_clock,
                        total_ticks: game.world.day_time,
                        partial_tick: 0.0,
                        rate: net_clock_rate,
                    }],
                },
            );
            host.hub.broadcast_level(&packet);
        }
        host.hub.synced.clear();
        game.lan = Some(host);
    }
    game.guest_packets.clear();
    game.guest_entity_events.clear();
    game.guest_player_sounds.clear();
    game.guest_block_sounds.clear();
    game.guest_cooldowns.clear();
    game.save_after_guest_turn = false;
    game.rotation_forced = false;

    if let Some(gs) = snap.guests.values().next() {
        if policy.player_abilities {
            game.set_mode_id(gs.game_mode);
        }
        if policy.player_cooldowns {
            game.shield_cooldown = gs.shield_cooldown;
            game.cooldowns = gs.cooldowns.clone();
        }
        if let Some(p) = game.player.as_mut() {
            restore_player_motion(p, gs, policy, snap.game_time);
            p.client_side = true;
        }
        if policy.player_inventory {
            game.location_armor = game.armor.clone();
            game.apply_enchantment_attributes();
        }
        if policy.player_potion_effects {
            game.potions.effects.list = gs.effects.clone();
            game.potions.effects.tick_count = gs.effects_tick_count;
            game.potions.effects.changes = gs.effects_changes;
            game.level.active_effects = game.potions.effects.saved();
        }
        if policy.player_potion_effects || policy.player_inventory {
            game.potions.applied.clear();
            game.apply_effect_status();
        }
    }

    game.ui.death_time = 0;
    game.ui.died_at = None;
    game.ui.death_confirm = None;
    game.item_activation.reset();
    game.titles = Default::default();
    game.flash = 0;
    game.screen = super::super::Screen::Playing;
    game.boss_overlay.reset();
    if policy.server_bossbars {
        game.bosses.sent.clear();
        game.publish_boss_bars();
    }
    game.using = snap.client_using;
    game.use_duration = snap.client_use_duration;
    game.crossbow_sounds = snap.client_crossbow_sounds;
}