use super::super::{Dim, Game};
use super::policy::*;
use horizon_client::lang;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
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

fn is_local_dev_allowed(game: &Game<'_>, requester: Option<u128>) -> bool {
    if game.remote.is_some() {
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

pub fn has_cheat_permission(game: &Game<'_>, requester: Option<u128>) -> bool {
    if is_local_dev_allowed(game, requester) {
        return true;
    }
    if game.allow_commands() {
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
        return true;
    }
    if let Some(host) = &game.lan {
        if let Some(req) = requester {
            return host.hub.ops.get(&req).copied().unwrap_or(0) >= 2
                || host
                    .hub
                    .guests
                    .iter()
                    .find(|g| g.uuid == req)
                    .is_some_and(|g| g.permission >= 2);
        }
        if let Some(owner) = &host.owner {
            return host.hub.ops.get(&owner.uuid).copied().unwrap_or(0) >= 2;
        }
    }
    false
}

pub fn has_level_4_permission(game: &Game<'_>, requester: Option<u128>) -> bool {
    if is_local_dev_allowed(game, requester) {
        return true;
    }
    if game.allow_commands() {
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
        return true;
    }
    if let Some(host) = &game.lan {
        if let Some(req) = requester {
            return host.hub.ops.get(&req).copied().unwrap_or(0) >= 4
                || host
                    .hub
                    .guests
                    .iter()
                    .find(|g| g.uuid == req)
                    .is_some_and(|g| g.permission >= 4);
        }
        if let Some(owner) = &host.owner {
            return host.hub.ops.get(&owner.uuid).copied().unwrap_or(0) >= 4;
        }
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorMode {
    Fixed,
    Multi,
    Everyone,
    Rotating,
}

impl AnchorMode {
    pub fn id(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Multi => "multi",
            Self::Everyone => "everyone",
            Self::Rotating => "rotating",
        }
    }

    pub fn lang_key(self) -> &'static str {
        match self {
            Self::Fixed => "config.rreturn.anchorMode.fixed",
            Self::Multi => "config.rreturn.anchorMode.multi",
            Self::Everyone => "config.rreturn.anchorMode.everyone",
            Self::Rotating => "config.rreturn.anchorMode.rotating",
        }
    }

    pub fn by_id(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "multi" => Self::Multi,
            "everyone" => Self::Everyone,
            "rotating" => Self::Rotating,
            _ => Self::Fixed,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Fixed => Self::Multi,
            Self::Multi => Self::Everyone,
            Self::Everyone => Self::Rotating,
            Self::Rotating => Self::Fixed,
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
            mode: AnchorMode::Fixed,
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

pub struct RReturnState {
    pub current_slot: String,
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
    pub anchor: AnchorSettings,
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
}

impl Default for RReturnState {
    fn default() -> Self {
        Self {
            current_slot: "default".to_owned(),
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
            save_on_boss_defeat: true,
            logs: false,
            policy: RollbackPolicy::default(),
            anchor: AnchorSettings::default(),
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

static STATE: OnceLock<Mutex<RReturnState>> = OnceLock::new();

fn state_lock() -> &'static Mutex<RReturnState> {
    STATE.get_or_init(|| Mutex::new(RReturnState::default()))
}

pub fn with_state<R>(f: impl FnOnce(&RReturnState) -> R) -> R {
    let guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    f(&guard)
}

pub fn with_state_mut<R>(f: impl FnOnce(&mut RReturnState) -> R) -> R {
    let mut guard = state_lock().lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
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
        AnchorMode::Fixed => {
            if let Some(req) = requester {
                vec![req]
            } else if !settings.pinned.is_empty() {
                settings.pinned.clone()
            } else if let Some(&first) = online.first() {
                vec![first]
            } else {
                Vec::new()
            }
        }
        AnchorMode::Multi => {
            if !settings.pinned.is_empty() {
                settings.pinned.clone()
            } else {
                online.to_vec()
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
