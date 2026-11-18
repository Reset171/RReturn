use super::super::Dim;
use super::snapshots::*;
use super::storage::*;
use super::types::*;
use horizon_client::{
    cooldowns::Cooldowns,
    effects::{PlayerEffects, SavedEffect},
    save::write_dat,
};
use horizon_core::geometry::Direction;
use horizon_world::{
    level::MovingPiston,
    nbt::Tag,
};
use std::{
    fs,
    io::Write,
    path::Path,
};

pub fn save_slot_to_disk(world_root: &Path, slot: &str, snap: &TimelineSnapshot) -> bool {
    let clean = slot.trim();
    if !is_valid_slot_name(clean) {
        return false;
    }
    let cp_dir = slot_dir(world_root, clean);
    let Some(parent_dir) = cp_dir.parent() else {
        return false;
    };
    if fs::create_dir_all(parent_dir).is_err() {
        return false;
    }

    let tag_name = if clean == "default" {
        "checkpoint"
    } else {
        clean
    };
    let staging_dir = parent_dir.join(format!(".{tag_name}.staging"));
    let backup_dir = parent_dir.join(format!(".{tag_name}.bak"));

    if staging_dir.exists() && fs::remove_dir_all(&staging_dir).is_err() {
        return false;
    }

    let write_stage = (|| -> std::io::Result<()> {
        fs::create_dir_all(&staging_dir)?;

        let level_dat = world_root.join("level.dat");
        if !level_dat.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "level.dat missing",
            ));
        }
        try_copy_file_preserve_mtime(&level_dat, &staging_dir.join("level.dat"))?;

        try_copy_dir_recursive(&world_root.join("players"), &staging_dir.join("players"))?;
        try_copy_dir_recursive(&world_root.join("data"), &staging_dir.join("data"))?;
        try_copy_dir_recursive(
            &world_root.join("dimensions"),
            &staging_dir.join("dimensions"),
        )?;
        try_copy_dir_recursive(
            &world_root.join("generated"),
            &staging_dir.join("generated"),
        )?;

        let guest_att_list: Vec<Tag> = snap
            .guest_attachments
            .iter()
            .map(|(&u, att)| {
                Tag::compound([
                    ("uuid", Tag::String((&format!("{:032x}", u)[..]).into())),
                    ("data", Tag::Compound(att.to_nbt())),
                ])
            })
            .collect();
        let entity_att_list: Vec<Tag> = snap
            .entity_attachments
            .iter()
            .map(|(&id, att)| {
                Tag::compound([
                    ("id", Tag::Int(id as i32)),
                    ("data", Tag::Compound(att.to_nbt())),
                ])
            })
            .collect();
        let mut globals_entries = vec![
            ("border", snap.border.pack()),
            ("scoreboard", snap.scoreboard.pack()),
            ("bossbars", snap.bossbars.clone()),
            ("sculk_tracker", snap.sculk_tracker.to_nbt()),
            ("rand_sequences", snap.rand_sequences.to_nbt()),
            (
                "player_attachments",
                Tag::Compound(snap.player_attachments.to_nbt()),
            ),
            ("guest_attachments", Tag::List(guest_att_list)),
            ("entity_attachments", Tag::List(entity_att_list)),
        ];
        if let Some(ls) = snap.levels.get(&Dim::Overworld) {
            globals_entries.push(("border_overworld", ls.border.pack()));
        }
        if let Some(ls) = snap.levels.get(&Dim::Nether) {
            globals_entries.push(("border_nether", ls.border.pack()));
        }
        if let Some(ls) = snap.levels.get(&Dim::End) {
            globals_entries.push(("border_end", ls.border.pack()));
        }
        if let Some(ft) = &snap.fight_tag {
            globals_entries.push(("fight_state", ft.clone()));
        }
        if let Some(pe) = &snap.player_extra {
            globals_entries.push(("player_extra", pe.clone()));
        }
        write_dat(
            &staging_dir.join("rreturn_globals.dat"),
            Tag::compound(globals_entries),
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;

        fn encode_saved_effect(se: &SavedEffect) -> serde_json::Value {
            serde_json::json!({
                "id": se.id,
                "duration": se.duration,
                "amplifier": se.amplifier,
                "ambient": se.ambient,
                "particles": se.particles,
                "icon": se.icon,
                "hidden": se.hidden.as_deref().map(encode_saved_effect),
            })
        }

        fn encode_cooldowns(cooldowns: &Cooldowns) -> Vec<serde_json::Value> {
            extract_active_cooldowns(cooldowns)
                .into_iter()
                .map(|(group, ticks)| {
                    serde_json::json!({
                        "group": &*group,
                        "ticks": ticks,
                    })
                })
                .collect()
        }

        let mut guests_json = Vec::new();
        for g in snap.guests.values() {
            let fx_holder = PlayerEffects {
                list: g.effects.clone(),
                ..Default::default()
            };
            let fx_json: Vec<serde_json::Value> = fx_holder
                .saved()
                .iter()
                .map(encode_saved_effect)
                .collect();
            let cooldowns_json = encode_cooldowns(&g.cooldowns);
            let velocity = [g.velocity.x, g.velocity.y, g.velocity.z];
            let stuck = [g.stuck.0, g.stuck.1];
            let supporting = g.ground.supporting_block.map(|p| [p.x, p.y, p.z]);
            let mut g_obj = serde_json::Map::with_capacity(53);
            g_obj.insert("uuid".to_owned(), serde_json::json!(format!("{:032x}", g.uuid)));
            g_obj.insert("dimension".to_owned(), serde_json::json!(g.dimension));
            g_obj.insert("position".to_owned(), serde_json::json!(g.position));
            g_obj.insert("velocity".to_owned(), serde_json::json!(velocity));
            g_obj.insert("yaw".to_owned(), serde_json::json!(g.yaw));
            g_obj.insert("pitch".to_owned(), serde_json::json!(g.pitch));
            g_obj.insert("head_yaw".to_owned(), serde_json::json!(g.head_yaw));
            g_obj.insert("on_ground".to_owned(), serde_json::json!(g.on_ground));
            g_obj.insert(
                "ground_horizontal_collision".to_owned(),
                serde_json::json!(g.ground.horizontal_collision),
            );
            g_obj.insert(
                "ground_no_blocks".to_owned(),
                serde_json::json!(g.ground.on_ground_no_blocks),
            );
            g_obj.insert("supporting_block".to_owned(), serde_json::json!(supporting));
            g_obj.insert("game_mode".to_owned(), serde_json::json!(g.game_mode));
            g_obj.insert(
                "previous_game_mode".to_owned(),
                serde_json::json!(g.previous_game_mode),
            );
            g_obj.insert("flying".to_owned(), serde_json::json!(g.flying));
            g_obj.insert("flying_speed".to_owned(), serde_json::json!(g.flying_speed));
            g_obj.insert("allow_flight".to_owned(), serde_json::json!(g.allow_flight));
            g_obj.insert("sprinting".to_owned(), serde_json::json!(g.sprinting));
            g_obj.insert("sneaking".to_owned(), serde_json::json!(g.sneaking));
            g_obj.insert("fall_flying".to_owned(), serde_json::json!(g.fall_flying));
            g_obj.insert("sleeping".to_owned(), serde_json::json!(g.sleeping));
            g_obj.insert("shoulders".to_owned(), serde_json::json!(g.shoulders));
            g_obj.insert("living_flags".to_owned(), serde_json::json!(g.living_flags));
            g_obj.insert("stuck".to_owned(), serde_json::json!(stuck));
            g_obj.insert("bed".to_owned(), serde_json::json!(g.bed));
            g_obj.insert("sleep_timer".to_owned(), serde_json::json!(g.sleep_timer));
            g_obj.insert("vehicle".to_owned(), serde_json::json!(g.vehicle));
            g_obj.insert("vehicle_order".to_owned(), serde_json::json!(g.vehicle_order));
            g_obj.insert("vehicle_good".to_owned(), serde_json::json!(g.vehicle_good));
            g_obj.insert("health".to_owned(), serde_json::json!(g.health));
            g_obj.insert("absorption".to_owned(), serde_json::json!(g.absorption));
            g_obj.insert("food".to_owned(), serde_json::json!(g.food));
            g_obj.insert("saturation".to_owned(), serde_json::json!(g.saturation));
            g_obj.insert("exhaustion".to_owned(), serde_json::json!(g.exhaustion));
            g_obj.insert("food_timer".to_owned(), serde_json::json!(g.food_timer));
            g_obj.insert("fire_ticks".to_owned(), serde_json::json!(g.fire_ticks));
            g_obj.insert("air".to_owned(), serde_json::json!(g.air));
            g_obj.insert("ticks_frozen".to_owned(), serde_json::json!(g.ticks_frozen));
            g_obj.insert("fall_distance".to_owned(), serde_json::json!(g.fall_distance));
            g_obj.insert("xp_level".to_owned(), serde_json::json!(g.xp_level));
            g_obj.insert("xp_progress".to_owned(), serde_json::json!(g.xp_progress));
            g_obj.insert("xp_total".to_owned(), serde_json::json!(g.xp_total));
            g_obj.insert("xp_seed".to_owned(), serde_json::json!(g.xp_seed));
            g_obj.insert("selected".to_owned(), serde_json::json!(g.selected));
            g_obj.insert(
                "shield_cooldown".to_owned(),
                serde_json::json!(g.shield_cooldown),
            );
            g_obj.insert("cooldowns".to_owned(), serde_json::Value::Array(cooldowns_json));
            g_obj.insert("seen_credits".to_owned(), serde_json::json!(g.seen_credits));
            g_obj.insert("post_effects".to_owned(), serde_json::json!(g.post_effects));
            g_obj.insert("effects".to_owned(), serde_json::Value::Array(fx_json));
            g_obj.insert(
                "effects_tick_count".to_owned(),
                serde_json::json!(g.effects_tick_count),
            );
            g_obj.insert(
                "effects_changes".to_owned(),
                serde_json::json!(g.effects_changes),
            );
            g_obj.insert(
                "advancements".to_owned(),
                serde_json::json!(g.advancements.criteria),
            );
            g_obj.insert("stats".to_owned(), serde_json::json!(g.stats));
            let hook_val = match &g.hook {
                Some(h) => {
                    let mut hm = serde_json::Map::with_capacity(11);
                    hm.insert("id".to_owned(), serde_json::json!(h.id));
                    hm.insert("position".to_owned(), serde_json::json!(h.position));
                    hm.insert("previous".to_owned(), serde_json::json!(h.previous));
                    hm.insert("velocity".to_owned(), serde_json::json!(h.velocity));
                    hm.insert("rng_state".to_owned(), serde_json::json!(h.rng_state));
                    hm.insert("nibble".to_owned(), serde_json::json!(h.nibble));
                    hm.insert(
                        "time_until_lured".to_owned(),
                        serde_json::json!(h.time_until_lured),
                    );
                    hm.insert(
                        "time_until_hooked".to_owned(),
                        serde_json::json!(h.time_until_hooked),
                    );
                    hm.insert("fish_angle".to_owned(), serde_json::json!(h.fish_angle));
                    hm.insert("open_water".to_owned(), serde_json::json!(h.open_water));
                    hm.insert(
                        "state_bobbing".to_owned(),
                        serde_json::json!(h.state_bobbing),
                    );
                    serde_json::Value::Object(hm)
                }
                None => serde_json::Value::Null,
            };
            g_obj.insert("hook".to_owned(), hook_val);
            guests_json.push(serde_json::Value::Object(g_obj));
        }

        let regions_list: Vec<String> = snap
            .region_files
            .iter()
            .filter_map(|p| {
                p.strip_prefix(world_root)
                    .ok()
                    .map(|rel| rel.to_string_lossy().to_string())
            })
            .collect();

        let anchors_list: Vec<String> = snap
            .anchor_ids
            .iter()
            .map(|u| format!("{:032x}", u))
            .collect();

        let dir_to_u8 = |d: Direction| match d {
            Direction::Down => 0u8,
            Direction::Up => 1,
            Direction::North => 2,
            Direction::South => 3,
            Direction::West => 4,
            Direction::East => 5,
        };
        let encode_piston = |p: &MovingPiston| {
            serde_json::json!({
                "moved_state": p.moved_state.0,
                "direction": dir_to_u8(p.direction),
                "extending": p.extending,
                "source": p.source,
                "progress": p.progress,
                "progress_o": p.progress_o,
                "last_ticked": p.last_ticked,
                "death_ticks": p.death_ticks,
                "id": p.id,
            })
        };

        let mut loaded_chunks_json = serde_json::Map::new();
        let mut level_rng_json = serde_json::Map::new();
        let mut random_tick_values_json = serde_json::Map::new();
        let mut end_clocks_json = serde_json::Map::new();
        let mut level_redstone_json = serde_json::Map::new();
        for (&dim, lsnap) in &snap.levels {
            let mut coords: Vec<[i32; 2]> =
                lsnap.chunks.keys().map(|&(cx, cz)| [cx, cz]).collect();
            coords.sort_unstable();
            loaded_chunks_json.insert(dim.id().to_owned(), serde_json::json!(coords));
            level_rng_json.insert(dim.id().to_owned(), serde_json::json!(lsnap.random.state()));
            random_tick_values_json.insert(
                dim.id().to_owned(),
                serde_json::json!(lsnap.random_tick_value),
            );
            end_clocks_json.insert(dim.id().to_owned(), serde_json::json!(lsnap.end_clock));
            let comps: Vec<serde_json::Value> = lsnap
                .comparator_outputs
                .iter()
                .map(|(pos, out)| serde_json::json!([pos.x, pos.y, pos.z, out]))
                .collect();
            let torches: Vec<serde_json::Value> = lsnap
                .torch_toggles
                .iter()
                .map(|(pos, time)| serde_json::json!([pos.x, pos.y, pos.z, time]))
                .collect();
            let moving: Vec<serde_json::Value> = lsnap
                .moving_pistons
                .iter()
                .map(|(pos, p)| serde_json::json!([pos.x, pos.y, pos.z, encode_piston(p)]))
                .collect();
            let pushes: Vec<serde_json::Value> = lsnap
                .piston_pushes
                .iter()
                .map(|(pos, p, next_p)| {
                    serde_json::json!([pos.x, pos.y, pos.z, encode_piston(p), next_p])
                })
                .collect();
            let events: Vec<serde_json::Value> = lsnap
                .block_events
                .iter()
                .map(|e| {
                    serde_json::json!([
                        e.pos.x, e.pos.y, e.pos.z, e.block.0, e.param_a, e.param_b
                    ])
                })
                .collect();
            let hurt_by: Vec<serde_json::Value> = lsnap
                .last_hurt_by_player
                .iter()
                .map(|(k, v)| serde_json::json!([k, v.0]))
                .collect();
            let love: Vec<serde_json::Value> = lsnap
                .love_cause
                .iter()
                .map(|(k, v)| serde_json::json!([k, v]))
                .collect();
            let uuids_list: Vec<serde_json::Value> = lsnap
                .persist_uuids
                .iter()
                .map(|(&id, u)| serde_json::json!([id, u[0], u[1], u[2], u[3]]))
                .collect();
            let mob_rt: Vec<serde_json::Value> = lsnap
                .mobs
                .iter()
                .map(|m| {
                    let vec_arr = [
                        m.extra.vector[0].to_bits(),
                        m.extra.vector[1].to_bits(),
                        m.extra.vector[2].to_bits(),
                    ];
                    let stamp = m.extra.anger_stamp.map(|(a, b)| [i64::from(a), i64::from(b)]);
                    serde_json::Value::Array(vec![
                        serde_json::json!(m.id),
                        serde_json::json!(m.age),
                        serde_json::json!(m.invulnerable),
                        serde_json::json!(m.last_hurt.to_bits()),
                        serde_json::json!(m.swell),
                        serde_json::json!(m.previous_swell),
                        serde_json::json!(m.ambient_timer),
                        serde_json::json!(m.random.state()),
                        serde_json::json!(m.extra.anger),
                        serde_json::json!(m.extra.anger_end),
                        serde_json::json!(stamp),
                        serde_json::json!(m.extra.counter),
                        serde_json::json!(m.extra.counter2),
                        serde_json::json!(m.extra.anim_ticks),
                        serde_json::json!(m.extra.egg_time),
                        serde_json::json!(m.extra.flag),
                        serde_json::json!(m.extra.flag2),
                        serde_json::json!(vec_arr),
                        serde_json::json!(m.extra.ints),
                        serde_json::json!(m.extra.adopted_body.is_some()),
                    ])
                })
                .collect();
            let item_rt: Vec<serde_json::Value> = lsnap
                .dropped
                .iter()
                .map(|d| {
                    serde_json::json!([
                        d.id,
                        d.ticks,
                        d.bob_offset.to_bits(),
                        d.random.state()
                    ])
                })
                .collect();
            let orb_rt: Vec<serde_json::Value> = lsnap
                .orbs
                .iter()
                .map(|o| serde_json::json!([o.id, o.ticks]))
                .collect();
            let openers_rt: Vec<serde_json::Value> = lsnap
                .chest_openers
                .iter()
                .map(|&((x, y, z), (count, range))| {
                    serde_json::json!([x, y, z, count, range.to_bits()])
                })
                .collect();
            let lid_rt: Vec<serde_json::Value> = lsnap
                .lid_openers
                .iter()
                .map(|(&(x, y, z), &c)| serde_json::json!([x, y, z, c]))
                .collect();
            let lectern_rt: Vec<serde_json::Value> = lsnap
                .lectern_pulses
                .iter()
                .map(|&((x, y, z), t)| serde_json::json!([x, y, z, t]))
                .collect();
            let portals_rt: Vec<serde_json::Value> = lsnap
                .portals_entities
                .iter()
                .map(|(&id, st)| serde_json::json!([id, st.cooldown]))
                .collect();
            let gateways_rt: Vec<serde_json::Value> = lsnap
                .gateway_anims
                .iter()
                .map(|&([x, y, z], gw)| {
                    serde_json::json!([x, y, z, gw.age, gw.teleport_cooldown])
                })
                .collect();
            let hoppers_rt: Vec<serde_json::Value> = lsnap
                .hoppers
                .iter()
                .map(|(&(x, y, z), h)| serde_json::json!([x, y, z, h.cooldown]))
                .collect();
            let tnts_rt: Vec<serde_json::Value> = lsnap
                .tnts
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "pos": [t.position.x, t.position.y, t.position.z],
                        "vel": [t.velocity.x, t.velocity.y, t.velocity.z],
                        "fuse": t.fuse,
                        "power": t.power,
                        "no_gravity": t.no_gravity,
                        "on_ground": t.on_ground,
                        "fall_distance": t.fall_distance,
                        "rot": [t.rotation[0], t.rotation[1]],
                    })
                })
                .collect();
            let falling_rt: Vec<serde_json::Value> = lsnap
                .falling
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "id": f.id,
                        "state": f.state.0,
                        "pos": [f.position.x, f.position.y, f.position.z],
                        "vel": [f.velocity.x, f.velocity.y, f.velocity.z],
                        "age": f.age,
                        "no_gravity": f.no_gravity,
                        "drop_item": f.drop_item,
                        "on_ground": f.on_ground,
                        "fall_distance": f.fall_distance,
                        "rot": [f.rotation[0], f.rotation[1]],
                    })
                })
                .collect();
            let boat_vels: Vec<serde_json::Value> = lsnap
                .boats
                .iter()
                .map(|b| serde_json::json!([b.id, b.velocity.x, b.velocity.y, b.velocity.z]))
                .collect();
            let cart_vels: Vec<serde_json::Value> = lsnap
                .carts
                .iter()
                .map(|c| serde_json::json!([c.id, c.velocity.x, c.velocity.y, c.velocity.z]))
                .collect();
            let projectiles_rt: Vec<serde_json::Value> = lsnap
                .projectiles
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "kind": p.kind.id(),
                        "pos": [p.position.x, p.position.y, p.position.z],
                        "vel": [p.velocity.x, p.velocity.y, p.velocity.z],
                        "yaw": p.yaw,
                        "pitch": p.pitch,
                        "age": p.age,
                        "stuck": p.stuck,
                    })
                })
                .collect();
            let mut rs_obj = serde_json::Map::with_capacity(22);
            rs_obj.insert("comparator_outputs".to_owned(), serde_json::Value::Array(comps));
            rs_obj.insert("torch_toggles".to_owned(), serde_json::Value::Array(torches));
            rs_obj.insert("moving_pistons".to_owned(), serde_json::Value::Array(moving));
            rs_obj.insert("piston_pushes".to_owned(), serde_json::Value::Array(pushes));
            rs_obj.insert("block_events".to_owned(), serde_json::Value::Array(events));
            rs_obj.insert("last_hurt_by_player".to_owned(), serde_json::Value::Array(hurt_by));
            rs_obj.insert("love_cause".to_owned(), serde_json::Value::Array(love));
            rs_obj.insert("persist_uuids".to_owned(), serde_json::Value::Array(uuids_list));
            rs_obj.insert("mob_runtime".to_owned(), serde_json::Value::Array(mob_rt));
            rs_obj.insert("item_runtime".to_owned(), serde_json::Value::Array(item_rt));
            rs_obj.insert("orb_runtime".to_owned(), serde_json::Value::Array(orb_rt));
            rs_obj.insert("chest_openers".to_owned(), serde_json::Value::Array(openers_rt));
            rs_obj.insert("lid_openers".to_owned(), serde_json::Value::Array(lid_rt));
            rs_obj.insert("lectern_pulses".to_owned(), serde_json::Value::Array(lectern_rt));
            rs_obj.insert("portals_entities".to_owned(), serde_json::Value::Array(portals_rt));
            rs_obj.insert("gateway_anims".to_owned(), serde_json::Value::Array(gateways_rt));
            rs_obj.insert("hoppers".to_owned(), serde_json::Value::Array(hoppers_rt));
            rs_obj.insert("tnts".to_owned(), serde_json::Value::Array(tnts_rt));
            rs_obj.insert("falling".to_owned(), serde_json::Value::Array(falling_rt));
            rs_obj.insert("boat_velocities".to_owned(), serde_json::Value::Array(boat_vels));
            rs_obj.insert("cart_velocities".to_owned(), serde_json::Value::Array(cart_vels));
            rs_obj.insert("projectiles".to_owned(), serde_json::Value::Array(projectiles_rt));
            level_redstone_json.insert(dim.id().to_owned(), serde_json::Value::Object(rs_obj));
        }

        let mob_players_json: Vec<serde_json::Value> = snap
            .mob_players
            .iter()
            .map(|(k, v)| serde_json::json!([k, v]))
            .collect();
        let lectern_pulses_json: Vec<serde_json::Value> = snap
            .lectern_pulses
            .iter()
            .map(|&((x, y, z), t)| serde_json::json!([x, y, z, t]))
            .collect();
        let player_beds_json: Vec<serde_json::Value> = snap
            .player_beds
            .iter()
            .map(|&(dim, [x, y, z])| serde_json::json!([dim.id(), x, y, z]))
            .collect();
        let lid_openers_json: Vec<serde_json::Value> = snap
            .lid_openers
            .iter()
            .map(|(&(x, y, z), &c)| serde_json::json!([x, y, z, c]))
            .collect();
        let portals_entities_json: Vec<serde_json::Value> = snap
            .portals_entities
            .iter()
            .map(|(&id, st)| serde_json::json!([id, st.cooldown]))
            .collect();
        let end_gateways_json: Vec<serde_json::Value> = snap
            .end_gateway_cooldowns
            .iter()
            .map(|(&id, &cd)| serde_json::json!([id, cd]))
            .collect();
        let shoulder_time_json: Vec<serde_json::Value> = snap
            .shoulder_time
            .iter()
            .map(|(&[u0, u1, u2, u3], &t)| serde_json::json!([u0, u1, u2, u3, t]))
            .collect();
        let scheduled_json: Vec<serde_json::Value> = snap
            .scheduled_events
            .iter()
            .map(|s| serde_json::json!([s.id, s.time, s.seq]))
            .collect();
        let command_clocks_json: Vec<serde_json::Value> = snap
            .command_clocks
            .iter()
            .map(|(k, (ticks, c))| serde_json::json!([k, ticks, c.partial, c.rate, c.paused]))
            .collect();
        let lead_killers_json: Vec<serde_json::Value> = snap
            .lead_killers
            .iter()
            .map(|(&id, &(u, dim))| serde_json::json!([id, format!("{:032x}", u), dim.id()]))
            .collect();

        let saved_at_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let sleep_seen_arr = [snap.sleep_seen.0, snap.sleep_seen.1];
        let sculk_darkness_arr = [snap.sculk_darkness.0, snap.sculk_darkness.1];
        let mut hr_map = serde_json::Map::with_capacity(5);
        hr_map.insert(
            "natural_regeneration".to_owned(),
            serde_json::json!(snap.health_rules.natural_regeneration),
        );
        hr_map.insert(
            "drowning_damage".to_owned(),
            serde_json::json!(snap.health_rules.drowning_damage),
        );
        hr_map.insert(
            "fall_damage".to_owned(),
            serde_json::json!(snap.health_rules.fall_damage),
        );
        hr_map.insert(
            "fire_damage".to_owned(),
            serde_json::json!(snap.health_rules.fire_damage),
        );
        hr_map.insert(
            "freeze_damage".to_owned(),
            serde_json::json!(snap.health_rules.freeze_damage),
        );

        let mut meta_map = serde_json::Map::with_capacity(41);
        meta_map.insert("slot".to_owned(), serde_json::json!(clean));
        meta_map.insert("saved_at_unix".to_owned(), serde_json::json!(saved_at_unix));
        meta_map.insert("game_time".to_owned(), serde_json::json!(snap.game_time));
        meta_map.insert("day_time".to_owned(), serde_json::json!(snap.day_time));
        meta_map.insert("difficulty".to_owned(), serde_json::json!(snap.difficulty));
        meta_map.insert(
            "difficulty_locked".to_owned(),
            serde_json::json!(snap.difficulty_locked),
        );
        meta_map.insert("clock_rate".to_owned(), serde_json::json!(snap.clock_rate));
        meta_map.insert("clock_frozen".to_owned(), serde_json::json!(snap.clock_frozen));
        meta_map.insert("time_rate".to_owned(), serde_json::json!(snap.time_rate));
        meta_map.insert("time_partial".to_owned(), serde_json::json!(snap.time_partial));
        meta_map.insert("time_paused".to_owned(), serde_json::json!(snap.time_paused));
        meta_map.insert(
            "command_clocks".to_owned(),
            serde_json::Value::Array(command_clocks_json),
        );
        meta_map.insert("world_spawn".to_owned(), serde_json::json!(snap.world_spawn));
        meta_map.insert(
            "world_spawn_angle".to_owned(),
            serde_json::json!(snap.world_spawn_angle),
        );
        meta_map.insert("next_entity".to_owned(), serde_json::json!(snap.next_entity));
        meta_map.insert(
            "ticked_entities_through".to_owned(),
            serde_json::json!(snap.ticked_entities_through),
        );
        meta_map.insert("cat_tick".to_owned(), serde_json::json!(snap.cat_tick));
        meta_map.insert("phantom_tick".to_owned(), serde_json::json!(snap.phantom_tick));
        meta_map.insert(
            "shoulder_time".to_owned(),
            serde_json::Value::Array(shoulder_time_json),
        );
        meta_map.insert(
            "portals_entities".to_owned(),
            serde_json::Value::Array(portals_entities_json),
        );
        meta_map.insert(
            "end_gateways".to_owned(),
            serde_json::Value::Array(end_gateways_json),
        );
        meta_map.insert(
            "lectern_pulses".to_owned(),
            serde_json::Value::Array(lectern_pulses_json),
        );
        meta_map.insert(
            "player_beds".to_owned(),
            serde_json::Value::Array(player_beds_json),
        );
        meta_map.insert(
            "lid_openers".to_owned(),
            serde_json::Value::Array(lid_openers_json),
        );
        meta_map.insert(
            "scheduled_events".to_owned(),
            serde_json::Value::Array(scheduled_json),
        );
        meta_map.insert("sleep_seen".to_owned(), serde_json::json!(sleep_seen_arr));
        meta_map.insert(
            "sound_seed_state".to_owned(),
            serde_json::json!(snap.sound_seed_state),
        );
        meta_map.insert(
            "cmd_post_effects".to_owned(),
            serde_json::json!(snap.cmd_post_effects),
        );
        meta_map.insert(
            "sculk_darkness".to_owned(),
            serde_json::json!(sculk_darkness_arr),
        );
        meta_map.insert("health_rules".to_owned(), serde_json::Value::Object(hr_map));
        meta_map.insert("policy".to_owned(), snap.policy.to_json());
        meta_map.insert("anchor_ids".to_owned(), serde_json::json!(anchors_list));
        meta_map.insert("region_files".to_owned(), serde_json::json!(regions_list));
        meta_map.insert(
            "loaded_chunks".to_owned(),
            serde_json::Value::Object(loaded_chunks_json),
        );
        meta_map.insert("level_rng".to_owned(), serde_json::Value::Object(level_rng_json));
        meta_map.insert(
            "random_tick_values".to_owned(),
            serde_json::Value::Object(random_tick_values_json),
        );
        meta_map.insert("end_clocks".to_owned(), serde_json::Value::Object(end_clocks_json));
        meta_map.insert(
            "level_redstone".to_owned(),
            serde_json::Value::Object(level_redstone_json),
        );
        meta_map.insert(
            "lead_killers".to_owned(),
            serde_json::Value::Array(lead_killers_json),
        );
        meta_map.insert(
            "mob_players".to_owned(),
            serde_json::Value::Array(mob_players_json),
        );
        meta_map.insert("guests".to_owned(), serde_json::Value::Array(guests_json));
        let meta = serde_json::Value::Object(meta_map);

        let json_str = serde_json::to_string_pretty(&meta).map_err(std::io::Error::other)?;
        let mut meta_file = fs::File::create(staging_dir.join("meta.json"))?;
        meta_file.write_all(json_str.as_bytes())?;
        meta_file.sync_all()?;
        Ok(())
    })();

    if write_stage.is_err() {
        let _ = fs::remove_dir_all(&staging_dir);
        return false;
    }

    if backup_dir.exists() && fs::remove_dir_all(&backup_dir).is_err() {
        let _ = fs::remove_dir_all(&staging_dir);
        return false;
    }

    let had_existing = cp_dir.exists();
    if had_existing && fs::rename(&cp_dir, &backup_dir).is_err() {
        let _ = fs::remove_dir_all(&staging_dir);
        return false;
    }

    if fs::rename(&staging_dir, &cp_dir).is_err() {
        if had_existing {
            let _ = fs::rename(&backup_dir, &cp_dir);
        }
        let _ = fs::remove_dir_all(&staging_dir);
        return false;
    }

    if had_existing {
        let _ = fs::remove_dir_all(&backup_dir);
    }

    true
}

pub fn save_checkpoint_to_disk(world_root: &Path, snap: &TimelineSnapshot) -> bool {
    save_slot_to_disk(world_root, "default", snap)
}