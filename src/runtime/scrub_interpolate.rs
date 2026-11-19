use super::super::{Game, Mob};
use super::scrub_timeline::{with_scrub_ring, MobPoseRecord};
use horizon_client::fast_hash::FxHashMap;

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
    }
}

pub fn apply_interpolated_poses_to_game(game: &mut Game<'_>, target_time: f64) {
    let Some((left_frame, right_frame, alpha)) = with_scrub_ring(|ring| {
        ring.find_frame_pair(target_time)
            .map(|(l, r, a)| (l.clone(), r.clone(), a))
    }) else {
        return;
    };

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
        }
    }
}
