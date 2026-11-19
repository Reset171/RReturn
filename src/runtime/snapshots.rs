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
use super::types::*;
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

#[derive(Clone, Copy, Debug)]
pub struct DiskMobRuntime {
    pub age: u32,
    pub invulnerable: i32,
    pub last_hurt: f32,
    pub swell: i32,
    pub previous_swell: i32,
    pub ambient_timer: i32,
    pub rng_state: u64,
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
}

#[derive(Clone, Copy, Debug)]
pub struct DiskItemRuntime {
    pub ticks: u32,
    pub bob_offset: f32,
    pub rng_state: u64,
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
    pub knot_ids: HashMap<[i32; 3], u32>,
    pub persist_uuids: HashMap<u32, [i32; 4]>,
    pub persist_raw: HashMap<u32, Tag>,
    pub persist_passenger_slots: HashMap<u32, Vec<super::super::persist::PassengerSlot>>,
    pub raids_dat: Option<Vec<u8>>,
    pub last_hurt_by_player: HashMap<u32, super::super::combat::PlayerCredit>,
    pub love_cause: HashMap<u32, Option<u32>>,
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
    pub ordered_announced: horizon_client::join_seq::JoinSeq,
    pub cat_tick: i32,
    pub phantom_tick: i32,
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
    pub sound_seed_state: Option<u64>,
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