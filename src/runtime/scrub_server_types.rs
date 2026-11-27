use super::super::{Dim, Game};
use super::rrtl::*;
use super::snapshots::*;
use super::types::*;
use horizon_client::{
    chunk::Chunk,
    command_storage::CommandStorage,
    containers::BlockEntity,
    fast_hash::FxHashMap,
    gamerules::GameRules,
    items::ItemStack,
    raid::Raids,
    scoreboard::Scoreboard,
    sculk::WardenSpawnTracker,
    world::{HealthRules, Weather},
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::Vec3,
    nbt::Tag,
    position::BlockPos,
    random::LevelRandom,
    ticks::ChunkTicks,
};
use horizon_mod_runtime::attachments::Attachments;
use horizon_world::level::{BlockEventData, MovingPiston};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::Arc,
};

pub const MAX_SERVER_SCRUB_TICKS: usize = 24_000;
pub const SERVER_SEGMENT_TICKS: usize = RRTL_SEGMENT_TICKS;
pub const HOT_MOB_SEGMENTS: usize = 1;
pub const HEADER_KEYFRAME_STRIDE: usize = 5;

#[derive(Clone)]
pub struct CowGuestRecord {
    pub uuid: u128,
    pub dimension: Arc<str>,
    pub position: [f64; 3],
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub on_ground: bool,
    pub ground: horizon_world::ground_support::GroundState,
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
    pub inventory: Arc<Vec<Option<ItemStack>>>,
    pub armor: Arc<[Option<ItemStack>; 4]>,
    pub offhand: Arc<Option<ItemStack>>,
    pub carried: Arc<Option<ItemStack>>,
    pub ender_items: Arc<Vec<Option<ItemStack>>>,
    pub selected: usize,
    pub craft_grid: Arc<[Option<ItemStack>; 4]>,
    pub craft_result: Arc<Option<ItemStack>>,
    pub effects: Arc<Vec<horizon_client::effects::EffectInstance>>,
    pub effects_tick_count: i32,
    pub effects_changes: u32,
    pub shield_cooldown: u32,
    pub cooldowns: Arc<horizon_client::cooldowns::Cooldowns>,
    pub recipe_book: Arc<horizon_client::recipe_book::Settings>,
    pub advancements: Arc<horizon_client::advancements::Progress>,
    pub stats: Arc<BTreeMap<String, BTreeMap<String, i32>>>,
    pub seen_credits: bool,
    pub post_effects: Arc<Vec<String>>,
    pub attachments: Arc<horizon_mod_runtime::attachments::ResidentAttachments>,
    pub saved: Arc<Option<Tag>>,
    pub hook: Option<HookSnapshot>,
    pub motion: Option<Arc<horizon_client::motion::PlayerMotion>>,
}

impl CowGuestRecord {
    pub fn to_guest_snapshot(&self, game_time: i64) -> GuestSnapshot {
        let motion = self.motion.as_ref().map(|m_arc| {
            let mut m = (**m_arc).clone();
            let pos = Vec3::new(self.position[0], self.position[1], self.position[2]);
            m.position = pos;
            m.old_position = pos;
            m.velocity = self.velocity;
            m.yaw = self.yaw;
            m.head_yaw = self.head_yaw;
            m.pitch = self.pitch;
            m.ground = self.ground;
            m.fall_distance = self.fall_distance;
            let _ = m.set_sprinting(self.sprinting);
            m.shift_key_down = self.sneaking;
            m.local_crouching = self.sneaking;
            m.fall_flying = self.fall_flying;
            m.sleeping = self.sleeping;
            m.fire_ticks = self.fire_ticks;
            m.air = self.air;
            m.ticks_frozen = self.ticks_frozen;
            let _ = m.set_health(self.health.max(1.0), false);
            let _ = m.internal_set_absorption(self.absorption);
            m.health.food.food = self.food;
            m.health.food.saturation = self.saturation;
            m.health.food.exhaustion = self.exhaustion;
            m.health.food.timer = self.food_timer;
            m.health.game_time = game_time;
            m.invulnerable = self.game_mode == 1 || self.game_mode == 3;
            m.may_fly = m.invulnerable || self.allow_flight;
            m.spectator = self.game_mode == 3;
            m.flying = self.flying;
            m.flying_speed = self.flying_speed;
            m
        });
        GuestSnapshot {
            uuid: self.uuid,
            dimension: self.dimension.to_string(),
            position: self.position,
            velocity: self.velocity,
            yaw: self.yaw,
            pitch: self.pitch,
            head_yaw: self.head_yaw,
            on_ground: self.on_ground,
            ground: self.ground,
            game_mode: self.game_mode,
            previous_game_mode: self.previous_game_mode,
            flying: self.flying,
            flying_speed: self.flying_speed,
            allow_flight: self.allow_flight,
            sprinting: self.sprinting,
            sneaking: self.sneaking,
            fall_flying: self.fall_flying,
            sleeping: self.sleeping,
            shoulders: self.shoulders,
            living_flags: self.living_flags,
            stuck: self.stuck,
            bed: self.bed,
            sleep_timer: self.sleep_timer,
            vehicle: self.vehicle,
            vehicle_order: self.vehicle_order,
            vehicle_good: self.vehicle_good,
            spawn: self.spawn,
            respawn_elsewhere: self.respawn_elsewhere.clone(),
            last_death: self.last_death.clone(),
            health: self.health,
            absorption: self.absorption,
            food: self.food,
            saturation: self.saturation,
            exhaustion: self.exhaustion,
            food_timer: self.food_timer,
            fire_ticks: self.fire_ticks,
            air: self.air,
            ticks_frozen: self.ticks_frozen,
            fall_distance: self.fall_distance,
            xp_level: self.xp_level,
            xp_progress: self.xp_progress,
            xp_total: self.xp_total,
            xp_seed: self.xp_seed,
            inventory: (*self.inventory).clone(),
            armor: (*self.armor).clone(),
            offhand: (*self.offhand).clone(),
            carried: (*self.carried).clone(),
            ender_items: (*self.ender_items).clone(),
            selected: self.selected,
            craft_grid: (*self.craft_grid).clone(),
            craft_result: (*self.craft_result).clone(),
            effects: (*self.effects).clone(),
            effects_tick_count: self.effects_tick_count,
            effects_changes: self.effects_changes,
            shield_cooldown: self.shield_cooldown,
            cooldowns: (*self.cooldowns).clone(),
            recipe_book: (*self.recipe_book).clone(),
            advancements: (*self.advancements).clone(),
            stats: (*self.stats).clone(),
            seen_credits: self.seen_credits,
            post_effects: (*self.post_effects).clone(),
            attachments: (*self.attachments).clone(),
            saved: (*self.saved).clone(),
            hook: self.hook.clone(),
            motion,
        }
    }
}

#[derive(Default)]
pub struct CowGuestPrev {
    pub dimension: Option<Arc<str>>,
    pub inventory: Option<Arc<Vec<Option<ItemStack>>>>,
    pub armor: Option<Arc<[Option<ItemStack>; 4]>>,
    pub offhand: Option<Arc<Option<ItemStack>>>,
    pub carried: Option<Arc<Option<ItemStack>>>,
    pub ender_items: Option<Arc<Vec<Option<ItemStack>>>>,
    pub craft_grid: Option<Arc<[Option<ItemStack>; 4]>>,
    pub craft_result: Option<Arc<Option<ItemStack>>>,
    pub advancements: Option<Arc<horizon_client::advancements::Progress>>,
    pub stats: Option<Arc<BTreeMap<String, BTreeMap<String, i32>>>>,
    pub post_effects: Option<Arc<Vec<String>>>,
    pub saved: Option<Arc<Option<Tag>>>,
    pub attachments: Option<Arc<horizon_mod_runtime::attachments::ResidentAttachments>>,
    pub recipe_book: Option<Arc<horizon_client::recipe_book::Settings>>,
    pub motion: Option<Arc<horizon_client::motion::PlayerMotion>>,
}

#[derive(Clone)]
pub struct CowLevelRecord {
    pub chunk_changes: Vec<((i32, i32), Option<Arc<Chunk>>)>,
    pub be_changes: Vec<((i32, i32, i32), Option<Arc<BlockEntity>>)>,
    pub be_ticker_order: Arc<Vec<(i32, i32, i32)>>,
    pub be_asleep: Arc<Vec<(i32, i32, i32)>>,
    pub block_names: Arc<FxHashMap<(i32, i32, i32), Tag>>,
    pub be_attachments: Arc<Vec<((i32, i32, i32), Attachments)>>,
    pub world_attachments: Arc<Attachments>,
    pub chest_openers: Arc<Vec<((i32, i32, i32), (i32, f64))>>,
    pub border: Arc<WorldBorder>,
    pub shulker_anims: Arc<Vec<([i32; 3], horizon_client::block_entity_anim::Shulker)>>,
    pub gateway_anims: Arc<Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)>>,
    pub fangs: Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>>,
    pub ominous_spawners: Arc<Vec<(u32, Tag)>>,
    pub portals_entities: Arc<HashMap<u32, horizon_client::nether_portal::PortalState>>,
    pub lectern_pulses: Arc<Vec<((i32, i32, i32), i32)>>,
    pub lid_openers: Arc<HashMap<(i32, i32, i32), i32>>,
    pub inhabited_changes: Vec<((i32, i32), Option<i64>)>,
    pub end_clock: i64,
    pub random: LevelRandom,
    pub forced_chunks: Arc<HashSet<(i32, i32)>>,
    pub chunk_tick_changes: Vec<((i32, i32), Option<Arc<(ChunkTicks, ChunkTicks)>>)>,
    pub random_tick_value: i32,
    pub comparator_outputs: Arc<HashMap<BlockPos, i32>>,
    pub torch_toggles: Arc<Vec<(BlockPos, i64)>>,
    pub moving_pistons: Arc<Vec<(BlockPos, MovingPiston)>>,
    pub piston_pushes: Arc<Vec<(BlockPos, MovingPiston, f32)>>,
    pub block_events: Arc<VecDeque<BlockEventData>>,
    pub hoppers: Arc<HashMap<(i32, i32, i32), super::super::machines::HopperState>>,
    pub mob_changes: Vec<(u32, Option<Arc<super::super::Mob>>)>,
    pub mob_order_changed: Option<Arc<Vec<u32>>>,
    pub dropped: Arc<horizon_client::join_seq::JoinList<super::super::ItemEntity>>,
    pub orbs: Arc<horizon_client::join_seq::JoinList<super::super::ExperienceOrb>>,
    pub projectiles:
        Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>>,
    pub boats: Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>>,
    pub carts: Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>>,
    pub tnts: Arc<horizon_client::join_seq::JoinList<horizon_gameplay::tnt::PrimedTnt>>,
    pub falling: Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>>,
    pub paintings: Arc<horizon_client::join_seq::JoinList<horizon_client::entities::Painting>>,
    pub item_frames: Arc<horizon_client::join_seq::JoinList<horizon_client::entities::ItemFrame>>,
    pub armor_stands: Arc<horizon_client::join_seq::JoinList<horizon_client::entities::ArmorStand>>,
    pub cushions: Arc<horizon_client::join_seq::JoinList<horizon_client::entities::Cushion>>,
    pub others: Arc<horizon_client::join_seq::ResidentList<super::super::commands::OtherEntity>>,
    pub crystals: Arc<horizon_client::join_seq::JoinList<super::super::dragon::Crystal>>,
    pub dragon: Option<Arc<DragonSnapshot>>,
    pub tridents: Arc<Vec<TridentSnapshot>>,
    pub rockets: Arc<Vec<RocketSnapshot>>,
    pub breath_clouds: Arc<Vec<BreathSnapshot>>,
    pub thrown_potions: Arc<ThrownPotionsSnapshot>,
    pub knot_ids: Arc<HashMap<[i32; 3], u32>>,
    pub persist_uuid_changes: Vec<(u32, Option<[i32; 4]>)>,
    pub persist_raw_changes: Vec<(u32, Option<Arc<Tag>>)>,
    pub persist_passenger_slots: Arc<HashMap<u32, Vec<super::super::persist::PassengerSlot>>>,
    pub raids_mem: Option<Arc<Raids>>,
    pub raid_runtime: Arc<RaidRuntimeSnapshot>,
    pub last_hurt_by_player: Arc<HashMap<u32, super::super::combat::PlayerCredit>>,
    pub love_cause: Arc<HashMap<u32, Option<u32>>>,
    pub died: Arc<HashSet<u32>>,
}

#[derive(Clone)]
pub struct ServerTickRecord {
    pub anchor_ids: Arc<HashSet<u128>>,
    pub policy: RollbackPolicy,
    pub game_time: i64,
    pub day_time: i64,
    pub weather: Weather,
    pub health_rules: HealthRules,
    pub rules: Arc<GameRules>,
    pub border: Arc<WorldBorder>,
    pub bossbars: Arc<Tag>,
    pub sculk_tracker: WardenSpawnTracker,
    pub sculk_darkness: (f32, f32),
    pub fight_tag: Arc<Option<Tag>>,
    pub trader_tag: Arc<Option<Tag>>,
    pub rand_sequences: Arc<horizon_client::random_sequences::RandomSequences>,
    pub difficulty: u8,
    pub difficulty_locked: bool,
    pub clock_rate: f32,
    pub clock_frozen: bool,
    pub time_rate: f32,
    pub time_partial: f32,
    pub time_paused: bool,
    pub command_clocks: Arc<BTreeMap<String, (i64, super::super::chat::WorldClock)>>,
    pub world_spawn: Option<[i32; 3]>,
    pub world_spawn_angle: [f32; 2],
    pub next_entity: u32,
    pub ticked_entities_through: u32,
    pub det_rng_uniquifier: i64,
    pub ordered_state: OrderedStateSnapshot,
    pub cat_tick: i32,
    pub phantom_tick: i32,
    pub patrol_next_tick: i32,
    pub trader_tick_delay: i32,
    pub trader_dirty: bool,
    pub village_siege: VillageSiegeSnapshot,
    pub player_extra: Arc<Option<Tag>>,
    pub shoulder_time: Arc<HashMap<[i32; 4], i64>>,
    pub fangs: Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>>,
    pub ominous_spawners: Arc<Vec<(u32, Tag)>>,
    pub portals_entities: Arc<HashMap<u32, horizon_client::nether_portal::PortalState>>,
    pub end_gateway_cooldowns: Arc<HashMap<u32, i32>>,
    pub lectern_pulses: Arc<Vec<((i32, i32, i32), i32)>>,
    pub player_beds: Arc<Vec<(Dim, [i32; 3])>>,
    pub sleep_seen: (i64, i64),
    pub lead_killers: Arc<HashMap<u32, (u128, Dim)>>,
    pub lid_openers: Arc<HashMap<(i32, i32, i32), i32>>,
    pub entity_attachments: Arc<FxHashMap<u32, Attachments>>,
    pub player_attachments: Arc<horizon_mod_runtime::attachments::ResidentAttachments>,
    pub guest_attachments: Arc<HashMap<u128, Attachments>>,
    pub scheduled_events: Arc<Vec<super::super::functions::Scheduled>>,
    pub scoreboard: Arc<Scoreboard>,
    pub command_storage: Arc<CommandStorage>,
    pub stopwatches_elapsed: Arc<BTreeMap<String, i64>>,
    pub cmd_post_effects: Arc<Vec<String>>,
    pub guests: Vec<CowGuestRecord>,
    pub levels: Vec<(Dim, CowLevelRecord)>,
    pub mob_players: Arc<FxHashMap<u32, u32>>,
    pub client_using: Option<(usize, u32)>,
    pub client_use_duration: i32,
    pub client_crossbow_sounds: [bool; 2],
}

pub struct ServerSegment {
    pub seg_id: u64,
    pub branch_id: u32,
    pub parent_seg_id: u64,
    pub is_keyframe: bool,
    pub start_tick: i64,
    pub end_tick: i64,
    pub sealed_mob_patches: Option<RrtlBlob>,
    pub dim_table: Vec<Dim>,
    pub header_chunks: HashMap<Dim, Arc<FxHashMap<(i32, i32), Arc<Chunk>>>>,
    pub header_chunk_ticks: HashMap<Dim, Arc<HashMap<(i32, i32), Arc<(ChunkTicks, ChunkTicks)>>>>,
    pub header_block_entities: HashMap<Dim, Arc<HashMap<(i32, i32, i32), Arc<BlockEntity>>>>,
    pub header_mobs: HashMap<Dim, Vec<Arc<super::super::Mob>>>,
    pub header_persist_raw: HashMap<Dim, Arc<HashMap<u32, Arc<Tag>>>>,
    pub header_persist_uuids: HashMap<Dim, Arc<HashMap<u32, [i32; 4]>>>,
    pub header_inhabited: HashMap<Dim, Arc<FxHashMap<(i32, i32), i64>>>,
    pub ticks: Vec<ServerTickRecord>,
}

impl ServerSegment {
    pub fn seal_mobs_to_rrtl(&mut self, known_by_dim: &mut HashMap<Dim, HashSet<u32>>) {
        if self.sealed_mob_patches.is_some() {
            return;
        }
        if self.is_keyframe {
            known_by_dim.clear();
            for (&dim, list) in &self.header_mobs {
                let set = list.iter().map(|m| m.id).collect();
                known_by_dim.insert(dim, set);
            }
        }
        let mut dim_table: Vec<Dim> = Vec::new();
        let mut batches: Vec<(u16, u16, Vec<MobCompactPatch>)> = Vec::new();
        for (t_idx, rec) in self.ticks.iter_mut().enumerate() {
            for (dim, lrec) in &mut rec.levels {
                let d_idx = match dim_table.iter().position(|d| d == dim) {
                    Some(i) => i as u16,
                    None => {
                        let i = dim_table.len() as u16;
                        dim_table.push(*dim);
                        i
                    }
                };
                let known = known_by_dim.entry(*dim).or_default();
                let mut kept_changes = Vec::new();
                let mut tick_patches = Vec::new();
                for (id, opt) in std::mem::take(&mut lrec.mob_changes) {
                    match opt {
                        None => {
                            known.remove(&id);
                            kept_changes.push((id, None));
                        }
                        Some(m) => {
                            if known.insert(id) {
                                kept_changes.push((id, Some(m)));
                            } else {
                                tick_patches.push(MobCompactPatch::from_mob(&m));
                            }
                        }
                    }
                }
                lrec.mob_changes = kept_changes;
                if !tick_patches.is_empty() {
                    batches.push((t_idx as u16, d_idx, tick_patches));
                }
            }
        }
        self.dim_table = dim_table;
        self.sealed_mob_patches = Some(encode_mob_patch_batches_rrtl(&batches));
    }
}

#[derive(Default)]
pub struct CowLevelPrev {
    pub chunk_touch_cursor: Option<(u64, u64)>,
    pub chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
    pub chunks_dirty_since_hdr: bool,
    pub chunks_hdr_arc: Option<Arc<FxHashMap<(i32, i32), Arc<Chunk>>>>,
    pub chunk_ticks: HashMap<(i32, i32), Arc<(ChunkTicks, ChunkTicks)>>,
    pub ticks_dirty_since_hdr: bool,
    pub ticks_hdr_arc: Option<Arc<HashMap<(i32, i32), Arc<(ChunkTicks, ChunkTicks)>>>>,
    pub block_entities: HashMap<(i32, i32, i32), Arc<BlockEntity>>,
    pub be_ticker_order: Option<Arc<Vec<(i32, i32, i32)>>>,
    pub be_asleep: Option<Arc<Vec<(i32, i32, i32)>>>,
    pub be_encoded_tags: FxHashMap<(i32, i32, i32), Tag>,
    pub be_dirty_since_hdr: bool,
    pub be_hdr_arc: Option<Arc<HashMap<(i32, i32, i32), Arc<BlockEntity>>>>,
    pub world_revision: u64,
    pub block_names: Option<Arc<FxHashMap<(i32, i32, i32), Tag>>>,
    pub be_attachments: Option<Arc<Vec<((i32, i32, i32), Attachments)>>>,
    pub world_attachments: Option<Arc<Attachments>>,
    pub border_tag: Option<Tag>,
    pub border: Option<Arc<WorldBorder>>,
    pub inhabited: FxHashMap<(i32, i32), i64>,
    pub inhabited_dirty_since_hdr: bool,
    pub inhabited_hdr_arc: Option<Arc<FxHashMap<(i32, i32), i64>>>,
    pub forced_chunks: Option<Arc<HashSet<(i32, i32)>>>,
    pub mobs: FxHashMap<u32, Arc<super::super::Mob>>,
    pub mob_order: Vec<u32>,
    pub dropped_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub dropped: Option<Arc<horizon_client::join_seq::JoinList<super::super::ItemEntity>>>,
    pub orbs_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub orbs: Option<Arc<horizon_client::join_seq::JoinList<super::super::ExperienceOrb>>>,
    pub tnts_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub tnts: Option<Arc<horizon_client::join_seq::JoinList<horizon_gameplay::tnt::PrimedTnt>>>,
    pub proj_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub projectiles:
        Option<Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>>>,
    pub boats_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub boats: Option<Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>>>,
    pub carts_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub carts:
        Option<Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>>>,
    pub falling_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub falling: Option<Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>>>,
    pub paintings_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub paintings: Option<Arc<horizon_client::join_seq::JoinList<horizon_client::entities::Painting>>>,
    pub frames_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub item_frames: Option<Arc<horizon_client::join_seq::JoinList<horizon_client::entities::ItemFrame>>>,
    pub stands_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub armor_stands: Option<Arc<horizon_client::join_seq::JoinList<horizon_client::entities::ArmorStand>>>,
    pub cushions_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub cushions: Option<Arc<horizon_client::join_seq::JoinList<horizon_client::entities::Cushion>>>,
    pub crystals_stamp: Option<horizon_client::join_seq::ListStamp>,
    pub crystals: Option<Arc<horizon_client::join_seq::JoinList<super::super::dragon::Crystal>>>,
    pub knot_ids: Option<Arc<HashMap<[i32; 3], u32>>>,
    pub persist_uuids: HashMap<u32, [i32; 4]>,
    pub persist_uuids_dirty: bool,
    pub persist_uuids_hdr_arc: Option<Arc<HashMap<u32, [i32; 4]>>>,
    pub persist_raw: HashMap<u32, Arc<Tag>>,
    pub persist_raw_dirty: bool,
    pub persist_raw_hdr_arc: Option<Arc<HashMap<u32, Arc<Tag>>>>,
    pub raids_mem: Option<Arc<Raids>>,
    pub raid_runtime: Option<Arc<RaidRuntimeSnapshot>>,
}

#[derive(Default)]
pub struct CowGlobalPrev {
    pub anchor_ids: Option<Arc<HashSet<u128>>>,
    pub border_tag: Option<Tag>,
    pub border: Option<Arc<WorldBorder>>,
    pub bossbars: Option<Arc<Tag>>,
    pub fight_tag: Option<Arc<Option<Tag>>>,
    pub trader_disk_loaded: bool,
    pub trader_tag: Option<Arc<Option<Tag>>>,
    pub scoreboard_tag: Option<Tag>,
    pub scoreboard: Option<Arc<Scoreboard>>,
    pub rand_seq_tag: Option<Tag>,
    pub rand_sequences: Option<Arc<horizon_client::random_sequences::RandomSequences>>,
    pub command_storage: Option<Arc<CommandStorage>>,
    pub stopwatches_elapsed: Option<Arc<BTreeMap<String, i64>>>,
    pub player_extra: Option<Arc<Option<Tag>>>,
    pub shoulder_time: Option<Arc<HashMap<[i32; 4], i64>>>,
    pub end_gateway_cooldowns: Option<Arc<HashMap<u32, i32>>>,
    pub player_beds: Option<Arc<Vec<(Dim, [i32; 3])>>>,
    pub lead_killers: Option<Arc<HashMap<u32, (u128, Dim)>>>,
    pub cmd_post_effects: Option<Arc<Vec<String>>>,
    pub mob_players: Option<Arc<FxHashMap<u32, u32>>>,
    pub player_attachments: Option<Arc<horizon_mod_runtime::attachments::ResidentAttachments>>,
    pub guests: HashMap<u128, CowGuestPrev>,
    pub levels: HashMap<Dim, CowLevelPrev>,
}
