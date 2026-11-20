use super::super::{Game, Mob};
use super::scrub_audio::play_scrub_sounds;
use super::scrub_timeline::{with_scrub_ring, MobPoseRecord};
use horizon_client::fast_hash::FxHashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct InterpolatedMobPose {
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

pub fn interpolate_mob_pose(left: &MobPoseRecord, right: &MobPoseRecord, alpha: f32) -> InterpolatedMobPose {
    let a64 = f64::from(alpha);
    InterpolatedMobPose {
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
        alive: if alpha > 0.5 { right.alive } else { left.alive },
        hurt_time: if alpha > 0.5 { right.hurt_time } else { left.hurt_time },
        death_time: if alpha > 0.5 { right.death_time } else { left.death_time },
    }
}

pub fn apply_interpolated_poses_to_game(game: &mut Game<'_>, target_time: f64) {
    let Some((left_frame, right_frame, alpha)) = with_scrub_ring(|ring| {
        ring.find_frame_pair(target_time)
            .map(|(l, r, a)| (l.clone(), r.clone(), a))
    }) else {
        return;
    };

    let a64 = f64::from(alpha);
    let interp_p_pos = [
        lerp_f64(left_frame.player_pos[0], right_frame.player_pos[0], a64),
        lerp_f64(left_frame.player_pos[1], right_frame.player_pos[1], a64),
        lerp_f64(left_frame.player_pos[2], right_frame.player_pos[2], a64),
    ];
    let interp_p_yaw = lerp_angle(left_frame.player_yaw, right_frame.player_yaw, alpha);
    let interp_p_pitch = lerp_f32(left_frame.player_pitch, right_frame.player_pitch, alpha);

    // Dynamic FOV interpolation (e.g. wider FOV while sprinting/speed)
    let interp_fov = lerp_f32(left_frame.fov_modifier, right_frame.fov_modifier, alpha);
    if interp_fov > 0.05 {
        game.view.fov_modifier = interp_fov;
        game.view.old_fov_modifier = interp_fov;
    }

    // Camera view bobbing and hand movement interpolation
    let interp_walk = lerp_f32(left_frame.avatar_walk_dist, right_frame.avatar_walk_dist, alpha);
    let interp_bob = lerp_f32(left_frame.avatar_bob, right_frame.avatar_bob, alpha);
    game.avatar.walk_dist = interp_walk;
    game.avatar.walk_dist_o = interp_walk;
    game.avatar.bob = interp_bob;
    game.avatar.bob_o = interp_bob;

    if let Some(p) = game.player.as_mut() {
        p.position.x = interp_p_pos[0];
        p.position.y = interp_p_pos[1];
        p.position.z = interp_p_pos[2];
        p.old_position = p.position;
        p.velocity = Default::default();
        p.yaw = interp_p_yaw;
        p.head_yaw = interp_p_yaw;
        p.pitch = interp_p_pitch;
        p.teleported = true;
        p.teleported_delta = [false; 3];
        p.sprinting = if alpha > 0.5 { right_frame.player_sprinting } else { left_frame.player_sprinting };
        p.health.hurt_animation_time = if alpha > 0.5 { right_frame.player_hurt_time } else { left_frame.player_hurt_time };
        p.health.hurt_dir = if alpha > 0.5 { right_frame.player_hurt_dir } else { left_frame.player_hurt_dir };
        game.previous_position = p.position;
    }
    game.yaw = interp_p_yaw;
    game.pitch = interp_p_pitch;
    game.snap_player_model_pitch();

    let mut right_mobs_by_id: FxHashMap<u32, &MobPoseRecord> = FxHashMap::default();
    for m in &right_frame.mobs {
        right_mobs_by_id.insert(m.id, m);
    }

    let mut interpolated: FxHashMap<u32, InterpolatedMobPose> = FxHashMap::default();
    for left_mob in &left_frame.mobs {
        if let Some(right_mob) = right_mobs_by_id.get(&left_mob.id) {
            interpolated.insert(left_mob.id, interpolate_mob_pose(left_mob, right_mob, alpha));
        } else {
            interpolated.insert(
                left_mob.id,
                InterpolatedMobPose {
                    pos: left_mob.pos,
                    yaw: left_mob.yaw,
                    body_yaw: left_mob.body_yaw,
                    head_yaw: left_mob.head_yaw,
                    head_pitch: left_mob.head_pitch,
                    limb_position: left_mob.limb_position,
                    limb_speed: left_mob.limb_speed,
                    attack_anim: left_mob.attack_anim,
                    swell: left_mob.swell,
                    health: left_mob.health,
                    alive: left_mob.alive,
                    hurt_time: left_mob.hurt_time,
                    death_time: left_mob.death_time,
                },
            );
        }
    }

    for live_mob in game.mobs.iter_mut() {
        if let Some(pose) = interpolated.get(&live_mob.id) {
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

            live_mob.health = pose.health;
            live_mob.hurt_time = pose.hurt_time;
            live_mob.death_time = pose.death_time;
        }
    }

    if let Some(r) = game.remote.as_mut() {
        r.stepped.clear();
    }

    // Restore block breaking crack stages for target time
    game.destroying.clear();
    let chosen_frame = if alpha > 0.5 { &right_frame } else { &left_frame };
    for bp in &chosen_frame.break_progress {
        game.destroying.insert(bp.breaker, (bp.pos, bp.stage, chosen_frame.game_time));
    }

    // First-person hand and arm swing interpolation
    let chosen_swing = if alpha > 0.5 { &right_frame.arm_swing } else { &left_frame.arm_swing };
    if chosen_swing.swinging {
        let anim = lerp_f32(left_frame.arm_swing.animation, right_frame.arm_swing.animation, alpha);
        game.swing.current = Some(horizon_client::first_person::SwingDescription {
            hand: if chosen_swing.hand == 1 {
                horizon_client::first_person::Hand::Off
            } else {
                horizon_client::first_person::Hand::Main
            },
            animation: horizon_client::first_person::SwingAnimation::DEFAULT,
            duration_ticks: chosen_swing.duration.max(1),
        });
        game.swing.animation = anim;
        game.swing.old_animation = anim;
        game.swing.ticks = ((1.0 - anim) * chosen_swing.duration as f32).round() as i32;
    } else {
        game.swing.current = None;
        game.swing.animation = 0.0;
        game.swing.old_animation = 0.0;
        game.swing.ticks = 0;
    }
    game.heights.main = lerp_f32(left_frame.arm_swing.height_main, right_frame.arm_swing.height_main, alpha);
    game.heights.old_main = game.heights.main;
    game.heights.off = lerp_f32(left_frame.arm_swing.height_off, right_frame.arm_swing.height_off, alpha);
    game.heights.old_off = game.heights.off;

    // Seamless particle playback on scrub: interpolate particles across sub-tick time
    let interpolated_particles = interpolate_timeline_particles(
        &left_frame.particles,
        &right_frame.particles,
        alpha,
    );

    game.particles.clear();
    for p in interpolated_particles {
        game.particles.push(p);
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

pub fn interpolate_timeline_particles(
    left_particles: &[horizon_client::particles::Particle],
    right_particles: &[horizon_client::particles::Particle],
    alpha: f32,
) -> Vec<horizon_client::particles::Particle> {
    if left_particles.is_empty() && right_particles.is_empty() {
        return Vec::new();
    }
    if right_particles.is_empty() || alpha <= 0.0 {
        let mut cloned = left_particles.to_vec();
        for p in &mut cloned {
            p.previous = p.position;
        }
        return cloned;
    }
    if left_particles.is_empty() || alpha >= 1.0 {
        let mut cloned = right_particles.to_vec();
        for p in &mut cloned {
            p.previous = p.position;
        }
        return cloned;
    }

    let a64 = f64::from(alpha);
    let mut result = Vec::with_capacity(left_particles.len().max(right_particles.len()));
    let mut matched_right = vec![false; right_particles.len()];

    for left_p in left_particles {
        let mut best_match: Option<usize> = None;
        let mut min_dist_sq = 9.0;

        for (r_idx, right_p) in right_particles.iter().enumerate() {
            if matched_right[r_idx] {
                continue;
            }
            if right_p.ty == left_p.ty
                && right_p.lifetime == left_p.lifetime
                && (right_p.age == left_p.age + 1 || (right_p.age == left_p.age && (alpha - 0.5).abs() < 0.49))
            {
                let dx = right_p.position[0] - left_p.position[0];
                let dy = right_p.position[1] - left_p.position[1];
                let dz = right_p.position[2] - left_p.position[2];
                let dist_sq = dx * dx + dy * dy + dz * dz;
                if dist_sq < min_dist_sq {
                    min_dist_sq = dist_sq;
                    best_match = Some(r_idx);
                }
            }
        }

        if let Some(r_idx) = best_match {
            matched_right[r_idx] = true;
            let right_p = &right_particles[r_idx];
            let mut interp = left_p.clone();
            let interp_pos = [
                lerp_f64(left_p.position[0], right_p.position[0], a64),
                lerp_f64(left_p.position[1], right_p.position[1], a64),
                lerp_f64(left_p.position[2], right_p.position[2], a64),
            ];
            interp.position = interp_pos;
            interp.previous = interp_pos;
            interp.size = lerp_f32(left_p.size, right_p.size, alpha);
            interp.rgba = [
                lerp_f32(left_p.rgba[0], right_p.rgba[0], alpha),
                lerp_f32(left_p.rgba[1], right_p.rgba[1], alpha),
                lerp_f32(left_p.rgba[2], right_p.rgba[2], alpha),
                lerp_f32(left_p.rgba[3], right_p.rgba[3], alpha),
            ];
            result.push(interp);
        } else {
            let mut interp = left_p.clone();
            let interp_pos = [
                left_p.position[0] + left_p.velocity[0] * a64,
                left_p.position[1] + left_p.velocity[1] * a64,
                left_p.position[2] + left_p.velocity[2] * a64,
            ];
            interp.position = interp_pos;
            interp.previous = interp_pos;
            interp.rgba[3] = (interp.rgba[3] * (1.0 - alpha)).clamp(0.0, 1.0);
            if interp.rgba[3] > 0.02 {
                result.push(interp);
            }
        }
    }

    for (r_idx, right_p) in right_particles.iter().enumerate() {
        if !matched_right[r_idx] {
            let mut interp = right_p.clone();
            let back = 1.0 - a64;
            let interp_pos = [
                right_p.position[0] - right_p.velocity[0] * back,
                right_p.position[1] - right_p.velocity[1] * back,
                right_p.position[2] - right_p.velocity[2] * back,
            ];
            interp.position = interp_pos;
            interp.previous = interp_pos;
            interp.rgba[3] = (interp.rgba[3] * alpha).clamp(0.0, 1.0);
            if interp.rgba[3] > 0.02 {
                result.push(interp);
            }
        }
    }

    result
}

pub fn apply_block_rewind_to_game(game: &mut Game<'_>, from_tick: i64, to_tick: i64) {
    if from_tick == to_tick {
        return;
    }

    with_scrub_ring(|ring| {
        if ring.frames.is_empty() {
            return;
        }

        if to_tick < from_tick {
            // Rewinding backwards in time: apply old_state in reverse chronological order
            for frame in ring.frames.iter().rev() {
                if frame.game_time <= to_tick {
                    break;
                }
                if frame.game_time <= from_tick {
                    for swap in frame.section_swaps.iter().rev() {
                        if let Some(chunk_arc) = game.world.chunks.get_mut(&(swap.cx, swap.cz)) {
                            if let Some(chunk) = Arc::get_mut(chunk_arc) {
                                if let Some(sec) = chunk.sections.get_mut(swap.s_idx) {
                                    *sec = swap.old_section.clone();
                                }
                            } else {
                                let mut chunk = (**chunk_arc).clone();
                                if let Some(sec) = chunk.sections.get_mut(swap.s_idx) {
                                    *sec = swap.old_section.clone();
                                }
                                *chunk_arc = Arc::new(chunk);
                            }
                            game.world.dirty_sections.insert((swap.cx, swap.s_idx as i32, swap.cz));
                        }
                    }
                    for delta in frame.block_deltas.iter().rev() {
                        let _ = game.world.set(
                            delta.x,
                            delta.y,
                            delta.z,
                            horizon_world::registry::BlockStateId(delta.old_state),
                        );
                    }
                }
            }
        } else {
            // Scrubbing forwards in time: apply new_state in forward chronological order
            for frame in ring.frames.iter() {
                if frame.game_time > to_tick {
                    break;
                }
                if frame.game_time > from_tick {
                    for swap in frame.section_swaps.iter() {
                        if let Some(chunk_arc) = game.world.chunks.get_mut(&(swap.cx, swap.cz)) {
                            if let Some(chunk) = Arc::get_mut(chunk_arc) {
                                if let Some(sec) = chunk.sections.get_mut(swap.s_idx) {
                                    *sec = swap.new_section.clone();
                                }
                            } else {
                                let mut chunk = (**chunk_arc).clone();
                                if let Some(sec) = chunk.sections.get_mut(swap.s_idx) {
                                    *sec = swap.new_section.clone();
                                }
                                *chunk_arc = Arc::new(chunk);
                            }
                            game.world.dirty_sections.insert((swap.cx, swap.s_idx as i32, swap.cz));
                        }
                    }
                    for delta in frame.block_deltas.iter() {
                        let _ = game.world.set(
                            delta.x,
                            delta.y,
                            delta.z,
                            horizon_world::registry::BlockStateId(delta.new_state),
                        );
                    }
                }
            }
        }
    });
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
            // Rewinding backwards in time: gather sounds in reverse chronological order
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
            // Scrubbing forwards in time: gather sounds in forward chronological order
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
