use super::super::{game_mode::mode_from_id, Game};
use super::rrtl::{MobPoseRecord, ParticleTickState};
use super::scrub::ensure_scrub_pointer_free;
use super::scrub_audio::play_scrub_sounds;
use super::scrub_timeline::{
    EntityPoseRecord, sync_mob_appearance_from_template, with_scrub_ring,
};
use super::types::*;
use super::ui_snapshot::*;
use horizon_client::{
    block_entity_anim::ChestLid,
    containers::BlockEntity,
    fast_hash::FxHashMap,
    particles::Particle,
};
use horizon_core::geometry::Vec3;
use horizon_world::registry::BlockStateId;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

thread_local! {
    static LAST_PREVIEW_UI: RefCell<Option<Arc<ClientUiSnapshot>>> = const { RefCell::new(None) };
    static LAST_PREVIEW_BES: RefCell<Option<Arc<Vec<((i32, i32, i32), BlockEntity)>>>> = const { RefCell::new(None) };
}

pub fn reset_scrub_ui_preview_cache() {
    LAST_PREVIEW_UI.with(|c| *c.borrow_mut() = None);
    LAST_PREVIEW_BES.with(|c| *c.borrow_mut() = None);
    reset_chat_restore_cache();
}

#[derive(Clone, Copy, Debug)]
pub struct InterpolatedMobPose {
    pub template_idx: u32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub body_yaw: f32,
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub limb_position: f32,
    pub limb_speed: f32,
    pub attack_anim: f32,
    pub swell: f32,
    pub health: f32,
    pub alive: bool,
    pub hurt_time: i32,
    pub death_time: i32,
    pub sheared: bool,
    pub color: u8,
    pub size: i32,
    pub profession: u8,
    pub villager_level: i32,
    pub variant: u8,
    pub variant2: u8,
    pub variant3: u8,
    pub age: i32,
    pub sitting: bool,
    pub sitting_pose: bool,
    pub charged: bool,
    pub invisible: bool,
    pub glowing: bool,
    pub on_fire: bool,
}

fn interpolated_pose_from_record(r: &MobPoseRecord) -> InterpolatedMobPose {
    InterpolatedMobPose {
        template_idx: r.template_idx,
        pos: r.pos,
        yaw: r.yaw,
        body_yaw: r.body_yaw,
        head_yaw: r.head_yaw,
        head_pitch: r.head_pitch,
        limb_position: r.limb_position,
        limb_speed: r.limb_speed,
        attack_anim: r.attack_anim,
        swell: r.swell,
        health: r.health,
        alive: r.alive,
        hurt_time: r.hurt_time,
        death_time: r.death_time,
        sheared: r.sheared,
        color: r.color,
        size: r.size,
        profession: r.profession,
        villager_level: r.villager_level,
        variant: r.variant,
        variant2: r.variant2,
        variant3: r.variant3,
        age: r.age,
        sitting: r.sitting,
        sitting_pose: r.sitting_pose,
        charged: r.charged,
        invisible: r.invisible,
        glowing: r.glowing,
        on_fire: r.on_fire,
    }
}

#[inline]
pub fn lerp_f64(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

#[inline]
pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
pub fn wrap_degrees(deg: f32) -> f32 {
    let mut d = deg % 360.0;
    if d >= 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

#[inline]
pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    a + wrap_degrees(b - a) * t
}

pub fn interpolate_mob_pose(
    left: &MobPoseRecord,
    right: &MobPoseRecord,
    alpha: f32,
) -> InterpolatedMobPose {
    let a64 = f64::from(alpha);
    let pick_right = alpha > 0.5;
    InterpolatedMobPose {
        template_idx: if pick_right {
            right.template_idx
        } else {
            left.template_idx
        },
        pos: [
            lerp_f64(left.pos[0], right.pos[0], a64),
            lerp_f64(left.pos[1], right.pos[1], a64),
            lerp_f64(left.pos[2], right.pos[2], a64),
        ],
        yaw: lerp_angle(left.yaw, right.yaw, alpha),
        body_yaw: lerp_angle(left.body_yaw, right.body_yaw, alpha),
        head_yaw: lerp_angle(left.head_yaw, right.head_yaw, alpha),
        head_pitch: lerp_f32(left.head_pitch, right.head_pitch, alpha),
        limb_position: lerp_f32(left.limb_position, right.limb_position, alpha),
        limb_speed: lerp_f32(left.limb_speed, right.limb_speed, alpha),
        attack_anim: lerp_f32(left.attack_anim, right.attack_anim, alpha),
        swell: lerp_f32(left.swell, right.swell, alpha),
        health: lerp_f32(left.health, right.health, alpha),
        alive: if pick_right { right.alive } else { left.alive },
        hurt_time: if pick_right {
            right.hurt_time
        } else {
            left.hurt_time
        },
        death_time: if pick_right {
            right.death_time
        } else {
            left.death_time
        },
        sheared: if pick_right {
            right.sheared
        } else {
            left.sheared
        },
        color: if pick_right { right.color } else { left.color },
        size: if pick_right { right.size } else { left.size },
        profession: if pick_right { right.profession } else { left.profession },
        villager_level: if pick_right { right.villager_level } else { left.villager_level },
        variant: if pick_right { right.variant } else { left.variant },
        variant2: if pick_right { right.variant2 } else { left.variant2 },
        variant3: if pick_right { right.variant3 } else { left.variant3 },
        age: if pick_right { right.age } else { left.age },
        sitting: if pick_right { right.sitting } else { left.sitting },
        sitting_pose: if pick_right { right.sitting_pose } else { left.sitting_pose },
        charged: if pick_right { right.charged } else { left.charged },
        invisible: if pick_right {
            right.invisible
        } else {
            left.invisible
        },
        glowing: if pick_right { right.glowing } else { left.glowing },
        on_fire: if pick_right { right.on_fire } else { left.on_fire },
    }
}

fn interpolate_entity_poses(
    left: &[EntityPoseRecord],
    right: &[EntityPoseRecord],
    alpha: f32,
) -> FxHashMap<u32, EntityPoseRecord> {
    let a64 = f64::from(alpha);
    let mut right_map: FxHashMap<u32, &EntityPoseRecord> = FxHashMap::default();
    for r in right {
        right_map.insert(r.id, r);
    }
    let mut out = FxHashMap::default();
    for l in left {
        if let Some(r) = right_map.get(&l.id) {
            out.insert(
                l.id,
                EntityPoseRecord {
                    id: l.id,
                    pos: [
                        lerp_f64(l.pos[0], r.pos[0], a64),
                        lerp_f64(l.pos[1], r.pos[1], a64),
                        lerp_f64(l.pos[2], r.pos[2], a64),
                    ],
                    vel: [
                        lerp_f64(l.vel[0], r.vel[0], a64),
                        lerp_f64(l.vel[1], r.vel[1], a64),
                        lerp_f64(l.vel[2], r.vel[2], a64),
                    ],
                    yaw: lerp_angle(l.yaw, r.yaw, alpha),
                    pitch: lerp_f32(l.pitch, r.pitch, alpha),
                    extra_i32: lerp_f32(l.extra_i32 as f32, r.extra_i32 as f32, alpha).round()
                        as i32,
                },
            );
        } else if alpha <= 0.5 {
            out.insert(l.id, *l);
        }
    }
    if alpha > 0.5 {
        for r in right {
            out.entry(r.id).or_insert(*r);
        }
    }
    out
}

fn interpolate_thrown_potions(
    left: &[(u32, [f64; 3], [f64; 3], u32)],
    right: &[(u32, [f64; 3], [f64; 3], u32)],
    alpha: f32,
) -> Vec<(u32, [f64; 3], u32)> {
    let a64 = f64::from(alpha);
    let mut right_map: FxHashMap<u32, ([f64; 3], u32)> = FxHashMap::default();
    for &(id, pos, _, age) in right {
        right_map.insert(id, (pos, age));
    }
    let mut out = Vec::with_capacity(left.len().max(right.len()));
    let mut seen = HashSet::new();
    for &(id, lpos, _, lage) in left {
        if let Some(&(rpos, rage)) = right_map.get(&id) {
            seen.insert(id);
            out.push((
                id,
                [
                    lerp_f64(lpos[0], rpos[0], a64),
                    lerp_f64(lpos[1], rpos[1], a64),
                    lerp_f64(lpos[2], rpos[2], a64),
                ],
                if alpha > 0.5 { rage } else { lage },
            ));
        } else if alpha <= 0.5 {
            seen.insert(id);
            out.push((id, lpos, lage));
        }
    }
    if alpha > 0.5 {
        for &(id, rpos, _, rage) in right {
            if !seen.contains(&id) {
                out.push((id, rpos, rage));
            }
        }
    }
    out
}

pub fn apply_interpolated_poses_to_game(game: &mut Game<'_>, target_time: f64) {
    let Some((
        left_frame,
        right_frame,
        alpha,
        left_particles,
        right_particles,
        left_mobs,
        right_mobs,
    )) = with_scrub_ring(|ring| {
        let (l, r, a) = ring.find_frame_pair(target_time)?;
        let lp = ring.resolve_particles(&l.particles);
        let rp = ring.resolve_particles(&r.particles);
        let lm = ring.resolve_mobs(&l.mobs);
        let rm = ring.resolve_mobs(&r.mobs);
        Some((l.clone(), r.clone(), a, lp, rp, lm, rm))
    })
    else {
        return;
    };

    let a64 = f64::from(alpha);
    let chosen_frame = if alpha > 0.5 {
        &right_frame
    } else {
        &left_frame
    };

    if chosen_frame.dim != game.dim {
        let target_chunks =
            with_scrub_ring(|ring| ring.dim_snapshots.get(&chosen_frame.dim).cloned());
        if let Some(chunks) = target_chunks {
            game.dim = chosen_frame.dim;
            game.world.chunks = Default::default();
            let num_sections = game.world.layout().sections() as i32;
            for ((cx, cz), chunk) in chunks {
                game.world.chunks.insert((cx, cz), chunk);
                for s in 0..num_sections {
                    game.world.dirty_sections.insert((cx, s, cz));
                }
            }
            game.world.revision = game.world.revision.wrapping_add(1);
        }
    }

    game.extras.frame_partial_ticks = Some((alpha, alpha));

    game.world.day_time = left_frame.day_time;

    let l_rain = left_frame.weather.rain_level.1;
    let r_rain = right_frame.weather.rain_level.1;
    let l_thunder = left_frame.weather.thunder_level.1;
    let r_thunder = right_frame.weather.thunder_level.1;
    game.world.weather = left_frame.weather;
    game.world.weather.rain_level = (l_rain, r_rain);
    game.world.weather.thunder_level = (l_thunder, r_thunder);

    game.border = (*chosen_frame.border).clone();
    game.boss_overlay = chosen_frame.boss_overlay.clone();
    game.sculk.darkness = (
        lerp_f32(left_frame.sculk_darkness.0, right_frame.sculk_darkness.0, alpha),
        lerp_f32(left_frame.sculk_darkness.1, right_frame.sculk_darkness.1, alpha),
    );
    game.flash = chosen_frame.flash;

    let mut be_anims = (*left_frame.be_anims).clone();
    for (&pos, right_lid) in &right_frame.be_anims.chests {
        let entry = be_anims.chests.entry(pos).or_insert(ChestLid::default());
        entry.o_openness = entry.openness;
        entry.openness = right_lid.openness;
        entry.should_be_open = right_lid.should_be_open;
    }
    for (&pos, right_shulker) in &right_frame.be_anims.shulkers {
        if let Some(left_shulker) = be_anims.shulkers.get_mut(&pos) {
            left_shulker.progress_old = left_shulker.progress;
            left_shulker.progress = right_shulker.progress;
            left_shulker.status = right_shulker.status;
        }
    }
    for (&pos, right_book) in &right_frame.be_anims.books {
        if let Some(left_book) = be_anims.books.get_mut(&pos) {
            left_book.o_open = left_book.open;
            left_book.open = right_book.open;
            left_book.o_flip = left_book.flip;
            left_book.flip = right_book.flip;
            left_book.o_rot = left_book.rot;
            left_book.rot = right_book.rot;
        }
    }
    let preserved_kinds = std::mem::take(&mut game.be_anims.kinds);
    game.be_anims = be_anims;
    game.be_anims.kinds = preserved_kinds;

    let mut right_spawners = FxHashMap::default();
    for sp in &right_frame.spawner_spins {
        right_spawners.insert(sp.pos, *sp);
    }
    for lsp in &left_frame.spawner_spins {
        if let Some(BlockEntity::Spawner(sp)) =
            game.world
                .block_entities
                .get_mut(&(lsp.pos[0], lsp.pos[1], lsp.pos[2]))
        {
            sp.o_spin = lsp.spin;
            sp.spin = right_spawners
                .get(&lsp.pos)
                .map_or(lsp.spin, |rsp| rsp.spin);
            sp.client_delay = lsp.client_delay;
        }
    }

    game.world.moving_pistons = left_frame.moving_pistons.clone();

    let scrub_policy = resolve_local_client_policy(game, true);

    let interp_p_pos = [
        lerp_f64(left_frame.player_pos[0], right_frame.player_pos[0], a64),
        lerp_f64(left_frame.player_pos[1], right_frame.player_pos[1], a64),
        lerp_f64(left_frame.player_pos[2], right_frame.player_pos[2], a64),
    ];
    let interp_p_yaw = lerp_angle(left_frame.player_yaw, right_frame.player_yaw, alpha);
    let interp_p_pitch = lerp_f32(left_frame.player_pitch, right_frame.player_pitch, alpha);

    let interp_fov = lerp_f32(left_frame.fov_modifier, right_frame.fov_modifier, alpha);
    if scrub_policy.player_motion && interp_fov > 0.05 {
        game.view.fov_modifier = interp_fov;
        game.view.old_fov_modifier = interp_fov;
    }

    let interp_walk = lerp_f32(
        left_frame.avatar_walk_dist,
        right_frame.avatar_walk_dist,
        alpha,
    );
    let interp_bob = lerp_f32(left_frame.avatar_bob, right_frame.avatar_bob, alpha);
    if scrub_policy.player_motion {
        game.avatar.walk_dist = interp_walk;
        game.avatar.walk_dist_o = interp_walk;
        game.avatar.bob = interp_bob;
        game.avatar.bob_o = interp_bob;
    }
    if scrub_policy.player_inventory {
        game.selected = chosen_frame.player_selected;
        game.inventory = (*chosen_frame.player_inventory).clone();
        game.armor = (*chosen_frame.player_armor).clone();
        game.offhand = (*chosen_frame.player_offhand).clone();
    }
    if scrub_policy.player_ender_chest {
        game.ender_items = (*chosen_frame.ender_items).clone();
    }
    if scrub_policy.player_abilities {
        if chosen_frame.player_game_mode != game.mode_id() {
            game.set_mode_id(chosen_frame.player_game_mode);
        }
        game.previous_mode = chosen_frame
            .player_previous_game_mode
            .map(mode_from_id);
        game.acting_abilities = chosen_frame.player_acting_abilities;
    }
    if scrub_policy.player_experience {
        game.experience = chosen_frame.player_experience;
    }
    let ui_changed = LAST_PREVIEW_UI.with(|c| {
        let mut cur = c.borrow_mut();
        if let Some(prev) = cur.as_ref()
            && (Arc::ptr_eq(prev, &chosen_frame.ui) || **prev == *chosen_frame.ui)
        {
            false
        } else {
            *cur = Some(Arc::clone(&chosen_frame.ui));
            true
        }
    });
    if ui_changed {
        if scrub_policy.player_inventory || scrub_policy.player_recipe_book || scrub_policy.player_abilities {
            apply_client_ui_exact(
                game,
                &chosen_frame.ui,
                scrub_policy.player_inventory,
                scrub_policy.player_recipe_book,
                true,
            );
        } else {
            apply_chat_history_from_snapshot(game, &chosen_frame.ui, false);
        }
    } else if scrub_policy.player_inventory {
        game.carried = (*chosen_frame.carried).clone();
        if chosen_frame.ui.craft_table.is_none()
            && chosen_frame.ui.craft_size < 3
            && chosen_frame.ui.station.is_none()
        {
            let need = chosen_frame.craft_grid.len();
            if game.craft_grid.len() < need {
                game.craft_grid.resize(need, None);
            }
            for (dst, src) in game.craft_grid.iter_mut().zip(chosen_frame.craft_grid.iter()) {
                *dst = src.clone();
            }
        }
    }
    ensure_scrub_pointer_free(game);
    if scrub_policy.block_entities {
        if let Some(bes) = &chosen_frame.open_container_bes {
            let changed = LAST_PREVIEW_BES.with(|c| {
                let mut cur = c.borrow_mut();
                if cur.as_ref().is_some_and(|p| Arc::ptr_eq(p, bes)) {
                    false
                } else {
                    *cur = Some(Arc::clone(bes));
                    true
                }
            });
            if changed {
                for (pos, be) in bes.iter() {
                    game.world.block_entities.insert(*pos, be.clone());
                }
            }
        } else {
            LAST_PREVIEW_BES.with(|c| *c.borrow_mut() = None);
        }
    }
    if scrub_policy.player_motion {
        game.using = chosen_frame.player_using;
        game.use_duration = chosen_frame.player_use_duration;
    }

    if let Some(p) = game.player.as_mut() {
        if scrub_policy.player_position {
            p.position.x = interp_p_pos[0];
            p.position.y = interp_p_pos[1];
            p.position.z = interp_p_pos[2];
            p.old_position = p.position;
            p.yaw = interp_p_yaw;
            p.head_yaw = interp_p_yaw;
            p.pitch = interp_p_pitch;
            p.teleported = true;
            p.teleported_delta = [false; 3];
            game.previous_position = p.position;
        }
        if scrub_policy.player_abilities {
            let (invuln, may_fly, spec, flying, fly_speed) = chosen_frame.player_abilities;
            p.invulnerable = invuln;
            p.may_fly = may_fly;
            p.spectator = spec;
            p.flying = flying;
            p.flying_speed = fly_speed;
        }
        if scrub_policy.player_motion {
            p.velocity = Default::default();
            p.sprinting = chosen_frame.player_sprinting;
            p.local_crouching = chosen_frame.player_sneaking;
            p.fall_flying = chosen_frame.player_fall_flying;
            p.fire_ticks = chosen_frame.player_fire_ticks;
        }
        if scrub_policy.player_health {
            p.air = chosen_frame.player_air;
            p.ticks_frozen = chosen_frame.player_ticks_frozen;
            let _ = p.set_health(chosen_frame.player_health.max(1.0), false);
            let _ = p.internal_set_absorption(chosen_frame.player_absorption);
            p.health.hurt_animation_time = chosen_frame.player_hurt_time;
            p.health.hurt_dir = chosen_frame.player_hurt_dir;
        }
        if scrub_policy.player_hunger {
            p.health.food.food = chosen_frame.player_food;
        }
    }
    if scrub_policy.player_position {
        game.yaw = interp_p_yaw;
        game.pitch = interp_p_pitch;
        game.snap_player_model_pitch();
    }

    let mut right_mobs_by_id: FxHashMap<u32, &MobPoseRecord> = FxHashMap::default();
    for m in &right_mobs {
        right_mobs_by_id.insert(m.id, m);
    }

    let mut interpolated_mobs: FxHashMap<u32, InterpolatedMobPose> = FxHashMap::default();
    for left_mob in &left_mobs {
        if let Some(right_mob) = right_mobs_by_id.get(&left_mob.id) {
            interpolated_mobs.insert(left_mob.id, interpolate_mob_pose(left_mob, right_mob, alpha));
        } else if alpha <= 0.5 {
            interpolated_mobs.insert(left_mob.id, interpolated_pose_from_record(left_mob));
        }
    }
    if alpha > 0.5 {
        for rm in &right_mobs {
            interpolated_mobs
                .entry(rm.id)
                .or_insert_with(|| interpolated_pose_from_record(rm));
        }
    }

    with_scrub_ring(|ring| {
        for (id, pose) in interpolated_mobs.iter_mut() {
            if let Some(lm) = left_mobs.iter().find(|m| m.id == *id)
                && let Some(rm) = right_mobs_by_id.get(id)
                && lm.template_idx != rm.template_idx
                && let (Some(lt), Some(rt)) = (
                    ring.entity_pool.mob_templates.get(lm.template_idx as usize),
                    ring.entity_pool.mob_templates.get(rm.template_idx as usize),
                )
                && !Arc::ptr_eq(&lt.kind, &rt.kind)
                && lt.kind.id != rt.kind.id
            {
                *pose = interpolated_pose_from_record(if alpha > 0.5 { rm } else { lm });
            }
        }

        let mut mobs_structure_changed = false;
        let mut present_mobs = HashSet::new();
        game.mobs.retain_mut(|live_mob| {
            let Some(pose) = interpolated_mobs.get(&live_mob.id) else {
                mobs_structure_changed = true;
                return false;
            };
            let Some(tpl) = ring.entity_pool.mob_templates.get(pose.template_idx as usize) else {
                mobs_structure_changed = true;
                return false;
            };
            if live_mob.removed
                || (!Arc::ptr_eq(&live_mob.kind, &tpl.kind) && live_mob.kind.id != tpl.kind.id)
            {
                mobs_structure_changed = true;
                return false;
            }
            present_mobs.insert(live_mob.id);
            sync_mob_appearance_from_template(live_mob, tpl);
            apply_mob_pose(live_mob, pose);
            true
        });
        for (&id, pose) in &interpolated_mobs {
            if !present_mobs.contains(&id)
                && let Some(tpl) = ring.entity_pool.mob_templates.get(pose.template_idx as usize)
            {
                let mut resurrected = tpl.clone();
                resurrected.removed = false;
                apply_mob_pose(&mut resurrected, pose);
                game.mobs.push(resurrected);
                mobs_structure_changed = true;
            }
        }
        if mobs_structure_changed {
            game.mob_index = Default::default();
        }
    });

    let interp_items = interpolate_entity_poses(&left_frame.items, &right_frame.items, alpha);
    let mut present_items = HashSet::new();
    game.dropped.retain_mut(|item| {
        if let Some(pose) = interp_items.get(&item.id) {
            present_items.insert(item.id);
            item.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
            item.previous = item.position;
            item.velocity = Vec3::ZERO;
            item.ticks = pose.extra_i32.max(0) as u32;
            true
        } else {
            false
        }
    });
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_items {
            if !present_items.contains(&id)
                && let Some(template) = ring.entity_pool.dropped.get(&id)
            {
                let mut d = template.clone();
                d.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                d.previous = d.position;
                d.velocity = Vec3::ZERO;
                d.ticks = pose.extra_i32.max(0) as u32;
                game.dropped.push(d);
            }
        }
    });

    let interp_orbs = interpolate_entity_poses(&left_frame.orbs, &right_frame.orbs, alpha);
    let mut present_orbs = HashSet::new();
    game.orbs.retain_mut(|orb| {
        if let Some(pose) = interp_orbs.get(&orb.id) {
            present_orbs.insert(orb.id);
            orb.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
            orb.previous = orb.position;
            orb.velocity = Vec3::ZERO;
            orb.ticks = pose.extra_i32.max(0) as u32;
            true
        } else {
            false
        }
    });
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_orbs {
            if !present_orbs.contains(&id)
                && let Some(template) = ring.entity_pool.orbs.get(&id)
            {
                let mut o = template.clone();
                o.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                o.previous = o.position;
                o.velocity = Vec3::ZERO;
                o.ticks = pose.extra_i32.max(0) as u32;
                game.orbs.push(o);
            }
        }
    });

    let interp_tnts = interpolate_entity_poses(&left_frame.tnts, &right_frame.tnts, alpha);
    let mut present_tnts = HashSet::new();
    game.tnts.retain_mut(|tnt| {
        if let Some(pose) = interp_tnts.get(&tnt.id) {
            present_tnts.insert(tnt.id);
            tnt.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
            tnt.previous = tnt.position;
            tnt.velocity = Vec3::ZERO;
            tnt.fuse = pose.extra_i32;
            true
        } else {
            false
        }
    });
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_tnts {
            if !present_tnts.contains(&id)
                && let Some(template) = ring.entity_pool.tnts.get(&id)
            {
                let mut t = template.clone();
                t.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                t.previous = t.position;
                t.velocity = Vec3::ZERO;
                t.fuse = pose.extra_i32;
                game.tnts.push(t);
            }
        }
    });

    let interp_falling =
        interpolate_entity_poses(&left_frame.falling, &right_frame.falling, alpha);
    game.falling.clear();
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_falling {
            if let Some(template) = ring.entity_pool.falling.get(&id) {
                let mut f = template.clone();
                f.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                f.previous = f.position;
                f.velocity = Vec3::ZERO;
                f.age = pose.extra_i32;
                game.falling.push(f);
            }
        }
    });

    let interp_boats = interpolate_entity_poses(&left_frame.boats, &right_frame.boats, alpha);
    game.boats.clear();
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_boats {
            if let Some(template) = ring.entity_pool.boats.get(&id) {
                let mut b = template.clone();
                b.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                b.previous = b.position;
                b.velocity = Vec3::ZERO;
                b.yaw = pose.yaw;
                b.previous_yaw = pose.yaw;
                b.hurt_time = pose.extra_i32;
                game.boats.push(b);
            }
        }
    });

    let interp_carts = interpolate_entity_poses(&left_frame.carts, &right_frame.carts, alpha);
    game.carts.clear();
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_carts {
            if let Some(template) = ring.entity_pool.carts.get(&id) {
                let mut c = template.clone();
                c.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                c.previous = c.position;
                c.velocity = Vec3::ZERO;
                c.yaw = pose.yaw;
                c.previous_yaw = pose.yaw;
                c.pitch = pose.pitch;
                c.previous_pitch = pose.pitch;
                c.hurt_time = pose.extra_i32;
                game.carts.push(c);
            }
        }
    });

    let interp_proj =
        interpolate_entity_poses(&left_frame.projectiles, &right_frame.projectiles, alpha);
    game.creatures.projectiles.clear();
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_proj {
            if let Some(template) = ring.entity_pool.projectiles.get(&id) {
                let mut p = template.clone();
                p.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                p.previous = p.position;
                p.velocity = Vec3::ZERO;
                p.yaw = pose.yaw;
                p.previous_yaw = pose.yaw;
                p.pitch = pose.pitch;
                p.previous_pitch = pose.pitch;
                p.age = pose.extra_i32.max(0) as u32;
                game.creatures.projectiles.push(p);
            }
        }
    });

    let interp_tridents =
        interpolate_entity_poses(&left_frame.tridents, &right_frame.tridents, alpha);
    let registry = game.pack.assets.registry;
    game.tridents.clear();
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_tridents {
            if let Some(ts) = ring.entity_pool.tridents.get(&id)
                && let Some(n) = horizon_client::entity_nbt::trident_from_nbt(&ts.nbt)
            {
                let mut tr =
                    horizon_gameplay::trident::ThrownTrident::from_nbt(n, &ts.nbt, registry);
                tr.id = id;
                tr.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                tr.previous = tr.position;
                tr.velocity = Vec3::ZERO;
                tr.yaw = pose.yaw;
                tr.pitch = pose.pitch;
                tr.in_ground_time = pose.extra_i32;
                game.tridents.push(tr);
            }
        }
    });

    let interp_rockets =
        interpolate_entity_poses(&left_frame.rockets, &right_frame.rockets, alpha);
    game.rockets.clear();
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_rockets {
            if let Some(rs) = ring.entity_pool.rockets.get(&id)
                && let Some(rn) =
                    horizon_client::entity_nbt::firework_rocket_from_nbt(&rs.nbt, None)
            {
                let mut r = horizon_gameplay::fireworks::Rocket::from_nbt(rn);
                r.id = id;
                r.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                r.previous = r.position;
                r.velocity = Vec3::ZERO;
                r.rotation = [pose.yaw, pose.pitch];
                r.life = pose.extra_i32;
                game.rockets.push(r);
            }
        }
    });

    let interp_crystals =
        interpolate_entity_poses(&left_frame.crystals, &right_frame.crystals, alpha);
    let mut present_crystals = HashSet::new();
    game.crystals.retain_mut(|c| {
        if let Some(pose) = interp_crystals.get(&c.id) {
            present_crystals.insert(c.id);
            c.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
            true
        } else {
            false
        }
    });
    with_scrub_ring(|ring| {
        for (&id, pose) in &interp_crystals {
            if !present_crystals.contains(&id)
                && let Some(template) = ring.entity_pool.crystals.get(&id)
            {
                let mut c = template.clone();
                c.position = Vec3::new(pose.pos[0], pose.pos[1], pose.pos[2]);
                game.crystals.push(c);
            }
        }
    });

    game.paintings.clear();
    for p in chosen_frame.paintings.iter() {
        game.paintings.push(p.clone());
    }

    game.item_frames.clear();
    for f in chosen_frame.item_frames.iter() {
        game.item_frames.push(f.clone());
    }

    game.armor_stands.clear();
    let mut right_stands: FxHashMap<u32, &horizon_client::entities::ArmorStand> =
        FxHashMap::default();
    for rs in right_frame.armor_stands.iter() {
        right_stands.insert(rs.id, rs);
    }
    for ls in left_frame.armor_stands.iter() {
        let mut s = ls.clone();
        if let Some(rs) = right_stands.get(&ls.id) {
            s.position = Vec3::new(
                lerp_f64(ls.position.x, rs.position.x, a64),
                lerp_f64(ls.position.y, rs.position.y, a64),
                lerp_f64(ls.position.z, rs.position.z, a64),
            );
            s.previous = s.position;
            s.yaw = lerp_angle(ls.yaw, rs.yaw, alpha);
            s.previous_yaw = s.yaw;
            if alpha > 0.5 {
                s.equipment = rs.equipment.clone();
                s.pose = rs.pose;
            }
        } else {
            s.previous = s.position;
            s.previous_yaw = s.yaw;
        }
        game.armor_stands.push(s);
    }

    game.cushions.clear();
    for c in chosen_frame.cushions.iter() {
        game.cushions.push(c.clone());
    }

    game.others.clear();
    for o in chosen_frame.others.iter() {
        game.others.push(o.clone());
    }

    if let Some(d_snap) = &chosen_frame.dragon {
        if let Some(d) = d_snap.to_live(game.level.seed) {
            game.dragon.replace(d);
        }
    } else {
        game.dragon.discard();
    }

    game.breath_clouds.clear();
    for bs in chosen_frame.breath_clouds.iter() {
        if let Some((_, mut cloud)) = super::super::potions::potion_cloud_from_nbt(&bs.nbt) {
            cloud.id = bs.id;
            cloud.motion = bs.motion;
            cloud.from_dragon = bs.from_dragon;
            cloud.victims = bs.victims.clone();
            game.breath_clouds.push(cloud);
        } else if bs.from_dragon || bs.potion_none {
            game.breath_clouds.push(super::super::dragon::Breath {
                attachment_lifetime: Default::default(),
                join: Default::default(),
                id: bs.id,
                at: bs.at,
                motion: bs.motion,
                radius: bs.radius,
                age: bs.age,
                duration: bs.duration,
                from_dragon: bs.from_dragon,
                victims: bs.victims.clone(),
                potion: None,
            });
        }
    }

    game.fangs.list.clear();
    for f in chosen_frame.fangs.iter() {
        game.fangs.list.push(f.clone());
    }

    let potion_poses = interpolate_thrown_potions(
        &left_frame.thrown_potions,
        &right_frame.thrown_potions,
        alpha,
    );
    with_scrub_ring(|ring| {
        game.potions
            .rreturn_apply_thrown_preview(&ring.entity_pool.thrown_potions, &potion_poses);
    });

    match (&left_frame.hook, &right_frame.hook) {
        (Some(lh), Some(rh)) if lh.id == rh.id => {
            let hpos = [
                lerp_f64(lh.pos[0], rh.pos[0], a64),
                lerp_f64(lh.pos[1], rh.pos[1], a64),
                lerp_f64(lh.pos[2], rh.pos[2], a64),
            ];
            game.rreturn_restore_hook_full(
                lh.id,
                hpos,
                hpos,
                [0.0; 3],
                0,
                if alpha > 0.5 { rh.nibble } else { lh.nibble },
                0,
                0,
                0.0,
                true,
                if alpha > 0.5 { rh.bobbing } else { lh.bobbing },
            );
        }
        (Some(lh), _) if alpha <= 0.5 => {
            game.rreturn_restore_hook_full(
                lh.id,
                lh.pos,
                lh.pos,
                [0.0; 3],
                0,
                lh.nibble,
                0,
                0,
                0.0,
                true,
                lh.bobbing,
            );
        }
        (_, Some(rh)) if alpha > 0.5 => {
            game.rreturn_restore_hook_full(
                rh.id,
                rh.pos,
                rh.pos,
                [0.0; 3],
                0,
                rh.nibble,
                0,
                0,
                0.0,
                true,
                rh.bobbing,
            );
        }
        _ => {
            game.hook.discard();
        }
    }

    if let Some(r) = game.remote.as_mut() {
        r.stepped.clear();
        r.lerps.clear();
        r.cart_lerps.clear();
        r.head_lerps.clear();
    }

    game.destroying.clear();
    for bp in &chosen_frame.break_progress {
        game.destroying
            .insert(bp.breaker, (bp.pos, bp.stage, chosen_frame.game_time));
    }

    if scrub_policy.player_motion {
        let chosen_swing = &chosen_frame.arm_swing;
        if chosen_swing.swinging {
            let anim = lerp_f32(
                left_frame.arm_swing.animation,
                right_frame.arm_swing.animation,
                alpha,
            );
            let duration = chosen_swing.duration.max(1);
            game.swing.current = Some(horizon_client::first_person::SwingDescription {
                hand: if chosen_swing.hand == 1 {
                    horizon_client::first_person::Hand::Off
                } else {
                    horizon_client::first_person::Hand::Main
                },
                animation: horizon_client::first_person::SwingAnimation::DEFAULT,
                duration_ticks: duration,
            });
            game.swing.animation = anim;
            game.swing.old_animation = anim;
            game.swing.ticks = (anim * duration as f32).round().clamp(0.0, duration as f32) as i32;
        } else {
            game.swing.current = None;
            game.swing.animation = 0.0;
            game.swing.old_animation = 0.0;
            game.swing.ticks = 0;
        }
        game.heights.main = lerp_f32(
            left_frame.arm_swing.height_main,
            right_frame.arm_swing.height_main,
            alpha,
        );
        game.heights.old_main = game.heights.main;
        game.heights.off = lerp_f32(
            left_frame.arm_swing.height_off,
            right_frame.arm_swing.height_off,
            alpha,
        );
        game.heights.old_off = game.heights.off;
    }

    let interpolated_particles =
        interpolate_lossless_particles(&left_particles, &right_particles, alpha);
    game.particles.clear();
    for p in interpolated_particles {
        game.particles.push(p);
    }
    for _ in 0..chosen_frame.elder_guardians.len() {
        game.particles.elder_guardian();
    }
    let world = &game.world;
    let facts = game.res.facts();
    let collision = game.res.collision();
    let player_vel = game
        .player
        .as_ref()
        .map(|p| ([p.position.x, p.position.y, p.position.z], p.velocity.y));
    let level = horizon_client::particles::ParticleLevel {
        world,
        facts,
        collision,
        player: player_vel,
        cached_chunk: std::cell::Cell::new(None),
    };
    game.particles.tick(&level);
    game.particles.sounds.clear();
}

fn apply_mob_pose(live_mob: &mut super::super::Mob, pose: &InterpolatedMobPose) {
    live_mob.position.x = pose.pos[0];
    live_mob.position.y = pose.pos[1];
    live_mob.position.z = pose.pos[2];
    live_mob.previous.x = pose.pos[0];
    live_mob.previous.y = pose.pos[1];
    live_mob.previous.z = pose.pos[2];
    live_mob.velocity = Default::default();
    live_mob.extra.previous_velocity = Default::default();

    live_mob.yaw = pose.yaw;
    live_mob.previous_yaw = pose.yaw;

    live_mob.body_yaw = pose.body_yaw;
    live_mob.previous_body_yaw = pose.body_yaw;

    live_mob.head_yaw = pose.head_yaw;
    live_mob.previous_head_yaw = pose.head_yaw;

    live_mob.head_pitch = pose.head_pitch;
    live_mob.previous_head_pitch = pose.head_pitch;

    live_mob.limb_position = pose.limb_position;
    live_mob.limb_speed = pose.limb_speed;
    live_mob.previous_limb_speed = pose.limb_speed;

    live_mob.swell = pose.swell.round() as i32;
    live_mob.previous_swell = live_mob.swell;

    live_mob.anim.attack_anim = pose.attack_anim;
    live_mob.anim.attack_anim_o = pose.attack_anim;

    live_mob.removed = false;
    live_mob.health = if pose.alive {
        pose.health.max(0.01)
    } else {
        0.0
    };
    live_mob.hurt_time = pose.hurt_time;
    live_mob.death_time = if pose.alive { 0 } else { pose.death_time };
    live_mob.invisible = pose.invisible;
    live_mob.glowing = pose.glowing;
    live_mob.fire_ticks = if pose.on_fire {
        live_mob.fire_ticks.max(20)
    } else {
        0
    };
    live_mob.extra.sheared = pose.sheared;
    live_mob.extra.color = pose.color;
    live_mob.extra.size = pose.size;
    if !matches!(
        live_mob.extra.profession,
        horizon_client::mobs::villager::ProfessionState::Registered { .. }
            | horizon_client::mobs::villager::ProfessionState::Unresolved(_)
    ) {
        live_mob.extra.profession =
            horizon_client::mobs::villager::ProfessionState::from_index(pose.profession);
    }
    live_mob.extra.villager_level = pose.villager_level;
    live_mob.extra.variant = pose.variant;
    live_mob.extra.variant2 = pose.variant2;
    live_mob.extra.variant3 = pose.variant3;
    live_mob.extra.age = pose.age;
    live_mob.extra.sitting = pose.sitting;
    live_mob.extra.sitting_pose = pose.sitting_pose;
    live_mob.extra.charged = pose.charged;
}

pub fn interpolate_lossless_particles(
    left_states: &[ParticleTickState],
    right_states: &[ParticleTickState],
    alpha: f32,
) -> Vec<Particle> {
    if left_states.is_empty() && right_states.is_empty() {
        return Vec::new();
    }
    let a64 = f64::from(alpha);
    let mut right_map: FxHashMap<u32, &ParticleTickState> = FxHashMap::default();
    for r in right_states {
        right_map.insert(r.id, r);
    }

    with_scrub_ring(|ring| {
        let mut out = Vec::with_capacity(left_states.len().max(right_states.len()));
        let mut seen = HashSet::with_capacity(left_states.len());

        for l in left_states {
            let Some(template) = ring.particle_templates.get(l.template_id as usize) else {
                continue;
            };
            seen.insert(l.id);
            let mut p = template.clone();
            if let Some(r) = right_map.get(&l.id) {
                let pos = [
                    lerp_f64(l.pos[0], r.pos[0], a64),
                    lerp_f64(l.pos[1], r.pos[1], a64),
                    lerp_f64(l.pos[2], r.pos[2], a64),
                ];
                p.position = pos;
                p.previous = pos;
                p.velocity = [
                    lerp_f64(l.vel[0], r.vel[0], a64),
                    lerp_f64(l.vel[1], r.vel[1], a64),
                    lerp_f64(l.vel[2], r.vel[2], a64),
                ];
                p.size = lerp_f32(l.size, r.size, alpha);
                p.rgba = [
                    lerp_f32(l.rgba[0], r.rgba[0], alpha),
                    lerp_f32(l.rgba[1], r.rgba[1], alpha),
                    lerp_f32(l.rgba[2], r.rgba[2], alpha),
                    lerp_f32(l.rgba[3], r.rgba[3], alpha),
                ];
                p.age = if alpha > 0.5 { r.age } else { l.age };
                out.push(p);
            } else {
                let pos = [
                    l.pos[0] + l.vel[0] * a64,
                    l.pos[1] + l.vel[1] * a64,
                    l.pos[2] + l.vel[2] * a64,
                ];
                p.position = pos;
                p.previous = pos;
                p.size = l.size;
                p.rgba = [
                    l.rgba[0],
                    l.rgba[1],
                    l.rgba[2],
                    (l.rgba[3] * (1.0 - alpha)).clamp(0.0, 1.0),
                ];
                p.age = l.age;
                if p.rgba[3] > 0.01 {
                    out.push(p);
                }
            }
        }

        for r in right_states {
            if seen.contains(&r.id) {
                continue;
            }
            let Some(template) = ring.particle_templates.get(r.template_id as usize) else {
                continue;
            };
            let mut p = template.clone();
            let back = 1.0 - a64;
            let pos = [
                r.pos[0] - r.vel[0] * back,
                r.pos[1] - r.vel[1] * back,
                r.pos[2] - r.vel[2] * back,
            ];
            p.position = pos;
            p.previous = pos;
            p.size = r.size;
            p.rgba = [
                r.rgba[0],
                r.rgba[1],
                r.rgba[2],
                (r.rgba[3] * alpha).clamp(0.0, 1.0),
            ];
            p.age = r.age;
            if p.rgba[3] > 0.01 {
                out.push(p);
            }
        }

        out
    })
}

fn mark_section_dirty_3x3(game: &mut Game<'_>, cx: i32, s_idx: usize, cz: i32) {
    let num_sections = game.world.layout().sections() as i32;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let s = s_idx as i32 + dy;
                if (0..num_sections).contains(&s) {
                    game.world.dirty_sections.insert((cx + dx, s, cz + dz));
                }
            }
        }
    }
}

pub fn apply_block_rewind_to_game(game: &mut Game<'_>, from_tick: i64, to_tick: i64) {
    if from_tick == to_tick {
        return;
    }

    let current_dim = game.dim;
    let min_y = game.world.shape.min_y;
    let num_sections = game.world.layout().sections();
    let is_backward = to_tick < from_tick;

    let mut coalesced_blocks: FxHashMap<(i32, i32, i32), (u32, i64)> = FxHashMap::default();
    let mut section_swaps: FxHashMap<(i32, i32, usize), Arc<horizon_client::chunk::Section>> =
        FxHashMap::default();
    let mut swap_ticks: FxHashMap<(i32, i32, usize), i64> = FxHashMap::default();
    let mut biome_swaps: FxHashMap<(i32, i32, usize), Arc<horizon_client::chunk::Section>> =
        FxHashMap::default();

    with_scrub_ring(|ring| {
        if ring.frames.is_empty() {
            return;
        }
        if is_backward {
            for frame in ring.frames.iter().rev() {
                if frame.game_time <= to_tick {
                    break;
                }
                if frame.game_time <= from_tick && frame.dim == current_dim {
                    for swap in frame.section_swaps.iter().rev() {
                        let key = (swap.cx, swap.cz, swap.s_idx);
                        section_swaps.insert(key, Arc::clone(&swap.old_section));
                        swap_ticks.insert(key, frame.game_time);
                    }
                    for b in frame.biome_deltas.iter().rev() {
                        biome_swaps.insert((b.cx, b.cz, b.s_idx), Arc::clone(&b.old_section));
                    }
                    for delta in frame.block_deltas.iter().rev() {
                        coalesced_blocks.insert(
                            (delta.x, delta.y, delta.z),
                            (delta.old_state, frame.game_time),
                        );
                    }
                }
            }
        } else {
            for frame in ring.frames.iter() {
                if frame.game_time > to_tick {
                    break;
                }
                if frame.game_time > from_tick && frame.dim == current_dim {
                    for swap in frame.section_swaps.iter() {
                        let key = (swap.cx, swap.cz, swap.s_idx);
                        section_swaps.insert(key, Arc::clone(&swap.new_section));
                        swap_ticks.insert(key, frame.game_time);
                    }
                    for b in frame.biome_deltas.iter() {
                        biome_swaps.insert((b.cx, b.cz, b.s_idx), Arc::clone(&b.new_section));
                    }
                    for delta in frame.block_deltas.iter() {
                        coalesced_blocks.insert(
                            (delta.x, delta.y, delta.z),
                            (delta.new_state, frame.game_time),
                        );
                    }
                }
            }
        }
    });

    let mut any_world_change = false;
    if !section_swaps.is_empty() {
        let mut by_chunk: FxHashMap<(i32, i32), Vec<(usize, Arc<horizon_client::chunk::Section>)>> =
            FxHashMap::default();
        for ((cx, cz, s_idx), target_sec) in section_swaps {
            by_chunk.entry((cx, cz)).or_default().push((s_idx, target_sec));
        }
        for ((cx, cz), secs) in by_chunk {
            if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)).cloned() {
                let mut chunk = (*live_chunk).clone();
                let mut chunk_changed = false;
                for (s_idx, target_sec) in secs {
                    if let Some(cur_sec) = chunk.sections.get(s_idx)
                        && !Arc::ptr_eq(cur_sec, &target_sec)
                    {
                        let mut values = [0u32; 4096];
                        target_sec.unpack(&mut values);
                        chunk.replace_blocks(s_idx, &values);
                        chunk.sections[s_idx] = target_sec;
                        mark_section_dirty_3x3(game, cx, s_idx, cz);
                        chunk_changed = true;
                    }
                }
                if chunk_changed {
                    game.world.chunks.insert((cx, cz), Arc::new(chunk));
                    game.world.modified.insert((cx, cz));
                    for dz in -1..=1 {
                        for dx in -1..=1 {
                            game.world.dirty_light.insert((cx + dx, cz + dz));
                        }
                    }
                    any_world_change = true;
                }
            }
        }
        if any_world_change {
            game.world.revision = game.world.revision.wrapping_add(1);
        }
    }

    for ((x, y, z), (state_id, delta_tick)) in coalesced_blocks {
        let rel_y = y - min_y;
        if rel_y >= 0 {
            let s_idx = (rel_y >> 4) as usize;
            if s_idx < num_sections {
                let cx = x >> 4;
                let cz = z >> 4;
                if let Some(&swap_tick) = swap_ticks.get(&(cx, cz, s_idx)) {
                    if is_backward && delta_tick >= swap_tick {
                        continue;
                    }
                    if !is_backward && delta_tick <= swap_tick {
                        continue;
                    }
                }
            }
        }
        if game.world.set(x, y, z, BlockStateId(state_id)).is_ok() {
            any_world_change = true;
        }
    }

    if !biome_swaps.is_empty() {
        for ((cx, cz, s_idx), target_sec) in biome_swaps {
            if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)).cloned() {
                let mut chunk = (*live_chunk).clone();
                if let Some(sec_arc) = chunk.sections.get_mut(s_idx) {
                    Arc::make_mut(sec_arc).biomes = target_sec.biomes.clone();
                }
                game.world.chunks.insert((cx, cz), Arc::new(chunk));
                mark_section_dirty_3x3(game, cx, s_idx, cz);
                any_world_change = true;
            }
        }
        game.world.revision = game.world.revision.wrapping_add(1);
    }

    if any_world_change {
        let cam_block = game
            .player
            .as_ref()
            .map(|p| {
                [
                    p.position.x.floor() as i32,
                    p.position.y.floor() as i32,
                    p.position.z.floor() as i32,
                ]
            })
            .unwrap_or([0, 64, 0]);
        game.world.mark_nearby_dirty_immediate(cam_block, 768.0, 256);
    }
}

pub fn apply_timeline_sounds_to_game(
    game: &Game<'_>,
    from_tick: i64,
    to_tick: i64,
    jog_accel: f64,
) {
    if from_tick == to_tick {
        return;
    }

    let mut sounds_to_play = Vec::new();
    with_scrub_ring(|ring| {
        if ring.frames.is_empty() {
            return;
        }
        if to_tick < from_tick {
            for frame in ring.frames.iter().rev() {
                if frame.game_time <= to_tick {
                    break;
                }
                if frame.game_time <= from_tick {
                    for s in frame.sounds.iter().rev() {
                        sounds_to_play.push((s.clone(), true));
                    }
                }
            }
        } else {
            for frame in ring.frames.iter() {
                if frame.game_time > to_tick {
                    break;
                }
                if frame.game_time > from_tick {
                    for s in frame.sounds.iter() {
                        sounds_to_play.push((s.clone(), false));
                    }
                }
            }
        }
    });

    if !sounds_to_play.is_empty() {
        play_scrub_sounds(game, sounds_to_play, to_tick, jog_accel);
    }
}