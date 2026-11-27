use super::super::{
    Dim, ExperienceOrb, FallingBlock, Game, ItemEntity, Mob,
    commands::OtherEntity,
    dragon::{Crystal, Dragon},
    machines::HopperState,
};
use horizon_gameplay::{
    boats::Boat,
    minecarts::Minecart,
    tnt::PrimedTnt,
};
use super::mob_disk_extra::*;
use super::types::*;
use super::ui_snapshot::clear_client_ui_slots;
use horizon_client::{
    advancements::Progress,
    chunk::Chunk,
    containers::BlockEntity,
    cooldowns::Cooldowns,
    effects::EffectInstance,
    entities::{ArmorStand, Cushion, ItemFrame, Painting},
    fast_hash::FxHashMap,
    gamerules::GameRules,
    items::ItemStack,
    recipe_book::Settings as RecipeBookSettings,
    scoreboard::Scoreboard,
    sculk::WardenSpawnTracker,
    world::{HealthRules, Weather},
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::Vec3,
    position::BlockPos,
    random::LevelRandom,
    ticks::ChunkTicks,
};
use horizon_world::{
    attachments::Attachments,
    ground_support::GroundState,
    level::{BlockEventData, MovingPiston},
    nbt::Tag,
    registry::BlockStateId,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct DragonSnapshot {
    pub id: u32,
    pub tag: Tag,
}

impl DragonSnapshot {
    pub fn from_live(d: &Dragon) -> Self {
        Self {
            id: d.id,
            tag: d.rreturn_pack(),
        }
    }

    pub fn to_live(&self, seed: i64) -> Option<Dragon> {
        Dragon::rreturn_unpack(&self.tag, seed)
    }
}

#[derive(Clone)]
pub struct BreathSnapshot {
    pub id: u32,
    pub nbt: Tag,
    pub at: [f64; 3],
    pub motion: [f64; 3],
    pub radius: f32,
    pub age: i32,
    pub duration: i32,
    pub from_dragon: bool,
    pub potion_none: bool,
    pub victims: Vec<(super::super::potions::Victim, i32)>,
}

#[derive(Clone)]
pub struct GuestSnapshot {
    pub uuid: u128,
    pub dimension: String,
    pub position: [f64; 3],
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub on_ground: bool,
    pub ground: GroundState,
    pub game_mode: i32,
    pub previous_game_mode: Option<i32>,
    pub flying: bool,
    pub flying_speed: f32,
    pub allow_flight: bool,
    pub sprinting: bool,
    pub sneaking: bool,
    pub fall_flying: bool,
    pub sleeping: bool,
    pub shoulders: [Option<i32>; 2],
    pub living_flags: i8,
    pub stuck: (i32, i32),
    pub bed: Option<[i32; 3]>,
    pub sleep_timer: u32,
    pub vehicle: Option<u32>,
    pub vehicle_order: i64,
    pub vehicle_good: Option<[f64; 3]>,
    pub spawn: Option<[f64; 3]>,
    pub respawn_elsewhere: Option<(String, [i32; 3], f32, f32, bool)>,
    pub last_death: Option<(String, [i32; 3])>,
    pub health: f32,
    pub absorption: f32,
    pub food: i32,
    pub saturation: f32,
    pub exhaustion: f32,
    pub food_timer: i32,
    pub fire_ticks: i32,
    pub air: i32,
    pub ticks_frozen: i32,
    pub fall_distance: f64,
    pub xp_level: i32,
    pub xp_progress: f32,
    pub xp_total: i32,
    pub xp_seed: i32,
    pub inventory: Vec<Option<ItemStack>>,
    pub armor: [Option<ItemStack>; 4],
    pub offhand: Option<ItemStack>,
    pub carried: Option<ItemStack>,
    pub ender_items: Vec<Option<ItemStack>>,
    pub selected: usize,
    pub craft_grid: [Option<ItemStack>; 4],
    pub craft_result: Option<ItemStack>,
    pub effects: Vec<EffectInstance>,
    pub effects_tick_count: i32,
    pub effects_changes: u32,
    pub shield_cooldown: u32,
    pub cooldowns: Cooldowns,
    pub recipe_book: RecipeBookSettings,
    pub advancements: Progress,
    pub stats: BTreeMap<String, BTreeMap<String, i32>>,
    pub seen_credits: bool,
    pub post_effects: Vec<String>,
    pub attachments: horizon_mod_runtime::attachments::ResidentAttachments,
    pub saved: Option<Tag>,
    pub hook: Option<HookSnapshot>,
    pub motion: Option<horizon_client::motion::PlayerMotion>,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskBeeExtra {
    pub flags: u8,
    pub anger_end: i64,
    pub roll_bits: u32,
    pub roll_o_bits: u32,
    pub time_since_sting: i32,
    pub ticks_without_nectar: i32,
    pub stay_out_of_hive: i32,
    pub crops_grown: i32,
    pub hive_cooldown: i32,
    pub flower_cooldown: i32,
    pub flower: Option<[i32; 3]>,
    pub hive: Option<[i32; 3]>,
    pub under_water: i32,
    pub pollinating: bool,
    pub validate_hive_cooldown: i32,
    pub validate_flower_cooldown: i32,
    pub hive_blacklist: [[i32; 3]; 3],
    pub hive_blacklist_len: u8,
}

impl DiskBeeExtra {
    fn from_bee(b: &horizon_client::mobs::bee::Bee) -> Self {
        let mut bl = [[0; 3]; 3];
        let bl_len = b.hive_blacklist.len().min(3);
        bl[..bl_len].copy_from_slice(&b.hive_blacklist[..bl_len]);
        Self {
            flags: b.flags,
            anger_end: b.anger_end,
            roll_bits: b.roll.to_bits(),
            roll_o_bits: b.roll_o.to_bits(),
            time_since_sting: b.time_since_sting,
            ticks_without_nectar: b.ticks_without_nectar,
            stay_out_of_hive: b.stay_out_of_hive,
            crops_grown: b.crops_grown,
            hive_cooldown: b.hive_cooldown,
            flower_cooldown: b.flower_cooldown,
            flower: b.flower,
            hive: b.hive,
            under_water: b.under_water,
            pollinating: b.pollinating,
            validate_hive_cooldown: b.validate_hive_cooldown,
            validate_flower_cooldown: b.validate_flower_cooldown,
            hive_blacklist: bl,
            hive_blacklist_len: bl_len as u8,
        }
    }

    fn apply_to_bee(&self, b: &mut horizon_client::mobs::bee::Bee) {
        b.flags = self.flags;
        b.anger_end = self.anger_end;
        b.roll = f32::from_bits(self.roll_bits);
        b.roll_o = f32::from_bits(self.roll_o_bits);
        b.time_since_sting = self.time_since_sting;
        b.ticks_without_nectar = self.ticks_without_nectar;
        b.stay_out_of_hive = self.stay_out_of_hive;
        b.crops_grown = self.crops_grown;
        b.hive_cooldown = self.hive_cooldown;
        b.flower_cooldown = self.flower_cooldown;
        b.flower = self.flower;
        b.hive = self.hive;
        b.under_water = self.under_water;
        b.pollinating = self.pollinating;
        b.validate_hive_cooldown = self.validate_hive_cooldown;
        b.validate_flower_cooldown = self.validate_flower_cooldown;
        let len = usize::from(self.hive_blacklist_len).min(3);
        b.hive_blacklist = self.hive_blacklist[..len].to_vec();
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskMobControls {
    pub mov_wanted_bits: [u64; 3],
    pub mov_speed_bits: u64,
    pub mov_strafe_bits: [u32; 2],
    pub mov_op: u8,
    pub mov_kind: Option<DiskMoveKind>,
    pub look_wanted_bits: [u64; 3],
    pub look_rot_bits: [u32; 2],
    pub look_cooldown: i32,
    pub look_kind: Option<DiskLookKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiskWho {
    Player,
    Mob(u32),
}

impl DiskWho {
    pub fn from_who(w: horizon_client::mobs::Who) -> Self {
        match w {
            horizon_client::mobs::Who::Player => Self::Player,
            horizon_client::mobs::Who::Mob(id) => Self::Mob(id),
        }
    }

    pub fn to_who(self, map_id: &dyn Fn(u32) -> Option<u32>) -> Option<horizon_client::mobs::Who> {
        match self {
            Self::Player => Some(horizon_client::mobs::Who::Player),
            Self::Mob(id) => map_id(id).map(horizon_client::mobs::Who::Mob),
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskPath {
    pub nodes: Vec<[i32; 4]>,
    pub next: usize,
    pub target: [i32; 3],
    pub dist_to_target_bits: u32,
    pub reached: bool,
}

impl DiskPath {
    pub fn from_path(p: &horizon_client::mobs::path::Path) -> Self {
        Self {
            nodes: p
                .nodes
                .iter()
                .map(|n| [n.x, n.y, n.z, n.kind as usize as i32])
                .collect(),
            next: p.next,
            target: p.target,
            dist_to_target_bits: p.dist_to_target.to_bits(),
            reached: p.reached,
        }
    }

    pub fn to_path(&self) -> horizon_client::mobs::path::Path {
        horizon_client::mobs::path::Path {
            nodes: self
                .nodes
                .iter()
                .map(|n| horizon_client::mobs::path::PathNode {
                    x: n[0],
                    y: n[1],
                    z: n[2],
                    kind: horizon_client::mobs::path::PathType::ALL
                        .get(n[3].max(0) as usize)
                        .copied()
                        .unwrap_or(horizon_client::mobs::path::PathType::Blocked),
                })
                .collect(),
            next: self.next,
            target: self.target,
            dist_to_target: f32::from_bits(self.dist_to_target_bits),
            reached: self.reached,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskMobSteps {
    pub floats_bits: [u32; 4],
    pub gallop: i32,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskNavState {
    pub path: Option<DiskPath>,
    pub pending_path: Option<DiskPath>,
    pub target_pos: Option<[i32; 3]>,
    pub speed_modifier_bits: u64,
    pub max_distance_to_waypoint_bits: u32,
    pub max_visited_multiplier_bits: u32,
    pub required_path_length_bits: u32,
    pub max_visited_nodes: i32,
    pub bools: [bool; 7],
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskPiglinMemories {
    pub guarded_angry_at: Option<Option<[i32; 4]>>,
    pub cannot_hunt: bool,
    pub hurt: bool,
    pub baby_hoglin: Option<u32>,
    pub ride_wait: i32,
    pub activity: u8,
    pub hurt_seen: i32,
    pub piglins: i32,
    pub hoglins: i32,
    pub nearest_piglin: Option<u32>,
    pub adults: Vec<u32>,
    pub zombified: Option<DiskWho>,
    pub nemesis: Option<DiskWho>,
    pub sees_loved: bool,
    pub retreats: Vec<(DiskWho, [u64; 3])>,
    pub seen_admire: i32,
    pub reach_time: Option<i32>,
    pub walk_off: i32,
    pub admire_disabled: i32,
    pub ate_recently: i32,
    pub hit_stop: bool,
    pub spear_status: Option<u8>,
    pub spear_engage_time: Option<i32>,
    pub spear_charge_pos: Option<[u64; 3]>,
    pub spear_flee_time: Option<i32>,
    pub spear_flee_pos: Option<[u64; 3]>,
}

impl DiskPiglinMemories {
    fn from_memories(p: &horizon_client::mobs::piglin::Memories) -> Option<Self> {
        let act = match p.activity {
            horizon_client::mobs::piglin::Activity::Idle => 0,
            horizon_client::mobs::piglin::Activity::Fight => 1,
            horizon_client::mobs::piglin::Activity::Avoid => 2,
            horizon_client::mobs::piglin::Activity::Admire => 3,
            horizon_client::mobs::piglin::Activity::Celebrate => 4,
            horizon_client::mobs::piglin::Activity::Ride => 5,
        };
        let spear_status = p.spear.status.map(|s| match s {
            horizon_client::mobs::spear_use::SpearStatus::Approach => 0,
            horizon_client::mobs::spear_use::SpearStatus::Charging => 1,
            horizon_client::mobs::spear_use::SpearStatus::Retreat => 2,
        });
        let has_data = p.guarded_angry_at.is_some()
            || p.cannot_hunt
            || p.hurt
            || p.baby_hoglin.is_some()
            || p.ride_wait != 0
            || act != 0
            || p.hurt_seen != 0
            || p.piglins != 0
            || p.hoglins != 0
            || p.nearest_piglin.is_some()
            || !p.adults.is_empty()
            || p.zombified.is_some()
            || p.nemesis.is_some()
            || p.sees_loved
            || !p.retreats.is_empty()
            || p.seen_admire != 0
            || p.reach_time.is_some()
            || p.walk_off != 0
            || p.admire_disabled != 0
            || p.ate_recently != 0
            || p.hit_stop
            || spear_status.is_some()
            || p.spear.engage_time.is_some()
            || p.spear.charge_position.is_some()
            || p.spear.fleeing_time.is_some()
            || p.spear.fleeing_position.is_some();
        has_data.then(|| Self {
            guarded_angry_at: p.guarded_angry_at,
            cannot_hunt: p.cannot_hunt,
            hurt: p.hurt,
            baby_hoglin: p.baby_hoglin,
            ride_wait: p.ride_wait,
            activity: act,
            hurt_seen: p.hurt_seen,
            piglins: p.piglins,
            hoglins: p.hoglins,
            nearest_piglin: p.nearest_piglin,
            adults: p.adults.clone(),
            zombified: p.zombified.map(DiskWho::from_who),
            nemesis: p.nemesis.map(DiskWho::from_who),
            sees_loved: p.sees_loved,
            retreats: p
                .retreats
                .iter()
                .map(|(w, v)| (DiskWho::from_who(*w), [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]))
                .collect(),
            seen_admire: p.seen_admire,
            reach_time: p.reach_time,
            walk_off: p.walk_off,
            admire_disabled: p.admire_disabled,
            ate_recently: p.ate_recently,
            hit_stop: p.hit_stop,
            spear_status,
            spear_engage_time: p.spear.engage_time,
            spear_charge_pos: p.spear.charge_position.map(|a| a.map(f64::to_bits)),
            spear_flee_time: p.spear.fleeing_time,
            spear_flee_pos: p.spear.fleeing_position.map(|a| a.map(f64::to_bits)),
        })
    }

    fn apply_scalar(&self, p: &mut horizon_client::mobs::piglin::Memories) {
        p.guarded_angry_at = self.guarded_angry_at;
        p.cannot_hunt = self.cannot_hunt;
        p.hurt = self.hurt;
        p.ride_wait = self.ride_wait;
        p.activity = match self.activity {
            1 => horizon_client::mobs::piglin::Activity::Fight,
            2 => horizon_client::mobs::piglin::Activity::Avoid,
            3 => horizon_client::mobs::piglin::Activity::Admire,
            4 => horizon_client::mobs::piglin::Activity::Celebrate,
            5 => horizon_client::mobs::piglin::Activity::Ride,
            _ => horizon_client::mobs::piglin::Activity::Idle,
        };
        p.hurt_seen = self.hurt_seen;
        p.piglins = self.piglins;
        p.hoglins = self.hoglins;
        p.sees_loved = self.sees_loved;
        p.seen_admire = self.seen_admire;
        p.reach_time = self.reach_time;
        p.walk_off = self.walk_off;
        p.admire_disabled = self.admire_disabled;
        p.ate_recently = self.ate_recently;
        p.hit_stop = self.hit_stop;
        p.spear.status = self.spear_status.map(|s| match s {
            1 => horizon_client::mobs::spear_use::SpearStatus::Charging,
            2 => horizon_client::mobs::spear_use::SpearStatus::Retreat,
            _ => horizon_client::mobs::spear_use::SpearStatus::Approach,
        });
        p.spear.engage_time = self.spear_engage_time;
        p.spear.charge_position = self.spear_charge_pos.map(|a| a.map(f64::from_bits));
        p.spear.fleeing_time = self.spear_flee_time;
        p.spear.fleeing_position = self.spear_flee_pos.map(|a| a.map(f64::from_bits));
    }

    fn apply_ids(
        &self,
        p: &mut horizon_client::mobs::piglin::Memories,
        map_id: &dyn Fn(u32) -> Option<u32>,
    ) {
        p.baby_hoglin = self.baby_hoglin.and_then(map_id);
        p.nearest_piglin = self.nearest_piglin.and_then(map_id);
        p.adults = self.adults.iter().copied().filter_map(map_id).collect();
        p.zombified = self.zombified.and_then(|w| w.to_who(map_id));
        p.nemesis = self.nemesis.and_then(|w| w.to_who(map_id));
        p.retreats = self
            .retreats
            .iter()
            .filter_map(|(w, v)| {
                w.to_who(map_id)
                    .map(|mw| (mw, Vec3::new(f64::from_bits(v[0]), f64::from_bits(v[1]), f64::from_bits(v[2]))))
            })
            .collect();
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskRaiderState {
    pub patrol_leader: bool,
    pub patrolling: bool,
    pub patrol_target: Option<[i32; 3]>,
    pub patrol_cooldown_until: i64,
    pub wave: i32,
    pub can_join_raid: bool,
    pub raid_id: Option<i32>,
    pub ticks_outside_raid: i32,
    pub celebrating: bool,
    pub active_raid: bool,
    pub raided: bool,
    pub raid_over: bool,
    pub raid_loss: bool,
    pub raid_center: [i32; 3],
    pub in_village: bool,
    pub banner_at_bits: Option<[u64; 3]>,
    pub banner_item: u32,
    pub unreachable_banners: Vec<(u32, i64)>,
    pub recruit_due: bool,
    pub homes: Option<Vec<[i32; 3]>>,
    pub rider: bool,
    pub registered: bool,
    pub hurt_seq: u64,
    pub pre_hurt_health_bits: u32,
}

impl DiskRaiderState {
    fn from_state(r: &horizon_client::mobs::raider::State) -> Self {
        Self {
            patrol_leader: r.patrol_leader,
            patrolling: r.patrolling,
            patrol_target: r.patrol_target,
            patrol_cooldown_until: r.patrol_cooldown_until,
            wave: r.wave,
            can_join_raid: r.can_join_raid,
            raid_id: r.raid_id,
            ticks_outside_raid: r.ticks_outside_raid,
            celebrating: r.celebrating,
            active_raid: r.active_raid,
            raided: r.raided,
            raid_over: r.raid_over,
            raid_loss: r.raid_loss,
            raid_center: r.raid_center,
            in_village: r.in_village,
            banner_at_bits: r.banner_at.map(|a| a.map(f64::to_bits)),
            banner_item: r.banner_item,
            unreachable_banners: r.unreachable_banners.clone(),
            recruit_due: r.recruit_due,
            homes: r.homes.as_deref().cloned(),
            rider: r.rider,
            registered: r.registered,
            hurt_seq: r.hurt_seq,
            pre_hurt_health_bits: r.pre_hurt_health.to_bits(),
        }
    }

    fn apply_to_state(&self, r: &mut horizon_client::mobs::raider::State) {
        r.patrol_leader = self.patrol_leader;
        r.patrolling = self.patrolling;
        r.patrol_target = self.patrol_target;
        r.patrol_cooldown_until = self.patrol_cooldown_until;
        r.wave = self.wave;
        r.can_join_raid = self.can_join_raid;
        r.raid_id = self.raid_id;
        r.ticks_outside_raid = self.ticks_outside_raid;
        r.celebrating = self.celebrating;
        r.active_raid = self.active_raid;
        r.raided = self.raided;
        r.raid_over = self.raid_over;
        r.raid_loss = self.raid_loss;
        r.raid_center = self.raid_center;
        r.in_village = self.in_village;
        r.banner_at = self.banner_at_bits.map(|a| a.map(f64::from_bits));
        r.banner_item = self.banner_item;
        r.unreachable_banners = self.unreachable_banners.clone();
        r.recruit_due = self.recruit_due;
        r.homes = self.homes.clone().map(std::sync::Arc::new);
        r.rider = self.rider;
        r.registered = self.registered;
        r.hurt_seq = self.hurt_seq;
        r.pre_hurt_health = f32::from_bits(self.pre_hurt_health_bits);
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskMobExtra {
    pub prev_bits: [u64; 3],
    pub vel_bits: [u64; 3],
    pub rot_bits: [u32; 8],
    pub flags: [bool; 13],
    pub hurt_time: i32,
    pub invulnerable_time: i32,
    pub death_time: i32,
    pub impulse_grace_time: i32,
    pub limb_bits: [u32; 3],
    pub fall_distance_bits: u64,
    pub stuck_bits: [u64; 3],
    pub absorption_bits: u32,
    pub dealt_absorbed: i32,
    pub anim_floats: [u32; 7],
    pub anim_v: [u32; 8],
    pub anim_vo: [u32; 8],
    pub fluid_water_bits: u64,
    pub fluid_lava_bits: u64,
    pub fluid_flags: [bool; 4],
    pub ai_floats: [u32; 4],
    pub ai_ints: [i32; 5],
    pub ai_u32: u32,
    pub ai_bools: [bool; 5],
    pub supporting: Option<[i32; 3]>,
    pub section_seq: Option<([i32; 3], u64)>,
    pub controls: Option<DiskMobControls>,
    pub bee: Option<DiskBeeExtra>,
    pub home: Option<([i32; 3], i32)>,
    pub ai_paused: bool,
    pub goal_disabled: [u8; 2],
    pub last_hurt_by_time: i32,
    pub last_hurt_by_ref: Option<[i32; 4]>,
    pub malus: Vec<Option<u32>>,
    pub steps: Option<DiskMobSteps>,
    pub nav: Option<DiskNavState>,
    pub target: Option<DiskWho>,
    pub last_hurt_by: Option<DiskWho>,
    pub seen_cache: Vec<(DiskWho, bool)>,
    pub pushed: Vec<u32>,
    pub angry_at: Option<DiskWho>,
    pub piglin: Option<DiskPiglinMemories>,
    pub raider: Option<DiskRaiderState>,
    pub ext: Option<DiskMobExtended>,
}

impl DiskMobExtra {
    /// Captures transient runtime fields of `Mob` lost across NBT serialization.
    pub fn from_mob(m: &Mob) -> Self {
        Self {
            prev_bits: [m.previous.x.to_bits(), m.previous.y.to_bits(), m.previous.z.to_bits()],
            vel_bits: [m.velocity.x.to_bits(), m.velocity.y.to_bits(), m.velocity.z.to_bits()],
            rot_bits: [
                m.yaw.to_bits(),
                m.previous_yaw.to_bits(),
                m.body_yaw.to_bits(),
                m.previous_body_yaw.to_bits(),
                m.head_yaw.to_bits(),
                m.previous_head_yaw.to_bits(),
                m.head_pitch.to_bits(),
                m.previous_head_pitch.to_bits(),
            ],
            flags: [
                m.on_ground,
                m.horizontal_collision,
                m.vertical_collision_below,
                m.needs_sync,
                m.sync_position,
                m.was_eye_in_water,
                m.first_tick,
                m.climbing,
                m.on_climbable,
                m.in_powder_snow,
                m.no_gravity,
                m.no_physics,
                m.effects_dirty,
            ],
            hurt_time: m.hurt_time,
            invulnerable_time: m.invulnerable_time,
            death_time: m.death_time,
            impulse_grace_time: m.impulse_grace_time,
            limb_bits: [
                m.limb_position.to_bits(),
                m.limb_speed.to_bits(),
                m.previous_limb_speed.to_bits(),
            ],
            fall_distance_bits: m.fall_distance.to_bits(),
            stuck_bits: [
                m.stuck_multiplier.x.to_bits(),
                m.stuck_multiplier.y.to_bits(),
                m.stuck_multiplier.z.to_bits(),
            ],
            absorption_bits: m.absorption.to_bits(),
            dealt_absorbed: m.dealt_absorbed,
            anim_floats: [
                m.anim.attack_anim.to_bits(),
                m.anim.attack_anim_o.to_bits(),
                m.anim.flap.to_bits(),
                m.anim.flap_o.to_bits(),
                m.anim.flap_speed.to_bits(),
                m.anim.flap_speed_o.to_bits(),
                m.anim.flapping.to_bits(),
            ],
            anim_v: m.anim.v.map(f32::to_bits),
            anim_vo: m.anim.vo.map(f32::to_bits),
            fluid_water_bits: m.fluids.water.to_bits(),
            fluid_lava_bits: m.fluids.lava.to_bits(),
            fluid_flags: [
                m.fluids.in_water,
                m.fluids.in_lava,
                m.fluids.eye_in_water,
                m.fluids.eye_in_lava,
            ],
            ai_floats: [
                m.ai.xxa.to_bits(),
                m.ai.yya.to_bits(),
                m.ai.zza.to_bits(),
                m.ai.speed.to_bits(),
            ],
            ai_ints: [
                m.ai.no_jump_delay,
                m.ai.no_action_time,
                m.ai.swing_time,
                m.ai.swing_duration,
                m.ai.last_hurt_by_player,
            ],
            ai_u32: m.ai.dab_turns,
            ai_bools: [
                m.ai.jump,
                m.ai.jumping,
                m.ai.aggressive,
                m.ai.panicking,
                m.ai.swinging,
            ],
            supporting: m.supporting,
            section_seq: Some((m.extra.section, m.extra.section_seq)),
            controls: Some(DiskMobControls {
                mov_wanted_bits: m.ai.mov.wanted.map(f64::to_bits),
                mov_speed_bits: m.ai.mov.speed_modifier.to_bits(),
                mov_strafe_bits: [
                    m.ai.mov.strafe_forwards.to_bits(),
                    m.ai.mov.strafe_right.to_bits(),
                ],
                mov_op: match m.ai.mov.operation {
                    horizon_client::mobs::control::Operation::Wait => 0,
                    horizon_client::mobs::control::Operation::MoveTo => 1,
                    horizon_client::mobs::control::Operation::Strafe => 2,
                    horizon_client::mobs::control::Operation::Jumping => 3,
                },
                mov_kind: Some(DiskMoveKind::from_mov(m.ai.mov.kind)),
                look_wanted_bits: m.ai.look.wanted.map(f64::to_bits),
                look_rot_bits: [
                    m.ai.look.y_max_rot_speed.to_bits(),
                    m.ai.look.x_max_rot_angle.to_bits(),
                ],
                look_cooldown: m.ai.look.cooldown,
                look_kind: Some(DiskLookKind::from_look(m.ai.look.kind)),
            }),
            bee: m.extra.bee.as_deref().map(DiskBeeExtra::from_bee),
            home: m.ai.home,
            ai_paused: m.ai.ai_paused,
            goal_disabled: [m.ai.goals.disabled, m.ai.targets.disabled],
            last_hurt_by_time: m.ai.last_hurt_by_time,
            last_hurt_by_ref: m.ai.last_hurt_by_ref,
            malus: if m.ai.malus.iter().any(Option::is_some) {
                m.ai.malus.iter().map(|v| v.map(f32::to_bits)).collect()
            } else {
                Vec::new()
            },
            steps: Some(DiskMobSteps {
                floats_bits: [
                    m.ai.steps.move_dist.to_bits(),
                    m.ai.steps.fly_dist.to_bits(),
                    m.ai.steps.next_step.to_bits(),
                    m.ai.steps.next_flap.to_bits(),
                ],
                gallop: m.ai.steps.gallop,
            }),
            nav: Some(DiskNavState {
                path: m.ai.nav.path.as_ref().map(DiskPath::from_path),
                pending_path: m.ai.pending_path.as_ref().map(DiskPath::from_path),
                target_pos: m.ai.nav.target_pos,
                speed_modifier_bits: m.ai.nav.speed_modifier.to_bits(),
                max_distance_to_waypoint_bits: m.ai.nav.max_distance_to_waypoint.to_bits(),
                max_visited_multiplier_bits: m.ai.nav.max_visited_multiplier.to_bits(),
                required_path_length_bits: m.ai.nav.required_path_length.to_bits(),
                max_visited_nodes: m.ai.nav.max_visited_nodes,
                bools: [
                    m.ai.nav.stuck,
                    m.ai.nav.can_float,
                    m.ai.nav.can_open_doors,
                    m.ai.nav.can_pass_doors,
                    m.ai.nav.can_walk_over_fences,
                    m.ai.nav.avoid_sun,
                    m.ai.nav.below_surface,
                ],
            }),
            target: m.ai.target.map(DiskWho::from_who),
            last_hurt_by: m.ai.last_hurt_by.map(DiskWho::from_who),
            seen_cache: m
                .ai
                .seen_cache
                .iter()
                .map(|&(w, b)| (DiskWho::from_who(w), b))
                .collect(),
            pushed: m.pushed.clone(),
            angry_at: m.extra.angry_at.map(DiskWho::from_who),
            piglin: DiskPiglinMemories::from_memories(&m.extra.piglin),
            raider: m.extra.raider.as_deref().map(DiskRaiderState::from_state),
            ext: DiskMobExtended::from_mob(m),
        }
    }

    /// Restores transient runtime fields onto a hydrated `Mob`.
    pub fn apply_to_mob(&self, m: &mut Mob) {
        self.apply_to_mob_with_orig(m, m.id);
    }

    /// Restores transient runtime fields onto a hydrated `Mob` using its pre-load ID for brain seeds.
    pub fn apply_to_mob_with_orig(&self, m: &mut Mob, orig_id: u32) {
        m.previous = Vec3::new(
            f64::from_bits(self.prev_bits[0]),
            f64::from_bits(self.prev_bits[1]),
            f64::from_bits(self.prev_bits[2]),
        );
        m.velocity = Vec3::new(
            f64::from_bits(self.vel_bits[0]),
            f64::from_bits(self.vel_bits[1]),
            f64::from_bits(self.vel_bits[2]),
        );
        m.yaw = f32::from_bits(self.rot_bits[0]);
        m.previous_yaw = f32::from_bits(self.rot_bits[1]);
        m.body_yaw = f32::from_bits(self.rot_bits[2]);
        m.previous_body_yaw = f32::from_bits(self.rot_bits[3]);
        m.head_yaw = f32::from_bits(self.rot_bits[4]);
        m.previous_head_yaw = f32::from_bits(self.rot_bits[5]);
        m.head_pitch = f32::from_bits(self.rot_bits[6]);
        m.previous_head_pitch = f32::from_bits(self.rot_bits[7]);
        m.on_ground = self.flags[0];
        m.horizontal_collision = self.flags[1];
        m.vertical_collision_below = self.flags[2];
        m.needs_sync = self.flags[3];
        m.sync_position = self.flags[4];
        m.was_eye_in_water = self.flags[5];
        m.first_tick = self.flags[6];
        m.climbing = self.flags[7];
        m.on_climbable = self.flags[8];
        m.in_powder_snow = self.flags[9];
        m.no_gravity = self.flags[10];
        m.no_physics = self.flags[11];
        m.effects_dirty = self.flags[12];
        m.hurt_time = self.hurt_time;
        m.invulnerable_time = self.invulnerable_time;
        m.death_time = self.death_time;
        m.impulse_grace_time = self.impulse_grace_time;
        m.limb_position = f32::from_bits(self.limb_bits[0]);
        m.limb_speed = f32::from_bits(self.limb_bits[1]);
        m.previous_limb_speed = f32::from_bits(self.limb_bits[2]);
        m.fall_distance = f64::from_bits(self.fall_distance_bits);
        m.stuck_multiplier = Vec3::new(
            f64::from_bits(self.stuck_bits[0]),
            f64::from_bits(self.stuck_bits[1]),
            f64::from_bits(self.stuck_bits[2]),
        );
        m.absorption = f32::from_bits(self.absorption_bits);
        m.dealt_absorbed = self.dealt_absorbed;
        m.anim.attack_anim = f32::from_bits(self.anim_floats[0]);
        m.anim.attack_anim_o = f32::from_bits(self.anim_floats[1]);
        m.anim.flap = f32::from_bits(self.anim_floats[2]);
        m.anim.flap_o = f32::from_bits(self.anim_floats[3]);
        m.anim.flap_speed = f32::from_bits(self.anim_floats[4]);
        m.anim.flap_speed_o = f32::from_bits(self.anim_floats[5]);
        m.anim.flapping = f32::from_bits(self.anim_floats[6]);
        m.anim.v = self.anim_v.map(f32::from_bits);
        m.anim.vo = self.anim_vo.map(f32::from_bits);
        m.fluids.water = f64::from_bits(self.fluid_water_bits);
        m.fluids.lava = f64::from_bits(self.fluid_lava_bits);
        m.fluids.in_water = self.fluid_flags[0];
        m.fluids.in_lava = self.fluid_flags[1];
        m.fluids.eye_in_water = self.fluid_flags[2];
        m.fluids.eye_in_lava = self.fluid_flags[3];
        m.ai.xxa = f32::from_bits(self.ai_floats[0]);
        m.ai.yya = f32::from_bits(self.ai_floats[1]);
        m.ai.zza = f32::from_bits(self.ai_floats[2]);
        m.ai.speed = f32::from_bits(self.ai_floats[3]);
        m.ai.no_jump_delay = self.ai_ints[0];
        m.ai.no_action_time = self.ai_ints[1];
        m.ai.swing_time = self.ai_ints[2];
        m.ai.swing_duration = self.ai_ints[3];
        m.ai.last_hurt_by_player = self.ai_ints[4];
        m.ai.dab_turns = self.ai_u32;
        m.ai.jump = self.ai_bools[0];
        m.ai.jumping = self.ai_bools[1];
        m.ai.aggressive = self.ai_bools[2];
        m.ai.panicking = self.ai_bools[3];
        m.ai.swinging = self.ai_bools[4];
        m.supporting = self.supporting;
        if let Some((sec, seq)) = self.section_seq {
            m.extra.section = sec;
            m.extra.section_seq = seq;
        }
        if let Some(c) = self.controls {
            m.ai.mov.wanted = c.mov_wanted_bits.map(f64::from_bits);
            m.ai.mov.speed_modifier = f64::from_bits(c.mov_speed_bits);
            m.ai.mov.strafe_forwards = f32::from_bits(c.mov_strafe_bits[0]);
            m.ai.mov.strafe_right = f32::from_bits(c.mov_strafe_bits[1]);
            m.ai.mov.operation = match c.mov_op {
                1 => horizon_client::mobs::control::Operation::MoveTo,
                2 => horizon_client::mobs::control::Operation::Strafe,
                3 => horizon_client::mobs::control::Operation::Jumping,
                _ => horizon_client::mobs::control::Operation::Wait,
            };
            if let Some(mk) = c.mov_kind {
                m.ai.mov.kind = mk.to_mov();
            }
            m.ai.look.wanted = c.look_wanted_bits.map(f64::from_bits);
            m.ai.look.y_max_rot_speed = f32::from_bits(c.look_rot_bits[0]);
            m.ai.look.x_max_rot_angle = f32::from_bits(c.look_rot_bits[1]);
            m.ai.look.cooldown = c.look_cooldown;
            if let Some(lk) = c.look_kind {
                m.ai.look.kind = lk.to_look();
            }
        }
        if let Some(bee_extra) = &self.bee {
            let dst = m.extra.bee.get_or_insert_with(Default::default);
            bee_extra.apply_to_bee(dst);
        }
        m.ai.home = self.home;
        m.ai.ai_paused = self.ai_paused;
        m.ai.goals.disabled = self.goal_disabled[0];
        m.ai.targets.disabled = self.goal_disabled[1];
        m.ai.last_hurt_by_time = self.last_hurt_by_time;
        m.ai.last_hurt_by_ref = self.last_hurt_by_ref;
        if self.malus.len() == horizon_client::mobs::path::PATH_TYPES {
            for (dst, src) in m.ai.malus.iter_mut().zip(&self.malus) {
                *dst = src.map(f32::from_bits);
            }
        }
        if let Some(s) = self.steps {
            m.ai.steps.move_dist = f32::from_bits(s.floats_bits[0]);
            m.ai.steps.fly_dist = f32::from_bits(s.floats_bits[1]);
            m.ai.steps.next_step = f32::from_bits(s.floats_bits[2]);
            m.ai.steps.next_flap = f32::from_bits(s.floats_bits[3]);
            m.ai.steps.gallop = s.gallop;
        }
        if let Some(nav) = &self.nav {
            m.ai.nav.path = nav.path.as_ref().map(DiskPath::to_path);
            m.ai.pending_path = nav.pending_path.as_ref().map(DiskPath::to_path);
            m.ai.nav.target_pos = nav.target_pos;
            m.ai.nav.speed_modifier = f64::from_bits(nav.speed_modifier_bits);
            m.ai.nav.max_distance_to_waypoint = f32::from_bits(nav.max_distance_to_waypoint_bits);
            m.ai.nav.max_visited_multiplier = f32::from_bits(nav.max_visited_multiplier_bits);
            m.ai.nav.required_path_length = f32::from_bits(nav.required_path_length_bits);
            m.ai.nav.max_visited_nodes = nav.max_visited_nodes;
            m.ai.nav.stuck = nav.bools[0];
            m.ai.nav.can_float = nav.bools[1];
            m.ai.nav.can_open_doors = nav.bools[2];
            m.ai.nav.can_pass_doors = nav.bools[3];
            m.ai.nav.can_walk_over_fences = nav.bools[4];
            m.ai.nav.avoid_sun = nav.bools[5];
            m.ai.nav.below_surface = nav.bools[6];
        }
        if let Some(p) = &self.piglin {
            p.apply_scalar(&mut m.extra.piglin);
        }
        if let Some(r) = &self.raider {
            let dst = m.extra.raider.get_or_insert_with(Default::default);
            r.apply_to_state(dst);
        }
        if let Some(ext) = &self.ext {
            ext.apply_scalar(m, orig_id);
        }
    }

    /// Remaps and restores entity-ID-bearing AI/memory references after all mobs are hydrated.
    pub fn apply_id_refs(&self, m: &mut Mob, map_id: &dyn Fn(u32) -> Option<u32>) {
        if let Some(target) = self.target {
            m.ai.target = target.to_who(map_id);
        }
        if let Some(hurt) = self.last_hurt_by {
            m.ai.last_hurt_by = hurt.to_who(map_id);
        }
        if !self.seen_cache.is_empty() {
            m.ai.seen_cache = self
                .seen_cache
                .iter()
                .filter_map(|&(w, b)| w.to_who(map_id).map(|mw| (mw, b)))
                .collect();
        }
        if !self.pushed.is_empty() {
            m.pushed = self.pushed.iter().copied().filter_map(map_id).collect();
        }
        if let Some(angry) = self.angry_at {
            m.extra.angry_at = angry.to_who(map_id);
        }
        if let Some(p) = &self.piglin {
            p.apply_ids(&mut m.extra.piglin, map_id);
        }
        if let Some(ext) = &self.ext {
            ext.apply_id_refs(m, map_id);
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiskMobRuntime {
    pub age: u32,
    pub invulnerable: i32,
    pub last_hurt: f32,
    pub swell: i32,
    pub previous_swell: i32,
    pub ambient_timer: i32,
    pub rng_state: u64,
    pub rng_gaussian: Option<f64>,
    pub anger: i32,
    pub anger_end: i64,
    pub anger_stamp: Option<(i32, u32)>,
    pub counter: i32,
    pub counter2: i32,
    pub anim_ticks: i32,
    pub egg_time: i32,
    pub flag: bool,
    pub flag2: bool,
    pub vector: [f64; 3],
    pub ints: [i32; 6],
    pub body_adopted: bool,
    pub extra: Option<DiskMobExtra>,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskItemExtra {
    pub prev_bits: [u64; 3],
    pub vel_bits: [u64; 3],
    pub stuck_bits: [u64; 3],
    pub fall_distance_bits: u64,
    pub age: i32,
    pub pickup_delay: i64,
    pub flags: [bool; 6],
}

impl DiskItemExtra {
    /// Captures transient physics fields of `ItemEntity` lost across NBT serialization.
    pub fn from_item(d: &ItemEntity) -> Self {
        Self {
            prev_bits: [d.previous.x.to_bits(), d.previous.y.to_bits(), d.previous.z.to_bits()],
            vel_bits: [d.velocity.x.to_bits(), d.velocity.y.to_bits(), d.velocity.z.to_bits()],
            stuck_bits: [d.stuck.x.to_bits(), d.stuck.y.to_bits(), d.stuck.z.to_bits()],
            fall_distance_bits: d.fall_distance.to_bits(),
            age: d.age,
            pickup_delay: d.pickup_delay,
            flags: [
                d.on_ground,
                d.no_gravity,
                d.was_touching_water,
                d.vertical_collision,
                d.horizontal_collision,
                d.needs_sync,
            ],
        }
    }

    /// Restores transient physics fields onto a hydrated `ItemEntity`.
    pub fn apply_to_item(&self, d: &mut ItemEntity) {
        d.previous = Vec3::new(
            f64::from_bits(self.prev_bits[0]),
            f64::from_bits(self.prev_bits[1]),
            f64::from_bits(self.prev_bits[2]),
        );
        d.velocity = Vec3::new(
            f64::from_bits(self.vel_bits[0]),
            f64::from_bits(self.vel_bits[1]),
            f64::from_bits(self.vel_bits[2]),
        );
        d.stuck = Vec3::new(
            f64::from_bits(self.stuck_bits[0]),
            f64::from_bits(self.stuck_bits[1]),
            f64::from_bits(self.stuck_bits[2]),
        );
        d.fall_distance = f64::from_bits(self.fall_distance_bits);
        d.age = self.age;
        d.pickup_delay = self.pickup_delay;
        d.on_ground = self.flags[0];
        d.no_gravity = self.flags[1];
        d.was_touching_water = self.flags[2];
        d.vertical_collision = self.flags[3];
        d.horizontal_collision = self.flags[4];
        d.needs_sync = self.flags[5];
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DiskItemRuntime {
    pub ticks: u32,
    pub bob_offset: f32,
    pub rng_state: u64,
    pub rng_gaussian: Option<f64>,
    pub extra: Option<DiskItemExtra>,
}

#[derive(Clone, Debug)]
pub struct HookSnapshot {
    pub id: u32,
    pub position: [f64; 3],
    pub previous: [f64; 3],
    pub velocity: [f64; 3],
    pub rng_state: u64,
    pub nibble: i32,
    pub time_until_lured: i32,
    pub time_until_hooked: i32,
    pub fish_angle: f32,
    pub open_water: bool,
    pub state_bobbing: bool,
}

pub fn extract_active_cooldowns(cooldowns: &Cooldowns) -> Vec<(Arc<str>, i32)> {
    let mut cl = cooldowns.clone();
    let mut out = Vec::new();
    let mut ticks = 0i32;
    while !cl.is_empty() && ticks < 72_000 {
        ticks += 1;
        for group in cl.tick() {
            out.push((group, ticks));
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct TridentSnapshot {
    pub join: horizon_client::join_seq::JoinSeq,
    pub id: u32,
    pub nbt: Tag,
    pub previous: Vec3,
    pub in_ground_time: i32,
    pub no_physics: bool,
    pub owner: u32,
    pub guest: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct RocketSnapshot {
    pub id: u32,
    pub nbt: Tag,
    pub attached: bool,
    pub guest: Option<u32>,
    pub owner: Option<horizon_client::mobs::Who>,
    pub attached_to: Option<u32>,
}

#[derive(Clone)]
pub struct LevelSnapshot {
    pub chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
    pub block_entities: HashMap<(i32, i32, i32), BlockEntity>,
    pub block_entity_ticker_order: Vec<(i32, i32, i32)>,
    pub block_entity_asleep: Vec<(i32, i32, i32)>,
    pub block_names: FxHashMap<(i32, i32, i32), Tag>,
    pub be_attachments: Vec<((i32, i32, i32), Attachments)>,
    pub world_attachments: Attachments,
    pub chest_openers: Vec<((i32, i32, i32), (i32, f64))>,
    pub border: WorldBorder,
    pub shulker_anims: Vec<([i32; 3], horizon_client::block_entity_anim::Shulker)>,
    pub gateway_anims: Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)>,
    pub fangs: horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>,
    pub ominous_spawners: Vec<(u32, Tag)>,
    pub portals_entities: HashMap<u32, horizon_client::nether_portal::PortalState>,
    pub lectern_pulses: Vec<((i32, i32, i32), i32)>,
    pub lid_openers: HashMap<(i32, i32, i32), i32>,
    pub inhabited: FxHashMap<(i32, i32), i64>,
    pub end_clock: i64,
    pub random: LevelRandom,
    pub forced_chunks: HashSet<(i32, i32)>,
    pub chunk_ticks: HashMap<(i32, i32), (ChunkTicks, ChunkTicks)>,
    pub random_tick_value: i32,
    pub comparator_outputs: HashMap<BlockPos, i32>,
    pub torch_toggles: Vec<(BlockPos, i64)>,
    pub moving_pistons: Vec<(BlockPos, MovingPiston)>,
    pub piston_pushes: Vec<(BlockPos, MovingPiston, f32)>,
    pub block_events: VecDeque<BlockEventData>,
    pub hoppers: HashMap<(i32, i32, i32), HopperState>,
    pub mobs: horizon_client::join_seq::JoinList<Mob>,
    pub dropped: horizon_client::join_seq::JoinList<ItemEntity>,
    pub orbs: horizon_client::join_seq::JoinList<ExperienceOrb>,
    pub projectiles: horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>,
    pub boats: horizon_client::join_seq::JoinList<Boat, false>,
    pub carts: horizon_client::join_seq::JoinList<Minecart, false>,
    pub tnts: horizon_client::join_seq::JoinList<PrimedTnt>,
    pub falling: horizon_client::join_seq::JoinList<FallingBlock, false>,
    pub paintings: horizon_client::join_seq::JoinList<Painting>,
    pub item_frames: horizon_client::join_seq::JoinList<ItemFrame>,
    pub armor_stands: horizon_client::join_seq::JoinList<ArmorStand>,
    pub cushions: horizon_client::join_seq::JoinList<Cushion>,
    pub others: horizon_client::join_seq::ResidentList<OtherEntity>,
    pub crystals: horizon_client::join_seq::JoinList<Crystal>,
    pub dragon: Option<DragonSnapshot>,
    pub tridents: Vec<TridentSnapshot>,
    pub rockets: Vec<RocketSnapshot>,
    pub breath_clouds: Vec<BreathSnapshot>,
    pub thrown_potions: ThrownPotionsSnapshot,
    pub knot_ids: HashMap<[i32; 3], u32>,
    pub persist_uuids: HashMap<u32, [i32; 4]>,
    pub persist_raw: HashMap<u32, Tag>,
    pub persist_passenger_slots: HashMap<u32, Vec<super::super::persist::PassengerSlot>>,
    pub raids_dat: Option<Vec<u8>>,
    pub raids_mem: Option<Raids>,
    pub raid_runtime: RaidRuntimeSnapshot,
    pub last_hurt_by_player: HashMap<u32, super::super::combat::PlayerCredit>,
    pub love_cause: HashMap<u32, Option<u32>>,
    pub died: HashSet<u32>,
}

#[derive(Clone)]
pub struct TimelineSnapshot {
    pub anchor_ids: HashSet<u128>,
    pub policy: RollbackPolicy,
    pub game_time: i64,
    pub day_time: i64,
    pub weather: Weather,
    pub health_rules: HealthRules,
    pub rules: GameRules,
    pub border: WorldBorder,
    pub bossbars: Tag,
    pub sculk_tracker: WardenSpawnTracker,
    pub sculk_darkness: (f32, f32),
    pub dragon_fight_dat: Option<Vec<u8>>,
    pub fight_tag: Option<Tag>,
    pub trader_dat: Option<Vec<u8>>,
    pub trader_tag: Option<Tag>,
    pub rand_sequences: horizon_client::random_sequences::RandomSequences,
    pub region_files: HashSet<PathBuf>,
    pub difficulty: u8,
    pub difficulty_locked: bool,
    pub clock_rate: f32,
    pub clock_frozen: bool,
    pub time_rate: f32,
    pub time_partial: f32,
    pub time_paused: bool,
    pub command_clocks: BTreeMap<String, (i64, super::super::chat::WorldClock)>,
    pub world_spawn: Option<[i32; 3]>,
    pub world_spawn_angle: [f32; 2],
    pub next_entity: u32,
    pub ticked_entities_through: u32,
    pub ordered_state: OrderedStateSnapshot,
    pub cat_tick: i32,
    pub phantom_tick: i32,
    pub patrol_next_tick: i32,
    pub trader_tick_delay: i32,
    pub trader_dirty: bool,
    pub village_siege: VillageSiegeSnapshot,
    pub player_extra: Option<Tag>,
    pub shoulder_time: HashMap<[i32; 4], i64>,
    pub fangs: horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>,
    pub ominous_spawners: Vec<(u32, Tag)>,
    pub portals_entities: HashMap<u32, horizon_client::nether_portal::PortalState>,
    pub end_gateway_cooldowns: HashMap<u32, i32>,
    pub lectern_pulses: Vec<((i32, i32, i32), i32)>,
    pub player_beds: Vec<(Dim, [i32; 3])>,
    pub sleep_seen: (i64, i64),
    pub lead_killers: HashMap<u32, (u128, Dim)>,
    pub lid_openers: HashMap<(i32, i32, i32), i32>,
    pub entity_attachments: FxHashMap<u32, Attachments>,
    pub player_attachments: horizon_mod_runtime::attachments::ResidentAttachments,
    pub guest_attachments: HashMap<u128, Attachments>,
    pub scheduled_events: Vec<super::super::functions::Scheduled>,
    pub scoreboard: Scoreboard,
    pub command_storage: Option<CommandStorage>,
    pub stopwatches_elapsed: Option<BTreeMap<String, i64>>,
    pub sound_seed_state: Option<u64>,
    pub det_rng_uniquifier: i64,
    pub cmd_post_effects: Vec<String>,
    pub guests: HashMap<u128, GuestSnapshot>,
    pub offline_player_data: HashMap<PathBuf, Vec<u8>>,
    pub levels: HashMap<Dim, LevelSnapshot>,
    pub mob_players: FxHashMap<u32, u32>,
    pub disk_restored: bool,
    pub client_using: Option<(usize, u32)>,
    pub client_use_duration: i32,
    pub client_use_ticks_set: bool,
    pub client_crossbow_sounds: [bool; 2],
}

#[derive(Clone)]
pub struct ClientBlockSnapshot {
    pub dim: Dim,
    pub chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
}

static CLIENT_SNAPSHOT: OnceLock<RwLock<Option<ClientBlockSnapshot>>> = OnceLock::new();
static CLIENT_SLOTS: OnceLock<RwLock<FxHashMap<String, ClientBlockSnapshot>>> = OnceLock::new();

fn client_snapshot_lock() -> &'static RwLock<Option<ClientBlockSnapshot>> {
    CLIENT_SNAPSHOT.get_or_init(|| RwLock::new(None))
}

fn client_slots_lock() -> &'static RwLock<FxHashMap<String, ClientBlockSnapshot>> {
    CLIENT_SLOTS.get_or_init(|| RwLock::new(FxHashMap::default()))
}

pub fn get_client_snapshot_clone() -> Option<ClientBlockSnapshot> {
    client_snapshot_lock().read().ok()?.clone()
}

pub fn set_client_snapshot(snap: Option<ClientBlockSnapshot>) {
    if let Ok(mut lock) = client_snapshot_lock().write() {
        *lock = snap;
    }
}

pub fn get_client_slot_snapshot_clone(slot: &str) -> Option<ClientBlockSnapshot> {
    if slot == "default" {
        return get_client_snapshot_clone();
    }
    client_slots_lock().read().ok()?.get(slot).cloned()
}

pub fn set_client_slot_snapshot(slot: &str, snap: ClientBlockSnapshot) {
    if slot == "default" {
        set_client_snapshot(Some(snap));
        return;
    }
    if let Ok(mut lock) = client_slots_lock().write() {
        lock.insert(slot.to_owned(), snap);
    }
}

use std::sync::atomic::{AtomicU64, Ordering};

pub static ROLLBACK_EPOCH: AtomicU64 = AtomicU64::new(0);

pub fn bump_rollback_epoch() {
    ROLLBACK_EPOCH.fetch_add(1, Ordering::SeqCst);
}

pub fn get_rollback_epoch() -> u64 {
    ROLLBACK_EPOCH.load(Ordering::SeqCst)
}

std::thread_local! {
    static CLIENT_LAST_EPOCH: RefCell<u64> = const { RefCell::new(0) };
}

pub fn sync_client_epoch() {
    CLIENT_LAST_EPOCH.with(|cell| *cell.borrow_mut() = get_rollback_epoch());
}

pub fn client_epoch_needs_sync() -> bool {
    let current_epoch = get_rollback_epoch();
    CLIENT_LAST_EPOCH.with(|cell| {
        let mut last = cell.borrow_mut();
        if *last != current_epoch {
            *last = current_epoch;
            true
        } else {
            false
        }
    })
}

pub fn clear_client_snapshots() {
    set_client_snapshot(None);
    if let Ok(mut lock) = client_slots_lock().write() {
        lock.clear();
    }
    clear_client_ui_slots();
}

std::thread_local! {
    pub static SNAPSHOT: RefCell<Option<Arc<TimelineSnapshot>>> = const { RefCell::new(None) };
    pub static SLOTS: RefCell<HashMap<String, Arc<TimelineSnapshot>>> = RefCell::new(HashMap::new());
}

pub fn get_snapshot_clone() -> Option<Arc<TimelineSnapshot>> {
    SNAPSHOT.with(|cell| cell.borrow().as_ref().map(Arc::clone))
}

pub fn set_snapshot(snap: Option<Arc<TimelineSnapshot>>) {
    SNAPSHOT.with(|cell| *cell.borrow_mut() = snap);
}

pub fn clear_snapshot() {
    SNAPSHOT.with(|cell| *cell.borrow_mut() = None);
}

pub fn clear_all_slots() {
    clear_snapshot();
    SLOTS.with(|cell| cell.borrow_mut().clear());
}

pub fn get_slot_snapshot_clone(slot: &str) -> Option<Arc<TimelineSnapshot>> {
    if slot == "default" {
        return get_snapshot_clone();
    }
    SLOTS.with(|cell| cell.borrow().get(slot).map(Arc::clone))
}

pub fn set_slot_snapshot(slot: &str, snap: Arc<TimelineSnapshot>) {
    if slot == "default" {
        set_snapshot(Some(snap));
        return;
    }
    SLOTS.with(|cell| {
        cell.borrow_mut().insert(slot.to_owned(), snap);
    });
}

pub fn list_memory_slots() -> Vec<String> {
    SLOTS.with(|cell| cell.borrow().keys().cloned().collect())
}

pub fn delete_slot_snapshot(slot: &str) -> bool {
    if slot == "default" {
        let existed = SNAPSHOT.with(|cell| cell.borrow().is_some());
        clear_snapshot();
        return existed;
    }
    SLOTS.with(|cell| cell.borrow_mut().remove(slot).is_some())
}

std::thread_local! {
    static SESSION_HIGH_ENTITY_ID: RefCell<u32> = const { RefCell::new(0) };
    static DIM_HIGH_ENTITY_ID: RefCell<HashMap<Dim, u32>> = RefCell::new(HashMap::new());
}

/// Records the upper bound of entity IDs issued or assigned to live players
/// during the current integrated server session (per dimension and globally).
pub fn observe_session_entity_ids(game: &Game<'_>) {
    let mut hi = game.next_entity;
    if let Some(host) = &game.lan {
        for g in host.hub.guests.as_slice() {
            if g.entity_id >= 0 {
                hi = hi.max(g.entity_id as u32);
            }
        }
    }
    SESSION_HIGH_ENTITY_ID.with(|cell| {
        let mut cur = cell.borrow_mut();
        if hi > *cur {
            *cur = hi;
        }
    });
    DIM_HIGH_ENTITY_ID.with(|cell| {
        let mut map = cell.borrow_mut();
        let entry = map.entry(game.dim).or_insert(0);
        if hi > *entry {
            *entry = hi;
        }
    });
}

pub fn session_entity_ceiling() -> u32 {
    SESSION_HIGH_ENTITY_ID.with(|cell| *cell.borrow())
}

pub fn other_dims_entity_ceiling(target_dim: Dim) -> u32 {
    DIM_HIGH_ENTITY_ID.with(|cell| {
        cell.borrow()
            .iter()
            .filter(|&(&dim, _)| dim != target_dim)
            .map(|(_, &hi)| hi)
            .max()
            .unwrap_or(0)
    })
}

pub fn bump_session_entity_ceiling(hi: u32, dim: Dim) {
    SESSION_HIGH_ENTITY_ID.with(|cell| {
        let mut cur = cell.borrow_mut();
        if hi > *cur {
            *cur = hi;
        }
    });
    DIM_HIGH_ENTITY_ID.with(|cell| {
        let mut map = cell.borrow_mut();
        let entry = map.entry(dim).or_insert(0);
        if hi > *entry {
            *entry = hi;
        }
    });
}

pub fn clear_session_entity_ids() {
    SESSION_HIGH_ENTITY_ID.with(|cell| *cell.borrow_mut() = 0);
    DIM_HIGH_ENTITY_ID.with(|cell| cell.borrow_mut().clear());
}