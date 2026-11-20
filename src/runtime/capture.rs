use super::super::{
    Dim, Game,
};
use super::persistence::{save_checkpoint_to_disk, save_slot_to_disk};
use super::snapshots::*;
use super::types::*;
use horizon_client::{
    chunk::Chunk,
    cooldowns::Cooldowns,
    fast_hash::FxHashMap,
    lan::guest::SavedBody,
};
use horizon_core::{
    geometry::Vec3,
    position::ChunkPos,
    ticks::ChunkTicks,
};
use horizon_world::{ground_support::GroundState, level::Ticks};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn capture_chunk_ticks(
    chunks: &FxHashMap<(i32, i32), Arc<Chunk>>,
    ticks: &Ticks,
) -> HashMap<(i32, i32), (ChunkTicks, ChunkTicks)> {
    let mut out = HashMap::new();
    for &(cx, cz) in chunks.keys() {
        let cpos = ChunkPos::new(cx, cz);
        let b = ticks.blocks.chunk(cpos).cloned().unwrap_or_default();
        let f = ticks.fluids.chunk(cpos).cloned().unwrap_or_default();
        out.insert((cx, cz), (b, f));
    }
    out
}

pub fn try_backup_offline_files(world_root: &Path) -> std::io::Result<HashMap<PathBuf, Vec<u8>>> {
    let mut map = HashMap::new();
    fn walk_dir(dir: &Path, map: &mut HashMap<PathBuf, Vec<u8>>) -> std::io::Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                walk_dir(&p, map)?;
            } else if p.is_file() {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "dat" || ext == "json" {
                    let bytes = fs::read(&p)?;
                    map.insert(p, bytes);
                }
            }
        }
        Ok(())
    }
    walk_dir(&world_root.join("data"), &mut map)?;
    walk_dir(&world_root.join("dimensions"), &mut map)?;
    walk_dir(&world_root.join("players"), &mut map)?;
    walk_dir(&world_root.join("generated"), &mut map)?;
    Ok(map)
}

pub fn backup_offline_files(world_root: &Path) -> HashMap<PathBuf, Vec<u8>> {
    try_backup_offline_files(world_root).unwrap_or_default()
}

pub fn capture_level_snapshot(game: &Game<'_>, dim: Dim) -> LevelSnapshot {
    let raids_dat = fs::read(raids_dat_path(&game.world_root, dim)).ok();
    let chunks_map: FxHashMap<(i32, i32), Arc<Chunk>> = (&*game.world.chunks).clone();
    let mut be_attachments = Vec::new();
    for &(cx, cz) in chunks_map.keys() {
        be_attachments.extend(game.world.block_entities.chunk_attachments(cx, cz));
    }
    let chunk_ticks = capture_chunk_ticks(&chunks_map, &game.block_ticks);
    let registry = game.pack.assets.registry;
    let tridents = game
        .tridents
        .iter()
        .map(|t| {
            let uuid = game.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&t.id);
            TridentSnapshot {
                join: t.join,
                id: t.id,
                nbt: t.to_nbt(uuid, registry, raw),
                previous: t.previous,
                in_ground_time: t.in_ground_time,
                no_physics: t.no_physics,
                owner: t.owner,
                guest: t.guest,
            }
        })
        .collect();
    let rockets = game
        .rockets
        .iter()
        .map(|r| {
            let uuid = game.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&r.id);
            RocketSnapshot {
                id: r.id,
                nbt: r.to_nbt(uuid, None, raw),
                attached: r.attached,
                guest: r.guest,
                owner: r.owner,
                attached_to: r.attached_to,
            }
        })
        .collect();
    let breath_clouds = game
        .breath_clouds
        .iter()
        .map(|c| {
            let uuid = game.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&c.id);
            BreathSnapshot {
                id: c.id,
                nbt: super::super::potions::potion_cloud_to_nbt(c, uuid, raw),
                at: c.at,
                motion: c.motion,
                radius: c.radius,
                age: c.age,
                duration: c.duration,
                from_dragon: c.from_dragon,
                potion_none: c.potion.is_none(),
                victims: c.victims.clone(),
            }
        })
        .collect();
    LevelSnapshot {
        chunks: chunks_map,
        block_entities: (&*game.world.block_entities).clone(),
        block_names: game.world.block_names.clone(),
        be_attachments,
        world_attachments: game.world.attachments.clone(),
        chest_openers: game
            .world
            .chest_openers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect(),
        border: game.border.clone(),
        shulker_anims: game
            .be_anims
            .shulkers
            .iter()
            .map(|(&k, v)| (k, v.clone()))
            .collect(),
        gateway_anims: game
            .be_anims
            .gateway_servers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect(),
        fangs: game.fangs.list.clone(),
        ominous_spawners: game
            .ominous_spawners
            .iter()
            .map(|s| {
                (
                    s.id,
                    horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                        &s.item,
                        Vec3::new(s.position[0], s.position[1], s.position[2]),
                        s.spawn_item_after,
                        s.uuid,
                        s.raw.as_deref(),
                    ),
                )
            })
            .collect(),
        portals_entities: game.portals.entities.clone(),
        lectern_pulses: game.lectern_pulses.clone(),
        lid_openers: game.lid_openers.clone(),
        inhabited: game.world.inhabited.clone(),
        end_clock: game.world.end_clock,
        random: game.world.random.clone(),
        forced_chunks: game.streaming.forced.clone(),
        chunk_ticks,
        random_tick_value: game.block_ticks.random_tick_value,
        comparator_outputs: game.block_ticks.comparator_outputs.clone(),
        torch_toggles: game.block_ticks.torch_toggles.clone(),
        moving_pistons: game.block_ticks.moving_pistons.clone(),
        piston_pushes: game.block_ticks.piston_pushes.clone(),
        block_events: game.block_ticks.block_events.clone(),
        hoppers: game.hoppers.clone(),
        mobs: game.mobs.clone(),
        dropped: game.dropped.clone(),
        orbs: game.orbs.clone(),
        projectiles: game.creatures.projectiles.clone(),
        boats: game.boats.clone(),
        carts: game.carts.clone(),
        tnts: game.tnts.clone(),
        falling: game.falling.clone(),
        paintings: game.paintings.clone(),
        item_frames: game.item_frames.clone(),
        armor_stands: game.armor_stands.clone(),
        cushions: game.cushions.clone(),
        others: game.others.clone(),
        crystals: game.crystals.clone(),
        dragon: game.dragon.as_ref().map(DragonSnapshot::from_live),
        tridents,
        rockets,
        breath_clouds,
        knot_ids: game.knot_ids.clone(),
        persist_uuids: (*game.persist.uuids).clone(),
        persist_raw: game.persist.raw.clone(),
        persist_passenger_slots: game.persist.passenger_slots.clone(),
        raids_dat,
        last_hurt_by_player: game.last_hurt_by_player.clone(),
        love_cause: game.love_cause.clone(),
    }
}

pub fn capture_parked_snapshot(
    game: &Game<'_>,
    dim: Dim,
    p: &crate::game::levels::Parked,
) -> LevelSnapshot {
    let raids_dat = fs::read(raids_dat_path(&game.world_root, dim)).ok();
    let chunks_map: FxHashMap<(i32, i32), Arc<Chunk>> = (&*p.world.chunks).clone();
    let mut be_attachments = Vec::new();
    for &(cx, cz) in chunks_map.keys() {
        be_attachments.extend(p.world.block_entities.chunk_attachments(cx, cz));
    }
    let chunk_ticks = capture_chunk_ticks(&chunks_map, &p.block_ticks);
    let registry = game.pack.assets.registry;
    let tridents = p
        .tridents
        .iter()
        .map(|t| {
            let uuid = p.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
            let raw = p.persist.raw.get(&t.id);
            TridentSnapshot {
                join: t.join,
                id: t.id,
                nbt: t.to_nbt(uuid, registry, raw),
                previous: t.previous,
                in_ground_time: t.in_ground_time,
                no_physics: t.no_physics,
                owner: t.owner,
                guest: t.guest,
            }
        })
        .collect();
    let rockets = p
        .rockets
        .iter()
        .map(|r| {
            let uuid = p.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
            let raw = p.persist.raw.get(&r.id);
            RocketSnapshot {
                id: r.id,
                nbt: r.to_nbt(uuid, None, raw),
                attached: r.attached,
                guest: r.guest,
                owner: r.owner,
                attached_to: r.attached_to,
            }
        })
        .collect();
    let breath_clouds = p
        .breath_clouds
        .iter()
        .map(|c| {
            let uuid = p.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
            let raw = p.persist.raw.get(&c.id);
            BreathSnapshot {
                id: c.id,
                nbt: super::super::potions::potion_cloud_to_nbt(c, uuid, raw),
                at: c.at,
                motion: c.motion,
                radius: c.radius,
                age: c.age,
                duration: c.duration,
                from_dragon: c.from_dragon,
                potion_none: c.potion.is_none(),
                victims: c.victims.clone(),
            }
        })
        .collect();
    LevelSnapshot {
        chunks: chunks_map,
        block_entities: (&*p.world.block_entities).clone(),
        block_names: p.world.block_names.clone(),
        be_attachments,
        world_attachments: p.world.attachments.clone(),
        chest_openers: p
            .world
            .chest_openers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect(),
        border: p.border.clone(),
        shulker_anims: p
            .anims
            .shulkers
            .iter()
            .map(|(&k, v)| (k, v.clone()))
            .collect(),
        gateway_anims: p
            .anims
            .gateway_servers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect(),
        fangs: p.fangs.list.clone(),
        ominous_spawners: p
            .ominous_spawners
            .iter()
            .map(|s| {
                (
                    s.id,
                    horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                        &s.item,
                        Vec3::new(s.position[0], s.position[1], s.position[2]),
                        s.spawn_item_after,
                        s.uuid,
                        s.raw.as_deref(),
                    ),
                )
            })
            .collect(),
        portals_entities: p.portal_states.clone(),
        lectern_pulses: p.lectern_pulses.clone(),
        lid_openers: p.lid_openers.clone(),
        inhabited: p.world.inhabited.clone(),
        end_clock: p.world.end_clock,
        random: p.world.random.clone(),
        forced_chunks: p.streaming.forced.clone(),
        chunk_ticks,
        random_tick_value: p.block_ticks.random_tick_value,
        comparator_outputs: p.block_ticks.comparator_outputs.clone(),
        torch_toggles: p.block_ticks.torch_toggles.clone(),
        moving_pistons: p.block_ticks.moving_pistons.clone(),
        piston_pushes: p.block_ticks.piston_pushes.clone(),
        block_events: p.block_ticks.block_events.clone(),
        hoppers: p.hoppers.clone(),
        mobs: p.mobs.clone(),
        dropped: p.dropped.clone(),
        orbs: p.orbs.clone(),
        projectiles: p.projectiles.clone(),
        boats: p.boats.clone(),
        carts: p.carts.clone(),
        tnts: p.tnts.clone(),
        falling: p.falling.clone(),
        paintings: p.paintings.clone(),
        item_frames: p.item_frames.clone(),
        armor_stands: p.armor_stands.clone(),
        cushions: p.cushions.clone(),
        others: p.others.clone(),
        crystals: p.crystals.clone(),
        dragon: p.dragon.as_ref().map(DragonSnapshot::from_live),
        tridents,
        rockets,
        breath_clouds,
        knot_ids: p.knot_ids.clone(),
        persist_uuids: (*p.persist.uuids).clone(),
        persist_raw: p.persist.raw.clone(),
        persist_passenger_slots: p.persist.passenger_slots.clone(),
        raids_dat,
        last_hurt_by_player: HashMap::new(),
        love_cause: HashMap::new(),
    }
}

pub fn create_checkpoint_on_server(game: &mut Game<'_>, requester: Option<u128>) -> bool {
    create_slot_checkpoint_on_server(game, requester, "default")
}

pub fn capture_guests_snapshot(game: &Game<'_>) -> (HashMap<u128, GuestSnapshot>, Vec<u128>) {
    let mut guests = HashMap::new();
    let mut online = Vec::new();
    if let Some(host) = &game.lan {
        for g in host.hub.guests.iter().filter(|g| g.placed) {
            online.push(g.uuid);
            let motion = g.motion.as_deref();
            let (
                velocity,
                absorption,
                exhaustion,
                food_timer,
                fire_ticks,
                air,
                fall_distance,
                ticks_frozen,
                ground,
            ) = match motion {
                Some(m) => (
                    m.velocity,
                    m.health.absorption(),
                    m.health.food.exhaustion,
                    m.health.food.timer,
                    m.fire_ticks,
                    m.air,
                    m.fall_distance,
                    m.ticks_frozen,
                    m.ground,
                ),
                None => {
                    let body = g.loaded_body.unwrap_or(SavedBody {
                        invulnerable: false,
                        invulnerable_time: 0,
                        fire_ticks: -20,
                        air: 300,
                        fall_distance: 0.0,
                        velocity: [0.0, 0.0, 0.0],
                        food_exhaustion: 0.0,
                        food_tick_timer: 0,
                        ticks_frozen: 0,
                    });
                    (
                        Vec3::new(body.velocity[0], body.velocity[1], body.velocity[2]),
                        g.saved_absorption.unwrap_or(0.0),
                        body.food_exhaustion,
                        body.food_tick_timer,
                        body.fire_ticks,
                        body.air,
                        body.fall_distance,
                        body.ticks_frozen,
                        GroundState {
                            on_ground: g.on_ground,
                            ..Default::default()
                        },
                    )
                }
            };
            let is_owner = host.owner.as_ref().is_some_and(|o| o.uuid == g.uuid)
                || host.hub.guests.len() == 1;
            let (shield_cd, cooldowns, stats, hook) = match host.play.get(&g.conn) {
                Some(p) => (
                    p.shield_cooldown,
                    p.cooldowns.clone(),
                    p.stats.values.clone(),
                    p.hook.as_ref().map(|h| {
                        let (nibble, time_lured, time_hooked, fish_angle, open_water, bobbing) =
                            h.rreturn_fishing_state();
                        HookSnapshot {
                            id: h.id(),
                            position: [h.position.x, h.position.y, h.position.z],
                            previous: [h.previous.x, h.previous.y, h.previous.z],
                            velocity: h.rreturn_velocity(),
                            rng_state: h.random.state(),
                            nibble,
                            time_until_lured: time_lured,
                            time_until_hooked: time_hooked,
                            fish_angle,
                            open_water,
                            state_bobbing: bobbing,
                        }
                    }),
                ),
                None => (0, Cooldowns::default(), BTreeMap::new(), None),
            };
            let shield_cooldown = if is_owner && shield_cd == 0 {
                game.shield_cooldown
            } else {
                shield_cd
            };
            let hook = hook.or_else(|| {
                if is_owner {
                    game.hook.as_ref().map(|h| {
                        let (nibble, time_lured, time_hooked, fish_angle, open_water, bobbing) =
                            h.rreturn_fishing_state();
                        HookSnapshot {
                            id: h.id(),
                            position: [h.position.x, h.position.y, h.position.z],
                            previous: [h.previous.x, h.previous.y, h.previous.z],
                            velocity: h.rreturn_velocity(),
                            rng_state: h.random.state(),
                            nibble,
                            time_until_lured: time_lured,
                            time_until_hooked: time_hooked,
                            fish_angle,
                            open_water,
                            state_bobbing: bobbing,
                        }
                    })
                } else {
                    None
                }
            });
            let effects = g.effects.list.clone();
            let effects_tick_count = g.effects.tick_count;
            let effects_changes = g.effects.changes;
            guests.insert(
                g.uuid,
                GuestSnapshot {
                    uuid: g.uuid,
                    dimension: g.dimension.clone(),
                    position: g.position,
                    velocity,
                    yaw: g.yaw,
                    pitch: g.pitch,
                    head_yaw: g.head_yaw,
                    on_ground: g.on_ground,
                    ground,
                    game_mode: g.game_mode,
                    previous_game_mode: g.previous_game_mode,
                    flying: g.abilities.flying,
                    flying_speed: g.abilities.flying_speed,
                    allow_flight: g.abilities.may_fly,
                    sprinting: g.sprinting,
                    sneaking: g.sneaking,
                    fall_flying: g.fall_flying,
                    sleeping: g.sleeping,
                    shoulders: g.shoulders,
                    living_flags: g.living_flags,
                    stuck: g.stuck,
                    bed: g.bed,
                    sleep_timer: g.sleep_timer,
                    vehicle: g.vehicle,
                    vehicle_order: g.vehicle_order,
                    vehicle_good: g.vehicle_good,
                    spawn: g.spawn,
                    respawn_elsewhere: g.respawn_elsewhere.clone(),
                    last_death: g.last_death.clone(),
                    health: g.health,
                    absorption,
                    food: g.food,
                    saturation: g.saturation,
                    exhaustion,
                    food_timer,
                    fire_ticks,
                    air,
                    ticks_frozen,
                    fall_distance,
                    xp_level: g.xp_level,
                    xp_progress: g.xp_progress,
                    xp_total: g.xp_total,
                    xp_seed: g.xp_seed,
                    inventory: g.inventory.clone(),
                    armor: g.armor.clone(),
                    offhand: g.offhand.clone(),
                    carried: g.carried.clone(),
                    ender_items: g.ender_items.clone(),
                    selected: g.selected,
                    craft_grid: g.craft_grid.clone(),
                    craft_result: g.craft_result.clone(),
                    effects,
                    effects_tick_count,
                    effects_changes,
                    shield_cooldown,
                    cooldowns,
                    recipe_book: g.recipe_book.clone(),
                    advancements: g.advancements.clone(),
                    stats,
                    seen_credits: g.seen_credits,
                    post_effects: g.post_effects.clone(),
                    attachments: g.attachments.clone(),
                    saved: g.saved.clone(),
                    hook,
                    motion: motion.cloned(),
                },
            );
        }
    }
    (guests, online)
}

pub fn capture_timeline_snapshot_in_memory(game: &Game<'_>) -> Arc<TimelineSnapshot> {
    let (guests, online) = capture_guests_snapshot(game);
    let mut levels = HashMap::new();
    levels.insert(game.dim, capture_level_snapshot(game, game.dim));
    for (&dim, parked) in &game.levels.parked {
        levels.insert(dim, capture_parked_snapshot(game, dim, parked));
    }
    let (anchor_set, policy) = with_state(|st| {
        let anchors: HashSet<u128> = resolve_anchors(&st.anchor, &online, game.world.game_time, None)
            .into_iter()
            .collect();
        (anchors, st.policy)
    });

    Arc::new(TimelineSnapshot {
        anchor_ids: anchor_set,
        policy,
        game_time: game.world.game_time,
        day_time: game.world.day_time,
        weather: game.world.weather,
        health_rules: game.world.health_rules,
        rules: game.ui.rules.clone(),
        border: game.border.clone(),
        bossbars: game.bosses.bars.pack(),
        sculk_tracker: game.sculk.tracker,
        sculk_darkness: game.sculk.darkness,
        dragon_fight_dat: None,
        fight_tag: Some(game.fight.rreturn_pack()),
        trader_dat: None,
        rand_sequences: game.rand_sequences.clone(),
        region_files: HashSet::new(),
        difficulty: game.ui.difficulty,
        difficulty_locked: game.level.difficulty_locked,
        clock_rate: game.clock.rate(),
        clock_frozen: game.clock.frozen(),
        time_rate: game.ui.clock.rate,
        time_partial: game.ui.clock.partial,
        time_paused: game.ui.clock.paused,
        command_clocks: game.ui.command_clocks.clone(),
        world_spawn: game.level.world_spawn,
        world_spawn_angle: game.level.world_spawn_angle,
        next_entity: game.next_entity,
        ticked_entities_through: game.ticked_entities_through,
        ordered_announced: game.ordered.announced,
        cat_tick: game.creatures.cat.next_tick,
        phantom_tick: game.stats.rreturn_phantom_tick(),
        player_extra: game.level.player_extra.clone(),
        shoulder_time: game.creatures.shoulder_time.clone(),
        fangs: game.fangs.list.clone(),
        ominous_spawners: game
            .ominous_spawners
            .iter()
            .map(|s| {
                (
                    s.id,
                    horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                        &s.item,
                        Vec3::new(s.position[0], s.position[1], s.position[2]),
                        s.spawn_item_after,
                        s.uuid,
                        s.raw.as_deref(),
                    ),
                )
            })
            .collect(),
        portals_entities: game.portals.entities.clone(),
        end_gateway_cooldowns: game.end.rreturn_gateway_cooldowns(),
        lectern_pulses: game.lectern_pulses.clone(),
        player_beds: game.player_beds.clone(),
        sleep_seen: game.sleep_seen,
        lead_killers: game.lead_killers.clone(),
        lid_openers: game.lid_openers.clone(),
        entity_attachments: game.entity_attachments.clone(),
        player_attachments: game.player_state.attachments.clone(),
        guest_attachments: game.lan.as_ref().map(|host| {
            host.hub.guests.iter().map(|g| (g.uuid, (*g.attachments).clone())).collect()
        }).unwrap_or_default(),
        scheduled_events: game.fns.schedule.clone(),
        scoreboard: game.score.board.clone(),
        sound_seed_state: None,
        cmd_post_effects: game.cmds.post_effects.clone(),
        guests,
        offline_player_data: HashMap::new(),
        levels,
        mob_players: game.lan.as_ref().map(|h| h.mob_players.clone()).unwrap_or_default(),
        disk_restored: true,
        client_using: game.using,
        client_use_duration: game.use_duration,
        client_use_ticks_set: false,
        client_crossbow_sounds: game.crossbow_sounds,
    })
}

pub fn create_slot_checkpoint_on_server(
    game: &mut Game<'_>,
    requester: Option<u128>,
    slot: &str,
) -> bool {
    if !is_valid_slot_name(slot.trim()) {
        return false;
    }
    let now = game.world.game_time;
    bench_begin(&format!("CHECKPOINT CAPTURE [{}]", slot), now);

    let t_raids = std::time::Instant::now();
    let open_cart_keys: Vec<(i32, i32, i32)> = game
        .world
        .block_entities
        .keys()
        .copied()
        .filter(|k| k.1 == horizon_gameplay::minecarts::CART_KEY_Y)
        .collect();
    for key in open_cart_keys {
        game.store_cart_items(key);
    }
    game.save_world_state();
    game.score.storage.save(&game.world_root);
    game.score.stopwatches.save(&game.world_root);
    game.save_scoreboard();
    game.save_bossbars();
    game.save_schedule();
    game.save_maps();
    game.save_portal_arrivals();
    game.raiding.raids.dirty = true;
    game.save_raids();
    game.save_fight();
    game.save_wandering_trader();
    if game.save_game_with(true).is_err()
        || game.save_entities(true).is_err()
        || game.save.flush().is_err()
    {
        let _ = bench_end_and_log();
        return false;
    }
    for parked in game.levels.parked.values() {
        if parked.save.flush().is_err() {
            let _ = bench_end_and_log();
            return false;
        }
    }
    bench_accum(Phase::CaptureRaids, t_raids);

    let t_players = std::time::Instant::now();
    let (guests, online) = capture_guests_snapshot(game);
    bench_add_players(online.len());
    bench_accum(Phase::CapturePlayers, t_players);

    let t_chunks = std::time::Instant::now();
    let mut levels = HashMap::new();
    levels.insert(game.dim, capture_level_snapshot(game, game.dim));
    for (&dim, parked) in &game.levels.parked {
        levels.insert(dim, capture_parked_snapshot(game, dim, parked));
    }
    let total_chunks: usize = levels.values().map(|l| l.chunks.len()).sum();
    let total_mobs: usize = levels.values().map(|l| l.mobs.len()).sum();
    bench_add_chunks(total_chunks);
    bench_add_mobs(total_mobs);
    bench_accum(Phase::CaptureChunks, t_chunks);

    let t_globals = std::time::Instant::now();
    let (anchor_set, policy) = with_state(|st| {
        let anchors: HashSet<u128> = resolve_anchors(&st.anchor, &online, now, requester)
            .into_iter()
            .collect();
        (anchors, st.policy)
    });

    let dragon_fight_dat = fs::read(fight_dat_path(&game.world_root)).ok();
    let trader_dat = fs::read(trader_dat_path(&game.world_root)).ok();
    let Ok(offline_player_data) = try_backup_offline_files(&game.world_root) else {
        let _ = bench_end_and_log();
        return false;
    };

    let mut region_files = HashSet::new();
    let subfolders = ["region", "entities", "poi"];
    let dims = [
        "dimensions/minecraft/overworld",
        "dimensions/minecraft/the_nether",
        "dimensions/minecraft/the_end",
    ];
    for dim_path in dims {
        for sub in subfolders {
            let rdir = game.world_root.join(dim_path).join(sub);
            if !rdir.exists() {
                continue;
            }
            let Ok(entries) = fs::read_dir(rdir) else {
                let _ = bench_end_and_log();
                return false;
            };
            for entry in entries {
                let Ok(e) = entry else {
                    let _ = bench_end_and_log();
                    return false;
                };
                let p = e.path();
                if p.is_file() {
                    region_files.insert(p);
                }
            }
        }
    }

    let timeline = Arc::new(TimelineSnapshot {
        anchor_ids: anchor_set.clone(),
        policy,
        game_time: game.world.game_time,
        day_time: game.world.day_time,
        weather: game.world.weather,
        health_rules: game.world.health_rules,
        rules: game.ui.rules.clone(),
        border: game.border.clone(),
        bossbars: game.bosses.bars.pack(),
        sculk_tracker: game.sculk.tracker,
        sculk_darkness: game.sculk.darkness,
        dragon_fight_dat,
        fight_tag: Some(game.fight.rreturn_pack()),
        trader_dat,
        rand_sequences: game.rand_sequences.clone(),
        region_files,
        difficulty: game.ui.difficulty,
        difficulty_locked: game.level.difficulty_locked,
        clock_rate: game.clock.rate(),
        clock_frozen: game.clock.frozen(),
        time_rate: game.ui.clock.rate,
        time_partial: game.ui.clock.partial,
        time_paused: game.ui.clock.paused,
        command_clocks: game.ui.command_clocks.clone(),
        world_spawn: game.level.world_spawn,
        world_spawn_angle: game.level.world_spawn_angle,
        next_entity: game.next_entity,
        ticked_entities_through: game.ticked_entities_through,
        ordered_announced: game.ordered.announced,
        cat_tick: game.creatures.cat.next_tick,
        phantom_tick: game.stats.rreturn_phantom_tick(),
        player_extra: game.level.player_extra.clone(),
        shoulder_time: game.creatures.shoulder_time.clone(),
        fangs: game.fangs.list.clone(),
        ominous_spawners: game
            .ominous_spawners
            .iter()
            .map(|s| {
                (
                    s.id,
                    horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                        &s.item,
                        Vec3::new(s.position[0], s.position[1], s.position[2]),
                        s.spawn_item_after,
                        s.uuid,
                        s.raw.as_deref(),
                    ),
                )
            })
            .collect(),
        portals_entities: game.portals.entities.clone(),
        end_gateway_cooldowns: game.end.rreturn_gateway_cooldowns(),
        lectern_pulses: game.lectern_pulses.clone(),
        player_beds: game.player_beds.clone(),
        sleep_seen: game.sleep_seen,
        lead_killers: game.lead_killers.clone(),
        lid_openers: game.lid_openers.clone(),
        entity_attachments: game.entity_attachments.clone(),
        player_attachments: game.player_state.attachments.clone(),
        guest_attachments: game.lan.as_ref().map(|host| {
            host.hub.guests.iter().map(|g| (g.uuid, (*g.attachments).clone())).collect()
        }).unwrap_or_default(),
        scheduled_events: game.fns.schedule.clone(),
        scoreboard: game.score.board.clone(),
        sound_seed_state: None,
        cmd_post_effects: game.cmds.post_effects.clone(),
        guests,
        offline_player_data,
        levels,
        mob_players: game.lan.as_ref().map(|h| h.mob_players.clone()).unwrap_or_default(),
        disk_restored: true,
        client_using: game.using,
        client_use_duration: game.use_duration,
        client_use_ticks_set: false,
        client_crossbow_sounds: game.crossbow_sounds,
    });

    bench_accum(Phase::CaptureGlobals, t_globals);

    let t_world_state = std::time::Instant::now();
    if !save_slot_to_disk(&game.world_root, slot, &timeline) {
        let _ = bench_end_and_log();
        return false;
    }

    with_state_mut(|st| {
        st.anchor_ids = anchor_set;
        st.has_checkpoint = true;
        st.last_save_tick = now;
        st.current_intensity = 0.0;
        st.peak_intensity = 0.0;
        st.relax_ticks = 0;
    });

    set_slot_snapshot(slot, timeline);

    let client_snap = ClientBlockSnapshot {
        dim: game.dim,
        chunks: (&*game.world.chunks).clone(),
    };
    set_client_slot_snapshot(slot, client_snap);

    if let Ok(mut tr) = loop_tracker().write() {
        tr.on_checkpoint(game.world.game_time);
    }
    bench_accum(Phase::CaptureWorldState, t_world_state);

    let bench_report = bench_end_and_log();
    log::info!("{}", bench_report);
    append_to_rreturn_log(&game.world_root, &bench_report);
    true
}
