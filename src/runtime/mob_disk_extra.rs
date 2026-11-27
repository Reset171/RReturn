use super::super::Mob;
use super::snapshots::{DiskPath, DiskWho};
use horizon_client::mobs::villager_brain;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiskLookKind {
    Default,
    SmoothSwimming(u32),
    Fixed,
    HappyGhast,
    Shulker,
}

impl DiskLookKind {
    pub fn from_look(k: horizon_client::mobs::control::LookKind) -> Self {
        match k {
            horizon_client::mobs::control::LookKind::Default => Self::Default,
            horizon_client::mobs::control::LookKind::SmoothSwimming(m) => {
                Self::SmoothSwimming(m.to_bits())
            }
            horizon_client::mobs::control::LookKind::Fixed => Self::Fixed,
            horizon_client::mobs::control::LookKind::HappyGhast => Self::HappyGhast,
            horizon_client::mobs::control::LookKind::Shulker => Self::Shulker,
        }
    }

    pub fn to_look(self) -> horizon_client::mobs::control::LookKind {
        match self {
            Self::Default => horizon_client::mobs::control::LookKind::Default,
            Self::SmoothSwimming(b) => {
                horizon_client::mobs::control::LookKind::SmoothSwimming(f32::from_bits(b))
            }
            Self::Fixed => horizon_client::mobs::control::LookKind::Fixed,
            Self::HappyGhast => horizon_client::mobs::control::LookKind::HappyGhast,
            Self::Shulker => horizon_client::mobs::control::LookKind::Shulker,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiskMoveKind {
    Ground,
    Flying { max_turn_bits: u32, hovers: bool },
    SmoothSwimming {
        max_turn_x_bits: u32,
        max_turn_y_bits: u32,
        in_water_bits: u32,
        outside_bits: u32,
        gravity: bool,
    },
    Custom,
}

impl DiskMoveKind {
    pub fn from_mov(k: horizon_client::mobs::control::MoveKind) -> Self {
        match k {
            horizon_client::mobs::control::MoveKind::Ground => Self::Ground,
            horizon_client::mobs::control::MoveKind::Flying { max_turn, hovers } => Self::Flying {
                max_turn_bits: max_turn.to_bits(),
                hovers,
            },
            horizon_client::mobs::control::MoveKind::SmoothSwimming {
                max_turn_x,
                max_turn_y,
                in_water,
                outside,
                gravity,
            } => Self::SmoothSwimming {
                max_turn_x_bits: max_turn_x.to_bits(),
                max_turn_y_bits: max_turn_y.to_bits(),
                in_water_bits: in_water.to_bits(),
                outside_bits: outside.to_bits(),
                gravity,
            },
            horizon_client::mobs::control::MoveKind::Custom => Self::Custom,
        }
    }

    pub fn to_mov(self) -> horizon_client::mobs::control::MoveKind {
        match self {
            Self::Ground => horizon_client::mobs::control::MoveKind::Ground,
            Self::Flying {
                max_turn_bits,
                hovers,
            } => horizon_client::mobs::control::MoveKind::Flying {
                max_turn: f32::from_bits(max_turn_bits),
                hovers,
            },
            Self::SmoothSwimming {
                max_turn_x_bits,
                max_turn_y_bits,
                in_water_bits,
                outside_bits,
                gravity,
            } => horizon_client::mobs::control::MoveKind::SmoothSwimming {
                max_turn_x: f32::from_bits(max_turn_x_bits),
                max_turn_y: f32::from_bits(max_turn_y_bits),
                in_water: f32::from_bits(in_water_bits),
                outside: f32::from_bits(outside_bits),
                gravity,
            },
            Self::Custom => horizon_client::mobs::control::MoveKind::Custom,
        }
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskMobPoseExtra {
    pub pose: u8,
    pub synced_pose: u8,
    pub pose_change_tick: i64,
    pub pose_epoch: Option<i64>,
    pub camel_sitting: bool,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskSchoolExtra {
    pub leader: Option<u32>,
    pub lone: bool,
    pub followers: i32,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskFrogExtra {
    pub tongue_target: Option<u32>,
    pub discard_friction: bool,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskPandaExtra {
    pub flags: u8,
    pub unhappy: i32,
    pub sneeze: i32,
    pub eat: i32,
    pub roll_counter: i32,
    pub food_near: bool,
    pub food_at_bits: Option<[u64; 3]>,
    pub held_count: u32,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskIllusionsExtra {
    pub ticks: i32,
    pub offsets_bits: [[[u64; 3]; 4]; 2],
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskAllayExtra {
    pub pickup_cooldown: Option<i32>,
    pub noteblock: Option<(String, [i32; 3])>,
    pub liked_player_ttl: Option<i64>,
    pub jukebox: Option<[i32; 3]>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskVillagerExtra {
    pub raid: u8,
    pub raid_wave: bool,
    pub last_traded_player: Option<[i32; 4]>,
    pub last_gossip_time: i64,
    pub last_gossip_decay: i64,
    pub food_level: i32,
    pub last_slept: Option<i64>,
    pub sleeping_pos: Option<[i32; 3]>,
    pub golem_until: i64,
    pub hurt_seen: i32,
    pub data_finalized: bool,
    pub lobotomized: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskEquineRider {
    pub strafe_bits: u32,
    pub forward_bits: u32,
    pub yaw_bits: u32,
    pub pitch_bits: u32,
    pub jump_bits: u32,
    pub steer: bool,
    pub sprint: bool,
    pub jumping: bool,
    pub sneak: bool,
    pub client: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiskBrainSlot<T> {
    pub v: T,
    pub ttl: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiskVbTracker {
    Entity(DiskWho, bool),
    Block([i32; 3]),
    Point([u64; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiskVbWalkTarget {
    pub target: DiskVbTracker,
    pub speed_bits: u32,
    pub close: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiskAbTracker {
    Entity {
        who: DiskWho,
        eye: bool,
        block_eye: bool,
    },
    Block([i32; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiskAbWalkTarget {
    pub target: DiskAbTracker,
    pub speed_bits: u32,
    pub close: i32,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskMonsterBrain {
    pub activity: u8,
    pub attack_target: Option<DiskBrainSlot<DiskWho>>,
    pub attack_cooling_down: Option<DiskBrainSlot<bool>>,
    pub cant_reach_walk_target_since: Option<DiskBrainSlot<i64>>,
    pub hurt_by: Option<DiskBrainSlot<i64>>,
    pub hurt_by_entity: Option<DiskBrainSlot<DiskWho>>,
    pub look_target: Option<DiskBrainSlot<DiskVbTracker>>,
    pub mobs: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub nearest_attackable: Option<DiskBrainSlot<DiskWho>>,
    pub nearest_players: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub nearest_visible_player: Option<DiskBrainSlot<DiskWho>>,
    pub nearest_visible_targetable_player: Option<DiskBrainSlot<DiskWho>>,
    pub nearest_visible_targetable_players: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub path: Option<DiskBrainSlot<DiskPath>>,
    pub walk_target: Option<DiskBrainSlot<DiskVbWalkTarget>>,
    pub breeze_shoot: Option<i64>,
    pub breeze_shoot_charging: Option<i64>,
    pub breeze_shoot_recovering: Option<i64>,
    pub breeze_shoot_cooldown: Option<i64>,
    pub breeze_jump_cooldown: Option<i64>,
    pub breeze_jump_inhaling: Option<i64>,
    pub breeze_jump_target: Option<DiskBrainSlot<[i32; 3]>>,
    pub breeze_leaving_water: Option<i64>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskAnimalBrain {
    pub activity: u8,
    pub mobs: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub nearest_players: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub nearest_visible_player: Option<DiskBrainSlot<DiskWho>>,
    pub nearest_visible_targetable_player: Option<DiskBrainSlot<DiskWho>>,
    pub nearest_visible_targetable_players: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub nearest_visible_adult: Option<DiskBrainSlot<DiskWho>>,
    pub walk_target: Option<DiskBrainSlot<DiskAbWalkTarget>>,
    pub look_target: Option<DiskBrainSlot<DiskAbTracker>>,
    pub breed_target: Option<DiskBrainSlot<DiskWho>>,
    pub path: Option<DiskBrainSlot<DiskPath>>,
    pub hurt_by: Option<DiskBrainSlot<bool>>,
    pub hurt_by_entity: Option<DiskBrainSlot<DiskWho>>,
    pub cant_reach_walk_target_since: Option<DiskBrainSlot<i64>>,
    pub tempting_player: Option<DiskBrainSlot<DiskWho>>,
    pub temptation_cooldown_ticks: Option<DiskBrainSlot<i32>>,
    pub is_panicking: Option<DiskBrainSlot<bool>>,
    pub sniffer_digging: Option<DiskBrainSlot<bool>>,
    pub sniffer_sniffing_target: Option<DiskBrainSlot<[i32; 3]>>,
    pub sniffer_happy: Option<DiskBrainSlot<bool>>,
    pub sniff_cooldown: Option<i64>,
    pub gaze_cooldown_ticks: Option<DiskBrainSlot<i32>>,
    pub transport_items_cooldown_ticks: Option<DiskBrainSlot<i32>>,
    pub nearest_attackable: Option<DiskBrainSlot<DiskWho>>,
    pub attack_target: Option<DiskBrainSlot<DiskWho>>,
    pub attack_cooling_down: Option<DiskBrainSlot<bool>>,
    pub hunting_cooldown: Option<DiskBrainSlot<bool>>,
    pub play_dead_ticks: Option<DiskBrainSlot<i32>>,
    pub visited_block_positions: Option<DiskBrainSlot<Vec<[i32; 3]>>>,
    pub unreachable_transport_block_positions: Option<DiskBrainSlot<Vec<[i32; 3]>>>,
    pub doors_to_close: Option<DiskBrainSlot<Vec<[i32; 3]>>>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskVillagerBrain {
    pub shuffle_seed: i64,
    pub activity: u8,
    pub home: Option<DiskBrainSlot<[i32; 3]>>,
    pub job_site: Option<DiskBrainSlot<[i32; 3]>>,
    pub potential_job_site: Option<DiskBrainSlot<[i32; 3]>>,
    pub meeting_point: Option<DiskBrainSlot<[i32; 3]>>,
    pub secondary_job_site: Option<DiskBrainSlot<Vec<[i32; 3]>>>,
    pub mobs: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub visible_villager_babies: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub nearest_players: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub nearest_visible_player: Option<DiskBrainSlot<DiskWho>>,
    pub nearest_visible_targetable_player: Option<DiskBrainSlot<DiskWho>>,
    pub nearest_visible_targetable_players: Option<DiskBrainSlot<Vec<DiskWho>>>,
    pub walk_target: Option<DiskBrainSlot<DiskVbWalkTarget>>,
    pub look_target: Option<DiskBrainSlot<DiskVbTracker>>,
    pub interaction_target: Option<DiskBrainSlot<DiskWho>>,
    pub breed_target: Option<DiskBrainSlot<DiskWho>>,
    pub path: Option<DiskBrainSlot<DiskPath>>,
    pub doors_to_close: Option<DiskBrainSlot<Vec<[i32; 3]>>>,
    pub nearest_bed: Option<DiskBrainSlot<[i32; 3]>>,
    pub hurt_by: Option<DiskBrainSlot<i64>>,
    pub hurt_by_entity: Option<DiskBrainSlot<DiskWho>>,
    pub nearest_hostile: Option<DiskBrainSlot<DiskWho>>,
    pub hiding_place: Option<DiskBrainSlot<[i32; 3]>>,
    pub heard_bell_time: Option<DiskBrainSlot<i64>>,
    pub nearest_wanted_item: Option<DiskBrainSlot<[u64; 3]>>,
    pub cant_reach_walk_target_since: Option<DiskBrainSlot<i64>>,
    pub golem_detected_recently: Option<DiskBrainSlot<bool>>,
    pub last_slept: Option<DiskBrainSlot<i64>>,
    pub last_woken: Option<DiskBrainSlot<i64>>,
    pub last_worked_at_poi: Option<DiskBrainSlot<i64>>,
}

impl DiskVillagerBrain {
    fn from_brain(b: &horizon_client::mobs::villager_brain::Brain) -> Self {
        let m = &b.mem;
        Self {
            shuffle_seed: b.shuffle_seed,
            activity: match b.activity {
                villager_brain::Act::Core => 0,
                villager_brain::Act::Idle => 1,
                villager_brain::Act::Work => 2,
                villager_brain::Act::Play => 3,
                villager_brain::Act::Rest => 4,
                villager_brain::Act::Meet => 5,
                villager_brain::Act::Panic => 6,
                villager_brain::Act::Raid => 7,
                villager_brain::Act::PreRaid => 8,
                villager_brain::Act::Hide => 9,
            },
            home: vb_from(&m.home, |v| *v),
            job_site: vb_from(&m.job_site, |v| *v),
            potential_job_site: vb_from(&m.potential_job_site, |v| *v),
            meeting_point: vb_from(&m.meeting_point, |v| *v),
            secondary_job_site: vb_from(&m.secondary_job_site, Clone::clone),
            mobs: vb_from(&m.mobs, |v| v.iter().copied().map(DiskWho::from_who).collect()),
            visible_villager_babies: vb_from(&m.visible_villager_babies, |v| {
                v.iter().copied().map(DiskWho::from_who).collect()
            }),
            nearest_players: vb_from(&m.nearest_players, |v| {
                v.iter().copied().map(DiskWho::from_who).collect()
            }),
            nearest_visible_player: vb_from(&m.nearest_visible_player, |w| DiskWho::from_who(*w)),
            nearest_visible_targetable_player: vb_from(
                &m.nearest_visible_targetable_player,
                |w| DiskWho::from_who(*w),
            ),
            nearest_visible_targetable_players: vb_from(
                &m.nearest_visible_targetable_players,
                |v| v.iter().copied().map(DiskWho::from_who).collect(),
            ),
            walk_target: vb_from(&m.walk_target, |wt| DiskVbWalkTarget {
                target: vb_tracker_from(wt.target),
                speed_bits: wt.speed.to_bits(),
                close: wt.close,
            }),
            look_target: vb_from(&m.look_target, |t| vb_tracker_from(*t)),
            interaction_target: vb_from(&m.interaction_target, |w| DiskWho::from_who(*w)),
            breed_target: vb_from(&m.breed_target, |w| DiskWho::from_who(*w)),
            path: vb_from(&m.path, DiskPath::from_path),
            doors_to_close: vb_from(&m.doors_to_close, Clone::clone),
            nearest_bed: vb_from(&m.nearest_bed, |v| *v),
            hurt_by: vb_from(&m.hurt_by, |v| *v),
            hurt_by_entity: vb_from(&m.hurt_by_entity, |w| DiskWho::from_who(*w)),
            nearest_hostile: vb_from(&m.nearest_hostile, |w| DiskWho::from_who(*w)),
            hiding_place: vb_from(&m.hiding_place, |v| *v),
            heard_bell_time: vb_from(&m.heard_bell_time, |v| *v),
            nearest_wanted_item: vb_from(&m.nearest_wanted_item, |a| a.map(f64::to_bits)),
            cant_reach_walk_target_since: vb_from(&m.cant_reach_walk_target_since, |v| *v),
            golem_detected_recently: vb_from(&m.golem_detected_recently, |v| *v),
            last_slept: vb_from(&m.last_slept, |v| *v),
            last_woken: vb_from(&m.last_woken, |v| *v),
            last_worked_at_poi: vb_from(&m.last_worked_at_poi, |v| *v),
        }
    }

    fn apply_scalar(&self, b: &mut horizon_client::mobs::villager_brain::Brain) {
        b.activity = match self.activity {
            0 => villager_brain::Act::Core,
            2 => villager_brain::Act::Work,
            3 => villager_brain::Act::Play,
            4 => villager_brain::Act::Rest,
            5 => villager_brain::Act::Meet,
            6 => villager_brain::Act::Panic,
            7 => villager_brain::Act::Raid,
            8 => villager_brain::Act::PreRaid,
            9 => villager_brain::Act::Hide,
            _ => villager_brain::Act::Idle,
        };
        let m = &mut b.mem;
        vb_apply(&mut m.home, self.home.as_ref(), |v| Some(*v));
        vb_apply(&mut m.job_site, self.job_site.as_ref(), |v| Some(*v));
        vb_apply(&mut m.potential_job_site, self.potential_job_site.as_ref(), |v| Some(*v));
        vb_apply(&mut m.meeting_point, self.meeting_point.as_ref(), |v| Some(*v));
        vb_apply(&mut m.secondary_job_site, self.secondary_job_site.as_ref(), |v| {
            Some(v.clone())
        });
        vb_apply(&mut m.path, self.path.as_ref(), |p| Some(p.to_path()));
        vb_apply(&mut m.doors_to_close, self.doors_to_close.as_ref(), |v| {
            Some(v.clone())
        });
        vb_apply(&mut m.nearest_bed, self.nearest_bed.as_ref(), |v| Some(*v));
        vb_apply(&mut m.hurt_by, self.hurt_by.as_ref(), |v| Some(*v));
        vb_apply(&mut m.hiding_place, self.hiding_place.as_ref(), |v| Some(*v));
        vb_apply(&mut m.heard_bell_time, self.heard_bell_time.as_ref(), |v| Some(*v));
        vb_apply(&mut m.nearest_wanted_item, self.nearest_wanted_item.as_ref(), |a| {
            Some(a.map(f64::from_bits))
        });
        vb_apply(
            &mut m.cant_reach_walk_target_since,
            self.cant_reach_walk_target_since.as_ref(),
            |v| Some(*v),
        );
        vb_apply(
            &mut m.golem_detected_recently,
            self.golem_detected_recently.as_ref(),
            |v| Some(*v),
        );
        vb_apply(&mut m.last_slept, self.last_slept.as_ref(), |v| Some(*v));
        vb_apply(&mut m.last_woken, self.last_woken.as_ref(), |v| Some(*v));
        vb_apply(&mut m.last_worked_at_poi, self.last_worked_at_poi.as_ref(), |v| Some(*v));
    }

    fn apply_ids(
        &self,
        b: &mut horizon_client::mobs::villager_brain::Brain,
        map_id: &dyn Fn(u32) -> Option<u32>,
    ) {
        let m = &mut b.mem;
        vb_apply(&mut m.mobs, self.mobs.as_ref(), |l| {
            Some(l.iter().filter_map(|w| w.to_who(map_id)).collect())
        });
        vb_apply(
            &mut m.visible_villager_babies,
            self.visible_villager_babies.as_ref(),
            |l| Some(l.iter().filter_map(|w| w.to_who(map_id)).collect()),
        );
        vb_apply(&mut m.nearest_players, self.nearest_players.as_ref(), |l| {
            Some(l.iter().filter_map(|w| w.to_who(map_id)).collect())
        });
        vb_apply(
            &mut m.nearest_visible_player,
            self.nearest_visible_player.as_ref(),
            |w| w.to_who(map_id),
        );
        vb_apply(
            &mut m.nearest_visible_targetable_player,
            self.nearest_visible_targetable_player.as_ref(),
            |w| w.to_who(map_id),
        );
        vb_apply(
            &mut m.nearest_visible_targetable_players,
            self.nearest_visible_targetable_players.as_ref(),
            |l| Some(l.iter().filter_map(|w| w.to_who(map_id)).collect()),
        );
        vb_apply(&mut m.walk_target, self.walk_target.as_ref(), |wt| {
            vb_tracker_to(&wt.target, map_id).map(|target| {
                horizon_client::mobs::villager_brain::WalkTarget {
                    target,
                    speed: f32::from_bits(wt.speed_bits),
                    close: wt.close,
                }
            })
        });
        vb_apply(&mut m.look_target, self.look_target.as_ref(), |t| {
            vb_tracker_to(t, map_id)
        });
        vb_apply(
            &mut m.interaction_target,
            self.interaction_target.as_ref(),
            |w| w.to_who(map_id),
        );
        vb_apply(&mut m.breed_target, self.breed_target.as_ref(), |w| w.to_who(map_id));
        vb_apply(&mut m.hurt_by_entity, self.hurt_by_entity.as_ref(), |w| w.to_who(map_id));
        vb_apply(&mut m.nearest_hostile, self.nearest_hostile.as_ref(), |w| w.to_who(map_id));
    }
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DiskMobExtended {
    pub positions: Vec<[i32; 3]>,
    pub extra_prev_vel_bits: [u64; 3],
    pub pose_extra: Option<DiskMobPoseExtra>,
    pub school: Option<DiskSchoolExtra>,
    pub point_bits: Option<[u64; 3]>,
    pub repellent: Option<[i32; 3]>,
    pub opened_chest: Option<[i32; 3]>,
    pub scratch: [i32; 4],
    pub last_stare_sound: Option<u32>,
    pub relax_one: bool,
    pub on_sleeper: bool,
    pub trap: bool,
    pub trap_time: i32,
    pub stand_sliding: bool,
    pub nautilus_attack_cooldown_ttl: Option<i64>,
    pub static_env_stamp: Option<(u64, [u64; 3], [u32; 2])>,
    pub max_swell: i32,
    pub fire_ticks: i32,
    pub ticks_frozen: i32,
    pub air: i32,
    pub persistent: bool,
    pub standing_on: Option<String>,
    pub gear_knockback_resistance_bits: Option<u64>,
    pub boarding_cooldown: i32,
    pub vehicle_order: i64,
    pub mount: Option<u32>,
    pub rider: Option<u32>,
    pub seat: Option<(u32, u64, [u64; 6])>,
    pub vehicle_body: Option<(u32, u32, u32)>,
    pub last_home_pos: Option<[i32; 3]>,
    pub memories: Vec<(String, i64, i64)>,
    pub armadillo: Option<(u8, i32)>,
    pub frog: Option<DiskFrogExtra>,
    pub goat_discard_friction: bool,
    pub panda: Option<DiskPandaExtra>,
    pub illusions: Option<DiskIllusionsExtra>,
    pub allay: Option<DiskAllayExtra>,
    pub warden_client_anger: Option<i32>,
    pub villager: Option<DiskVillagerExtra>,
    pub rider_input: Option<DiskEquineRider>,
    pub monster_brain: Option<DiskMonsterBrain>,
    pub animal_brain: Option<DiskAnimalBrain>,
    #[serde(default)]
    pub villager_brain: Option<DiskVillagerBrain>,
}

fn intern_extra_memory_key(key: &str) -> Option<&'static str> {
    match key {
        "long_jump_cooling_down" => Some("long_jump_cooling_down"),
        "ram_cooldown_ticks" => Some("ram_cooldown_ticks"),
        "is_pregnant" => Some("is_pregnant"),
        "hunted_recently" => Some("hunted_recently"),
        "attack_target_cooldown" => Some("attack_target_cooldown"),
        "home" => Some("home"),
        "dig_cooldown" => Some("dig_cooldown"),
        "is_emerging" => Some("is_emerging"),
        "minecraft:long_jump_cooling_down" => Some("minecraft:long_jump_cooling_down"),
        "minecraft:ram_cooldown_ticks" => Some("minecraft:ram_cooldown_ticks"),
        "minecraft:is_pregnant" => Some("minecraft:is_pregnant"),
        "minecraft:attack_target_cooldown" => Some("minecraft:attack_target_cooldown"),
        "minecraft:has_hunting_cooldown" => Some("minecraft:has_hunting_cooldown"),
        "minecraft:last_slept" => Some("minecraft:last_slept"),
        "minecraft:last_woken" => Some("minecraft:last_woken"),
        _ => None,
    }
}

fn vb_from<T, U>(
    slot: &horizon_client::mobs::villager_brain::Slot<T>,
    f: impl FnOnce(&T) -> U,
) -> Option<DiskBrainSlot<U>> {
    slot.get().map(|v| DiskBrainSlot {
        v: f(v),
        ttl: slot.ttl(),
    })
}

fn vb_apply<T, U>(
    slot: &mut horizon_client::mobs::villager_brain::Slot<T>,
    src: Option<&DiskBrainSlot<U>>,
    f: impl FnOnce(&U) -> Option<T>,
) {
    match src.and_then(|d| f(&d.v).map(|v| (v, d.ttl))) {
        Some((v, ttl)) => slot.set_ttl(v, ttl),
        None => slot.clear(),
    }
}

fn vb_unit_from(slot: &horizon_client::mobs::villager_brain::Slot<()>) -> Option<i64> {
    slot.present().then(|| slot.ttl())
}

fn vb_unit_apply(
    slot: &mut horizon_client::mobs::villager_brain::Slot<()>,
    src: Option<i64>,
) {
    match src {
        Some(ttl) => slot.set_ttl((), ttl),
        None => slot.clear(),
    }
}

fn ab_from<T, U>(
    slot: &horizon_client::mobs::animal_brain::Slot<T>,
    f: impl FnOnce(&T) -> U,
) -> Option<DiskBrainSlot<U>> {
    slot.get().map(|v| DiskBrainSlot {
        v: f(v),
        ttl: slot.ttl(),
    })
}

fn ab_apply<T, U>(
    slot: &mut horizon_client::mobs::animal_brain::Slot<T>,
    src: Option<&DiskBrainSlot<U>>,
    f: impl FnOnce(&U) -> Option<T>,
) {
    match src.and_then(|d| f(&d.v).map(|v| (v, d.ttl))) {
        Some((v, ttl)) => slot.set_ttl(v, ttl),
        None => slot.clear(),
    }
}

fn vb_tracker_from(t: horizon_client::mobs::villager_brain::Tracker) -> DiskVbTracker {
    match t {
        horizon_client::mobs::villager_brain::Tracker::Entity(w, eye) => {
            DiskVbTracker::Entity(DiskWho::from_who(w), eye)
        }
        horizon_client::mobs::villager_brain::Tracker::Block(p) => DiskVbTracker::Block(p),
        horizon_client::mobs::villager_brain::Tracker::Point(v) => {
            DiskVbTracker::Point(v.map(f64::to_bits))
        }
    }
}

fn vb_tracker_to(
    t: &DiskVbTracker,
    map_id: &dyn Fn(u32) -> Option<u32>,
) -> Option<horizon_client::mobs::villager_brain::Tracker> {
    match *t {
        DiskVbTracker::Entity(w, eye) => w
            .to_who(map_id)
            .map(|mw| horizon_client::mobs::villager_brain::Tracker::Entity(mw, eye)),
        DiskVbTracker::Block(p) => Some(horizon_client::mobs::villager_brain::Tracker::Block(p)),
        DiskVbTracker::Point(v) => Some(horizon_client::mobs::villager_brain::Tracker::Point(
            v.map(f64::from_bits),
        )),
    }
}

fn ab_tracker_from(t: horizon_client::mobs::animal_brain::Tracker) -> DiskAbTracker {
    match t {
        horizon_client::mobs::animal_brain::Tracker::Entity {
            who,
            eye,
            block_eye,
        } => DiskAbTracker::Entity {
            who: DiskWho::from_who(who),
            eye,
            block_eye,
        },
        horizon_client::mobs::animal_brain::Tracker::Block(p) => DiskAbTracker::Block(p),
    }
}

fn ab_tracker_to(
    t: &DiskAbTracker,
    map_id: &dyn Fn(u32) -> Option<u32>,
) -> Option<horizon_client::mobs::animal_brain::Tracker> {
    match *t {
        DiskAbTracker::Entity {
            who,
            eye,
            block_eye,
        } => who
            .to_who(map_id)
            .map(|mw| horizon_client::mobs::animal_brain::Tracker::Entity {
                who: mw,
                eye,
                block_eye,
            }),
        DiskAbTracker::Block(p) => Some(horizon_client::mobs::animal_brain::Tracker::Block(p)),
    }
}

impl DiskMonsterBrain {
    fn from_brain(b: &horizon_client::mobs::monster_brain::Brain) -> Self {
        let m = &b.mem;
        Self {
            activity: match b.activity {
                horizon_client::mobs::monster_brain::Act::Core => 0,
                horizon_client::mobs::monster_brain::Act::Idle => 1,
                horizon_client::mobs::monster_brain::Act::Fight => 2,
            },
            attack_target: vb_from(&m.attack_target, |w| DiskWho::from_who(*w)),
            attack_cooling_down: vb_from(&m.attack_cooling_down, |v| *v),
            cant_reach_walk_target_since: vb_from(&m.cant_reach_walk_target_since, |v| *v),
            hurt_by: vb_from(&m.hurt_by, |v| *v),
            hurt_by_entity: vb_from(&m.hurt_by_entity, |w| DiskWho::from_who(*w)),
            look_target: vb_from(&m.look_target, |t| vb_tracker_from(*t)),
            mobs: vb_from(&m.mobs, |list| {
                list.iter().copied().map(DiskWho::from_who).collect()
            }),
            nearest_attackable: vb_from(&m.nearest_attackable, |w| DiskWho::from_who(*w)),
            nearest_players: vb_from(&m.nearest_players, |list| {
                list.iter().copied().map(DiskWho::from_who).collect()
            }),
            nearest_visible_player: vb_from(&m.nearest_visible_player, |w| DiskWho::from_who(*w)),
            nearest_visible_targetable_player: vb_from(
                &m.nearest_visible_targetable_player,
                |w| DiskWho::from_who(*w),
            ),
            nearest_visible_targetable_players: vb_from(
                &m.nearest_visible_targetable_players,
                |list| list.iter().copied().map(DiskWho::from_who).collect(),
            ),
            path: vb_from(&m.path, DiskPath::from_path),
            walk_target: vb_from(&m.walk_target, |wt| DiskVbWalkTarget {
                target: vb_tracker_from(wt.target),
                speed_bits: wt.speed.to_bits(),
                close: wt.close,
            }),
            breeze_shoot: vb_unit_from(&m.breeze_shoot),
            breeze_shoot_charging: vb_unit_from(&m.breeze_shoot_charging),
            breeze_shoot_recovering: vb_unit_from(&m.breeze_shoot_recovering),
            breeze_shoot_cooldown: vb_unit_from(&m.breeze_shoot_cooldown),
            breeze_jump_cooldown: vb_unit_from(&m.breeze_jump_cooldown),
            breeze_jump_inhaling: vb_unit_from(&m.breeze_jump_inhaling),
            breeze_jump_target: vb_from(&m.breeze_jump_target, |v| *v),
            breeze_leaving_water: vb_unit_from(&m.breeze_leaving_water),
        }
    }

    fn apply_scalar(&self, b: &mut horizon_client::mobs::monster_brain::Brain) {
        b.activity = match self.activity {
            0 => horizon_client::mobs::monster_brain::Act::Core,
            2 => horizon_client::mobs::monster_brain::Act::Fight,
            _ => horizon_client::mobs::monster_brain::Act::Idle,
        };
        let m = &mut b.mem;
        vb_apply(&mut m.attack_cooling_down, self.attack_cooling_down.as_ref(), |v| Some(*v));
        vb_apply(
            &mut m.cant_reach_walk_target_since,
            self.cant_reach_walk_target_since.as_ref(),
            |v| Some(*v),
        );
        vb_apply(&mut m.hurt_by, self.hurt_by.as_ref(), |v| Some(*v));
        vb_apply(&mut m.path, self.path.as_ref(), |p| Some(p.to_path()));
        vb_unit_apply(&mut m.breeze_shoot, self.breeze_shoot);
        vb_unit_apply(&mut m.breeze_shoot_charging, self.breeze_shoot_charging);
        vb_unit_apply(&mut m.breeze_shoot_recovering, self.breeze_shoot_recovering);
        vb_unit_apply(&mut m.breeze_shoot_cooldown, self.breeze_shoot_cooldown);
        vb_unit_apply(&mut m.breeze_jump_cooldown, self.breeze_jump_cooldown);
        vb_unit_apply(&mut m.breeze_jump_inhaling, self.breeze_jump_inhaling);
        vb_apply(&mut m.breeze_jump_target, self.breeze_jump_target.as_ref(), |v| Some(*v));
        vb_unit_apply(&mut m.breeze_leaving_water, self.breeze_leaving_water);
    }

    fn apply_ids(
        &self,
        b: &mut horizon_client::mobs::monster_brain::Brain,
        map_id: &dyn Fn(u32) -> Option<u32>,
    ) {
        let m = &mut b.mem;
        vb_apply(&mut m.attack_target, self.attack_target.as_ref(), |w| w.to_who(map_id));
        vb_apply(&mut m.hurt_by_entity, self.hurt_by_entity.as_ref(), |w| w.to_who(map_id));
        vb_apply(&mut m.look_target, self.look_target.as_ref(), |t| vb_tracker_to(t, map_id));
        vb_apply(&mut m.mobs, self.mobs.as_ref(), |l| {
            Some(l.iter().filter_map(|w| w.to_who(map_id)).collect())
        });
        vb_apply(&mut m.nearest_attackable, self.nearest_attackable.as_ref(), |w| {
            w.to_who(map_id)
        });
        vb_apply(&mut m.nearest_players, self.nearest_players.as_ref(), |l| {
            Some(l.iter().filter_map(|w| w.to_who(map_id)).collect())
        });
        vb_apply(
            &mut m.nearest_visible_player,
            self.nearest_visible_player.as_ref(),
            |w| w.to_who(map_id),
        );
        vb_apply(
            &mut m.nearest_visible_targetable_player,
            self.nearest_visible_targetable_player.as_ref(),
            |w| w.to_who(map_id),
        );
        vb_apply(
            &mut m.nearest_visible_targetable_players,
            self.nearest_visible_targetable_players.as_ref(),
            |l| Some(l.iter().filter_map(|w| w.to_who(map_id)).collect()),
        );
        vb_apply(&mut m.walk_target, self.walk_target.as_ref(), |wt| {
            vb_tracker_to(&wt.target, map_id).map(|target| {
                horizon_client::mobs::villager_brain::WalkTarget {
                    target,
                    speed: f32::from_bits(wt.speed_bits),
                    close: wt.close,
                }
            })
        });
    }
}

impl DiskAnimalBrain {
    fn from_brain(b: &horizon_client::mobs::animal_brain::Brain) -> Self {
        let m = &b.mem;
        Self {
            activity: match b.activity {
                horizon_client::mobs::animal_brain::Act::Core => 0,
                horizon_client::mobs::animal_brain::Act::Fight => 1,
                horizon_client::mobs::animal_brain::Act::PlayDead => 2,
                horizon_client::mobs::animal_brain::Act::Idle => 3,
                horizon_client::mobs::animal_brain::Act::Sniff => 4,
                horizon_client::mobs::animal_brain::Act::Dig => 5,
                horizon_client::mobs::animal_brain::Act::Panic => 6,
            },
            mobs: ab_from(&m.mobs, |l| l.iter().copied().map(DiskWho::from_who).collect()),
            nearest_players: ab_from(&m.nearest_players, |l| {
                l.iter().copied().map(DiskWho::from_who).collect()
            }),
            nearest_visible_player: ab_from(&m.nearest_visible_player, |w| DiskWho::from_who(*w)),
            nearest_visible_targetable_player: ab_from(
                &m.nearest_visible_targetable_player,
                |w| DiskWho::from_who(*w),
            ),
            nearest_visible_targetable_players: ab_from(
                &m.nearest_visible_targetable_players,
                |l| l.iter().copied().map(DiskWho::from_who).collect(),
            ),
            nearest_visible_adult: ab_from(&m.nearest_visible_adult, |w| DiskWho::from_who(*w)),
            walk_target: ab_from(&m.walk_target, |wt| DiskAbWalkTarget {
                target: ab_tracker_from(wt.target),
                speed_bits: wt.speed.to_bits(),
                close: wt.close,
            }),
            look_target: ab_from(&m.look_target, |t| ab_tracker_from(*t)),
            breed_target: ab_from(&m.breed_target, |w| DiskWho::from_who(*w)),
            path: ab_from(&m.path, DiskPath::from_path),
            hurt_by: ab_from(&m.hurt_by, |v| *v),
            hurt_by_entity: ab_from(&m.hurt_by_entity, |w| DiskWho::from_who(*w)),
            cant_reach_walk_target_since: ab_from(&m.cant_reach_walk_target_since, |v| *v),
            tempting_player: ab_from(&m.tempting_player, |w| DiskWho::from_who(*w)),
            temptation_cooldown_ticks: ab_from(&m.temptation_cooldown_ticks, |v| *v),
            is_panicking: ab_from(&m.is_panicking, |v| *v),
            sniffer_digging: ab_from(&m.sniffer_digging, |v| *v),
            sniffer_sniffing_target: ab_from(&m.sniffer_sniffing_target, |v| *v),
            sniffer_happy: ab_from(&m.sniffer_happy, |v| *v),
            sniff_cooldown: m.sniff_cooldown.present().then(|| m.sniff_cooldown.ttl()),
            gaze_cooldown_ticks: ab_from(&m.gaze_cooldown_ticks, |v| *v),
            transport_items_cooldown_ticks: ab_from(&m.transport_items_cooldown_ticks, |v| *v),
            nearest_attackable: ab_from(&m.nearest_attackable, |w| DiskWho::from_who(*w)),
            attack_target: ab_from(&m.attack_target, |w| DiskWho::from_who(*w)),
            attack_cooling_down: ab_from(&m.attack_cooling_down, |v| *v),
            hunting_cooldown: ab_from(&m.hunting_cooldown, |v| *v),
            play_dead_ticks: ab_from(&m.play_dead_ticks, |v| *v),
            visited_block_positions: ab_from(&m.visited_block_positions, Clone::clone),
            unreachable_transport_block_positions: ab_from(
                &m.unreachable_transport_block_positions,
                Clone::clone,
            ),
            doors_to_close: ab_from(&m.doors_to_close, Clone::clone),
        }
    }

    fn apply_scalar(&self, b: &mut horizon_client::mobs::animal_brain::Brain) {
        b.activity = match self.activity {
            0 => horizon_client::mobs::animal_brain::Act::Core,
            1 => horizon_client::mobs::animal_brain::Act::Fight,
            2 => horizon_client::mobs::animal_brain::Act::PlayDead,
            4 => horizon_client::mobs::animal_brain::Act::Sniff,
            5 => horizon_client::mobs::animal_brain::Act::Dig,
            6 => horizon_client::mobs::animal_brain::Act::Panic,
            _ => horizon_client::mobs::animal_brain::Act::Idle,
        };
        let m = &mut b.mem;
        ab_apply(&mut m.path, self.path.as_ref(), |p| Some(p.to_path()));
        ab_apply(&mut m.hurt_by, self.hurt_by.as_ref(), |v| Some(*v));
        ab_apply(
            &mut m.cant_reach_walk_target_since,
            self.cant_reach_walk_target_since.as_ref(),
            |v| Some(*v),
        );
        ab_apply(
            &mut m.temptation_cooldown_ticks,
            self.temptation_cooldown_ticks.as_ref(),
            |v| Some(*v),
        );
        ab_apply(&mut m.is_panicking, self.is_panicking.as_ref(), |v| Some(*v));
        ab_apply(&mut m.sniffer_digging, self.sniffer_digging.as_ref(), |v| Some(*v));
        ab_apply(
            &mut m.sniffer_sniffing_target,
            self.sniffer_sniffing_target.as_ref(),
            |v| Some(*v),
        );
        ab_apply(&mut m.sniffer_happy, self.sniffer_happy.as_ref(), |v| Some(*v));
        match self.sniff_cooldown {
            Some(ttl) => m.sniff_cooldown.set_ttl((), ttl),
            None => m.sniff_cooldown.clear(),
        }
        ab_apply(&mut m.gaze_cooldown_ticks, self.gaze_cooldown_ticks.as_ref(), |v| Some(*v));
        ab_apply(
            &mut m.transport_items_cooldown_ticks,
            self.transport_items_cooldown_ticks.as_ref(),
            |v| Some(*v),
        );
        ab_apply(
            &mut m.attack_cooling_down,
            self.attack_cooling_down.as_ref(),
            |v| Some(*v),
        );
        ab_apply(&mut m.hunting_cooldown, self.hunting_cooldown.as_ref(), |v| Some(*v));
        ab_apply(&mut m.play_dead_ticks, self.play_dead_ticks.as_ref(), |v| Some(*v));
        ab_apply(
            &mut m.visited_block_positions,
            self.visited_block_positions.as_ref(),
            |v| Some(v.clone()),
        );
        ab_apply(
            &mut m.unreachable_transport_block_positions,
            self.unreachable_transport_block_positions.as_ref(),
            |v| Some(v.clone()),
        );
        ab_apply(&mut m.doors_to_close, self.doors_to_close.as_ref(), |v| Some(v.clone()));
    }

    fn apply_ids(
        &self,
        b: &mut horizon_client::mobs::animal_brain::Brain,
        map_id: &dyn Fn(u32) -> Option<u32>,
    ) {
        let m = &mut b.mem;
        ab_apply(&mut m.mobs, self.mobs.as_ref(), |l| {
            Some(l.iter().filter_map(|w| w.to_who(map_id)).collect())
        });
        ab_apply(&mut m.nearest_players, self.nearest_players.as_ref(), |l| {
            Some(l.iter().filter_map(|w| w.to_who(map_id)).collect())
        });
        ab_apply(
            &mut m.nearest_visible_player,
            self.nearest_visible_player.as_ref(),
            |w| w.to_who(map_id),
        );
        ab_apply(
            &mut m.nearest_visible_targetable_player,
            self.nearest_visible_targetable_player.as_ref(),
            |w| w.to_who(map_id),
        );
        ab_apply(
            &mut m.nearest_visible_targetable_players,
            self.nearest_visible_targetable_players.as_ref(),
            |l| Some(l.iter().filter_map(|w| w.to_who(map_id)).collect()),
        );
        ab_apply(
            &mut m.nearest_visible_adult,
            self.nearest_visible_adult.as_ref(),
            |w| w.to_who(map_id),
        );
        ab_apply(&mut m.walk_target, self.walk_target.as_ref(), |wt| {
            ab_tracker_to(&wt.target, map_id).map(|target| {
                horizon_client::mobs::animal_brain::WalkTarget {
                    target,
                    speed: f32::from_bits(wt.speed_bits),
                    close: wt.close,
                }
            })
        });
        ab_apply(&mut m.look_target, self.look_target.as_ref(), |t| ab_tracker_to(t, map_id));
        ab_apply(&mut m.breed_target, self.breed_target.as_ref(), |w| w.to_who(map_id));
        ab_apply(&mut m.hurt_by_entity, self.hurt_by_entity.as_ref(), |w| w.to_who(map_id));
        ab_apply(&mut m.tempting_player, self.tempting_player.as_ref(), |w| w.to_who(map_id));
        ab_apply(&mut m.nearest_attackable, self.nearest_attackable.as_ref(), |w| {
            w.to_who(map_id)
        });
        ab_apply(&mut m.attack_target, self.attack_target.as_ref(), |w| w.to_who(map_id));
    }
}

impl DiskMobExtended {
    /// Captures extended `Mob`/`Extra`/`Brain` runtime fields not covered by core NBT.
    pub fn from_mob(m: &Mob) -> Option<Self> {
        let e = &m.extra;
        let pose_extra = (e.pose != 0
            || e.synced_pose != 0
            || e.pose_change_tick != 0
            || e.pose_epoch.is_some()
            || e.camel_sitting)
            .then_some(DiskMobPoseExtra {
                pose: e.pose,
                synced_pose: e.synced_pose,
                pose_change_tick: e.pose_change_tick,
                pose_epoch: e.pose_epoch,
                camel_sitting: e.camel_sitting,
            });
        let school = (e.leader.is_some() || e.lone || e.followers != 0).then_some(
            DiskSchoolExtra {
                leader: e.leader,
                lone: e.lone,
                followers: e.followers,
            },
        );
        let seat = e.seat.map(|(id, off, box_aabb)| {
            let min = box_aabb.min_position();
            let max = box_aabb.max_position();
            (
                id,
                off.to_bits(),
                [
                    min.x.to_bits(),
                    min.y.to_bits(),
                    min.z.to_bits(),
                    max.x.to_bits(),
                    max.y.to_bits(),
                    max.z.to_bits(),
                ],
            )
        });
        let vehicle_body = e
            .vehicle_body
            .map(|(id, a, b)| (id, a.to_bits(), b.to_bits()));
        let last_home_pos = (m.ai.home.is_none() && m.ai.home_position() != [0; 3])
            .then(|| m.ai.home_position());
        let armadillo = (e.armadillo.state != 0 || e.armadillo.in_state_ticks != 0)
            .then_some((e.armadillo.state, e.armadillo.in_state_ticks));
        let frog = (e.frog.tongue_target.is_some() || e.frog.discard_friction).then_some(
            DiskFrogExtra {
                tongue_target: e.frog.tongue_target,
                discard_friction: e.frog.discard_friction,
            },
        );
        let panda = e.panda.as_deref().map(|p| DiskPandaExtra {
            flags: p.flags,
            unhappy: p.unhappy,
            sneeze: p.sneeze,
            eat: p.eat,
            roll_counter: p.roll_counter,
            food_near: p.food_near,
            food_at_bits: p.food_at.map(|v| v.map(f64::to_bits)),
            held_count: p.held_count,
        });
        let illusions = e.illusions.as_ref().map(|il| DiskIllusionsExtra {
            ticks: il.ticks,
            offsets_bits: il.offsets.map(|arr| arr.map(|pt| pt.map(f64::to_bits))),
        });
        let allay = e.allay.as_deref().map(|a| DiskAllayExtra {
            pickup_cooldown: a.pickup_cooldown,
            noteblock: a.noteblock.map(|g| (g.dimension.to_owned(), g.pos)),
            liked_player_ttl: a.liked_player_ttl,
            jukebox: a.jukebox,
        });
        let warden_client_anger = e.warden.as_deref().map(|w| w.client_anger);
        let villager = e.villager.as_deref().map(|v| DiskVillagerExtra {
            raid: v.raid,
            raid_wave: v.raid_wave,
            last_traded_player: v.last_traded_player,
            last_gossip_time: v.last_gossip_time,
            last_gossip_decay: v.last_gossip_decay,
            food_level: v.food_level,
            last_slept: v.last_slept,
            sleeping_pos: v.sleeping_pos,
            golem_until: v.golem_until,
            hurt_seen: v.hurt_seen,
            data_finalized: v.data_finalized,
            lobotomized: v.lobotomized,
        });
        let rider_input = m.rider.map(|r| DiskEquineRider {
            strafe_bits: r.strafe.to_bits(),
            forward_bits: r.forward.to_bits(),
            yaw_bits: r.yaw.to_bits(),
            pitch_bits: r.pitch.to_bits(),
            jump_bits: r.jump.to_bits(),
            steer: r.steer,
            sprint: r.sprint,
            jumping: r.jumping,
            sneak: r.sneak,
            client: r.client,
        });
        let monster_brain = e
            .brain
            .as_deref()
            .map(DiskMonsterBrain::from_brain);
        let animal_brain = e
            .animal_brain
            .as_deref()
            .map(DiskAnimalBrain::from_brain);
        let villager_brain = e
            .villager
            .as_deref()
            .and_then(|v| v.brain.as_deref())
            .map(DiskVillagerBrain::from_brain);

        Some(Self {
            positions: e.positions.clone(),
            extra_prev_vel_bits: [
                e.previous_velocity.x.to_bits(),
                e.previous_velocity.y.to_bits(),
                e.previous_velocity.z.to_bits(),
            ],
            pose_extra,
            school,
            point_bits: e.point.map(|p| p.map(f64::to_bits)),
            repellent: e.repellent,
            opened_chest: e.opened_chest,
            scratch: e.scratch,
            last_stare_sound: e.last_stare_sound,
            relax_one: e.relax_one,
            on_sleeper: e.on_sleeper,
            trap: e.trap,
            trap_time: e.trap_time,
            stand_sliding: e.stand_sliding,
            nautilus_attack_cooldown_ttl: e.nautilus_attack_cooldown_ttl,
            static_env_stamp: e.static_env_stamp,
            max_swell: m.max_swell,
            fire_ticks: m.fire_ticks,
            ticks_frozen: m.ticks_frozen,
            air: m.air,
            persistent: m.persistent,
            standing_on: m.standing_on.clone(),
            gear_knockback_resistance_bits: m.gear_knockback_resistance.map(f64::to_bits),
            boarding_cooldown: e.boarding_cooldown,
            vehicle_order: e.vehicle_order,
            mount: e.mount,
            rider: e.rider,
            seat,
            vehicle_body,
            last_home_pos,
            memories: e
                .memories
                .iter()
                .map(|&(k, v, t)| (k.to_owned(), v, t))
                .collect(),
            armadillo,
            frog,
            goat_discard_friction: e.goat.discard_friction,
            panda,
            illusions,
            allay,
            warden_client_anger,
            villager,
            rider_input,
            monster_brain,
            animal_brain,
            villager_brain,
        })
    }

    pub fn apply_scalar(&self, m: &mut Mob, orig_id: u32) {
        m.max_swell = self.max_swell;
        m.fire_ticks = self.fire_ticks;
        m.ticks_frozen = self.ticks_frozen;
        m.air = self.air;
        m.persistent = self.persistent;
        m.standing_on = self.standing_on.clone();
        m.gear_knockback_resistance = self.gear_knockback_resistance_bits.map(f64::from_bits);
        m.rider = self.rider_input.map(|r| horizon_client::mobs::equine::Rider {
            strafe: f32::from_bits(r.strafe_bits),
            forward: f32::from_bits(r.forward_bits),
            yaw: f32::from_bits(r.yaw_bits),
            pitch: f32::from_bits(r.pitch_bits),
            jump: f32::from_bits(r.jump_bits),
            steer: r.steer,
            sprint: r.sprint,
            jumping: r.jumping,
            sneak: r.sneak,
            client: r.client,
        });
        if let Some(pos) = self.last_home_pos
            && m.ai.home.is_none()
        {
            m.ai.restore_home(pos, -1);
        }

        m.extra.positions = self.positions.clone();
        m.extra.previous_velocity = Vec3::new(
            f64::from_bits(self.extra_prev_vel_bits[0]),
            f64::from_bits(self.extra_prev_vel_bits[1]),
            f64::from_bits(self.extra_prev_vel_bits[2]),
        );
        if let Some(p) = &self.pose_extra {
            m.extra.pose = p.pose;
            m.extra.synced_pose = p.synced_pose;
            m.extra.pose_change_tick = p.pose_change_tick;
            m.extra.pose_epoch = p.pose_epoch;
            m.extra.camel_sitting = p.camel_sitting;
        }
        if let Some(s) = &self.school {
            m.extra.lone = s.lone;
            m.extra.followers = s.followers;
        }
        m.extra.point = self.point_bits.map(|b| b.map(f64::from_bits));
        m.extra.repellent = self.repellent;
        m.extra.opened_chest = self.opened_chest;
        m.extra.scratch = self.scratch;
        m.extra.last_stare_sound = self.last_stare_sound;
        m.extra.relax_one = self.relax_one;
        m.extra.on_sleeper = self.on_sleeper;
        m.extra.trap = self.trap;
        m.extra.trap_time = self.trap_time;
        m.extra.stand_sliding = self.stand_sliding;
        m.extra.nautilus_attack_cooldown_ttl = self.nautilus_attack_cooldown_ttl;
        m.extra.static_env_stamp = self.static_env_stamp;
        m.extra.boarding_cooldown = self.boarding_cooldown;
        m.extra.vehicle_order = self.vehicle_order;
        if !self.memories.is_empty() || !m.extra.memories.is_empty() {
            m.extra.memories.clear();
            for (k, v, t) in &self.memories {
                if let Some(static_k) = intern_extra_memory_key(k) {
                    m.extra.memories.push((static_k, *v, *t));
                }
            }
        }
        if let Some((st, ticks)) = self.armadillo {
            m.extra.armadillo.state = st;
            m.extra.armadillo.in_state_ticks = ticks;
        }
        if let Some(f) = &self.frog {
            m.extra.frog.discard_friction = f.discard_friction;
        }
        m.extra.goat.discard_friction = self.goat_discard_friction;
        if let Some(p) = &self.panda
            && let Some(dst) = m.extra.panda.as_deref_mut()
        {
            dst.flags = p.flags;
            dst.unhappy = p.unhappy;
            dst.sneeze = p.sneeze;
            dst.eat = p.eat;
            dst.roll_counter = p.roll_counter;
            dst.food_near = p.food_near;
            dst.food_at = p.food_at_bits.map(|v| v.map(f64::from_bits));
            dst.held_count = p.held_count;
        }
        if let Some(il) = &self.illusions
            && let Some(dst) = m.extra.illusions.as_mut()
        {
            dst.ticks = il.ticks;
            dst.offsets = il
                .offsets_bits
                .map(|arr| arr.map(|pt| pt.map(f64::from_bits)));
        }
        if let Some(al) = &self.allay {
            let dst = m.extra.allay.get_or_insert_with(Default::default);
            dst.pickup_cooldown = al.pickup_cooldown;
            dst.noteblock = al.noteblock.as_ref().map(|(dim, pos)| {
                horizon_client::mobs::allay::GlobalPos {
                    dimension: horizon_client::mobs::allay::dimension_id(dim),
                    pos: *pos,
                }
            });
            dst.liked_player_ttl = al.liked_player_ttl;
            dst.jukebox = al.jukebox;
        }
        if let Some(ca) = self.warden_client_anger
            && let Some(dst) = m.extra.warden.as_deref_mut()
        {
            dst.client_anger = ca;
        }
        if let Some(vil) = &self.villager {
            let dst = m.extra.villager.get_or_insert_with(Box::default);
            dst.raid = vil.raid;
            dst.raid_wave = vil.raid_wave;
            dst.last_traded_player = vil.last_traded_player;
            dst.last_gossip_time = vil.last_gossip_time;
            dst.last_gossip_decay = vil.last_gossip_decay;
            dst.food_level = vil.food_level;
            dst.last_slept = vil.last_slept;
            dst.sleeping_pos = vil.sleeping_pos;
            dst.golem_until = vil.golem_until;
            dst.hurt_seen = vil.hurt_seen;
            dst.data_finalized = vil.data_finalized;
            dst.lobotomized = vil.lobotomized;
        }
        if let Some(mb) = &self.monster_brain
            && horizon_client::mobs::monster_brain::has_brain(m.species())
        {
            if m.extra.brain.is_none() {
                let saved_rng = m.random.clone();
                horizon_client::mobs::monster_brain::init(m, i64::from(orig_id));
                m.random = saved_rng;
            }
            if let Some(dst) = m.extra.brain.as_deref_mut() {
                mb.apply_scalar(dst);
            }
        }
        if let Some(ab) = &self.animal_brain {
            if m.extra.animal_brain.is_none() {
                let saved_rng = m.random.clone();
                horizon_client::mobs::animal_brain::init(m, i64::from(orig_id));
                m.random = saved_rng;
            }
            if let Some(dst) = m.extra.animal_brain.as_deref_mut() {
                ab.apply_scalar(dst);
            }
        }
    }

    pub fn apply_id_refs(&self, m: &mut Mob, map_id: &dyn Fn(u32) -> Option<u32>) {
        if let Some(s) = &self.school {
            m.extra.leader = s.leader.and_then(map_id);
        }
        if let Some(id) = self.mount
            && let Some(nid) = map_id(id)
        {
            m.extra.mount = Some(nid);
        }
        if let Some(id) = self.rider
            && let Some(nid) = map_id(id)
        {
            m.extra.rider = Some(nid);
        }
        if let Some((id, off, b)) = self.seat
            && let Some(nid) = map_id(id)
        {
            m.extra.seat = Some((
                nid,
                f64::from_bits(off),
                horizon_core::geometry::Aabb::new(
                    Vec3::new(f64::from_bits(b[0]), f64::from_bits(b[1]), f64::from_bits(b[2])),
                    Vec3::new(f64::from_bits(b[3]), f64::from_bits(b[4]), f64::from_bits(b[5])),
                ),
            ));
        }
        if let Some((id, a, b)) = self.vehicle_body
            && let Some(nid) = map_id(id)
        {
            m.extra.vehicle_body = Some((nid, f32::from_bits(a), f32::from_bits(b)));
        }
        if let Some(f) = &self.frog {
            m.extra.frog.tongue_target = f.tongue_target.and_then(map_id);
        }
        if let Some(mb) = &self.monster_brain
            && let Some(dst) = m.extra.brain.as_deref_mut()
        {
            mb.apply_ids(dst, map_id);
        }
        if let Some(ab) = &self.animal_brain
            && let Some(dst) = m.extra.animal_brain.as_deref_mut()
        {
            ab.apply_ids(dst, map_id);
        }
    }

    pub fn apply_villager_brain(
        &self,
        m: &mut Mob,
        _orig_id: u32,
        game_time: i64,
        day_time: i64,
        map_id: &dyn Fn(u32) -> Option<u32>,
        vb: &DiskVillagerBrain,
    ) {
        let has_brain = m
            .extra
            .villager
            .as_deref()
            .is_some_and(|v| v.brain.is_some());
        if !has_brain {
            let saved_rng = m.random.clone();
            villager_brain::init(m, game_time, day_time, vb.shuffle_seed);
            m.random = saved_rng;
        }
        if let Some(dst) = m.extra.villager.as_deref_mut().and_then(|v| v.brain.as_deref_mut()) {
            vb.apply_scalar(dst);
            vb.apply_ids(dst, map_id);
        }
    }
}
