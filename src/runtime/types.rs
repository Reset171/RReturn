use super::super::{Dim, Game};
use super::policy::*;
use horizon_client::lang;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
};

pub const MOD_ID: &str = "rreturn";
pub const MOD_VERSION: &str = env!("CARGO_PKG_VERSION");

static LANG_MAPS: OnceLock<(HashMap<String, String>, HashMap<String, String>)> = OnceLock::new();

fn lang_maps() -> &'static (HashMap<String, String>, HashMap<String, String>) {
    LANG_MAPS.get_or_init(|| {
        let en: HashMap<String, String> =
            serde_json::from_str(LANG_EN_US_JSON).unwrap_or_default();
        let ru: HashMap<String, String> =
            serde_json::from_str(LANG_RU_RU_JSON).unwrap_or_default();
        (en, ru)
    })
}

pub fn is_ru_locale() -> bool {
    lang::code().to_ascii_lowercase().starts_with("ru")
}

pub fn tr(key: &str) -> String {
    let (en, ru) = lang_maps();
    if is_ru_locale() {
        if let Some(val) = ru.get(key) {
            return val.clone();
        }
    }
    en.get(key).cloned().unwrap_or_else(|| key.to_owned())
}

pub fn tr_args(key: &str, args: &[&str]) -> String {
    let mut out = tr(key);
    for arg in args {
        if let Some(pos) = out.find("%s") {
            out.replace_range(pos..pos + 2, arg);
        }
    }
    out
}

pub fn is_valid_slot_name(name: &str) -> bool {
    let len = name.len();
    (1..=32).contains(&len)
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[derive(Clone, Default)]
pub struct UnloadedChunkEntities {
    pub mobs: Vec<Mob>,
    pub dropped: Vec<ItemEntity>,
    pub orbs: Vec<ExperienceOrb>,
    pub projectiles: Vec<horizon_client::projectiles::Projectile>,
    pub boats: Vec<Boat>,
    pub carts: Vec<Minecart>,
    pub tnts: Vec<PrimedTnt>,
    pub falling: Vec<FallingBlock>,
    pub paintings: Vec<Painting>,
    pub item_frames: Vec<ItemFrame>,
    pub armor_stands: Vec<ArmorStand>,
    pub cushions: Vec<Cushion>,
    pub others: Vec<OtherEntity>,
    pub crystals: Vec<Crystal>,
    pub tridents: Vec<TridentSnapshot>,
    pub rockets: Vec<RocketSnapshot>,
    pub breath_clouds: Vec<BreathSnapshot>,
    pub fangs: Vec<super::super::fangs::Fangs>,
    pub ominous_spawners: Vec<(u32, Tag)>,
    pub persist_uuids: HashMap<u32, [i32; 4]>,
    pub persist_raw: HashMap<u32, Tag>,
    pub persist_passenger_slots: HashMap<u32, Vec<super::super::persist::PassengerSlot>>,
}

#[derive(Clone)]
pub struct UnloadedChunkRestore {
    pub dim: Dim,
    pub pos: (i32, i32),
    pub chunk: Option<Arc<Chunk>>,
    pub block_entities: Vec<((i32, i32, i32), BlockEntity)>,
    pub block_names: Vec<((i32, i32, i32), Tag)>,
    pub entities: UnloadedChunkEntities,
}

#[derive(Clone)]
pub enum RollbackOrigin<'a> {
    DiskSlot(&'a str),
    ScrubSnapshot {
        target_tick: i64,
        touched_unloaded: Vec<UnloadedChunkRestore>,
        future_region_files: Vec<PathBuf>,
    },
}

fn is_local_dev_allowed(game: &Game<'_>, requester: Option<u128>) -> bool {
    if game.remote.is_some() && game.integrated.is_none() {
        return false;
    }
    let is_local_or_owner = match (requester, &game.lan) {
        (None, _) => true,
        (Some(req), Some(host)) => host.owner.as_ref().is_some_and(|o| o.uuid == req),
        _ => false,
    };
    is_local_or_owner
        && (with_state(|st| st.dev_mode)
            || std::env::var_os("RRETURN_DEV").is_some_and(|v| v == "1" || v == "true"))
}

fn is_cheats_allowed_in_world(game: &Game<'_>) -> bool {
    if game.allow_commands() {
        return true;
    }
    if game.ui.registries.permission >= 2 {
        return true;
    }
    if let Some(srv) = &game.integrated {
        if srv
            .call(|s| s.allow_commands() || s.ui.registries.permission >= 2)
            .unwrap_or(false)
        {
            return true;
        }
    }
    if let Some(host) = &game.lan {
        if let Some(owner) = &host.owner {
            if host.hub.ops.get(&owner.uuid).copied().unwrap_or(0) >= 2 {
                return true;
            }
        }
    }
    false
}

pub fn has_cheat_permission(game: &Game<'_>, requester: Option<u128>) -> bool {
    if is_local_dev_allowed(game, requester) {
        return true;
    }
    if !is_cheats_allowed_in_world(game) {
        return false;
    }
    let Some(req) = requester else {
        return true;
    };
    if let Some(host) = &game.lan {
        if host.owner.as_ref().is_some_and(|o| o.uuid == req) {
            return true;
        }
        if host.hub.ops.get(&req).copied().unwrap_or(0) >= 2 {
            return true;
        }
        return host
            .hub
            .guests
            .iter()
            .find(|g| g.uuid == req)
            .is_some_and(|g| g.permission >= 2);
    }
    true
}

pub fn has_level_4_permission(game: &Game<'_>, requester: Option<u128>) -> bool {
    if is_local_dev_allowed(game, requester) {
        return true;
    }
    if !is_cheats_allowed_in_world(game) {
        return false;
    }
    let Some(req) = requester else {
        return true;
    };
    if let Some(host) = &game.lan {
        if host.owner.as_ref().is_some_and(|o| o.uuid == req) {
            return true;
        }
        if host.hub.ops.get(&req).copied().unwrap_or(0) >= 4 {
            return true;
        }
        return host
            .hub
            .guests
            .iter()
            .find(|g| g.uuid == req)
            .is_some_and(|g| g.permission >= 4);
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorMode {
    Manual,
    Everyone,
    Rotating,
}

impl AnchorMode {
    pub fn id(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Everyone => "everyone",
            Self::Rotating => "rotating",
        }
    }

    pub fn lang_key(self) -> &'static str {
        match self {
            Self::Manual => "config.rreturn.anchorMode.manual",
            Self::Everyone => "config.rreturn.anchorMode.everyone",
            Self::Rotating => "config.rreturn.anchorMode.rotating",
        }
    }

    pub fn by_id(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "2" | "everyone" => Self::Everyone,
            "3" | "rotating" => Self::Rotating,
            _ => Self::Manual,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Manual => Self::Everyone,
            Self::Everyone => Self::Rotating,
            Self::Rotating => Self::Manual,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AnchorSettings {
    pub mode: AnchorMode,
    pub rotation_seconds: i32,
    pub rollback_cooldown_seconds: i32,
    pub pinned: Vec<u128>,
}

impl Default for AnchorSettings {
    fn default() -> Self {
        Self {
            mode: AnchorMode::Manual,
            rotation_seconds: 300,
            rollback_cooldown_seconds: 0,
            pinned: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RollbackBenchmark {
    pub total_ms: f64,
    pub globals_ms: f64,
    pub blocks_ms: f64,
    pub containers_ms: f64,
    pub entities_ms: f64,
    pub player_ms: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnlinePlayerInfo {
    pub uuid: u128,
    pub name: String,
    pub dimension: String,
    pub is_owner: bool,
}

pub struct RReturnState {
    pub current_slot: String,
    pub death_rollback: bool,
    pub auto_save: bool,
    pub adaptive_mode: bool,
    pub use_random_interval: bool,
    pub fixed_interval_sec: i32,
    pub random_min_sec: i32,
    pub random_max_sec: i32,
    pub adaptive_interval_sec: i32,
    pub min_save_gap_sec: i32,
    pub relax_time_sec: i32,
    pub post_combat_save_pct: i32,
    pub save_on_boss_defeat: bool,
    pub logs: bool,
    pub policy: RollbackPolicy,
    pub scrub_policy: RollbackPolicy,
    pub scrub_history_seconds: i32,
    pub scrub_wheel_sensitivity: i32,
    pub scrub_reverse_audio: bool,
    pub scrub_show_hud: bool,
    pub anchor: AnchorSettings,
    pub player_overrides: HashMap<u128, PlayerProfileOverrides>,
    pub online_players: Vec<OnlinePlayerInfo>,
    pub anchor_ids: HashSet<u128>,
    pub has_checkpoint: bool,
    pub last_save_tick: i64,
    pub current_interval_ticks: i64,
    pub last_rollback_tick: Option<i64>,
    pub last_boss_death_tick: i64,
    pub current_intensity: f64,
    pub peak_intensity: f64,
    pub relax_ticks: i32,
    pub pending_death_rollback: Option<u128>,
    pub pending_save_request: bool,
    pub pending_load_request: bool,
    pub last_benchmark: Option<RollbackBenchmark>,
    pub f3_loop_hitboxes: bool,
    pub dev_mode: bool,
    pub scrub_enabled: bool,
}

impl Default for RReturnState {
    fn default() -> Self {
        Self {
            current_slot: "default".to_owned(),
            death_rollback: true,
            auto_save: true,
            adaptive_mode: true,
            use_random_interval: false,
            fixed_interval_sec: 300,
            random_min_sec: 300,
            random_max_sec: 600,
            adaptive_interval_sec: 300,
            min_save_gap_sec: 30,
            relax_time_sec: 5,
            post_combat_save_pct: 70,
            save_on_boss_defeat: false,
            logs: false,
            scrub_enabled: false,
            policy: RollbackPolicy::default(),
            scrub_policy: RollbackPolicy::default(),
            scrub_history_seconds: 1200,
            scrub_wheel_sensitivity: 15,
            scrub_reverse_audio: true,
            scrub_show_hud: true,
            anchor: AnchorSettings::default(),
            player_overrides: HashMap::new(),
            online_players: Vec::new(),
            anchor_ids: HashSet::new(),
            has_checkpoint: false,
            last_save_tick: 0,
            current_interval_ticks: 6000,
            last_rollback_tick: None,
            last_boss_death_tick: -9999,
            current_intensity: 0.0,
            peak_intensity: 0.0,
            relax_ticks: 0,
            pending_death_rollback: None,
            pending_save_request: false,
            pending_load_request: false,
            last_benchmark: None,
            f3_loop_hitboxes: true,
            dev_mode: std::env::var_os("RRETURN_DEV").is_some_and(|v| v == "1" || v == "true"),
        }
    }
}

static SCRUB_MAX_TICKS_ATOMIC: AtomicUsize = AtomicUsize::new(24_000);
static SCRUB_WHEEL_SENS_ATOMIC: AtomicI32 = AtomicI32::new(15);
static SCRUB_REVERSE_AUDIO_ATOMIC: AtomicBool = AtomicBool::new(true);
static SCRUB_SHOW_HUD_ATOMIC: AtomicBool = AtomicBool::new(true);

pub fn sync_scrub_atomics_from_state(st: &RReturnState) {
    let ticks = (st.scrub_history_seconds.clamp(30, 1200) as usize) * 20;
    SCRUB_MAX_TICKS_ATOMIC.store(ticks, Ordering::Relaxed);
    SCRUB_WHEEL_SENS_ATOMIC.store(st.scrub_wheel_sensitivity.clamp(5, 40), Ordering::Relaxed);
    SCRUB_REVERSE_AUDIO_ATOMIC.store(st.scrub_reverse_audio, Ordering::Relaxed);
    SCRUB_SHOW_HUD_ATOMIC.store(st.scrub_show_hud, Ordering::Relaxed);
}

pub fn scrub_max_history_ticks() -> usize {
    SCRUB_MAX_TICKS_ATOMIC.load(Ordering::Relaxed).clamp(600, 24_000)
}

pub fn scrub_wheel_sens_factor() -> f64 {
    f64::from(SCRUB_WHEEL_SENS_ATOMIC.load(Ordering::Relaxed).clamp(5, 40)) / 10.0
}

pub fn scrub_reverse_audio_enabled() -> bool {
    SCRUB_REVERSE_AUDIO_ATOMIC.load(Ordering::Relaxed)
}

pub fn scrub_show_hud_enabled() -> bool {
    SCRUB_SHOW_HUD_ATOMIC.load(Ordering::Relaxed)
}

pub fn collect_online_players(game: &Game<'_>) -> Vec<OnlinePlayerInfo> {
    let Some(host) = game.lan.as_ref() else {
        return Vec::new();
    };
    let owner_uuid = host.owner.as_ref().map(|o| o.uuid);
    let mut list: Vec<OnlinePlayerInfo> = host
        .hub
        .guests
        .iter()
        .filter(|g| g.placed)
        .map(|g| OnlinePlayerInfo {
            uuid: g.uuid,
            name: g.name.clone(),
            dimension: g.dimension.clone(),
            is_owner: owner_uuid == Some(g.uuid),
        })
        .collect();
    list.sort_by(|a, b| {
        b.is_owner
            .cmp(&a.is_owner)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    list
}

static STATE: OnceLock<Mutex<RReturnState>> = OnceLock::new();
static GLOBAL_STATE: OnceLock<Mutex<RReturnState>> = OnceLock::new();

fn state_lock() -> &'static Mutex<RReturnState> {
    STATE.get_or_init(|| Mutex::new(RReturnState::default()))
}

fn global_state_lock() -> &'static Mutex<RReturnState> {
    GLOBAL_STATE.get_or_init(|| Mutex::new(RReturnState::default()))
}

pub fn with_state<R>(f: impl FnOnce(&RReturnState) -> R) -> R {
    let guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    f(&guard)
}

pub fn with_state_mut<R>(f: impl FnOnce(&mut RReturnState) -> R) -> R {
    let mut guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

pub fn with_global_state<R>(f: impl FnOnce(&RReturnState) -> R) -> R {
    let guard = global_state_lock().lock().unwrap_or_else(|e| e.into_inner());
    f(&guard)
}

pub fn with_global_state_mut<R>(f: impl FnOnce(&mut RReturnState) -> R) -> R {
    let mut guard = global_state_lock().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

pub fn mod_chat(game: &mut Game<'_>, msg: &str) {
    if with_state(|st| st.dev_mode) {
        game.native_chat(msg);
    }
}

pub fn raids_dat_path(world_root: &Path, dim: Dim) -> PathBuf {
    horizon_client::save::dimension_dir(world_root, dim.id())
        .join("data/minecraft/raids.dat")
}

pub fn fight_dat_path(world_root: &Path) -> PathBuf {
    horizon_client::save::dimension_dir(world_root, "minecraft:the_end")
        .join("data/minecraft/ender_dragon_fight.dat")
}

pub fn trader_dat_path(world_root: &Path) -> PathBuf {
    world_root.join("data/minecraft/wandering_trader.dat")
}

pub fn resolve_anchors(
    settings: &AnchorSettings,
    online: &[u128],
    game_time: i64,
    requester: Option<u128>,
) -> Vec<u128> {
    match settings.mode {
        AnchorMode::Manual => {
            if !settings.pinned.is_empty() {
                settings.pinned.clone()
            } else if let Some(req) = requester {
                vec![req]
            } else if let Some(&first) = online.first() {
                vec![first]
            } else {
                Vec::new()
            }
        }
        AnchorMode::Everyone => online.to_vec(),
        AnchorMode::Rotating => {
            if online.is_empty() {
                return Vec::new();
            }
            let rotation_ticks = i64::from(settings.rotation_seconds.max(5)) * 20;
            let index = ((game_time / rotation_ticks).max(0) as usize) % online.len();
            vec![online[index]]
        }
    }
}

pub fn resolve_player_effective_policy(
    st: &RReturnState,
    base: &RollbackPolicy,
    is_scrub: bool,
    uuid: u128,
    name: Option<&str>,
) -> RollbackPolicy {
    let profile = st.player_overrides.get(&uuid).or_else(|| {
        let wanted = name?;
        st.player_overrides
            .values()
            .find(|p| !p.name.is_empty() && p.name.eq_ignore_ascii_case(wanted))
    });
    match profile {
        Some(p) => {
            let ov = if is_scrub { &p.scrub } else { &p.rollback };
            ov.apply_to_policy(base)
        }
        None => *base,
    }
}

pub fn find_local_player_identity(game: &Game<'_>) -> Option<(u128, String)> {
    if let Some(host) = game.lan.as_ref() {
        let target = game
            .serving
            .as_ref()
            .map(|s| s.owner)
            .or_else(|| host.owner.as_ref().map(|o| o.uuid));
        if let Some(u) = target {
            if let Some(g) = host.hub.guests.iter().find(|g| g.uuid == u) {
                return Some((g.uuid, g.name.clone()));
            }
            if let Some(o) = host.owner.as_ref().filter(|o| o.uuid == u) {
                return Some((o.uuid, String::new()));
            }
            return Some((u, String::new()));
        }
        if let Some(g) = host.hub.guests.first() {
            return Some((g.uuid, g.name.clone()));
        }
    }
    None
}

pub fn resolve_local_client_policy(game: &Game<'_>, is_scrub: bool) -> RollbackPolicy {
    let ident = find_local_player_identity(game);
    with_state(|st| {
        let base = if is_scrub { st.scrub_policy } else { st.policy };
        if let Some((uuid, name)) = ident {
            resolve_policy_via_online(st, &base, is_scrub, uuid, Some(&name))
        } else if let Some(owner) = st
            .online_players
            .iter()
            .find(|p| p.is_owner)
            .or_else(|| st.online_players.first())
        {
            resolve_player_effective_policy(st, &base, is_scrub, owner.uuid, Some(&owner.name))
        } else {
            base
        }
    })
}

fn resolve_policy_via_online(
    st: &RReturnState,
    base: &RollbackPolicy,
    is_scrub: bool,
    uuid: u128,
    name: Option<&str>,
) -> RollbackPolicy {
    let resolved_name = name.filter(|s| !s.is_empty()).or_else(|| {
        st.online_players
            .iter()
            .find(|p| p.uuid == uuid)
            .map(|p| p.name.as_str())
    });
    resolve_player_effective_policy(st, base, is_scrub, uuid, resolved_name)
}
