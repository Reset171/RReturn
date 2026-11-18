use super::super::{
    Dim, ExperienceOrb, FallingBlock, Game, ItemEntity, Mob,
    boats::Boat,
    commands::OtherEntity,
    dragon::{Crystal, Dragon},
    fishing,
    lan::LanHost,
    machines::HopperState,
    minecarts::Minecart,
    tnt::PrimedTnt,
};
use mod_api::WorldPhase;
use crate::game::dragon::rreturn_dragon::{DragonExt, FightExt};
use crate::game::end::rreturn_end::EndExt;
use crate::game::fishing::rreturn_fishing::{GameFishingExt, HookExt};
use crate::game::remote::rreturn_remote::RemoteExt;
use crate::game::stats::rreturn_stats::StatsExt;
use horizon_mod_runtime::attachments::Attachments;
use horizon_client::{
    advancements::Progress,
    blocks::Fluid,
    bossbar::BossBars,
    chunk::Chunk,
    containers::BlockEntity,
    cooldowns::Cooldowns,
    effects::{EffectInstance, PlayerEffects, SavedEffect},
    entities::{ArmorStand, Cushion, ItemFrame, Painting},
    fast_hash::FxHashMap,
    gamerules::GameRules,
    items::ItemStack,
    lan::guest::SavedBody,
    lang,
    mobs::Species,
    recipe_book::Settings as RecipeBookSettings,
    save::{item_from_nbt, read_dat, write_dat, Level},
    scoreboard::Scoreboard,
    sculk::WardenSpawnTracker,
    world::{HealthRules, Weather},
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::{Direction, Vec3},
    nbt::Tag,
    position::{BlockPos, ChunkPos},
    random::{LegacyRandom, LevelRandom, RandomSource},
    ticks::ChunkTicks,
};
use horizon_world::food::Difficulty;
use horizon_protocol::{
    packets::{
        play::{self, Clientbound as C},
        recipes,
    },
    text::literal,
};
use horizon_world::{
    ground_support::GroundState,
    level::{BlockEventData, BlockId, MovingPiston, Ticks},
    registry::BlockStateId,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, RwLock,
    },
    time::Instant,
};

pub const LANG_EN_US_JSON: &str = "{\n  \"mod.rreturn.description\": \"Return by Death. Completely rewind the timeline and the world state to your last checkpoint upon death.\",\n  \"key.rreturn.save\": \"Create Checkpoint\",\n  \"key.rreturn.load\": \"Return to Checkpoint\",\n  \"key.rreturn.anchor_menu\": \"Key Player Menu\",\n  \"gui.rreturn.anchor.no_permission\": \"[RReturn] You do not have permission to manage key players.\",\n  \"config.rreturn.anchorMode.fixed\": \"Single Player\",\n  \"config.rreturn.anchorMode.multi\": \"Multiple Key Players\",\n  \"config.rreturn.anchorMode.everyone\": \"All Players\",\n  \"config.rreturn.anchorMode.rotating\": \"Rotation\",\n  \"ui.rreturn.on\": \"ON\",\n  \"ui.rreturn.off\": \"OFF\",\n  \"ui.rreturn.auto_save\": \"Auto-Save: %s\",\n  \"ui.rreturn.adaptive_save\": \"Adaptive Save: %s\",\n  \"ui.rreturn.interval\": \"Auto-Save Interval: %s s\",\n  \"ui.rreturn.anchor_mode\": \"Key Player Mode: %s\",\n  \"ui.rreturn.cooldown_off\": \"Rollback Cooldown: Off\",\n  \"ui.rreturn.cooldown\": \"Rollback Cooldown: %s s\",\n  \"ui.rreturn.boss_save\": \"Checkpoint on Boss Defeat: %s\",\n  \"ui.rreturn.logs\": \"Logs: %s\",\n  \"ui.rreturn.f3_hitboxes\": \"F3 Loop Hitboxes: %s\",\n  \"ui.rreturn.dev_mode\": \"Developer Mode: %s\",\n  \"command.rreturn.no_permission\": \"[RReturn] You do not have permission (enable cheats for manual control).\",\n  \"command.rreturn.set.success\": \"[RReturn] Checkpoint created successfully.\",\n  \"command.rreturn.set.failed\": \"[RReturn] Failed to create checkpoint (I/O error).\",\n  \"command.rreturn.load.success\": \"[RReturn] Timeline rolled back to checkpoint (tick=%s).\",\n  \"command.rreturn.load.failed\": \"[RReturn] Rollback aborted: No timeline origin found.\",\n  \"command.rreturn.set.slot_success\": \"[RReturn] Checkpoint saved to slot '%s' successfully.\",\n  \"command.rreturn.set.slot_failed\": \"[RReturn] Failed to save checkpoint to slot '%s' (I/O error).\",\n  \"command.rreturn.load.slot_success\": \"[RReturn] Timeline rolled back to slot '%s' (tick=%s).\",\n  \"command.rreturn.load.slot_failed\": \"[RReturn] Rollback aborted: No timeline origin found for slot '%s'.\",\n  \"command.rreturn.anchor.mode\": \"[RReturn] Key player mode: %s\",\n  \"command.rreturn.f3.status\": \"[RReturn] F3 loop hitboxes: %s\",\n  \"command.rreturn.dev.status\": \"[RReturn] Developer Mode: %s\"\n}";
pub const LANG_RU_RU_JSON: &str = "{\n  \"mod.rreturn.description\": \"Посмертный возврат. Полностью откатывает время и состояние мира к последней точке сохранения при гибели.\",\n  \"key.rreturn.save\": \"Создать точку сохранения\",\n  \"key.rreturn.load\": \"Вернуться к точке сохранения\",\n  \"key.rreturn.anchor_menu\": \"Меню ключевых игроков\",\n  \"gui.rreturn.anchor.no_permission\": \"[RReturn] У вас нет прав оператора для управления ключевыми игроками.\",\n  \"config.rreturn.anchorMode.fixed\": \"Один игрок\",\n  \"config.rreturn.anchorMode.multi\": \"Несколько ключевых игроков\",\n  \"config.rreturn.anchorMode.everyone\": \"Все игроки\",\n  \"config.rreturn.anchorMode.rotating\": \"Циклическая смена\",\n  \"ui.rreturn.on\": \"ВКЛ\",\n  \"ui.rreturn.off\": \"ВЫКЛ\",\n  \"ui.rreturn.auto_save\": \"Автосохранение: %s\",\n  \"ui.rreturn.adaptive_save\": \"Адаптивное сохранение: %s\",\n  \"ui.rreturn.interval\": \"Интервал автосохранения: %s сек\",\n  \"ui.rreturn.anchor_mode\": \"Режим ключевого игрока: %s\",\n  \"ui.rreturn.cooldown_off\": \"Перезарядка отката: Выкл\",\n  \"ui.rreturn.cooldown\": \"Перезарядка отката: %s сек\",\n  \"ui.rreturn.boss_save\": \"Точка сохранения при победе над боссом: %s\",\n  \"ui.rreturn.logs\": \"Логи: %s\",\n  \"ui.rreturn.f3_hitboxes\": \"Подсветка петель F3+B: %s\",\n  \"ui.rreturn.dev_mode\": \"Режим разработчика: %s\",\n  \"command.rreturn.no_permission\": \"[RReturn] У вас нет прав оператора (включите читы в мире для ручного управления).\",\n  \"command.rreturn.set.success\": \"[RReturn] Точка сохранения успешно создана.\",\n  \"command.rreturn.set.failed\": \"[RReturn] Не удалось создать точку сохранения (ошибка записи на диск).\",\n  \"command.rreturn.load.success\": \"[RReturn] Мир возвращён к точке сохранения (тик=%s).\",\n  \"command.rreturn.load.failed\": \"[RReturn] Откат отменён: точка сохранения не найдена.\",\n  \"command.rreturn.set.slot_success\": \"[RReturn] Точка сохранения успешно записана в слот '%s'.\",\n  \"command.rreturn.set.slot_failed\": \"[RReturn] Не удалось сохранить точку сохранения в слот '%s' (ошибка записи на диск).\",\n  \"command.rreturn.load.slot_success\": \"[RReturn] Мир возвращён к слоту '%s' (тик=%s).\",\n  \"command.rreturn.load.slot_failed\": \"[RReturn] Откат отменён: точка сохранения для слота '%s' не найдена.\",\n  \"command.rreturn.anchor.mode\": \"[RReturn] Режим ключевого игрока: %s\",\n  \"command.rreturn.f3.status\": \"[RReturn] Подсветка петель F3+B: %s\",\n  \"command.rreturn.dev.status\": \"[RReturn] Режим разработчика: %s\"\n}";


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

#[derive(Clone, Copy, Debug)]
pub struct RollbackPolicy {
    pub blocks: bool,
    pub biomes: bool,
    pub block_entities: bool,
    pub block_ticks: bool,
    pub fluid_ticks: bool,
    pub block_events: bool,
    pub entities_presence: bool,
    pub dropped_items: bool,
    pub experience_orbs: bool,
    pub player_position: bool,
    pub player_motion: bool,
    pub player_health: bool,
    pub player_hunger: bool,
    pub player_experience: bool,
    pub player_inventory: bool,
    pub player_ender_chest: bool,
    pub player_potion_effects: bool,
    pub player_cooldowns: bool,
    pub player_abilities: bool,
    pub player_recipe_book: bool,
    pub player_spawn_point: bool,
    pub player_stats: bool,
    pub preserve_new_player_inventory: bool,
    pub world_time: bool,
    pub world_weather: bool,
    pub world_raids: bool,
    pub world_dragon_fight: bool,
    pub world_scoreboard: bool,
    pub world_level_rng: bool,
    pub world_difficulty: bool,
    pub world_spawn: bool,
    pub server_bossbars: bool,
    pub server_forced_chunks: bool,
    pub server_world_border: bool,
    pub server_game_rules: bool,
}

impl Default for RollbackPolicy {
    fn default() -> Self {
        Self {
            blocks: true,
            biomes: true,
            block_entities: true,
            block_ticks: true,
            fluid_ticks: true,
            block_events: true,
            entities_presence: true,
            dropped_items: true,
            experience_orbs: true,
            player_position: true,
            player_motion: true,
            player_health: true,
            player_hunger: true,
            player_experience: true,
            player_inventory: true,
            player_ender_chest: true,
            player_potion_effects: true,
            player_cooldowns: true,
            player_abilities: true,
            player_recipe_book: true,
            player_spawn_point: true,
            player_stats: true,
            preserve_new_player_inventory: false,
            world_time: true,
            world_weather: true,
            world_raids: true,
            world_dragon_fight: true,
            world_scoreboard: true,
            world_level_rng: true,
            world_difficulty: true,
            world_spawn: true,
            server_bossbars: true,
            server_forced_chunks: true,
            server_world_border: true,
            server_game_rules: true,
        }
    }
}

impl RollbackPolicy {
    pub fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::with_capacity(35);
        for (key, val) in [
            ("blocks", self.blocks),
            ("biomes", self.biomes),
            ("block_entities", self.block_entities),
            ("block_ticks", self.block_ticks),
            ("fluid_ticks", self.fluid_ticks),
            ("block_events", self.block_events),
            ("entities_presence", self.entities_presence),
            ("dropped_items", self.dropped_items),
            ("experience_orbs", self.experience_orbs),
            ("player_position", self.player_position),
            ("player_motion", self.player_motion),
            ("player_health", self.player_health),
            ("player_hunger", self.player_hunger),
            ("player_experience", self.player_experience),
            ("player_inventory", self.player_inventory),
            ("player_ender_chest", self.player_ender_chest),
            ("player_potion_effects", self.player_potion_effects),
            ("player_cooldowns", self.player_cooldowns),
            ("player_abilities", self.player_abilities),
            ("player_recipe_book", self.player_recipe_book),
            ("player_spawn_point", self.player_spawn_point),
            ("player_stats", self.player_stats),
            (
                "preserve_new_player_inventory",
                self.preserve_new_player_inventory,
            ),
            ("world_time", self.world_time),
            ("world_weather", self.world_weather),
            ("world_raids", self.world_raids),
            ("world_dragon_fight", self.world_dragon_fight),
            ("world_scoreboard", self.world_scoreboard),
            ("world_level_rng", self.world_level_rng),
            ("world_difficulty", self.world_difficulty),
            ("world_spawn", self.world_spawn),
            ("server_bossbars", self.server_bossbars),
            ("server_forced_chunks", self.server_forced_chunks),
            ("server_world_border", self.server_world_border),
            ("server_game_rules", self.server_game_rules),
        ] {
            map.insert(key.to_owned(), serde_json::Value::Bool(val));
        }
        serde_json::Value::Object(map)
    }

    pub fn set_field(&mut self, key: &str, val: bool) -> bool {
        let norm = key.trim().to_ascii_lowercase().replace('.', "_");
        match norm.as_str() {
            "blocks" => self.blocks = val,
            "biomes" => self.biomes = val,
            "block_entities" => self.block_entities = val,
            "block_ticks" => self.block_ticks = val,
            "fluid_ticks" => self.fluid_ticks = val,
            "block_events" => self.block_events = val,
            "entities_presence" | "entities" => self.entities_presence = val,
            "dropped_items" => self.dropped_items = val,
            "experience_orbs" => self.experience_orbs = val,
            "player_position" | "position" => self.player_position = val,
            "player_motion" | "motion" => self.player_motion = val,
            "player_health" | "health" => self.player_health = val,
            "player_hunger" | "hunger" => self.player_hunger = val,
            "player_experience" | "experience" | "xp" => self.player_experience = val,
            "player_inventory" | "inventory" => self.player_inventory = val,
            "player_ender_chest" | "ender_chest" => self.player_ender_chest = val,
            "player_potion_effects" | "effects" => self.player_potion_effects = val,
            "player_cooldowns" | "cooldowns" => self.player_cooldowns = val,
            "player_abilities" | "abilities" => self.player_abilities = val,
            "player_recipe_book" | "recipe_book" => self.player_recipe_book = val,
            "player_spawn_point" | "spawn_point" => self.player_spawn_point = val,
            "player_stats" | "stats" => self.player_stats = val,
            "preserve_new_player_inventory" => self.preserve_new_player_inventory = val,
            "world_time" | "time" => self.world_time = val,
            "world_weather" | "weather" => self.world_weather = val,
            "world_raids" | "raids" => self.world_raids = val,
            "world_dragon_fight" | "dragon_fight" => self.world_dragon_fight = val,
            "world_scoreboard" | "scoreboard" => self.world_scoreboard = val,
            "world_level_rng" | "level_rng" | "rng" => self.world_level_rng = val,
            "world_difficulty" | "difficulty" => self.world_difficulty = val,
            "world_spawn" | "spawn" => self.world_spawn = val,
            "server_bossbars" | "bossbars" => self.server_bossbars = val,
            "server_forced_chunks" | "forced_chunks" => self.server_forced_chunks = val,
            "server_world_border" | "world_border" | "border" => self.server_world_border = val,
            "server_game_rules" | "game_rules" | "rules" => self.server_game_rules = val,
            _ => return false,
        }
        true
    }
}


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


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    FlushSave = 0,
    HydrateSnapshot = 1,
    RestoreDiskFiles = 2,
    SaveDirStreaming = 3,
    PlayersOnline = 4,
    ServerGlobals = 5,
    Bosses = 6,
    FutureMca = 7,
    ChunkBlocks = 8,
    ChunkBeLoad = 9,
    SpawnNbt = 10,
    ClientSnapUpdate = 11,
    CapturePlayers = 12,
    CaptureGlobals = 13,
    CaptureRaids = 14,
    CaptureWorldState = 15,
    CaptureChunks = 16,
}

const PHASE_COUNT: usize = 17;

#[derive(Clone, Debug)]
pub struct BenchmarkData {
    pub open: bool,
    pub title: String,
    pub start_time: Option<Instant>,
    pub start_tick: i64,
    pub nanos: [u64; PHASE_COUNT],
    pub counts: [u32; PHASE_COUNT],
    pub blocks_changed: usize,
    pub chunks: usize,
    pub mobs: usize,
    pub players: usize,
    pub last_report: String,
}

impl Default for BenchmarkData {
    fn default() -> Self {
        Self {
            open: false,
            title: String::new(),
            start_time: None,
            start_tick: 0,
            nanos: [0; PHASE_COUNT],
            counts: [0; PHASE_COUNT],
            blocks_changed: 0,
            chunks: 0,
            mobs: 0,
            players: 0,
            last_report: String::new(),
        }
    }
}

std::thread_local! {
    static BENCHMARK: RefCell<BenchmarkData> = RefCell::new(BenchmarkData::default());
}

pub fn bench_begin(title: &str, current_tick: i64) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        b.open = true;
        b.title = title.to_owned();
        b.start_time = Some(Instant::now());
        b.start_tick = current_tick;
        b.nanos = [0; PHASE_COUNT];
        b.counts = [0; PHASE_COUNT];
        b.blocks_changed = 0;
        b.chunks = 0;
        b.mobs = 0;
        b.players = 0;
    });
}

pub fn bench_accum(phase: Phase, phase_start: Instant) {
    let elapsed = phase_start.elapsed().as_nanos() as u64;
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            let idx = phase as usize;
            if idx < PHASE_COUNT {
                b.nanos[idx] += elapsed;
                b.counts[idx] += 1;
            }
        }
    });
}

pub fn bench_add_blocks_changed(n: usize) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            b.blocks_changed += n;
        }
    });
}

pub fn bench_add_chunks(n: usize) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            b.chunks += n;
        }
    });
}

pub fn bench_add_mobs(n: usize) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            b.mobs += n;
        }
    });
}

pub fn bench_add_players(n: usize) {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.open {
            b.players += n;
        }
    });
}

pub fn bench_end_and_log() -> String {
    BENCHMARK.with(|cell| {
        let mut b = cell.borrow_mut();
        if !b.open {
            return b.last_report.clone();
        }
        b.open = false;

        let total_nanos = b.start_time.map_or(0, |t| t.elapsed().as_nanos() as u64);
        let is_rollback = b.title.starts_with("ROLLBACK");

        let top_level = if is_rollback {
            &[
                Phase::FlushSave,
                Phase::HydrateSnapshot,
                Phase::RestoreDiskFiles,
                Phase::SaveDirStreaming,
                Phase::PlayersOnline,
                Phase::ServerGlobals,
                Phase::Bosses,
                Phase::FutureMca,
                Phase::ChunkBlocks,
                Phase::ChunkBeLoad,
                Phase::SpawnNbt,
                Phase::ClientSnapUpdate,
            ][..]
        } else {
            &[
                Phase::CapturePlayers,
                Phase::CaptureGlobals,
                Phase::CaptureRaids,
                Phase::CaptureWorldState,
                Phase::CaptureChunks,
            ][..]
        };

        let work_nanos: u64 = top_level.iter().map(|&p| b.nanos[p as usize]).sum();

        let mut out = String::with_capacity(1024);
        out.push_str(&format!(
            "[RReturn][Benchmark] === {}: {:.2} ms (wall) | work {:.2} ms ===",
            b.title,
            total_nanos as f64 / 1_000_000.0,
            work_nanos as f64 / 1_000_000.0
        ));

        let nanos = b.nanos;
        let get_ms = |p: Phase| nanos[p as usize] as f64 / 1_000_000.0;
        let sum_ms = |phases: &[Phase]| -> f64 {
            phases
                .iter()
                .map(|&p| nanos[p as usize] as f64 / 1_000_000.0)
                .sum()
        };

        if is_rollback {
            let disk_init = sum_ms(&[
                Phase::FlushSave,
                Phase::HydrateSnapshot,
                Phase::RestoreDiskFiles,
                Phase::SaveDirStreaming,
            ]);
            if disk_init > 0.0 {
                out.push_str(&format!("\n|-- Disk & Snapshot Init: {:.2} ms", disk_init));
                let fs_ms = get_ms(Phase::FlushSave);
                if fs_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Flush & Save Live: {:.2} ms", fs_ms));
                }
                let hyd_ms = get_ms(Phase::HydrateSnapshot);
                if hyd_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Hydrate Snapshot: {:.2} ms", hyd_ms));
                }
                let disk_ms = get_ms(Phase::RestoreDiskFiles);
                if disk_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Restore Disk Files: {:.2} ms", disk_ms));
                }
                let st_ms = get_ms(Phase::SaveDirStreaming);
                if st_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- SaveDir & Streaming: {:.2} ms", st_ms));
                }
            }

            let players_work = get_ms(Phase::PlayersOnline);
            if players_work > 0.0 || b.players > 0 {
                out.push_str(&format!("\n|-- Players ({}): {:.2} ms", b.players, players_work));
            }

            let world = sum_ms(&[Phase::ServerGlobals, Phase::Bosses, Phase::FutureMca]);
            if world > 0.0 {
                out.push_str(&format!("\n|-- World & Globals: {:.2} ms", world));
                let sg = get_ms(Phase::ServerGlobals);
                if sg > 0.0 {
                    out.push_str(&format!("\n|   |-- ServerGlobals: {:.2} ms", sg));
                }
                let b_ms = get_ms(Phase::Bosses);
                if b_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Bosses & Sculk: {:.2} ms", b_ms));
                }
                let mca_ms = get_ms(Phase::FutureMca);
                if mca_ms > 0.0 {
                    out.push_str(&format!("\n|   |-- Clean Future MCA: {:.2} ms", mca_ms));
                }
            }

            let chunk_sum = sum_ms(&[Phase::ChunkBlocks, Phase::ChunkBeLoad]);
            if chunk_sum > 0.0 || b.chunks > 0 {
                out.push_str(&format!(
                    "\n|-- Chunks ({}, blocks changed {}): {:.2} ms",
                    b.chunks, b.blocks_changed, chunk_sum
                ));
                let cb = get_ms(Phase::ChunkBlocks);
                if cb > 0.0 {
                    out.push_str(&format!("\n|   |-- Blocks: {:.2} ms", cb));
                }
                let bel = get_ms(Phase::ChunkBeLoad);
                if bel > 0.0 {
                    out.push_str(&format!("\n|   |-- Block Entities: {:.2} ms", bel));
                }
            }

            let entity_sum = get_ms(Phase::SpawnNbt);
            if entity_sum > 0.0 || b.mobs > 0 {
                out.push_str(&format!("\n|-- Entities ({}): {:.2} ms", b.mobs, entity_sum));
            }

            let cs_ms = get_ms(Phase::ClientSnapUpdate);
            if cs_ms > 0.0 {
                out.push_str(&format!("\n|-- Client Snap Update: {:.2} ms", cs_ms));
            }
        } else {
            out.push_str(&format!(
                "\n|-- Globals: {:.2} ms",
                get_ms(Phase::CaptureGlobals)
            ));
            out.push_str(&format!(
                "\n|-- Raids & Dragon: {:.2} ms",
                get_ms(Phase::CaptureRaids)
            ));
            out.push_str(&format!(
                "\n|-- World State: {:.2} ms",
                get_ms(Phase::CaptureWorldState)
            ));
            out.push_str(&format!(
                "\n|-- Chunks ({}): {:.2} ms",
                b.chunks,
                get_ms(Phase::CaptureChunks)
            ));
        }

        b.last_report = out.clone();
        out
    })
}

pub fn append_to_rreturn_log(world_root: &std::path::Path, text: &str) {

    if let Some(instance_dir) = world_root.parent().and_then(|p| p.parent()) {
        let logs_dir = instance_dir.join("logs");
        if logs_dir.is_dir() {
            let log_file = logs_dir.join("rreturn.log");
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file) {
                let _ = writeln!(f, "{}", text);
            }
        }
    }

    let rreturn_dir = world_root.join("rreturn");
    let _ = std::fs::create_dir_all(&rreturn_dir);
    let world_log = rreturn_dir.join("benchmark.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(world_log) {
        let _ = writeln!(f, "{}", text);
    }
}


pub fn is_guest_safe(game: &Game<'_>, g: &horizon_client::lan::guest::Guest) -> bool {
    if !g.placed || g.dead || g.health <= 1.0 {
        return false;
    }
    if g.effects.list.iter().any(|e| e.id == "levitation") {
        return false;
    }
    let (fire_ticks, fall_distance) = match g.motion.as_deref() {
        Some(m) => (m.fire_ticks, m.fall_distance as f32),
        None => (0, 0.0),
    };
    if fire_ticks > 0 {
        return false;
    }
    let bx = g.position[0].floor() as i32;
    let by = g.position[1].floor() as i32;
    let bz = g.position[2].floor() as i32;
    let facts = game.res.facts();
    let here_fluid = facts.get(game.world.block(bx, by, bz)).fluid;
    if here_fluid == Fluid::Lava {
        return false;
    }
    let pos_vec = Vec3::new(g.position[0], g.position[1], g.position[2]);
    let imminent_tnt = game
        .tnts
        .iter()
        .any(|t| t.fuse <= 60 && t.position.distance_squared(pos_vec) <= 64.0);
    let imminent_creeper = game.mobs.iter().any(|m| {
        m.alive()
            && m.species() == Species::Creeper
            && (m.extra.counter > 0 || m.extra.flag)
            && m.position.distance_squared(pos_vec) <= 49.0
    });
    if imminent_tnt || imminent_creeper {
        return false;
    }
    if !g.on_ground && !g.abilities.flying && !g.creative() && !g.spectator() {
        let min_y = game.world.shape.min_y;
        let mut probe_y = by;
        let mut found_ground = false;
        let mut expected_damage = 0.0f32;
        for _ in 0..256 {
            probe_y -= 1;
            if probe_y < min_y {
                break;
            }
            let st = game.world.block(bx, probe_y, bz);
            let f = facts.get(st);
            if facts.is_air(st) && f.fluid == Fluid::None {
                continue;
            }
            if f.fluid == Fluid::Lava {
                return false;
            }
            found_ground = true;
            if f.fluid == Fluid::None {
                let total_fall = fall_distance + (g.position[1] as f32 - probe_y as f32);
                expected_damage = total_fall - 3.0;
            }
            break;
        }
        if !found_ground || expected_damage >= g.health {
            return false;
        }
    }
    true
}

pub fn pack_pos(x: i32, y: i32, z: i32) -> i64 {
    ((x as i64 & 0x3FF_FFFF) << 38) | ((z as i64 & 0x3FF_FFFF) << 12) | (y as i64 & 0xFFF)
}

pub fn unpack_pos(packed: i64) -> (i32, i32, i32) {
    let x = (packed >> 38) as i32;
    let z = ((packed << 26) >> 38) as i32;
    let y = ((packed << 52) >> 52) as i32;
    (x, y, z)
}

pub fn flood_escape_space(
    game: &Game<'_>,
    start: [i32; 3],
    blocked: Option<&HashSet<i64>>,
    allow_drops: bool,
) -> (usize, usize) {
    const MAX_VISITED: usize = 200;
    const MAX_DIST: i32 = 6;
    const DX: [i32; 4] = [0, 0, -1, 1];
    const DZ: [i32; 4] = [-1, 1, 0, 0];

    let min_dy = if allow_drops { -3 } else { -2 };
    let facts = game.res.facts();
    let mut visited = HashSet::with_capacity(MAX_VISITED);
    let mut queue = VecDeque::with_capacity(MAX_VISITED);
    let start_long = pack_pos(start[0], start[1], start[2]);
    visited.insert(start_long);
    queue.push_back(start_long);
    let mut cliffs = 0usize;

    while let Some(curr) = queue.pop_front() {
        if visited.len() >= MAX_VISITED {
            break;
        }
        let (cx, cy, cz) = unpack_pos(curr);
        for dir in 0..4 {
            let nx = cx + DX[dir];
            let nz = cz + DZ[dir];
            let mut target = None;
            let mut is_cliff = false;
            for dy in (min_dy..=1).rev() {
                let ny = cy + dy;
                let st_f = facts.get(game.world.block(nx, ny, nz));
                let fl_f = facts.get(game.world.block(nx, ny - 1, nz));
                let state_blocks = st_f.full_collision || st_f.legacy_solid;
                let floor_blocks = fl_f.full_collision || fl_f.legacy_solid;
                let floor_fluid = fl_f.fluid != Fluid::None;
                let floor_lava = fl_f.fluid == Fluid::Lava;
                let standable = if allow_drops {
                    floor_blocks || floor_fluid
                } else {
                    floor_blocks
                };
                if !state_blocks && standable {
                    target = Some((nx, ny, nz));
                    if allow_drops && (dy <= min_dy || floor_lava) {
                        is_cliff = true;
                    }
                    break;
                }
            }
            if is_cliff {
                cliffs += 1;
            }
            let Some((tx, ty, tz)) = target else {
                continue;
            };
            let t_long = pack_pos(tx, ty, tz);
            if visited.contains(&t_long) {
                continue;
            }
            if blocked.is_some_and(|b| b.contains(&t_long)) {
                continue;
            }
            let dist = (tx - start[0]).abs() + (ty - start[1]).abs() + (tz - start[2]).abs();
            if dist <= MAX_DIST {
                visited.insert(t_long);
                queue.push_back(t_long);
            }
        }
    }
    (visited.len(), cliffs)
}

pub fn calculate_guest_threat(game: &Game<'_>, g: &horizon_client::lan::guest::Guest) -> f64 {
    if !is_guest_safe(game, g) {
        return -1.0;
    }
    let hp_pct = (g.health / 20.0).clamp(0.0, 1.0);
    let mut env_threat = f64::from(1.0 - hp_pct) * 60.0;
    if g.food < 6 {
        env_threat += 15.0;
    }
    let air = g.motion.as_deref().map_or(300, |m| m.air);
    if air < 150 {
        let air_pct = air as f32 / 300.0;
        env_threat += f64::from(0.5 - air_pct) * 80.0;
    }
    if g.effects
        .list
        .iter()
        .any(|e| matches!(e.id, "poison" | "wither"))
    {
        env_threat += 40.0;
    }

    let p_vec = Vec3::new(g.position[0], g.position[1], g.position[2]);
    let mut mobs_tracking = 0;
    let mut mobs_attacking = 0;
    let mut mob_cells = HashSet::new();
    for m in &game.mobs {
        if !m.alive() || !m.hostile() {
            continue;
        }
        let d2 = m.position.distance_squared(p_vec);
        if d2 > 256.0 {
            continue;
        }
        if m.ai.target.is_some() || d2 < 16.0 {
            mobs_tracking += 1;
            if d2 < 9.0 || m.ai.aggressive {
                mobs_attacking += 1;
            }
            let mx = m.position.x.floor() as i32;
            let my = m.position.y.floor() as i32;
            let mz = m.position.z.floor() as i32;
            mob_cells.insert(pack_pos(mx, my, mz));
        }
    }

    let mob_threat = (f64::from(mobs_tracking) * 5.0 + f64::from(mobs_attacking) * 25.0).min(60.0);
    if mob_threat == 0.0 {
        env_threat *= 0.5;
    }
    let mut total_threat = (env_threat + mob_threat).min(100.0);
    if mobs_tracking > 0 {
        let start = [
            g.position[0].floor() as i32,
            g.position[1].floor() as i32,
            g.position[2].floor() as i32,
        ];
        let (free_space, cliffs) = flood_escape_space(game, start, None, true);
        let (safe_space, _) = flood_escape_space(game, start, Some(&mob_cells), false);
        if free_space > 0 {
            let cornered = 1.0 - (safe_space as f64 / free_space as f64);
            total_threat = (total_threat + cornered * 15.0 + (cliffs.min(4) as f64) * 2.5).min(100.0);
        }
    }
    total_threat
}

pub fn tick_autosave_on_server(game: &mut Game<'_>) {
    let now = game.world.game_time;

    let (auto_save, adaptive_mode, has_checkpoint) =
        with_state(|st| (st.auto_save, st.adaptive_mode, st.has_checkpoint));
    if auto_save && !has_checkpoint && !game.world.chunks.is_empty() {
        let initial_player = game.lan.as_ref().and_then(|h| {
            h.hub
                .guests
                .iter()
                .find(|g| g.placed && is_guest_safe(game, g))
                .map(|g| g.uuid)
        });
        if let Some(uuid) = initial_player {
            if create_checkpoint_on_server(game, Some(uuid)) {
                with_state_mut(|st| {
                    st.current_interval_ticks = if st.use_random_interval {
                        let min = st.random_min_sec.max(5);
                        let max = st.random_max_sec.max(min + 1);
                        let mut rng = LegacyRandom::new(now);
                        let extra = rng.next_int_bound((max - min) + 1).unwrap_or(0);
                        i64::from(min + extra) * 20
                    } else {
                        i64::from(st.fixed_interval_sec.max(5)) * 20
                    };
                });
                if with_state(|st| st.logs) {
                    log::info!("[RReturn] Initial checkpoint created on player join.");
                }
            }
            return;
        }
    }

    let boss_just_died = game.dragon.as_ref().is_some_and(|d| d.death_time == 1)
        || game.mobs.iter().any(|m| {
            m.species() == Species::Wither && !m.alive() && m.death_time == 1
        });
    if boss_just_died {
        let should_boss_save = with_state_mut(|st| {
            if st.save_on_boss_defeat && now - st.last_boss_death_tick > 100 {
                st.last_boss_death_tick = now;
                true
            } else {
                false
            }
        });
        if should_boss_save {
            let safe = game
                .lan
                .as_ref()
                .is_some_and(|h| h.hub.guests.iter().any(|g| is_guest_safe(game, g)));
            if safe {
                if create_checkpoint_on_server(game, None) && with_state(|st| st.logs) {
                    log::info!("[RReturn] Boss defeat checkpoint created.");
                }
                return;
            }
        }
    }

    if !auto_save {
        return;
    }

    if !adaptive_mode {
        let due = with_state(|st| now - st.last_save_tick >= st.current_interval_ticks.max(100));
        if !due {
            return;
        }
        let all_anchors_safe = game.lan.as_ref().is_some_and(|host| {
            let online: Vec<u128> = host
                .hub
                .guests
                .iter()
                .filter(|g| g.placed)
                .map(|g| g.uuid)
                .collect();
            let anchors = with_state(|st| resolve_anchors(&st.anchor, &online, now, None));
            let mut count = 0usize;
            for g in host
                .hub
                .guests
                .iter()
                .filter(|g| g.placed && anchors.contains(&g.uuid))
            {
                count += 1;
                if !is_guest_safe(game, g) {
                    return false;
                }
            }
            count > 0
        });
        if all_anchors_safe && create_checkpoint_on_server(game, None) {
            with_state_mut(|st| {
                st.current_interval_ticks = if st.use_random_interval {
                    let min = st.random_min_sec.max(5);
                    let max = st.random_max_sec.max(min + 1);
                    let mut rng = LegacyRandom::new(now);
                    let extra = rng.next_int_bound((max - min) + 1).unwrap_or(0);
                    i64::from(min + extra) * 20
                } else {
                    i64::from(st.fixed_interval_sec.max(5)) * 20
                };
            });
            if with_state(|st| st.logs) {
                log::info!("[RReturn] Auto-saved checkpoint.");
            }
        }
        return;
    }

    let Some(host) = &game.lan else {
        return;
    };
    let online: Vec<u128> = host
        .hub
        .guests
        .iter()
        .filter(|g| g.placed)
        .map(|g| g.uuid)
        .collect();
    let anchors = with_state(|st| resolve_anchors(&st.anchor, &online, now, None));
    let mut max_threat = 0.0f64;
    let mut any_anchor_here = false;
    for g in host
        .hub
        .guests
        .iter()
        .filter(|g| g.placed && anchors.contains(&g.uuid) && g.dimension == game.dim.id())
    {
        any_anchor_here = true;
        let threat = calculate_guest_threat(game, g);
        if threat < 0.0 {
            with_state_mut(|st| st.relax_ticks = 0);
            return;
        }
        if threat > max_threat {
            max_threat = threat;
        }
    }
    if !any_anchor_here {
        return;
    }

    let mut rng = LegacyRandom::new(now ^ 0x5DEECE66D);
    let rand_f = rng.next_float();
    let should_save = with_state_mut(|st| {
        let since_last = now - st.last_save_tick;
        if max_threat > st.current_intensity {
            st.current_intensity += (max_threat - st.current_intensity) * 0.2;
            st.relax_ticks = 0;
        } else {
            st.current_intensity -= (st.current_intensity - max_threat) * 0.02;
        }
        if st.current_intensity > st.peak_intensity {
            st.peak_intensity = st.current_intensity;
        }

        let relax_req = (st.relax_time_sec * 20).max(20);
        let min_gap = (i64::from(st.min_save_gap_sec) * 20).max(100);
        let chance = st.post_combat_save_pct as f32 / 100.0;

        let mut trigger = false;
        if st.peak_intensity > 35.0 {
            if st.current_intensity < st.peak_intensity * 0.5 && max_threat <= 20.0 {
                st.relax_ticks += 1;
                if st.relax_ticks > relax_req {
                    if rand_f < chance {
                        trigger = true;
                    } else {
                        st.peak_intensity = 0.0;
                        st.current_intensity = 0.0;
                        st.relax_ticks = 0;
                    }
                }
            } else {
                st.relax_ticks = 0;
            }
        } else {
            let tranquil_ticks = i64::from(st.adaptive_interval_sec.max(60)) * 20;
            if since_last >= tranquil_ticks && max_threat <= 10.0 {
                trigger = true;
            }
        }
        trigger && since_last > min_gap
    });

    if should_save && create_checkpoint_on_server(game, None) && with_state(|st| st.logs) {
        log::info!("[RReturn] Adaptive checkpoint created.");
    }
}


pub fn capture_chunk_ticks(
    chunks: &FxHashMap<(i32, i32), Arc<Chunk>>,
    ticks: &Ticks,
) -> HashMap<(i32, i32), (ChunkTicks, ChunkTicks)> {
    let mut out = HashMap::new();
    for &(cx, cz) in chunks.keys() {
        let cpos = ChunkPos::new(cx, cz);
        let b = ticks.blocks.chunk(cpos).cloned().unwrap_or_default();
        let f = ticks.fluids.chunk(cpos).cloned().unwrap_or_default();
        out.insert((cx, cz), (b, f));
    }
    out
}

pub fn try_backup_offline_files(world_root: &Path) -> std::io::Result<HashMap<PathBuf, Vec<u8>>> {
    let mut map = HashMap::new();
    fn walk_dir(dir: &Path, map: &mut HashMap<PathBuf, Vec<u8>>) -> std::io::Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                walk_dir(&p, map)?;
            } else if p.is_file() {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "dat" || ext == "json" {
                    let bytes = fs::read(&p)?;
                    map.insert(p, bytes);
                }
            }
        }
        Ok(())
    }
    walk_dir(&world_root.join("data"), &mut map)?;
    walk_dir(&world_root.join("dimensions"), &mut map)?;
    walk_dir(&world_root.join("players"), &mut map)?;
    walk_dir(&world_root.join("generated"), &mut map)?;
    Ok(map)
}

pub fn backup_offline_files(world_root: &Path) -> HashMap<PathBuf, Vec<u8>> {
    try_backup_offline_files(world_root).unwrap_or_default()
}

pub fn capture_level_snapshot(game: &Game<'_>, dim: Dim) -> LevelSnapshot {
    let raids_dat = fs::read(raids_dat_path(&game.world_root, dim)).ok();
    let chunks_map: FxHashMap<(i32, i32), Arc<Chunk>> = (&*game.world.chunks).clone();
    let mut be_attachments = Vec::new();
    for &(cx, cz) in chunks_map.keys() {
        be_attachments.extend(game.world.block_entities.chunk_attachments(cx, cz));
    }
    let chunk_ticks = capture_chunk_ticks(&chunks_map, &game.block_ticks);
    let registry = game.pack.assets.registry;
    let tridents = game
        .tridents
        .iter()
        .map(|t| {
            let uuid = game.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&t.id);
            TridentSnapshot {
                join: t.join,
                id: t.id,
                nbt: t.to_nbt(uuid, registry, raw),
                previous: t.previous,
                in_ground_time: t.in_ground_time,
                no_physics: t.no_physics,
                owner: t.owner,
                guest: t.guest,
            }
        })
        .collect();
    let rockets = game
        .rockets
        .iter()
        .map(|r| {
            let uuid = game.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&r.id);
            RocketSnapshot {
                id: r.id,
                nbt: r.to_nbt(uuid, None, raw),
                attached: r.attached,
                guest: r.guest,
                owner: r.owner,
                attached_to: r.attached_to,
            }
        })
        .collect();
    let breath_clouds = game
        .breath_clouds
        .iter()
        .map(|c| {
            let uuid = game.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&c.id);
            BreathSnapshot {
                id: c.id,
                nbt: super::super::potions::potion_cloud_to_nbt(c, uuid, raw),
                at: c.at,
                motion: c.motion,
                radius: c.radius,
                age: c.age,
                duration: c.duration,
                from_dragon: c.from_dragon,
                potion_none: c.potion.is_none(),
                victims: c.victims.clone(),
            }
        })
        .collect();
    LevelSnapshot {
        chunks: chunks_map,
        block_entities: (&*game.world.block_entities).clone(),
        block_names: game.world.block_names.clone(),
        be_attachments,
        world_attachments: game.world.attachments.clone(),
        chest_openers: game
            .world
            .chest_openers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect(),
        border: game.border.clone(),
        shulker_anims: game
            .be_anims
            .shulkers
            .iter()
            .map(|(&k, v)| (k, v.clone()))
            .collect(),
        gateway_anims: game
            .be_anims
            .gateway_servers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect(),
        fangs: game.fangs.list.clone(),
        ominous_spawners: game
            .ominous_spawners
            .iter()
            .map(|s| {
                (
                    s.id,
                    horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                        &s.item,
                        Vec3::new(s.position[0], s.position[1], s.position[2]),
                        s.spawn_item_after,
                        s.uuid,
                        s.raw.as_deref(),
                    ),
                )
            })
            .collect(),
        portals_entities: game.portals.entities.clone(),
        lectern_pulses: game.lectern_pulses.clone(),
        lid_openers: game.lid_openers.clone(),
        inhabited: game.world.inhabited.clone(),
        end_clock: game.world.end_clock,
        random: game.world.random.clone(),
        forced_chunks: game.streaming.forced.clone(),
        chunk_ticks,
        random_tick_value: game.block_ticks.random_tick_value,
        comparator_outputs: game.block_ticks.comparator_outputs.clone(),
        torch_toggles: game.block_ticks.torch_toggles.clone(),
        moving_pistons: game.block_ticks.moving_pistons.clone(),
        piston_pushes: game.block_ticks.piston_pushes.clone(),
        block_events: game.block_ticks.block_events.clone(),
        hoppers: game.hoppers.clone(),
        mobs: game.mobs.clone(),
        dropped: game.dropped.clone(),
        orbs: game.orbs.clone(),
        projectiles: game.creatures.projectiles.clone(),
        boats: game.boats.clone(),
        carts: game.carts.clone(),
        tnts: game.tnts.clone(),
        falling: game.falling.clone(),
        paintings: game.paintings.clone(),
        item_frames: game.item_frames.clone(),
        armor_stands: game.armor_stands.clone(),
        cushions: game.cushions.clone(),
        others: game.others.clone(),
        crystals: game.crystals.clone(),
        dragon: game.dragon.as_ref().map(DragonSnapshot::from_live),
        tridents,
        rockets,
        breath_clouds,
        knot_ids: game.knot_ids.clone(),
        persist_uuids: (*game.persist.uuids).clone(),
        persist_raw: game.persist.raw.clone(),
        persist_passenger_slots: game.persist.passenger_slots.clone(),
        raids_dat,
        last_hurt_by_player: game.last_hurt_by_player.clone(),
        love_cause: game.love_cause.clone(),
    }
}

pub fn capture_parked_snapshot(
    game: &Game<'_>,
    dim: Dim,
    p: &crate::game::levels::Parked,
) -> LevelSnapshot {
    let raids_dat = fs::read(raids_dat_path(&game.world_root, dim)).ok();
    let chunks_map: FxHashMap<(i32, i32), Arc<Chunk>> = (&*p.world.chunks).clone();
    let mut be_attachments = Vec::new();
    for &(cx, cz) in chunks_map.keys() {
        be_attachments.extend(p.world.block_entities.chunk_attachments(cx, cz));
    }
    let chunk_ticks = capture_chunk_ticks(&chunks_map, &p.block_ticks);
    let registry = game.pack.assets.registry;
    let tridents = p
        .tridents
        .iter()
        .map(|t| {
            let uuid = p.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
            let raw = p.persist.raw.get(&t.id);
            TridentSnapshot {
                join: t.join,
                id: t.id,
                nbt: t.to_nbt(uuid, registry, raw),
                previous: t.previous,
                in_ground_time: t.in_ground_time,
                no_physics: t.no_physics,
                owner: t.owner,
                guest: t.guest,
            }
        })
        .collect();
    let rockets = p
        .rockets
        .iter()
        .map(|r| {
            let uuid = p.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
            let raw = p.persist.raw.get(&r.id);
            RocketSnapshot {
                id: r.id,
                nbt: r.to_nbt(uuid, None, raw),
                attached: r.attached,
                guest: r.guest,
                owner: r.owner,
                attached_to: r.attached_to,
            }
        })
        .collect();
    let breath_clouds = p
        .breath_clouds
        .iter()
        .map(|c| {
            let uuid = p.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
            let raw = p.persist.raw.get(&c.id);
            BreathSnapshot {
                id: c.id,
                nbt: super::super::potions::potion_cloud_to_nbt(c, uuid, raw),
                at: c.at,
                motion: c.motion,
                radius: c.radius,
                age: c.age,
                duration: c.duration,
                from_dragon: c.from_dragon,
                potion_none: c.potion.is_none(),
                victims: c.victims.clone(),
            }
        })
        .collect();
    LevelSnapshot {
        chunks: chunks_map,
        block_entities: (&*p.world.block_entities).clone(),
        block_names: p.world.block_names.clone(),
        be_attachments,
        world_attachments: p.world.attachments.clone(),
        chest_openers: p
            .world
            .chest_openers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect(),
        border: p.border.clone(),
        shulker_anims: p
            .anims
            .shulkers
            .iter()
            .map(|(&k, v)| (k, v.clone()))
            .collect(),
        gateway_anims: p
            .anims
            .gateway_servers
            .iter()
            .map(|(&k, &v)| (k, v))
            .collect(),
        fangs: p.fangs.list.clone(),
        ominous_spawners: p
            .ominous_spawners
            .iter()
            .map(|s| {
                (
                    s.id,
                    horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                        &s.item,
                        Vec3::new(s.position[0], s.position[1], s.position[2]),
                        s.spawn_item_after,
                        s.uuid,
                        s.raw.as_deref(),
                    ),
                )
            })
            .collect(),
        portals_entities: p.portal_states.clone(),
        lectern_pulses: p.lectern_pulses.clone(),
        lid_openers: p.lid_openers.clone(),
        inhabited: p.world.inhabited.clone(),
        end_clock: p.world.end_clock,
        random: p.world.random.clone(),
        forced_chunks: p.streaming.forced.clone(),
        chunk_ticks,
        random_tick_value: p.block_ticks.random_tick_value,
        comparator_outputs: p.block_ticks.comparator_outputs.clone(),
        torch_toggles: p.block_ticks.torch_toggles.clone(),
        moving_pistons: p.block_ticks.moving_pistons.clone(),
        piston_pushes: p.block_ticks.piston_pushes.clone(),
        block_events: p.block_ticks.block_events.clone(),
        hoppers: p.hoppers.clone(),
        mobs: p.mobs.clone(),
        dropped: p.dropped.clone(),
        orbs: p.orbs.clone(),
        projectiles: p.projectiles.clone(),
        boats: p.boats.clone(),
        carts: p.carts.clone(),
        tnts: p.tnts.clone(),
        falling: p.falling.clone(),
        paintings: p.paintings.clone(),
        item_frames: p.item_frames.clone(),
        armor_stands: p.armor_stands.clone(),
        cushions: p.cushions.clone(),
        others: p.others.clone(),
        crystals: p.crystals.clone(),
        dragon: p.dragon.as_ref().map(DragonSnapshot::from_live),
        tridents,
        rockets,
        breath_clouds,
        knot_ids: p.knot_ids.clone(),
        persist_uuids: (*p.persist.uuids).clone(),
        persist_raw: p.persist.raw.clone(),
        persist_passenger_slots: p.persist.passenger_slots.clone(),
        raids_dat,
        last_hurt_by_player: HashMap::new(),
        love_cause: HashMap::new(),
    }
}

pub fn create_checkpoint_on_server(game: &mut Game<'_>, requester: Option<u128>) -> bool {
    create_slot_checkpoint_on_server(game, requester, "default")
}

pub fn create_slot_checkpoint_on_server(
    game: &mut Game<'_>,
    requester: Option<u128>,
    slot: &str,
) -> bool {
    if !is_valid_slot_name(slot.trim()) {
        return false;
    }
    let now = game.world.game_time;
    bench_begin(&format!("CHECKPOINT CAPTURE [{}]", slot), now);

    let t_raids = std::time::Instant::now();
    let open_cart_keys: Vec<(i32, i32, i32)> = game
        .world
        .block_entities
        .keys()
        .copied()
        .filter(|k| k.1 == super::super::minecarts::CART_KEY_Y)
        .collect();
    for key in open_cart_keys {
        game.store_cart_items(key);
    }
    game.save_world_state();
    game.score.storage.save(&game.world_root);
    game.score.stopwatches.save(&game.world_root);
    game.save_scoreboard();
    game.save_bossbars();
    game.save_schedule();
    game.save_maps();
    game.save_portal_arrivals();
    game.raiding.raids.dirty = true;
    game.save_raids();
    game.save_fight();
    game.save_wandering_trader();
    if game.save_game_with(true).is_err()
        || game.save_entities(true).is_err()
        || game.save.flush().is_err()
    {
        let _ = bench_end_and_log();
        return false;
    }
    for parked in game.levels.parked.values() {
        if parked.save.flush().is_err() {
            let _ = bench_end_and_log();
            return false;
        }
    }
    bench_accum(Phase::CaptureRaids, t_raids);

    let t_players = std::time::Instant::now();
    let mut guests = HashMap::new();
    let mut online = Vec::new();
    if let Some(host) = &game.lan {
        for g in host.hub.guests.iter().filter(|g| g.placed) {
            online.push(g.uuid);
            let motion = g.motion.as_deref();
            let (
                velocity,
                absorption,
                exhaustion,
                food_timer,
                fire_ticks,
                air,
                fall_distance,
                ticks_frozen,
                ground,
            ) = match motion {
                Some(m) => (
                    m.velocity,
                    m.health.absorption(),
                    m.health.food.exhaustion,
                    m.health.food.timer,
                    m.fire_ticks,
                    m.air,
                    m.fall_distance,
                    m.ticks_frozen,
                    m.ground,
                ),
                None => {
                    let body = g.loaded_body.unwrap_or(SavedBody {
                        invulnerable: false,
                        invulnerable_time: 0,
                        fire_ticks: -20,
                        air: 300,
                        fall_distance: 0.0,
                        velocity: [0.0, 0.0, 0.0],
                        food_exhaustion: 0.0,
                        food_tick_timer: 0,
                        ticks_frozen: 0,
                    });
                    (
                        Vec3::new(body.velocity[0], body.velocity[1], body.velocity[2]),
                        g.saved_absorption.unwrap_or(0.0),
                        body.food_exhaustion,
                        body.food_tick_timer,
                        body.fire_ticks,
                        body.air,
                        body.fall_distance,
                        body.ticks_frozen,
                        GroundState {
                            on_ground: g.on_ground,
                            ..Default::default()
                        },
                    )
                }
            };
            let is_owner = host.owner.as_ref().is_some_and(|o| o.uuid == g.uuid)
                || host.hub.guests.len() == 1;
            let (shield_cd, cooldowns, stats, hook) = match host.play.get(&g.conn) {
                Some(p) => (
                    p.shield_cooldown,
                    p.cooldowns.clone(),
                    p.stats.values.clone(),
                    p.hook.as_ref().map(|h| {
                        let (nibble, time_lured, time_hooked, fish_angle, open_water, bobbing) =
                            h.rreturn_fishing_state();
                        HookSnapshot {
                            id: h.id(),
                            position: [h.position.x, h.position.y, h.position.z],
                            previous: [h.previous.x, h.previous.y, h.previous.z],
                            velocity: h.rreturn_velocity(),
                            rng_state: h.random.state(),
                            nibble,
                            time_until_lured: time_lured,
                            time_until_hooked: time_hooked,
                            fish_angle,
                            open_water,
                            state_bobbing: bobbing,
                        }
                    }),
                ),
                None => (0, Cooldowns::default(), BTreeMap::new(), None),
            };
            let shield_cooldown = if is_owner && shield_cd == 0 {
                game.shield_cooldown
            } else {
                shield_cd
            };
            let hook = hook.or_else(|| {
                if is_owner {
                    game.hook.as_ref().map(|h| {
                        let (nibble, time_lured, time_hooked, fish_angle, open_water, bobbing) =
                            h.rreturn_fishing_state();
                        HookSnapshot {
                            id: h.id(),
                            position: [h.position.x, h.position.y, h.position.z],
                            previous: [h.previous.x, h.previous.y, h.previous.z],
                            velocity: h.rreturn_velocity(),
                            rng_state: h.random.state(),
                            nibble,
                            time_until_lured: time_lured,
                            time_until_hooked: time_hooked,
                            fish_angle,
                            open_water,
                            state_bobbing: bobbing,
                        }
                    })
                } else {
                    None
                }
            });
            let (effects, effects_tick_count, effects_changes) =
                if !g.effects.list.is_empty() || !is_owner || game.potions.effects.list.is_empty() {
                    (
                        g.effects.list.clone(),
                        g.effects.tick_count,
                        g.effects.changes,
                    )
                } else {
                    (
                        game.potions.effects.list.clone(),
                        game.potions.effects.tick_count,
                        game.potions.effects.changes,
                    )
                };
            guests.insert(
                g.uuid,
                GuestSnapshot {
                    uuid: g.uuid,
                    dimension: g.dimension.clone(),
                    position: g.position,
                    velocity,
                    yaw: g.yaw,
                    pitch: g.pitch,
                    head_yaw: g.head_yaw,
                    on_ground: g.on_ground,
                    ground,
                    game_mode: g.game_mode,
                    previous_game_mode: g.previous_game_mode,
                    flying: g.abilities.flying,
                    flying_speed: g.abilities.flying_speed,
                    allow_flight: g.abilities.may_fly,
                    sprinting: g.sprinting,
                    sneaking: g.sneaking,
                    fall_flying: g.fall_flying,
                    sleeping: g.sleeping,
                    shoulders: g.shoulders,
                    living_flags: g.living_flags,
                    stuck: g.stuck,
                    bed: g.bed,
                    sleep_timer: g.sleep_timer,
                    vehicle: g.vehicle,
                    vehicle_order: g.vehicle_order,
                    vehicle_good: g.vehicle_good,
                    spawn: g.spawn,
                    respawn_elsewhere: g.respawn_elsewhere.clone(),
                    last_death: g.last_death.clone(),
                    health: g.health,
                    absorption,
                    food: g.food,
                    saturation: g.saturation,
                    exhaustion,
                    food_timer,
                    fire_ticks,
                    air,
                    ticks_frozen,
                    fall_distance,
                    xp_level: g.xp_level,
                    xp_progress: g.xp_progress,
                    xp_total: g.xp_total,
                    xp_seed: g.xp_seed,
                    inventory: g.inventory.clone(),
                    armor: g.armor.clone(),
                    offhand: g.offhand.clone(),
                    carried: g.carried.clone(),
                    ender_items: g.ender_items.clone(),
                    selected: g.selected,
                    craft_grid: g.craft_grid.clone(),
                    craft_result: g.craft_result.clone(),
                    effects,
                    effects_tick_count,
                    effects_changes,
                    shield_cooldown,
                    cooldowns,
                    recipe_book: g.recipe_book.clone(),
                    advancements: g.advancements.clone(),
                    stats,
                    seen_credits: g.seen_credits,
                    post_effects: g.post_effects.clone(),
                    attachments: g.attachments.clone(),
                    saved: g.saved.clone(),
                    hook,
                    motion: motion.cloned(),
                },
            );
        }
    }

    bench_add_players(online.len());
    bench_accum(Phase::CapturePlayers, t_players);

    let t_chunks = std::time::Instant::now();
    let mut levels = HashMap::new();
    levels.insert(game.dim, capture_level_snapshot(game, game.dim));
    for (&dim, parked) in &game.levels.parked {
        levels.insert(dim, capture_parked_snapshot(game, dim, parked));
    }
    let total_chunks: usize = levels.values().map(|l| l.chunks.len()).sum();
    let total_mobs: usize = levels.values().map(|l| l.mobs.len()).sum();
    bench_add_chunks(total_chunks);
    bench_add_mobs(total_mobs);
    bench_accum(Phase::CaptureChunks, t_chunks);

    let t_globals = std::time::Instant::now();
    let (anchor_set, policy) = with_state(|st| {
        let anchors: HashSet<u128> = resolve_anchors(&st.anchor, &online, now, requester)
            .into_iter()
            .collect();
        (anchors, st.policy)
    });

    let dragon_fight_dat = fs::read(fight_dat_path(&game.world_root)).ok();
    let trader_dat = fs::read(trader_dat_path(&game.world_root)).ok();
    let Ok(offline_player_data) = try_backup_offline_files(&game.world_root) else {
        let _ = bench_end_and_log();
        return false;
    };

    let mut region_files = HashSet::new();
    let subfolders = ["region", "entities", "poi"];
    let dims = [
        "dimensions/minecraft/overworld",
        "dimensions/minecraft/the_nether",
        "dimensions/minecraft/the_end",
    ];
    for dim_path in dims {
        for sub in subfolders {
            let rdir = game.world_root.join(dim_path).join(sub);
            if !rdir.exists() {
                continue;
            }
            let Ok(entries) = fs::read_dir(rdir) else {
                let _ = bench_end_and_log();
                return false;
            };
            for entry in entries {
                let Ok(e) = entry else {
                    let _ = bench_end_and_log();
                    return false;
                };
                let p = e.path();
                if p.is_file() {
                    region_files.insert(p);
                }
            }
        }
    }

    let timeline = Arc::new(TimelineSnapshot {
        anchor_ids: anchor_set.clone(),
        policy,
        game_time: game.world.game_time,
        day_time: game.world.day_time,
        weather: game.world.weather,
        health_rules: game.world.health_rules,
        rules: game.ui.rules.clone(),
        border: game.border.clone(),
        bossbars: game.bosses.bars.pack(),
        sculk_tracker: game.sculk.tracker,
        sculk_darkness: game.sculk.darkness,
        dragon_fight_dat,
        fight_tag: Some(game.fight.rreturn_pack()),
        trader_dat,
        rand_sequences: game.rand_sequences.clone(),
        region_files,
        difficulty: game.ui.difficulty,
        difficulty_locked: game.level.difficulty_locked,
        clock_rate: game.clock.rate(),
        clock_frozen: game.clock.frozen(),
        time_rate: game.ui.clock.rate,
        time_partial: game.ui.clock.partial,
        time_paused: game.ui.clock.paused,
        command_clocks: game.ui.command_clocks.clone(),
        world_spawn: game.level.world_spawn,
        world_spawn_angle: game.level.world_spawn_angle,
        next_entity: game.next_entity,
        ticked_entities_through: game.ticked_entities_through,
        ordered_announced: game.ordered.announced,
        cat_tick: game.creatures.cat_tick,
        phantom_tick: game.stats.rreturn_phantom_tick(),
        player_extra: game.level.player_extra.clone(),
        shoulder_time: game.creatures.shoulder_time.clone(),
        fangs: game.fangs.list.clone(),
        ominous_spawners: game
            .ominous_spawners
            .iter()
            .map(|s| {
                (
                    s.id,
                    horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                        &s.item,
                        Vec3::new(s.position[0], s.position[1], s.position[2]),
                        s.spawn_item_after,
                        s.uuid,
                        s.raw.as_deref(),
                    ),
                )
            })
            .collect(),
        portals_entities: game.portals.entities.clone(),
        end_gateway_cooldowns: game.end.rreturn_gateway_cooldowns(),
        lectern_pulses: game.lectern_pulses.clone(),
        player_beds: game.player_beds.clone(),
        sleep_seen: game.sleep_seen,
        lead_killers: game.lead_killers.clone(),
        lid_openers: game.lid_openers.clone(),
        entity_attachments: game.entity_attachments.clone(),
        player_attachments: game.player_state.attachments.clone(),
        guest_attachments: game.lan.as_ref().map(|host| {
            host.hub.guests.iter().map(|g| (g.uuid, (*g.attachments).clone())).collect()
        }).unwrap_or_default(),
        scheduled_events: game.fns.schedule.clone(),
        scoreboard: game.score.board.clone(),
        sound_seed_state: None,
        cmd_post_effects: game.cmds.post_effects.clone(),
        guests,
        offline_player_data,
        levels,
        mob_players: game.lan.as_ref().map(|h| h.mob_players.clone()).unwrap_or_default(),
        disk_restored: true,
        client_using: game.using,
        client_use_duration: game.use_duration,
        client_use_ticks_set: false,
        client_crossbow_sounds: game.crossbow_sounds,
    });

    bench_accum(Phase::CaptureGlobals, t_globals);

    let t_world_state = std::time::Instant::now();
    if !save_slot_to_disk(&game.world_root, slot, &timeline) {
        let _ = bench_end_and_log();
        return false;
    }

    with_state_mut(|st| {
        st.anchor_ids = anchor_set;
        st.has_checkpoint = true;
        st.last_save_tick = now;
        st.current_intensity = 0.0;
        st.peak_intensity = 0.0;
        st.relax_ticks = 0;
    });

    set_slot_snapshot(slot, timeline);

    let client_snap = ClientBlockSnapshot {
        dim: game.dim,
        chunks: (&*game.world.chunks).clone(),
    };
    set_client_slot_snapshot(slot, client_snap);

    if let Ok(mut tr) = loop_tracker().write() {
        tr.on_checkpoint(game.world.game_time);
    }
    bench_accum(Phase::CaptureWorldState, t_world_state);

    let bench_report = bench_end_and_log();
    log::info!("{}", bench_report);
    append_to_rreturn_log(&game.world_root, &bench_report);
    true
}


pub fn slot_dir(world_root: &Path, slot: &str) -> PathBuf {
    let clean = slot.trim();
    if clean.is_empty() || clean == "default" || !is_valid_slot_name(clean) {
        world_root.join("rreturn").join("checkpoint")
    } else {
        world_root.join("rreturn").join("slots").join(clean)
    }
}

pub fn checkpoint_dir(world_root: &Path) -> PathBuf {
    slot_dir(world_root, "default")
}

pub fn has_disk_checkpoint(world_root: &Path) -> bool {
    has_disk_slot(world_root, "default")
}

pub fn has_disk_slot(world_root: &Path, slot: &str) -> bool {
    slot_dir(world_root, slot).join("meta.json").is_file()
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> usize {
    copy_dir_recursive_opts(src, dst, false)
}

pub fn copy_dir_recursive_opts(src: &Path, dst: &Path, force: bool) -> usize {
    if !src.is_dir() {
        return 0;
    }
    let mut copied = 0;
    let _ = fs::create_dir_all(dst);
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let p = entry.path();
            let target = dst.join(entry.file_name());
            if p.is_dir() {
                copied += copy_dir_recursive_opts(&p, &target, force);
            } else if p.is_file() {
                if !force {
                    if let (Ok(m_src), Ok(m_dst)) = (p.metadata(), target.metadata()) {
                        if m_src.len() == m_dst.len() {
                            if let (Ok(t_src), Ok(t_dst)) = (m_src.modified(), m_dst.modified()) {
                                if t_src == t_dst {
                                    continue;
                                }
                            }
                        }
                    }
                }
                if fs::copy(&p, &target).is_ok() {
                    copied += 1;
                    if let (Ok(m), Ok(f)) =
                        (p.metadata(), fs::File::options().write(true).open(&target))
                    {
                        if let Ok(mtime) = m.modified() {
                            let _ = f.set_times(std::fs::FileTimes::new().set_modified(mtime));
                        }
                    }
                }
            }
        }
    }
    copied
}

pub fn collect_files_recursive(
    dir: &Path,
    map: &mut HashMap<PathBuf, Vec<u8>>,
    target_base: &Path,
    rel_prefix: &Path,
) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let fname = entry.file_name();
            let next_rel = rel_prefix.join(fname);
            if p.is_dir() {
                collect_files_recursive(&p, map, target_base, &next_rel);
            } else if p.is_file() {
                if let Ok(bytes) = fs::read(&p) {
                    map.insert(target_base.join(&next_rel), bytes);
                }
            }
        }
    }
}

pub fn try_copy_file_preserve_mtime(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::copy(src, dst)?;
    if let (Ok(m), Ok(f)) = (src.metadata(), fs::File::options().write(true).open(dst)) {
        if let Ok(mtime) = m.modified() {
            let _ = f.set_times(std::fs::FileTimes::new().set_modified(mtime));
        }
    }
    Ok(())
}

pub fn try_copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<usize> {
    if !src.exists() {
        return Ok(0);
    }
    let mut copied = 0;
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let p = entry.path();
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copied += try_copy_dir_recursive(&p, &target)?;
        } else if p.is_file() {
            try_copy_file_preserve_mtime(&p, &target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

pub fn prune_extra_dat_json_recursive(src: &Path, dst: &Path) -> usize {
    if !dst.is_dir() {
        return 0;
    }
    let mut removed = 0;
    if let Ok(entries) = fs::read_dir(dst) {
        for entry in entries.flatten() {
            let p = entry.path();
            let src_entry = src.join(entry.file_name());
            if p.is_dir() {
                removed += prune_extra_dat_json_recursive(&src_entry, &p);
            } else if p.is_file() {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if (ext == "dat" || ext == "json")
                    && !src_entry.exists()
                    && fs::remove_file(&p).is_ok()
                {
                    removed += 1;
                }
            }
        }
    }
    removed
}

pub fn prune_extra_nbt_recursive(src: &Path, dst: &Path) -> usize {
    if !dst.is_dir() {
        return 0;
    }
    let mut removed = 0;
    if let Ok(entries) = fs::read_dir(dst) {
        for entry in entries.flatten() {
            let p = entry.path();
            let src_entry = src.join(entry.file_name());
            if p.is_dir() {
                removed += prune_extra_nbt_recursive(&src_entry, &p);
            } else if p.is_file() {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "nbt" && !src_entry.exists() && fs::remove_file(&p).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    removed
}

pub fn restore_disk_files_for_slot(world_root: &Path, slot: &str) -> usize {
    restore_disk_files_for_slot_opts(world_root, slot, false)
}

pub fn restore_disk_files_for_slot_opts(world_root: &Path, slot: &str, force: bool) -> usize {
    if !is_valid_slot_name(slot.trim()) {
        return 0;
    }
    let cp_dir = slot_dir(world_root, slot);
    if !cp_dir.join("meta.json").is_file() {
        return 0;
    }
    let mut changed = 0;
    let cp_level = cp_dir.join("level.dat");
    let target_level = world_root.join("level.dat");
    if cp_level.is_file() {
        let mut skip = false;
        if !force {
            if let (Ok(m_src), Ok(m_dst)) = (cp_level.metadata(), target_level.metadata()) {
                if m_src.len() == m_dst.len() {
                    if let (Ok(t_src), Ok(t_dst)) = (m_src.modified(), m_dst.modified()) {
                        if t_src == t_dst {
                            skip = true;
                        }
                    }
                }
            }
        }
        if !skip && fs::copy(&cp_level, &target_level).is_ok() {
            if let (Ok(m), Ok(f)) = (
                cp_level.metadata(),
                fs::File::options().write(true).open(&target_level),
            ) {
                if let Ok(mtime) = m.modified() {
                    let _ = f.set_times(std::fs::FileTimes::new().set_modified(mtime));
                }
            }
        }
    }
    let cp_players = cp_dir.join("players");
    let target_players = world_root.join("players");
    changed += prune_extra_dat_json_recursive(&cp_players, &target_players);
    if cp_players.is_dir() {
        changed += copy_dir_recursive_opts(&cp_players, &target_players, force);
    }
    let cp_data = cp_dir.join("data");
    let target_data = world_root.join("data");
    changed += prune_extra_dat_json_recursive(&cp_data, &target_data);
    if cp_data.is_dir() {
        changed += copy_dir_recursive_opts(&cp_data, &target_data, force);
    }
    let cp_dims = cp_dir.join("dimensions");
    let target_dims = world_root.join("dimensions");
    changed += prune_extra_dat_json_recursive(&cp_dims, &target_dims);
    if cp_dims.is_dir() {
        changed += copy_dir_recursive_opts(&cp_dims, &target_dims, force);
    }
    let cp_generated = cp_dir.join("generated");
    let target_generated = world_root.join("generated");
    changed += prune_extra_nbt_recursive(&cp_generated, &target_generated);
    if cp_generated.is_dir() {
        changed += copy_dir_recursive_opts(&cp_generated, &target_generated, force);
    }
    changed
}

pub fn restore_disk_files_on_rollback(world_root: &Path) {
    restore_disk_files_for_slot(world_root, "default");
}

pub fn list_disk_slots(world_root: &Path) -> Vec<serde_json::Value> {
    let mut list = Vec::new();
    let def_meta = checkpoint_dir(world_root).join("meta.json");
    if let Ok(s) = fs::read_to_string(&def_meta) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&s) {
            list.push(serde_json::json!({
                "slot": "default",
                "game_time": val.get("game_time"),
                "saved_at_unix": val.get("saved_at_unix"),
                "guests_count": val.get("guests").and_then(|g| g.as_array()).map(|a| a.len()).unwrap_or(0)
            }));
        }
    }

    let slots_base = world_root.join("rreturn").join("slots");
    if let Ok(entries) = fs::read_dir(slots_base) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !is_valid_slot_name(&name) {
                    continue;
                }
                let meta_f = p.join("meta.json");
                if let Ok(s) = fs::read_to_string(&meta_f) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&s) {
                        list.push(serde_json::json!({
                            "slot": name,
                            "game_time": val.get("game_time"),
                            "saved_at_unix": val.get("saved_at_unix"),
                            "guests_count": val.get("guests").and_then(|g| g.as_array()).map(|a| a.len()).unwrap_or(0)
                        }));
                    }
                }
            }
        }
    }

    list
}

pub fn delete_disk_slot(world_root: &Path, slot: &str) -> bool {
    let clean = slot.trim();
    if clean == "default" || !is_valid_slot_name(clean) {
        return false;
    }
    let dir = world_root.join("rreturn").join("slots").join(clean);
    if dir.exists() {
        fs::remove_dir_all(dir).is_ok()
    } else {
        false
    }
}

pub fn format_uuid_std(u: u128) -> String {
    let hex = format!("{:032x}", u);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}


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


pub fn load_slot_from_disk(world_root: &Path, slot: &str) -> Option<TimelineSnapshot> {
    if !is_valid_slot_name(slot.trim()) {
        return None;
    }
    let cp_dir = slot_dir(world_root, slot);
    let meta_file = cp_dir.join("meta.json");
    let meta_str = fs::read_to_string(meta_file).ok()?;
    let meta: serde_json::Value = serde_json::from_str(&meta_str).ok()?;

    let game_time = meta["game_time"].as_i64()?;
    let day_time = meta["day_time"].as_i64().unwrap_or(game_time);
    let difficulty = meta["difficulty"].as_u64().unwrap_or(2) as u8;
    let difficulty_locked = meta["difficulty_locked"].as_bool().unwrap_or(false);
    let next_entity = meta["next_entity"].as_u64().unwrap_or(100) as u32;

    let world_spawn = meta["world_spawn"].as_array().and_then(|arr| {
        if arr.len() == 3 {
            Some([
                arr[0].as_i64()? as i32,
                arr[1].as_i64()? as i32,
                arr[2].as_i64()? as i32,
            ])
        } else {
            None
        }
    });

    let world_spawn_angle = meta["world_spawn_angle"].as_array().map_or([0.0, 0.0], |arr| {
        if arr.len() == 2 {
            [
                arr[0].as_f64().unwrap_or(0.0) as f32,
                arr[1].as_f64().unwrap_or(0.0) as f32,
            ]
        } else {
            [0.0, 0.0]
        }
    });

    let mut anchor_ids = HashSet::new();
    if let Some(arr) = meta["anchor_ids"].as_array() {
        for v in arr {
            if let Some(s) = v.as_str() {
                if let Ok(u) = u128::from_str_radix(s, 16) {
                    anchor_ids.insert(u);
                }
            }
        }
    }

    let mut region_files = HashSet::new();
    if let Some(arr) = meta["region_files"].as_array() {
        for v in arr {
            if let Some(s) = v.as_str() {
                region_files.insert(world_root.join(s));
            }
        }
    }

    let mut offline_player_data = HashMap::new();
    let cp_players = cp_dir.join("players");
    collect_files_recursive(
        &cp_players,
        &mut offline_player_data,
        &world_root.join("players"),
        Path::new(""),
    );

    let mut guests = HashMap::new();
    if let Some(g_arr) = meta["guests"].as_array() {
        for g_val in g_arr {
            let uuid_str = g_val["uuid"].as_str().unwrap_or("");
            let uuid_clean = uuid_str.replace('-', "");
            let uuid = u128::from_str_radix(&uuid_clean, 16).unwrap_or(0);
            if uuid == 0 {
                continue;
            }

            let pos = g_val["position"].as_array().map_or([0.0, 65.0, 0.0], |arr| {
                [
                    arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
                    arr.get(1).and_then(|v| v.as_f64()).unwrap_or(65.0),
                    arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
                ]
            });

            let vel = g_val["velocity"].as_array().map_or(Vec3::ZERO, |arr| {
                Vec3::new(
                    arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
                    arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
                    arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0),
                )
            });

            let player_dat_hex = cp_players.join("data").join(format!("{:032x}.dat", uuid));
            let player_dat_std = cp_players
                .join("data")
                .join(format!("{}.dat", format_uuid_std(uuid)));
            let player_dat = if player_dat_hex.is_file() {
                player_dat_hex
            } else {
                player_dat_std
            };
            let player_tag = read_dat(&player_dat);

            fn decode_saved_effect(v: &serde_json::Value) -> Option<SavedEffect> {
                Some(SavedEffect {
                    id: v.get("id")?.as_str()?.to_owned(),
                    duration: v.get("duration")?.as_i64()? as i32,
                    amplifier: v.get("amplifier")?.as_i64().unwrap_or(0) as i32,
                    ambient: v.get("ambient").and_then(|b| b.as_bool()).unwrap_or(false),
                    particles: v.get("particles").and_then(|b| b.as_bool()).unwrap_or(true),
                    icon: v.get("icon").and_then(|b| b.as_bool()).unwrap_or(true),
                    hidden: v
                        .get("hidden")
                        .filter(|h| !h.is_null())
                        .and_then(decode_saved_effect)
                        .map(Box::new),
                })
            }

            let saved_fx: Vec<SavedEffect> = g_val["effects"]
                .as_array()
                .map(|arr| arr.iter().filter_map(decode_saved_effect).collect())
                .unwrap_or_default();
            let decoded_effects = PlayerEffects::from_saved(&saved_fx).list;

            let decoded_adv = if let Some(criteria) =
                serde_json::from_value::<BTreeMap<String, BTreeMap<String, String>>>(
                    g_val["advancements"].clone(),
                )
                .ok()
                .filter(|m| !m.is_empty())
            {
                Progress { criteria }
            } else {
                let adv_path = cp_players
                    .join("advancements")
                    .join(format!("{}.json", format_uuid_std(uuid)));
                fs::read_to_string(adv_path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .map(|v| Progress::from_json(&v))
                    .unwrap_or_default()
            };

            let decoded_stats: BTreeMap<String, BTreeMap<String, i32>> =
                serde_json::from_value(g_val["stats"].clone()).unwrap_or_default();

            let mut decoded_cooldowns = Cooldowns::default();
            if let Some(cd_arr) = g_val["cooldowns"].as_array() {
                for entry in cd_arr {
                    if let (Some(group), Some(ticks)) = (
                        entry.get("group").and_then(|v| v.as_str()),
                        entry.get("ticks").and_then(|v| v.as_i64()),
                    ) && ticks > 0
                    {
                        decoded_cooldowns.add(Arc::from(group), ticks as i32);
                    }
                }
            }

            let bed = g_val["bed"].as_array().and_then(|arr| {
                if arr.len() == 3 {
                    Some([
                        arr[0].as_i64()? as i32,
                        arr[1].as_i64()? as i32,
                        arr[2].as_i64()? as i32,
                    ])
                } else {
                    None
                }
            });

            let vehicle_good = g_val["vehicle_good"].as_array().and_then(|arr| {
                if arr.len() == 3 {
                    Some([
                        arr[0].as_f64()?,
                        arr[1].as_f64()?,
                        arr[2].as_f64()?,
                    ])
                } else {
                    None
                }
            });

            let supporting_block = g_val["supporting_block"].as_array().and_then(|arr| {
                if arr.len() == 3 {
                    Some(BlockPos::new(
                        arr[0].as_i64()? as i32,
                        arr[1].as_i64()? as i32,
                        arr[2].as_i64()? as i32,
                    ))
                } else {
                    None
                }
            });
            let on_ground_val = g_val["on_ground"].as_bool().unwrap_or(true);
            let ground = GroundState {
                on_ground: on_ground_val,
                horizontal_collision: g_val["ground_horizontal_collision"]
                    .as_bool()
                    .unwrap_or(false),
                on_ground_no_blocks: g_val["ground_no_blocks"].as_bool().unwrap_or(false),
                supporting_block,
            };

            let food_timer = g_val["food_timer"]
                .as_i64()
                .or_else(|| {
                    player_tag
                        .as_ref()
                        .and_then(|t| t.get("foodTickTimer"))
                        .and_then(|v| match v {
                            Tag::Int(i) => Some(i64::from(*i)),
                            Tag::Short(s) => Some(i64::from(*s)),
                            Tag::Byte(b) => Some(i64::from(*b)),
                            _ => None,
                        })
                })
                .unwrap_or(0) as i32;

            let shoulders = g_val["shoulders"]
                .as_array()
                .map(|arr| {
                    [
                        arr.first().and_then(|v| v.as_i64()).map(|v| v as i32),
                        arr.get(1).and_then(|v| v.as_i64()).map(|v| v as i32),
                    ]
                })
                .unwrap_or([None, None]);

            let stuck = g_val["stuck"]
                .as_array()
                .and_then(|arr| {
                    if arr.len() == 2 {
                        Some((arr[0].as_i64()? as i32, arr[1].as_i64()? as i32))
                    } else {
                        None
                    }
                })
                .unwrap_or((0, 0));

            let post_effects: Vec<String> = g_val["post_effects"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();

            let guest_attachments = match horizon_mod_runtime::attachments::attachment_compound(
                player_tag
                    .as_ref()
                    .and_then(|tag| tag.get(horizon_mod_runtime::attachments::ATTACHMENTS_KEY)),
            ) {
                Ok(Some(data)) => {
                    let mut a = horizon_mod_runtime::attachments::ResidentAttachments::default();
                    *a = horizon_mod_runtime::attachments::Attachments::from_nbt(data.clone());
                    a
                }
                _ => horizon_mod_runtime::attachments::ResidentAttachments::default(),
            };

            let mut guest_snap = GuestSnapshot {
                uuid,
                dimension: g_val["dimension"]
                    .as_str()
                    .unwrap_or("minecraft:overworld")
                    .to_owned(),
                position: pos,
                velocity: vel,
                yaw: g_val["yaw"].as_f64().unwrap_or(0.0) as f32,
                pitch: g_val["pitch"].as_f64().unwrap_or(0.0) as f32,
                head_yaw: g_val["head_yaw"].as_f64().unwrap_or(0.0) as f32,
                on_ground: on_ground_val,
                ground,
                game_mode: g_val["game_mode"].as_i64().unwrap_or(0) as i32,
                previous_game_mode: g_val["previous_game_mode"].as_i64().map(|v| v as i32),
                flying: g_val["flying"].as_bool().unwrap_or(false),
                flying_speed: g_val["flying_speed"]
                    .as_f64()
                    .map(|v| v as f32)
                    .filter(|s| s.is_finite())
                    .unwrap_or(0.05),
                allow_flight: g_val["allow_flight"].as_bool().unwrap_or(false),
                sprinting: g_val["sprinting"].as_bool().unwrap_or(false),
                sneaking: g_val["sneaking"].as_bool().unwrap_or(false),
                fall_flying: g_val["fall_flying"].as_bool().unwrap_or(false),
                sleeping: g_val["sleeping"].as_bool().unwrap_or(false),
                shoulders,
                living_flags: g_val["living_flags"].as_i64().unwrap_or(0) as i8,
                stuck,
                bed,
                sleep_timer: g_val["sleep_timer"].as_u64().unwrap_or(0) as u32,
                vehicle: g_val["vehicle"].as_u64().map(|v| v as u32),
                vehicle_order: g_val["vehicle_order"].as_i64().unwrap_or(0),
                vehicle_good,
                spawn: None,
                respawn_elsewhere: None,
                last_death: None,
                health: g_val["health"].as_f64().unwrap_or(20.0) as f32,
                absorption: g_val["absorption"].as_f64().unwrap_or(0.0) as f32,
                food: g_val["food"].as_i64().unwrap_or(20) as i32,
                saturation: g_val["saturation"].as_f64().unwrap_or(5.0) as f32,
                exhaustion: g_val["exhaustion"].as_f64().unwrap_or(0.0) as f32,
                food_timer,
                fire_ticks: g_val["fire_ticks"].as_i64().unwrap_or(-20) as i32,
                air: g_val["air"].as_i64().unwrap_or(300) as i32,
                ticks_frozen: g_val["ticks_frozen"].as_i64().unwrap_or(0) as i32,
                fall_distance: g_val["fall_distance"].as_f64().unwrap_or(0.0),
                xp_level: g_val["xp_level"].as_i64().unwrap_or(0) as i32,
                xp_progress: g_val["xp_progress"].as_f64().unwrap_or(0.0) as f32,
                xp_total: g_val["xp_total"].as_i64().unwrap_or(0) as i32,
                xp_seed: g_val["xp_seed"].as_i64().unwrap_or(0) as i32,
                inventory: vec![None; 36],
                armor: [None, None, None, None],
                offhand: None,
                carried: None,
                ender_items: vec![None; 27],
                selected: g_val["selected"].as_u64().unwrap_or(0) as usize,
                craft_grid: [None, None, None, None],
                craft_result: None,
                effects: decoded_effects,
                effects_tick_count: g_val["effects_tick_count"].as_i64().unwrap_or(0) as i32,
                effects_changes: g_val["effects_changes"].as_u64().unwrap_or(0) as u32,
                shield_cooldown: g_val["shield_cooldown"].as_u64().unwrap_or(0) as u32,
                cooldowns: decoded_cooldowns,
                recipe_book: RecipeBookSettings::default(),
                advancements: decoded_adv,
                stats: decoded_stats,
                seen_credits: g_val["seen_credits"].as_bool().unwrap_or(false),
                post_effects,
                attachments: guest_attachments,
                saved: player_tag.clone(),
                hook: g_val
                    .get("hook")
                    .filter(|v| !v.is_null())
                    .and_then(|v| {
                        let id = v.get("id")?.as_u64()? as u32;
                        let p = v.get("position")?.as_array()?;
                        let prev = v.get("previous")?.as_array()?;
                        let vel_arr = v.get("velocity").and_then(|vel| vel.as_array());
                        let vel = if let Some(va) = vel_arr
                            && va.len() == 3
                        {
                            [
                                va[0].as_f64().unwrap_or(0.0),
                                va[1].as_f64().unwrap_or(0.0),
                                va[2].as_f64().unwrap_or(0.0),
                            ]
                        } else {
                            [0.0, 0.0, 0.0]
                        };
                        let nibble = v.get("nibble").and_then(|n| n.as_i64()).unwrap_or(0) as i32;
                        let time_until_lured = v
                            .get("time_until_lured")
                            .and_then(|t| t.as_i64())
                            .unwrap_or(0) as i32;
                        let time_until_hooked = v
                            .get("time_until_hooked")
                            .and_then(|t| t.as_i64())
                            .unwrap_or(0) as i32;
                        let fish_angle = v
                            .get("fish_angle")
                            .and_then(|a| a.as_f64())
                            .unwrap_or(0.0) as f32;
                        let open_water = v
                            .get("open_water")
                            .and_then(|o| o.as_bool())
                            .unwrap_or(true);
                        let state_bobbing = v
                            .get("state_bobbing")
                            .and_then(|b| b.as_bool())
                            .unwrap_or(false);
                        if p.len() == 3 && prev.len() == 3 {
                            Some(HookSnapshot {
                                id,
                                position: [p[0].as_f64()?, p[1].as_f64()?, p[2].as_f64()?],
                                previous: [
                                    prev[0].as_f64()?,
                                    prev[1].as_f64()?,
                                    prev[2].as_f64()?,
                                ],
                                velocity: vel,
                                rng_state: v
                                    .get("rng_state")
                                    .and_then(|r| r.as_u64())
                                    .unwrap_or(0),
                                nibble,
                                time_until_lured,
                                time_until_hooked,
                                fish_angle,
                                open_water,
                                state_bobbing,
                            })
                        } else {
                            None
                        }
                    }),
                motion: None,
            };

            if let Some(tag) = &player_tag {
                let mut p_level = Level::default();
                p_level.read_player(tag);
                guest_snap.spawn = p_level.spawn;
                guest_snap.respawn_elsewhere = p_level.respawn_elsewhere;
                guest_snap.last_death = p_level.last_death;
                guest_snap.recipe_book = p_level.recipe_book;

                if let Some(Tag::List(items)) = tag.get("Inventory") {
                    for it in items {
                        let slot = it
                            .get("Slot")
                            .and_then(|v| match v {
                                Tag::Byte(b) => Some(*b as i16),
                                Tag::Short(s) => Some(*s),
                                Tag::Int(i) => Some(*i as i16),
                                _ => None,
                            })
                            .unwrap_or(0);
                        if let Some(stack) = item_from_nbt(it) {
                            match slot {
                                0..=35 => guest_snap.inventory[slot as usize] = Some(stack),
                                100 => guest_snap.armor[3] = Some(stack),
                                101 => guest_snap.armor[2] = Some(stack),
                                102 => guest_snap.armor[1] = Some(stack),
                                103 => guest_snap.armor[0] = Some(stack),
                                -106 | 150 => guest_snap.offhand = Some(stack),
                                _ => {}
                            }
                        }
                    }
                }
                if let Some(eq) = tag.get("equipment") {
                    if let Some(s) = eq.get("head").and_then(item_from_nbt) {
                        guest_snap.armor[0] = Some(s);
                    }
                    if let Some(s) = eq.get("chest").and_then(item_from_nbt) {
                        guest_snap.armor[1] = Some(s);
                    }
                    if let Some(s) = eq.get("legs").and_then(item_from_nbt) {
                        guest_snap.armor[2] = Some(s);
                    }
                    if let Some(s) = eq.get("feet").and_then(item_from_nbt) {
                        guest_snap.armor[3] = Some(s);
                    }
                    if let Some(s) = eq.get("offhand").and_then(item_from_nbt) {
                        guest_snap.offhand = Some(s);
                    }
                }
                if let Some(Tag::List(ender)) = tag.get("EnderItems") {
                    for it in ender {
                        let slot = it
                            .get("Slot")
                            .and_then(|v| match v {
                                Tag::Byte(b) => Some(*b as usize),
                                Tag::Short(s) => Some(*s as usize),
                                Tag::Int(i) => Some(*i as usize),
                                _ => None,
                            })
                            .unwrap_or(0);
                        if slot < guest_snap.ender_items.len() {
                            if let Some(stack) = item_from_nbt(it) {
                                guest_snap.ender_items[slot] = Some(stack);
                            }
                        }
                    }
                }
            }

            guests.insert(uuid, guest_snap);
        }
    }

    let mut policy = RollbackPolicy::default();
    if let Some(pol_obj) = meta["policy"].as_object() {
        for (k, v) in pol_obj {
            if let Some(b) = v.as_bool() {
                policy.set_field(k, b);
            }
        }
    }

    let disk_level = Level::read(&cp_dir);
    let weather = disk_level.as_ref().map(|l| l.weather).unwrap_or_default();
    let enabled_features = disk_level
        .as_ref()
        .map(|l| &l.enabled_features[..])
        .unwrap_or(&[]);
    let rules = horizon_client::gamerules::GameRules::load_with_features(&cp_dir, enabled_features);
    let dragon_fight_dat = fs::read(fight_dat_path(&cp_dir)).ok();
    let trader_dat = fs::read(trader_dat_path(&cp_dir)).ok();

    let globals_tag = read_dat(&cp_dir.join("rreturn_globals.dat"));
    let rand_sequences = globals_tag
        .as_ref()
        .and_then(|t| t.get("rand_sequences"))
        .map(horizon_client::random_sequences::RandomSequences::from_nbt)
        .or_else(|| {
            read_dat(&cp_dir.join("data/minecraft/random_sequences.dat"))
                .and_then(|t| t.get("data").cloned())
                .map(|d| horizon_client::random_sequences::RandomSequences::from_nbt(&d))
        })
        .unwrap_or_default();
    let border = globals_tag
        .as_ref()
        .and_then(|t| t.get("border"))
        .map(WorldBorder::unpack)
        .or_else(|| {
            read_dat(&cp_dir.join("dimensions/minecraft/overworld/data/minecraft/world_border.dat"))
                .or_else(|| read_dat(&cp_dir.join("data/minecraft/world_border.dat")))
                .and_then(|t| t.get("data").map(WorldBorder::unpack))
        })
        .unwrap_or_default();

    let scoreboard = globals_tag
        .as_ref()
        .and_then(|t| t.get("scoreboard"))
        .map(Scoreboard::unpack)
        .or_else(|| {
            read_dat(&cp_dir.join("data/minecraft/scoreboard.dat"))
                .and_then(|t| t.get("data").map(Scoreboard::unpack))
        })
        .unwrap_or_default();

    let bossbars = globals_tag
        .as_ref()
        .and_then(|t| t.get("bossbars").cloned())
        .unwrap_or_else(|| Tag::Compound(Default::default()));

    let sculk_tracker = globals_tag
        .as_ref()
        .and_then(|t| t.get("sculk_tracker"))
        .map(WardenSpawnTracker::from_nbt)
        .unwrap_or_default();

    let fight_tag = globals_tag
        .as_ref()
        .and_then(|t| t.get("fight_state").cloned());

    let player_extra = globals_tag
        .as_ref()
        .and_then(|t| t.get("player_extra").cloned())
        .or_else(|| disk_level.as_ref().and_then(|l| l.player_extra.clone()));

    let player_attachments = globals_tag
        .as_ref()
        .and_then(|t| match t.get("player_attachments") {
            Some(Tag::Compound(c)) => {
                let mut a = horizon_mod_runtime::attachments::ResidentAttachments::default();
                *a = horizon_mod_runtime::attachments::Attachments::from_nbt(c.clone());
                Some(a)
            }
            _ => None,
        })
        .unwrap_or_default();

    let mut guest_attachments = HashMap::new();
    if let Some(Tag::List(list)) = globals_tag.as_ref().and_then(|t| t.get("guest_attachments")) {
        for item in list {
            if let (Some(Tag::String(u_str)), Some(Tag::Compound(c))) =
                (item.get("uuid"), item.get("data"))
                && let Ok(s) = u_str.to_string()
                && let Ok(u) = u128::from_str_radix(&s, 16)
            {
                guest_attachments.insert(
                    u,
                    horizon_mod_runtime::attachments::Attachments::from_nbt(c.clone()),
                );
            }
        }
    }

    let mut entity_attachments = FxHashMap::default();
    if let Some(Tag::List(list)) = globals_tag
        .as_ref()
        .and_then(|t| t.get("entity_attachments"))
    {
        for item in list {
            if let (Some(Tag::Int(id)), Some(Tag::Compound(c))) =
                (item.get("id"), item.get("data"))
            {
                entity_attachments.insert(
                    *id as u32,
                    horizon_mod_runtime::attachments::Attachments::from_nbt(c.clone()),
                );
            }
        }
    }

    let sculk_darkness = meta["sculk_darkness"]
        .as_array()
        .and_then(|arr| {
            if arr.len() == 2 {
                Some((arr[0].as_f64()? as f32, arr[1].as_f64()? as f32))
            } else {
                None
            }
        })
        .unwrap_or((0.0, 0.0));

    let mut mob_players = horizon_client::fast_hash::FxHashMap::default();
    if let Some(arr) = meta["mob_players"].as_array() {
        for item in arr {
            if let Some(pair) = item.as_array()
                && pair.len() == 2
                && let (Some(k), Some(v)) = (pair[0].as_u64(), pair[1].as_u64())
            {
                mob_players.insert(k as u32, v as u32);
            }
        }
    }

    let ticked_entities_through = meta["ticked_entities_through"]
        .as_u64()
        .map(|v| v as u32)
        .unwrap_or(next_entity);
    let cat_tick = meta["cat_tick"].as_i64().map(|v| v as i32).unwrap_or(1200);
    let phantom_tick = meta["phantom_tick"]
        .as_i64()
        .map(|v| v as i32)
        .unwrap_or(1200);

    let mut shoulder_time = HashMap::new();
    if let Some(arr) = meta["shoulder_time"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 5
                && let (Some(u0), Some(u1), Some(u2), Some(u3), Some(t)) = (
                    a[0].as_i64(),
                    a[1].as_i64(),
                    a[2].as_i64(),
                    a[3].as_i64(),
                    a[4].as_i64(),
                )
            {
                shoulder_time.insert([u0 as i32, u1 as i32, u2 as i32, u3 as i32], t);
            }
        }
    }

    let mut portals_entities = HashMap::new();
    if let Some(arr) = meta["portals_entities"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 2
                && let (Some(id), Some(cd)) = (a[0].as_u64(), a[1].as_i64())
            {
                let mut st = horizon_client::nether_portal::PortalState::default();
                st.cooldown = cd as i32;
                portals_entities.insert(id as u32, st);
            }
        }
    }

    let mut end_gateway_cooldowns = HashMap::new();
    if let Some(arr) = meta["end_gateways"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 2
                && let (Some(id), Some(cd)) = (a[0].as_u64(), a[1].as_i64())
            {
                end_gateway_cooldowns.insert(id as u32, cd as i32);
            }
        }
    }

    let mut lectern_pulses = Vec::new();
    if let Some(arr) = meta["lectern_pulses"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 4
                && let (Some(x), Some(y), Some(z), Some(t)) =
                    (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
            {
                lectern_pulses.push(((x as i32, y as i32, z as i32), t as i32));
            }
        }
    }

    let mut player_beds = Vec::new();
    if let Some(arr) = meta["player_beds"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 4
                && let (Some(dim_str), Some(x), Some(y), Some(z)) =
                    (a[0].as_str(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
            {
                player_beds.push((Dim::from_id(dim_str), [x as i32, y as i32, z as i32]));
            }
        }
    }

    let mut lid_openers = HashMap::new();
    if let Some(arr) = meta["lid_openers"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 4
                && let (Some(x), Some(y), Some(z), Some(c)) =
                    (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
            {
                lid_openers.insert((x as i32, y as i32, z as i32), c as i32);
            }
        }
    }

    let mut scheduled_events = Vec::new();
    if let Some(arr) = meta["scheduled_events"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 3
                && let (Some(id), Some(time), Some(seq)) =
                    (a[0].as_str(), a[1].as_i64(), a[2].as_u64())
            {
                scheduled_events.push(super::super::functions::Scheduled {
                    id: id.to_owned(),
                    time,
                    seq,
                });
            }
        }
    }

    let mut command_clocks = BTreeMap::new();
    if let Some(arr) = meta["command_clocks"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 5
                && let (Some(k), Some(ticks), Some(partial), Some(rate), Some(paused)) = (
                    a[0].as_str(),
                    a[1].as_i64(),
                    a[2].as_f64(),
                    a[3].as_f64(),
                    a[4].as_bool(),
                )
            {
                command_clocks.insert(
                    k.to_owned(),
                    (
                        ticks,
                        super::super::chat::WorldClock {
                            partial: partial as f32,
                            rate: rate as f32,
                            paused,
                        },
                    ),
                );
            }
        }
    }

    let cmd_post_effects: Vec<String> = meta["cmd_post_effects"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    let mut lead_killers = HashMap::new();
    if let Some(arr) = meta["lead_killers"].as_array() {
        for row in arr {
            if let Some(a) = row.as_array()
                && a.len() == 3
                && let (Some(id), Some(u_str), Some(dim_str)) =
                    (a[0].as_u64(), a[1].as_str(), a[2].as_str())
                && let Ok(u) = u128::from_str_radix(u_str, 16)
            {
                lead_killers.insert(id as u32, (u, Dim::from_id(dim_str)));
            }
        }
    }

    let sleep_seen = meta["sleep_seen"]
        .as_array()
        .and_then(|a| {
            if a.len() == 2 {
                Some((a[0].as_i64()?, a[1].as_i64()?))
            } else {
                None
            }
        })
        .unwrap_or((0, 0));

    let hr_json = &meta["health_rules"];
    let diff_enum = match difficulty {
        0 => Difficulty::Peaceful,
        1 => Difficulty::Easy,
        3 => Difficulty::Hard,
        _ => Difficulty::Normal,
    };
    let rule_bool = |name: &str, def: bool| -> bool {
        match rules.get(name) {
            Some(horizon_client::gamerules::Value::Bool(b)) => b,
            _ => def,
        }
    };
    let health_rules = HealthRules {
        difficulty: diff_enum,
        natural_regeneration: hr_json["natural_regeneration"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("natural_health_regeneration", true)),
        drowning_damage: hr_json["drowning_damage"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("drowning_damage", true)),
        fall_damage: hr_json["fall_damage"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("fall_damage", true)),
        fire_damage: hr_json["fire_damage"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("fire_damage", true)),
        freeze_damage: hr_json["freeze_damage"]
            .as_bool()
            .unwrap_or_else(|| rule_bool("freeze_damage", true)),
    };

    Some(TimelineSnapshot {
        anchor_ids,
        policy,
        game_time,
        day_time,
        weather,
        health_rules,
        rules,
        border,
        bossbars,
        sculk_tracker,
        sculk_darkness,
        dragon_fight_dat,
        fight_tag,
        trader_dat,
        rand_sequences,
        region_files,
        difficulty,
        difficulty_locked,
        clock_rate: meta["clock_rate"].as_f64().map(|v| v as f32).unwrap_or(20.0),
        clock_frozen: meta["clock_frozen"].as_bool().unwrap_or(false),
        time_rate: meta["time_rate"].as_f64().map(|v| v as f32).unwrap_or(1.0),
        time_partial: meta["time_partial"].as_f64().map(|v| v as f32).unwrap_or(0.0),
        time_paused: meta["time_paused"].as_bool().unwrap_or(false),
        command_clocks,
        world_spawn,
        world_spawn_angle,
        next_entity,
        ticked_entities_through,
        ordered_announced: horizon_client::join_seq::JoinSeq::upcoming(),
        cat_tick,
        phantom_tick,
        player_extra,
        shoulder_time,
        fangs: Default::default(),
        ominous_spawners: Vec::new(),
        portals_entities,
        end_gateway_cooldowns,
        lectern_pulses,
        player_beds,
        sleep_seen,
        lead_killers,
        lid_openers,
        entity_attachments,
        player_attachments,
        guest_attachments,
        scheduled_events,
        scoreboard,
        sound_seed_state: meta["sound_seed_state"].as_u64(),
        cmd_post_effects,
        guests,
        offline_player_data,
        levels: HashMap::new(),
        mob_players,
        disk_restored: false,
        client_using: None,
        client_use_duration: 0,
        client_use_ticks_set: false,
        client_crossbow_sounds: [false; 2],
    })
}

pub fn load_checkpoint_from_disk(world_root: &Path) -> Option<TimelineSnapshot> {
    load_slot_from_disk(world_root, "default")
}


pub fn hydrate_level_from_disk(
    game: &mut Game<'_>,
    slot: &str,
    dim: Dim,
    snap: &TimelineSnapshot,
) -> Option<LevelSnapshot> {
    let cp_dir = slot_dir(&game.world_root, slot);
    let cp_dim_dir = horizon_client::save::dimension_dir(&cp_dir, dim.id());
    if !cp_dim_dir.is_dir() {
        return None;
    }

    let shape = game.level_shapes.get(dim.id());
    let cp_save = horizon_client::save::SaveDir::new(cp_dim_dir.clone(), shape);
    if let Some(tables) = game.save.tables.get() {
        let _ = cp_save.tables.set(Arc::clone(tables));
    } else {
        let tables = Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
        let _ = cp_save.tables.set(tables);
    }

    let u8_to_dir = |v: u64| match v {
        0 => Direction::Down,
        1 => Direction::Up,
        2 => Direction::North,
        3 => Direction::South,
        4 => Direction::West,
        _ => Direction::East,
    };
    let decode_piston = |v: &serde_json::Value| -> Option<MovingPiston> {
        Some(MovingPiston {
            moved_state: BlockStateId(v.get("moved_state")?.as_u64()? as u32),
            direction: u8_to_dir(v.get("direction")?.as_u64()?),
            extending: v.get("extending")?.as_bool()?,
            source: v.get("source")?.as_bool()?,
            progress: v.get("progress")?.as_f64()? as f32,
            progress_o: v.get("progress_o")?.as_f64()? as f32,
            last_ticked: v.get("last_ticked")?.as_i64()?,
            death_ticks: v.get("death_ticks")?.as_i64()? as i32,
            id: v.get("id")?.as_u64()?,
        })
    };

    let mut candidate_set: HashSet<(i32, i32)> = HashSet::new();
    let mut disk_level_rng: Option<u64> = None;
    let mut disk_random_tick_value: Option<i32> = None;
    let mut disk_end_clock: i64 = 0;
    let mut disk_comparator_outputs = HashMap::new();
    let mut disk_torch_toggles = Vec::new();
    let mut disk_moving_pistons = Vec::new();
    let mut disk_piston_pushes = Vec::new();
    let mut disk_block_events = VecDeque::new();
    let mut disk_last_hurt_by_player = HashMap::new();
    let mut disk_love_cause = HashMap::new();
    let mut disk_uuid_to_id: HashMap<[i32; 4], u32> = HashMap::new();
    let mut disk_mob_rt: HashMap<u32, DiskMobRuntime> = HashMap::new();
    let mut disk_item_rt: HashMap<u32, DiskItemRuntime> = HashMap::new();
    let mut disk_orb_rt: HashMap<u32, u32> = HashMap::new();
    let mut disk_chest_openers: Vec<((i32, i32, i32), (i32, f64))> = Vec::new();
    let mut disk_lid_openers: HashMap<(i32, i32, i32), i32> = HashMap::new();
    let mut disk_lectern_pulses: Vec<((i32, i32, i32), i32)> = Vec::new();
    let mut disk_portals_entities: HashMap<u32, horizon_client::nether_portal::PortalState> =
        HashMap::new();
    let mut disk_gateway_anims: Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)> =
        Vec::new();
    let mut disk_hoppers: HashMap<(i32, i32, i32), super::super::machines::HopperState> =
        HashMap::new();
    struct DiskTnt {
        id: u32,
        pos: Vec3,
        vel: Vec3,
        fuse: i32,
        power: f32,
        no_gravity: bool,
        on_ground: bool,
        fall_distance: f64,
        rot: [f32; 2],
    }
    struct DiskFalling {
        id: u32,
        state: BlockStateId,
        pos: Vec3,
        vel: Vec3,
        age: i32,
        no_gravity: bool,
        drop_item: bool,
        on_ground: bool,
        fall_distance: f64,
        rot: [f32; 2],
    }
    struct DiskProjectile {
        id: u32,
        kind: String,
        pos: Vec3,
        vel: Vec3,
        yaw: f32,
        pitch: f32,
        age: u32,
        stuck: bool,
    }
    let mut disk_tnts: Vec<DiskTnt> = Vec::new();
    let mut disk_falling: Vec<DiskFalling> = Vec::new();
    let mut disk_boat_velocities: HashMap<u32, Vec3> = HashMap::new();
    let mut disk_cart_velocities: HashMap<u32, Vec3> = HashMap::new();
    let mut disk_projectiles: Vec<DiskProjectile> = Vec::new();

    if let Ok(meta_str) = fs::read_to_string(cp_dir.join("meta.json")) {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
            if let Some(arr) = meta["loaded_chunks"][dim.id()].as_array() {
                for item in arr {
                    if let Some(pair) = item.as_array() {
                        if pair.len() == 2 {
                            if let (Some(cx), Some(cz)) = (pair[0].as_i64(), pair[1].as_i64()) {
                                candidate_set.insert((cx as i32, cz as i32));
                            }
                        }
                    }
                }
            }
            disk_level_rng = meta["level_rng"][dim.id()].as_u64();
            disk_random_tick_value = meta["random_tick_values"][dim.id()]
                .as_i64()
                .map(|v| v as i32);
            disk_end_clock = meta["end_clocks"][dim.id()].as_i64().unwrap_or(0);
            let rs = &meta["level_redstone"][dim.id()];
            if let Some(arr) = rs["comparator_outputs"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 4
                        && let (Some(x), Some(y), Some(z), Some(out)) =
                            (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                    {
                        disk_comparator_outputs.insert(
                            BlockPos::new(x as i32, y as i32, z as i32),
                            out as i32,
                        );
                    }
                }
            }
            if let Some(arr) = rs["torch_toggles"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 4
                        && let (Some(x), Some(y), Some(z), Some(time)) =
                            (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                    {
                        disk_torch_toggles.push((
                            BlockPos::new(x as i32, y as i32, z as i32),
                            time,
                        ));
                    }
                }
            }
            if let Some(arr) = rs["moving_pistons"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 4
                        && let (Some(x), Some(y), Some(z), Some(piston)) = (
                            a[0].as_i64(),
                            a[1].as_i64(),
                            a[2].as_i64(),
                            decode_piston(&a[3]),
                        )
                    {
                        disk_moving_pistons.push((
                            BlockPos::new(x as i32, y as i32, z as i32),
                            piston,
                        ));
                    }
                }
            }
            if let Some(arr) = rs["piston_pushes"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 5
                        && let (Some(x), Some(y), Some(z), Some(piston), Some(next_p)) = (
                            a[0].as_i64(),
                            a[1].as_i64(),
                            a[2].as_i64(),
                            decode_piston(&a[3]),
                            a[4].as_f64(),
                        )
                    {
                        disk_piston_pushes.push((
                            BlockPos::new(x as i32, y as i32, z as i32),
                            piston,
                            next_p as f32,
                        ));
                    }
                }
            }
            if let Some(arr) = rs["block_events"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 6
                        && let (Some(x), Some(y), Some(z), Some(blk), Some(pa), Some(pb)) = (
                            a[0].as_i64(),
                            a[1].as_i64(),
                            a[2].as_i64(),
                            a[3].as_u64(),
                            a[4].as_i64(),
                            a[5].as_i64(),
                        )
                    {
                        disk_block_events.push_back(BlockEventData {
                            pos: BlockPos::new(x as i32, y as i32, z as i32),
                            block: BlockId(blk as u16),
                            param_a: pa as i32,
                            param_b: pb as i32,
                            note_override: None,
                        });
                    }
                }
            }
            if let Some(arr) = rs["last_hurt_by_player"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 2
                        && let Some(k) = a[0].as_u64()
                    {
                        disk_last_hurt_by_player.insert(
                            k as u32,
                            super::super::combat::PlayerCredit(a[1].as_u64().unwrap_or(0) as u128),
                        );
                    }
                }
            }
            if let Some(arr) = rs["love_cause"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 2
                        && let Some(k) = a[0].as_u64()
                    {
                        disk_love_cause.insert(k as u32, a[1].as_u64().map(|v| v as u32));
                    }
                }
            }
            if let Some(arr) = rs["persist_uuids"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 5
                        && let (Some(id), Some(u0), Some(u1), Some(u2), Some(u3)) = (
                            a[0].as_u64(),
                            a[1].as_i64(),
                            a[2].as_i64(),
                            a[3].as_i64(),
                            a[4].as_i64(),
                        )
                    {
                        disk_uuid_to_id
                            .insert([u0 as i32, u1 as i32, u2 as i32, u3 as i32], id as u32);
                    }
                }
            }
            if let Some(arr) = rs["mob_runtime"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() >= 7
                        && let (
                            Some(id),
                            Some(age),
                            Some(inv),
                            Some(lh),
                            Some(sw),
                            Some(psw),
                            Some(amb),
                        ) = (
                            a[0].as_u64(),
                            a[1].as_u64(),
                            a[2].as_i64(),
                            a[3].as_u64(),
                            a[4].as_i64(),
                            a[5].as_i64(),
                            a[6].as_i64(),
                        )
                    {
                        let rng_state = a.get(7).and_then(|v| v.as_u64()).unwrap_or(0);
                        let anger = a.get(8).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let anger_end = a.get(9).and_then(|v| v.as_i64()).unwrap_or(0);
                        let anger_stamp = a
                            .get(10)
                            .and_then(|v| v.as_array())
                            .and_then(|pair| {
                                if pair.len() == 2 {
                                    Some((pair[0].as_i64()? as i32, pair[1].as_u64()? as u32))
                                } else {
                                    None
                                }
                            });
                        let counter = a.get(11).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let counter2 = a.get(12).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let anim_ticks = a.get(13).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let egg_time = a.get(14).and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let flag = a.get(15).and_then(|v| v.as_bool()).unwrap_or(false);
                        let flag2 = a.get(16).and_then(|v| v.as_bool()).unwrap_or(false);
                        let vector = a
                            .get(17)
                            .and_then(|v| v.as_array())
                            .and_then(|va| {
                                if va.len() == 3 {
                                    Some([
                                        f64::from_bits(va[0].as_u64()?),
                                        f64::from_bits(va[1].as_u64()?),
                                        f64::from_bits(va[2].as_u64()?),
                                    ])
                                } else {
                                    None
                                }
                            })
                            .unwrap_or([0.0; 3]);
                        let ints = a
                            .get(18)
                            .and_then(|v| v.as_array())
                            .and_then(|ia| {
                                if ia.len() == 6 {
                                    Some([
                                        ia[0].as_i64()? as i32,
                                        ia[1].as_i64()? as i32,
                                        ia[2].as_i64()? as i32,
                                        ia[3].as_i64()? as i32,
                                        ia[4].as_i64()? as i32,
                                        ia[5].as_i64()? as i32,
                                    ])
                                } else {
                                    None
                                }
                            })
                            .unwrap_or([0; 6]);
                        let body_adopted = a
                            .get(19)
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        disk_mob_rt.insert(
                            id as u32,
                            DiskMobRuntime {
                                age: age as u32,
                                invulnerable: inv as i32,
                                last_hurt: f32::from_bits(lh as u32),
                                swell: sw as i32,
                                previous_swell: psw as i32,
                                ambient_timer: amb as i32,
                                rng_state,
                                anger,
                                anger_end,
                                anger_stamp,
                                counter,
                                counter2,
                                anim_ticks,
                                egg_time,
                                flag,
                                flag2,
                                vector,
                                ints,
                                body_adopted,
                            },
                        );
                    }
                }
            }
            if let Some(arr) = rs["item_runtime"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() >= 3
                        && let (Some(id), Some(ticks), Some(bob)) =
                            (a[0].as_u64(), a[1].as_u64(), a[2].as_u64())
                    {
                        let rng_state = a.get(3).and_then(|v| v.as_u64()).unwrap_or(0);
                        disk_item_rt.insert(
                            id as u32,
                            DiskItemRuntime {
                                ticks: ticks as u32,
                                bob_offset: f32::from_bits(bob as u32),
                                rng_state,
                            },
                        );
                    }
                }
            }
            if let Some(arr) = rs["orb_runtime"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 2
                        && let (Some(id), Some(ticks)) = (a[0].as_u64(), a[1].as_u64())
                    {
                        disk_orb_rt.insert(id as u32, ticks as u32);
                    }
                }
            }
            if let Some(arr) = rs["chest_openers"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 5
                        && let (Some(x), Some(y), Some(z), Some(count), Some(range_bits)) = (
                            a[0].as_i64(),
                            a[1].as_i64(),
                            a[2].as_i64(),
                            a[3].as_i64(),
                            a[4].as_u64(),
                        )
                    {
                        disk_chest_openers.push((
                            (x as i32, y as i32, z as i32),
                            (count as i32, f64::from_bits(range_bits)),
                        ));
                    }
                }
            }
            if let Some(arr) = rs["lid_openers"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 4
                        && let (Some(x), Some(y), Some(z), Some(c)) =
                            (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                    {
                        disk_lid_openers.insert((x as i32, y as i32, z as i32), c as i32);
                    }
                }
            } else if dim == Dim::Overworld {
                disk_lid_openers = snap.lid_openers.clone();
            }
            if let Some(arr) = rs["lectern_pulses"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 4
                        && let (Some(x), Some(y), Some(z), Some(t)) =
                            (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                    {
                        disk_lectern_pulses.push(((x as i32, y as i32, z as i32), t as i32));
                    }
                }
            } else if dim == Dim::Overworld {
                disk_lectern_pulses = snap.lectern_pulses.clone();
            }
            if let Some(arr) = rs["portals_entities"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 2
                        && let (Some(id), Some(cd)) = (a[0].as_u64(), a[1].as_i64())
                    {
                        let mut st = horizon_client::nether_portal::PortalState::default();
                        st.cooldown = cd as i32;
                        disk_portals_entities.insert(id as u32, st);
                    }
                }
            } else if dim == Dim::Overworld {
                disk_portals_entities = snap.portals_entities.clone();
            }
            if let Some(arr) = rs["gateway_anims"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 5
                        && let (Some(x), Some(y), Some(z), Some(age), Some(cd)) = (
                            a[0].as_i64(),
                            a[1].as_i64(),
                            a[2].as_i64(),
                            a[3].as_i64(),
                            a[4].as_i64(),
                        )
                    {
                        disk_gateway_anims.push((
                            [x as i32, y as i32, z as i32],
                            horizon_client::block_entity_anim::EndGateway {
                                age,
                                teleport_cooldown: cd as i32,
                            },
                        ));
                    }
                }
            }
            if let Some(arr) = rs["hoppers"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 4
                        && let (Some(x), Some(y), Some(z), Some(cd)) =
                            (a[0].as_i64(), a[1].as_i64(), a[2].as_i64(), a[3].as_i64())
                    {
                        let tag = Tag::compound([("TransferCooldown", Tag::Int(cd as i32))]);
                        disk_hoppers.insert(
                            (x as i32, y as i32, z as i32),
                            super::super::machines::HopperState::load(&tag),
                        );
                    }
                }
            }
            if let Some(arr) = rs["tnts"].as_array() {
                for item in arr {
                    if let (Some(id), Some(pos), Some(vel), Some(fuse)) = (
                        item["id"].as_u64(),
                        item["pos"].as_array(),
                        item["vel"].as_array(),
                        item["fuse"].as_i64(),
                    ) && pos.len() == 3
                        && vel.len() == 3
                    {
                        let rot_arr = item["rot"].as_array();
                        let rot = if let Some(ra) = rot_arr
                            && ra.len() == 2
                        {
                            [
                                ra[0].as_f64().unwrap_or(0.0) as f32,
                                ra[1].as_f64().unwrap_or(0.0) as f32,
                            ]
                        } else {
                            [0.0, 0.0]
                        };
                        disk_tnts.push(DiskTnt {
                            id: id as u32,
                            pos: Vec3::new(
                                pos[0].as_f64().unwrap_or(0.0),
                                pos[1].as_f64().unwrap_or(0.0),
                                pos[2].as_f64().unwrap_or(0.0),
                            ),
                            vel: Vec3::new(
                                vel[0].as_f64().unwrap_or(0.0),
                                vel[1].as_f64().unwrap_or(0.0),
                                vel[2].as_f64().unwrap_or(0.0),
                            ),
                            fuse: fuse as i32,
                            power: item["power"].as_f64().unwrap_or(4.0) as f32,
                            no_gravity: item["no_gravity"].as_bool().unwrap_or(false),
                            on_ground: item["on_ground"].as_bool().unwrap_or(false),
                            fall_distance: item["fall_distance"].as_f64().unwrap_or(0.0),
                            rot,
                        });
                    }
                }
            }
            if let Some(arr) = rs["falling"].as_array() {
                for item in arr {
                    if let (Some(id), Some(st), Some(pos), Some(vel), Some(age)) = (
                        item["id"].as_u64(),
                        item["state"].as_u64(),
                        item["pos"].as_array(),
                        item["vel"].as_array(),
                        item["age"].as_i64(),
                    ) && pos.len() == 3
                        && vel.len() == 3
                    {
                        let rot_arr = item["rot"].as_array();
                        let rot = if let Some(ra) = rot_arr
                            && ra.len() == 2
                        {
                            [
                                ra[0].as_f64().unwrap_or(0.0) as f32,
                                ra[1].as_f64().unwrap_or(0.0) as f32,
                            ]
                        } else {
                            [0.0, 0.0]
                        };
                        disk_falling.push(DiskFalling {
                            id: id as u32,
                            state: BlockStateId(st as u32),
                            pos: Vec3::new(
                                pos[0].as_f64().unwrap_or(0.0),
                                pos[1].as_f64().unwrap_or(0.0),
                                pos[2].as_f64().unwrap_or(0.0),
                            ),
                            vel: Vec3::new(
                                vel[0].as_f64().unwrap_or(0.0),
                                vel[1].as_f64().unwrap_or(0.0),
                                vel[2].as_f64().unwrap_or(0.0),
                            ),
                            age: age as i32,
                            no_gravity: item["no_gravity"].as_bool().unwrap_or(false),
                            drop_item: item["drop_item"].as_bool().unwrap_or(true),
                            on_ground: item["on_ground"].as_bool().unwrap_or(false),
                            fall_distance: item["fall_distance"].as_f64().unwrap_or(0.0),
                            rot,
                        });
                    }
                }
            }
            if let Some(arr) = rs["boat_velocities"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 4
                        && let (Some(id), Some(vx), Some(vy), Some(vz)) = (
                            a[0].as_u64(),
                            a[1].as_f64(),
                            a[2].as_f64(),
                            a[3].as_f64(),
                        )
                    {
                        disk_boat_velocities.insert(id as u32, Vec3::new(vx, vy, vz));
                    }
                }
            }
            if let Some(arr) = rs["cart_velocities"].as_array() {
                for row in arr {
                    if let Some(a) = row.as_array()
                        && a.len() == 4
                        && let (Some(id), Some(vx), Some(vy), Some(vz)) = (
                            a[0].as_u64(),
                            a[1].as_f64(),
                            a[2].as_f64(),
                            a[3].as_f64(),
                        )
                    {
                        disk_cart_velocities.insert(id as u32, Vec3::new(vx, vy, vz));
                    }
                }
            }
            if let Some(arr) = rs["projectiles"].as_array() {
                for item in arr {
                    if let (Some(id), Some(kind), Some(pos), Some(vel)) = (
                        item["id"].as_u64(),
                        item["kind"].as_str(),
                        item["pos"].as_array(),
                        item["vel"].as_array(),
                    ) && pos.len() == 3
                        && vel.len() == 3
                    {
                        disk_projectiles.push(DiskProjectile {
                            id: id as u32,
                            kind: kind.to_owned(),
                            pos: Vec3::new(
                                pos[0].as_f64().unwrap_or(0.0),
                                pos[1].as_f64().unwrap_or(0.0),
                                pos[2].as_f64().unwrap_or(0.0),
                            ),
                            vel: Vec3::new(
                                vel[0].as_f64().unwrap_or(0.0),
                                vel[1].as_f64().unwrap_or(0.0),
                                vel[2].as_f64().unwrap_or(0.0),
                            ),
                            yaw: item["yaw"].as_f64().unwrap_or(0.0) as f32,
                            pitch: item["pitch"].as_f64().unwrap_or(0.0) as f32,
                            age: item["age"].as_u64().unwrap_or(0) as u32,
                            stuck: item["stuck"].as_bool().unwrap_or(false),
                        });
                    }
                }
            }
        }
    }

    if game.dim == dim {
        candidate_set.extend(game.world.chunks.keys().copied());
    } else if let Some(parked) = game.levels.parked.get(&dim) {
        candidate_set.extend(parked.world.chunks.keys().copied());
    }

    for g in snap.guests.values() {
        if Dim::from_id(&g.dimension) == dim {
            let gx = (g.position[0].floor() as i32) >> 4;
            let gz = (g.position[2].floor() as i32) >> 4;
            for dz in -3..=3 {
                for dx in -3..=3 {
                    candidate_set.insert((gx + dx, gz + dz));
                }
            }
        }
    }

    let mut candidate_chunks: Vec<(i32, i32)> = candidate_set.into_iter().collect();
    candidate_chunks.sort_unstable();

    let mut chunks = FxHashMap::default();
    let mut block_entities = HashMap::new();
    let mut block_names: FxHashMap<(i32, i32, i32), Tag> = FxHashMap::default();
    let mut be_attachments = Vec::new();
    let mut inhabited = FxHashMap::default();
    let mut chunk_ticks = HashMap::new();

    for &(cx, cz) in &candidate_chunks {
        let Some(mut disk_chunk) =
            cp_save.load(cx, cz, game.pack.assets.registry, game.res.biomes())
        else {
            continue;
        };
        for &(pos, ref be) in &disk_chunk.block_entities {
            block_entities.insert(pos, be.clone());
        }
        for &(pos, ref name) in &disk_chunk.block_names {
            block_names.insert(pos, name.clone());
        }
        if let Some(saved) = disk_chunk.saved.as_deref() {
            if let Some(Tag::Long(t)) = saved.root.get(&"InhabitedTime".into()) {
                inhabited.insert((cx, cz), *t);
            }
            be_attachments.extend(saved.block_entity_attachments.iter().cloned());
            let mut b_ticks = ChunkTicks::from_saved(saved.block_ticks.clone());
            let mut f_ticks = ChunkTicks::from_saved(saved.fluid_ticks.clone());
            b_ticks.unpack(snap.game_time);
            f_ticks.unpack(snap.game_time);
            chunk_ticks.insert((cx, cz), (b_ticks, f_ticks));
        } else {
            chunk_ticks.insert((cx, cz), (ChunkTicks::default(), ChunkTicks::default()));
        }

        let live_chunk_opt = if game.dim == dim {
            game.world.chunks.get(&(cx, cz))
        } else {
            game.levels
                .parked
                .get(&dim)
                .and_then(|p| p.world.chunks.get(&(cx, cz)))
        };

        if let Some(live_chunk) = live_chunk_opt {
            for (s_idx, disk_sec) in disk_chunk.sections.iter_mut().enumerate() {
                if let Some(live_sec) = live_chunk.sections.get(s_idx)
                    && live_sec.blocks == disk_sec.blocks
                    && live_sec.biomes == disk_sec.biomes
                {
                    *disk_sec = Arc::clone(live_sec);
                }
            }
        }
        chunks.insert((cx, cz), Arc::new(disk_chunk));
    }

    if chunks.is_empty() {
        return None;
    }

    let forced_chunks = cp_save.load_forced_chunks();
    let world_attachments = cp_save.load_attachments();
    let raids_dat = fs::read(raids_dat_path(&cp_dir, dim)).ok();

    let saved_game_time = game.world.game_time;
    let saved_random = game.world.random.clone();
    let saved_next_entity = game.next_entity;
    let saved_loaded_entities = std::mem::take(&mut game.world.loaded_entities);
    let saved_mobs = std::mem::take(&mut game.mobs);
    let saved_dropped = std::mem::take(&mut game.dropped);
    let saved_orbs = std::mem::take(&mut game.orbs);
    let saved_projectiles = std::mem::take(&mut game.creatures.projectiles);
    let saved_boats = std::mem::take(&mut game.boats);
    let saved_carts = std::mem::take(&mut game.carts);
    let saved_tnts = std::mem::take(&mut game.tnts);
    let saved_falling = std::mem::take(&mut game.falling);
    let saved_paintings = std::mem::take(&mut game.paintings);
    let saved_item_frames = std::mem::take(&mut game.item_frames);
    let saved_armor_stands = std::mem::take(&mut game.armor_stands);
    let saved_cushions = std::mem::take(&mut game.cushions);
    let saved_others = std::mem::take(&mut game.others);
    let saved_crystals = std::mem::take(&mut game.crystals);
    let saved_dragon = game.dragon.extract();
    let saved_knot_ids = std::mem::take(&mut game.knot_ids);
    let saved_persist = std::mem::take(&mut game.persist);
    let saved_tridents = std::mem::take(&mut game.tridents);
    let saved_rockets = std::mem::take(&mut game.rockets);
    let saved_breath = std::mem::take(&mut game.breath_clouds);
    let saved_fangs = std::mem::take(&mut game.fangs.list);
    let saved_ominous = std::mem::take(&mut game.ominous_spawners);

    game.world.game_time = snap.game_time;
    game.next_entity = saved_next_entity.max(snap.next_entity);

    let mut chunk_keys: Vec<(i32, i32)> = chunks.keys().copied().collect();
    chunk_keys.sort_unstable();
    for (cx, cz) in chunk_keys {
        let tags = cp_save.load_entities(cx, cz);
        if !tags.is_empty() {
            game.world.queue_entity_nbt((cx, cz), tags);
        }
    }
    game.spawn_loaded_entities();

    if !disk_uuid_to_id.is_empty() {
        let mut id_remap: HashMap<u32, u32> = HashMap::new();
        for (&spawned_id, uuid) in game.persist.uuids.iter() {
            if let Some(&orig_id) = disk_uuid_to_id.get(uuid) {
                id_remap.insert(spawned_id, orig_id);
            }
        }
        let map_id = |id: &mut u32| {
            if let Some(&orig) = id_remap.get(id) {
                *id = orig;
            }
        };
        let map_opt_id = |opt: &mut Option<u32>| {
            if let Some(id) = opt
                && let Some(&orig) = id_remap.get(id)
            {
                *id = orig;
            }
        };
        let map_who = |who: &mut horizon_client::mobs::Who| {
            if let horizon_client::mobs::Who::Mob(id) = who
                && let Some(&orig) = id_remap.get(id)
            {
                *id = orig;
            }
        };
        for m in game.mobs.iter_mut() {
            map_id(&mut m.id);
            m.random = horizon_client::mobs::entity_random(m.id, m.position);
            map_opt_id(&mut m.extra.mount);
            map_opt_id(&mut m.extra.rider);
            map_opt_id(&mut m.extra.leader);
            if let Some(target) = m.ai.target.as_mut() {
                map_who(target);
            }
            if let Some(hurt_by) = m.ai.last_hurt_by.as_mut() {
                map_who(hurt_by);
            }
            if let Some(angry_at) = m.extra.angry_at.as_mut() {
                map_who(angry_at);
            }
            if let Some(
                horizon_client::mobs::leash::Holder::Mob(id)
                | horizon_client::mobs::leash::Holder::Boat(id),
            ) = m.extra.leash.as_mut()
            {
                map_id(id);
            }
            if let Some(bee) = m.extra.bee.as_mut()
                && let Some(angry_at) = bee.angry_at.as_mut()
            {
                map_who(angry_at);
            }
            map_opt_id(&mut m.extra.frog.tongue_target);
            if let Some(rt) = disk_mob_rt.get(&m.id) {
                m.age = rt.age;
                m.invulnerable = rt.invulnerable;
                m.last_hurt = rt.last_hurt;
                m.swell = rt.swell;
                m.previous_swell = rt.previous_swell;
                m.ambient_timer = rt.ambient_timer;
                if rt.rng_state != 0 {
                    m.random = horizon_core::random::EntityRandom::own(LegacyRandom::from_state(
                        rt.rng_state,
                    ));
                }
                m.extra.anger = rt.anger;
                m.extra.anger_end = rt.anger_end;
                m.extra.anger_stamp = rt.anger_stamp;
                m.extra.counter = rt.counter;
                m.extra.counter2 = rt.counter2;
                m.extra.anim_ticks = rt.anim_ticks;
                m.extra.egg_time = rt.egg_time;
                m.extra.flag = rt.flag;
                m.extra.flag2 = rt.flag2;
                m.extra.vector = rt.vector;
                m.extra.ints = rt.ints;
                if rt.body_adopted {
                    horizon_client::mobs::sulfur_cube::adopt_body(m);
                }
            }
        }
        for d in game.dropped.iter_mut() {
            map_id(&mut d.id);
            if let Some(rt) = disk_item_rt.get(&d.id) {
                d.ticks = rt.ticks;
                d.bob_offset = rt.bob_offset;
                if rt.rng_state != 0 {
                    d.random = horizon_core::random::EntityRandom::own(LegacyRandom::from_state(rt.rng_state));
                }
            }
        }
        for o in game.orbs.iter_mut() {
            map_id(&mut o.id);
            if let Some(&ticks) = disk_orb_rt.get(&o.id) {
                o.ticks = ticks;
            }
        }
        game.creatures.projectiles.for_each_mut(|p| {
            map_id(&mut p.id);
            if let Some(owner) = p.owner.as_mut() {
                map_who(owner);
            }
        });
        game.boats.for_each_mut(|b| {
            map_id(&mut b.id);
            if let Some(&vel) = disk_boat_velocities.get(&b.id) {
                b.velocity = vel;
            }
        });
        game.carts.for_each_mut(|c| {
            map_id(&mut c.id);
            if let Some(&vel) = disk_cart_velocities.get(&c.id) {
                c.velocity = vel;
            }
        });
        for t in game.tnts.iter_mut() {
            map_id(&mut t.id);
        }
        game.falling.for_each_mut(|f| {
            map_id(&mut f.id);
        });
        for tnt_val in disk_tnts {
            if game.tnts.iter().all(|t| t.id != tnt_val.id) {
                game.tnts.push(super::super::tnt::PrimedTnt {
                    attachment_lifetime: Default::default(),
                    id: tnt_val.id,
                    position: tnt_val.pos,
                    previous: tnt_val.pos,
                    velocity: tnt_val.vel,
                    fuse: tnt_val.fuse,
                    power: tnt_val.power,
                    no_gravity: tnt_val.no_gravity,
                    on_ground: tnt_val.on_ground,
                    fall_distance: tnt_val.fall_distance,
                    rotation: tnt_val.rot,
                    used_portal: false,
                    join: Default::default(),
                    stuck: Vec3::ZERO,
                    in_water: false,
                    owner: None,
                    guest: None,
                    block_state: None,
                });
            }
        }
        for fb in disk_falling {
            if game.falling.iter().all(|f| f.id != fb.id) {
                let block_name = game
                    .pack
                    .assets
                    .registry
                    .state(fb.state)
                    .map(|s| s.name.as_str())
                    .unwrap_or("minecraft:sand");
                let tag = Tag::compound([
                    ("id", Tag::String("minecraft:falling_block".into())),
                    (
                        "Pos",
                        Tag::List(vec![
                            Tag::Double(fb.pos.x.to_bits()),
                            Tag::Double(fb.pos.y.to_bits()),
                            Tag::Double(fb.pos.z.to_bits()),
                        ]),
                    ),
                    (
                        "Motion",
                        Tag::List(vec![
                            Tag::Double(fb.vel.x.to_bits()),
                            Tag::Double(fb.vel.y.to_bits()),
                            Tag::Double(fb.vel.z.to_bits()),
                        ]),
                    ),
                    ("Time", Tag::Int(fb.age)),
                    (
                        "BlockState",
                        Tag::compound([("Name", Tag::String(block_name.into()))]),
                    ),
                    ("DropItem", Tag::Byte(if fb.drop_item { 1 } else { 0 })),
                    ("NoGravity", Tag::Byte(if fb.no_gravity { 1 } else { 0 })),
                    ("FallDistance", Tag::Float((fb.fall_distance as f32).to_bits())),
                    ("OnGround", Tag::Byte(if fb.on_ground { 1 } else { 0 })),
                ]);
                game.load_falling(tag, None);
                if !game.falling.is_empty() {
                    let last_idx = game.falling.len() - 1;
                    game.falling.update(last_idx, |last| {
                        last.id = fb.id;
                        last.rotation = fb.rot;
                    });
                }
            }
        }
        for pr in disk_projectiles {
            if game.creatures.projectiles.iter().all(|p| p.id != pr.id) {
                let tag = Tag::compound([
                    ("id", Tag::String((&pr.kind[..]).into())),
                    (
                        "Pos",
                        Tag::List(vec![
                            Tag::Double(pr.pos.x.to_bits()),
                            Tag::Double(pr.pos.y.to_bits()),
                            Tag::Double(pr.pos.z.to_bits()),
                        ]),
                    ),
                    (
                        "Motion",
                        Tag::List(vec![
                            Tag::Double(pr.vel.x.to_bits()),
                            Tag::Double(pr.vel.y.to_bits()),
                            Tag::Double(pr.vel.z.to_bits()),
                        ]),
                    ),
                    (
                        "Rotation",
                        Tag::List(vec![
                            Tag::Float(pr.yaw.to_bits()),
                            Tag::Float(pr.pitch.to_bits()),
                        ]),
                    ),
                    ("Age", Tag::Int(pr.age as i32)),
                    ("inGround", Tag::Byte(if pr.stuck { 1 } else { 0 })),
                ]);
                let _ = game.load_listed(&pr.kind, None, None, tag);
                if !game.creatures.projectiles.is_empty() {
                    let last_idx = game.creatures.projectiles.len() - 1;
                    game.creatures.projectiles.update(last_idx, |last| {
                        last.id = pr.id;
                    });
                }
            }
        }
        for p in &mut game.paintings {
            map_id(&mut p.id);
        }
        for f in &mut game.item_frames {
            map_id(&mut f.id);
        }
        for a in game.armor_stands.iter_mut() {
            map_id(&mut a.id);
            map_opt_id(&mut a.vehicle);
        }
        for c in &mut game.cushions {
            map_id(&mut c.id);
        }
        game.others.for_each_mut(|o| map_id(&mut o.id));
        for c in &mut game.crystals {
            map_id(&mut c.id);
        }
        if let Some(d) = game.dragon.as_mut() {
            map_id(&mut d.id);
        }
        for t in &mut game.tridents {
            map_id(&mut t.id);
            map_id(&mut t.owner);
        }
        for r in &mut game.rockets {
            map_id(&mut r.id);
            if let Some(owner) = r.owner.as_mut() {
                map_who(owner);
            }
            map_opt_id(&mut r.attached_to);
        }
        game.breath_clouds.for_each_mut(|c| map_id(&mut c.id));
        game.fangs.list.for_each_mut(|f| map_id(&mut f.id));
        game.ominous_spawners.for_each_mut(|o| map_id(&mut o.id));
        for id in game.knot_ids.values_mut() {
            map_id(id);
        }
        let old_uuids: HashMap<u32, [i32; 4]> = (*std::mem::take(&mut game.persist.uuids)).clone();
        for (mut k, v) in old_uuids {
            map_id(&mut k);
            game.persist.uuids.insert(k, v);
        }
        let old_raw = std::mem::take(&mut game.persist.raw);
        for (mut k, v) in old_raw {
            map_id(&mut k);
            game.persist.raw.insert(k, v);
        }
        let old_pslots = std::mem::take(&mut game.persist.passenger_slots);
        for (mut vid, mut slots) in old_pslots {
            map_id(&mut vid);
            for slot in &mut slots {
                if let super::super::persist::PassengerSlot::Live(rider_id) = slot {
                    map_id(rider_id);
                }
            }
            game.persist.passenger_slots.insert(vid, slots);
        }
    }

    let registry = game.pack.assets.registry;
    let tridents = game
        .tridents
        .iter()
        .map(|t| {
            let uuid = game.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&t.id);
            TridentSnapshot {
                join: t.join,
                id: t.id,
                nbt: t.to_nbt(uuid, registry, raw),
                previous: t.previous,
                in_ground_time: t.in_ground_time,
                no_physics: t.no_physics,
                owner: t.owner,
                guest: t.guest,
            }
        })
        .collect();
    let rockets = game
        .rockets
        .iter()
        .map(|r| {
            let uuid = game.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&r.id);
            RocketSnapshot {
                id: r.id,
                nbt: r.to_nbt(uuid, None, raw),
                attached: r.attached,
                guest: r.guest,
                owner: r.owner,
                attached_to: r.attached_to,
            }
        })
        .collect();
    let breath_clouds = game
        .breath_clouds
        .iter()
        .map(|c| {
            let uuid = game.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
            let raw = game.persist.raw.get(&c.id);
            BreathSnapshot {
                id: c.id,
                nbt: super::super::potions::potion_cloud_to_nbt(c, uuid, raw),
                at: c.at,
                motion: c.motion,
                radius: c.radius,
                age: c.age,
                duration: c.duration,
                from_dragon: c.from_dragon,
                potion_none: c.potion.is_none(),
                victims: c.victims.clone(),
            }
        })
        .collect();
    let ominous_spawners = game
        .ominous_spawners
        .iter()
        .map(|s| {
            (
                s.id,
                horizon_client::entity_nbt::ominous_item_spawner_to_nbt(
                    &s.item,
                    Vec3::new(s.position[0], s.position[1], s.position[2]),
                    s.spawn_item_after,
                    s.uuid,
                    s.raw.as_deref(),
                ),
            )
        })
        .collect();
    let fangs = std::mem::replace(&mut game.fangs.list, saved_fangs);

    let mobs = std::mem::replace(&mut game.mobs, saved_mobs);
    let dropped = std::mem::replace(&mut game.dropped, saved_dropped);
    let orbs = std::mem::replace(&mut game.orbs, saved_orbs);
    let projectiles = std::mem::replace(&mut game.creatures.projectiles, saved_projectiles);
    let boats = std::mem::replace(&mut game.boats, saved_boats);
    let carts = std::mem::replace(&mut game.carts, saved_carts);
    let tnts = std::mem::replace(&mut game.tnts, saved_tnts);
    let falling = std::mem::replace(&mut game.falling, saved_falling);
    let paintings = std::mem::replace(&mut game.paintings, saved_paintings);
    let item_frames = std::mem::replace(&mut game.item_frames, saved_item_frames);
    let armor_stands = std::mem::replace(&mut game.armor_stands, saved_armor_stands);
    let cushions = std::mem::replace(&mut game.cushions, saved_cushions);
    let others = std::mem::replace(&mut game.others, saved_others);
    let crystals = std::mem::replace(&mut game.crystals, saved_crystals);
    let dragon = game.dragon.extract().as_ref().map(DragonSnapshot::from_live);
    if let Some(d) = saved_dragon {
        game.dragon.replace(d);
    } else {
        game.dragon.discard();
    }
    let knot_ids = std::mem::replace(&mut game.knot_ids, saved_knot_ids);
    let persist_mem = std::mem::replace(&mut game.persist, saved_persist);
    game.tridents = saved_tridents;
    game.rockets = saved_rockets;
    game.breath_clouds = saved_breath;
    game.ominous_spawners = saved_ominous;
    game.world.loaded_entities = saved_loaded_entities;
    game.world.game_time = saved_game_time;
    game.world.random = saved_random.clone();
    game.next_entity = saved_next_entity;

    let hydrated_random = disk_level_rng
        .map(|st| LevelRandom::for_level(LegacyRandom::from_state(st)))
        .unwrap_or(saved_random);
    let hydrated_rtv = disk_random_tick_value.unwrap_or(game.block_ticks.random_tick_value);

    let globals_tag = read_dat(&cp_dir.join("rreturn_globals.dat"));
    let border_key = match dim {
        Dim::Overworld => "border_overworld",
        Dim::Nether => "border_nether",
        Dim::End => "border_end",
        Dim::Extra(_) => "border_extra",
    };
    let hydrated_border = globals_tag
        .as_ref()
        .and_then(|t| t.get(border_key))
        .map(WorldBorder::unpack)
        .or_else(|| {
            read_dat(&cp_dim_dir.join("data/minecraft/world_border.dat"))
                .and_then(|t| t.get("data").map(WorldBorder::unpack))
        })
        .unwrap_or_else(|| {
            if dim == Dim::Overworld {
                snap.border.clone()
            } else {
                WorldBorder::default()
            }
        });

    Some(LevelSnapshot {
        chunks,
        block_entities,
        block_names,
        be_attachments,
        world_attachments,
        chest_openers: disk_chest_openers,
        border: hydrated_border,
        shulker_anims: Vec::new(),
        gateway_anims: disk_gateway_anims,
        fangs,
        ominous_spawners,
        portals_entities: disk_portals_entities,
        lectern_pulses: disk_lectern_pulses,
        lid_openers: disk_lid_openers,
        inhabited,
        end_clock: disk_end_clock,
        random: hydrated_random,
        forced_chunks,
        chunk_ticks,
        random_tick_value: hydrated_rtv,
        comparator_outputs: disk_comparator_outputs,
        torch_toggles: disk_torch_toggles,
        moving_pistons: disk_moving_pistons,
        piston_pushes: disk_piston_pushes,
        block_events: disk_block_events,
        hoppers: disk_hoppers,
        mobs,
        dropped,
        orbs,
        projectiles,
        boats,
        carts,
        tnts,
        falling,
        paintings,
        item_frames,
        armor_stands,
        cushions,
        others,
        crystals,
        dragon,
        tridents,
        rockets,
        breath_clouds,
        knot_ids,
        persist_uuids: (*persist_mem.uuids).clone(),
        persist_raw: persist_mem.raw,
        persist_passenger_slots: persist_mem.passenger_slots,
        raids_dat,
        last_hurt_by_player: disk_last_hurt_by_player,
        love_cause: disk_love_cause,
    })
}

pub fn hydrate_slot_from_disk_on_server(
    game: &mut Game<'_>,
    slot: &str,
) -> Option<Arc<TimelineSnapshot>> {
    let mut snap = match get_slot_snapshot_clone(slot) {
        Some(existing) => (*existing).clone(),
        None => load_slot_from_disk(&game.world_root, slot)?,
    };

    for dim in [Dim::Overworld, Dim::Nether, Dim::End] {
        if !snap.levels.contains_key(&dim) {
            if let Some(lsnap) = hydrate_level_from_disk(game, slot, dim, &snap) {
                if dim == game.dim {
                    set_client_slot_snapshot(
                        slot,
                        ClientBlockSnapshot {
                            dim,
                            chunks: lsnap.chunks.clone(),
                        },
                    );
                }
                snap.levels.insert(dim, lsnap);
            }
        }
    }

    let arc = Arc::new(snap);
    set_slot_snapshot(slot, Arc::clone(&arc));
    Some(arc)
}


pub fn apply_level_snapshot_in_place(
    game: &mut Game<'_>,
    dim: Dim,
    snap: &LevelSnapshot,
    policy: &RollbackPolicy,
    snap_game_time: i64,
) -> (f64, f64, f64) {
    let old_chunks: FxHashMap<(i32, i32), Arc<Chunk>> = (&*game.world.chunks).clone();

    let t_blocks = std::time::Instant::now();
    if policy.blocks || policy.biomes {
        let mut changed_coords = Vec::new();
        let mut biome_changed_chunks = Vec::new();

        for (&(cx, cz), snap_chunk) in snap.chunks.iter() {
            if let Some(live_chunk) = old_chunks.get(&(cx, cz)) {
                if Arc::ptr_eq(live_chunk, snap_chunk) {
                    continue;
                }
                let mut chunk_biome_changed = false;
                let min_y = snap_chunk.min_y();
                for (s_idx, snap_sec) in snap_chunk.sections.iter().enumerate() {
                    let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                        continue;
                    };
                    if Arc::ptr_eq(live_sec, snap_sec) {
                        continue;
                    }
                    if policy.biomes && snap_sec.biomes != live_sec.biomes {
                        chunk_biome_changed = true;
                    }
                    if !policy.blocks {
                        continue;
                    }
                    if let (Some(u1), Some(u2)) =
                        (snap_sec.blocks.uniform(), live_sec.blocks.uniform())
                    {
                        if u1 == u2 {
                            continue;
                        }
                    }
                    if snap_sec.blocks == live_sec.blocks {
                        continue;
                    }

                    let y_base = min_y + (s_idx as i32) * 16;
                    let x_base = cx * 16;
                    let z_base = cz * 16;
                    for sy in 0..16 {
                        for sz in 0..16 {
                            for sx in 0..16 {
                                let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                                let want = snap_sec.get(idx);
                                if live_sec.get(idx) != want {
                                    changed_coords.push((
                                        x_base + sx,
                                        y_base + sy,
                                        z_base + sz,
                                        want,
                                    ));
                                }
                            }
                        }
                    }
                }
                if chunk_biome_changed {
                    biome_changed_chunks.push((cx, cz));
                }
            }
        }
        for (&(cx, cz), live_chunk) in old_chunks.iter() {
            if !snap.chunks.contains_key(&(cx, cz)) {
                if let Some(disk_chunk) =
                    game.save.load(cx, cz, game.pack.assets.registry, game.res.biomes())
                {
                    let mut chunk_biome_changed = false;
                    let min_y = disk_chunk.min_y();
                    for (s_idx, disk_sec) in disk_chunk.sections.iter().enumerate() {
                        let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                            continue;
                        };
                        if policy.biomes && disk_sec.biomes != live_sec.biomes {
                            chunk_biome_changed = true;
                        }
                        if !policy.blocks {
                            continue;
                        }
                        if disk_sec.blocks == live_sec.blocks {
                            continue;
                        }
                        let y_base = min_y + (s_idx as i32) * 16;
                        let x_base = cx * 16;
                        let z_base = cz * 16;
                        for sy in 0..16 {
                            for sz in 0..16 {
                                for sx in 0..16 {
                                    let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                                    let want = disk_sec.get(idx);
                                    if live_sec.get(idx) != want {
                                        changed_coords.push((
                                            x_base + sx,
                                            y_base + sy,
                                            z_base + sz,
                                            want,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    if chunk_biome_changed {
                        biome_changed_chunks.push((cx, cz));
                    }
                }
            }
        }

        bench_add_chunks(snap.chunks.len());
        bench_add_blocks_changed(changed_coords.len());

        if policy.blocks {
            game.world.block_log_held.clear();
            for (wx, wy, wz, want) in changed_coords {
                let _ = game.world.set(wx, wy, wz, BlockStateId(want));
            }
        }
        bench_accum(Phase::ChunkBlocks, t_blocks);

        if policy.blocks && policy.biomes {
            game.world.chunks = Default::default();
            for (&k, v) in &snap.chunks {
                game.world.chunks.insert(k, v.clone());
            }
            game.world.inhabited = snap.inhabited.clone();
        } else {
            game.world.chunks = Default::default();
            for (&(cx, cz), snap_chunk) in &snap.chunks {
                if let Some(live_chunk) = old_chunks.get(&(cx, cz)) {
                    let mut c = (**live_chunk).clone();
                    for (s_idx, snap_sec) in snap_chunk.sections.iter().enumerate() {
                        if let Some(sec_arc) = c.sections.get_mut(s_idx) {
                            let sec = Arc::make_mut(sec_arc);
                            if policy.blocks {
                                sec.blocks = snap_sec.blocks.clone();
                            }
                            if policy.biomes {
                                sec.biomes = snap_sec.biomes.clone();
                            }
                        }
                    }
                    game.world.chunks.insert((cx, cz), Arc::new(c));
                } else {
                    game.world.chunks.insert((cx, cz), snap_chunk.clone());
                }
            }
        }
        if policy.biomes
            && !biome_changed_chunks.is_empty()
            && let Some(host) = game.lan.as_mut()
        {
            host.hub.resend_biomes(&game.world, &biome_changed_chunks);
        }
    }

    let blocks_ms = (std::time::Instant::now() - t_blocks).as_secs_f64() * 1000.0;
    let t_containers = std::time::Instant::now();
    if policy.block_entities {
        let mut be_positions: HashSet<(i32, i32, i32)> =
            game.world.block_entities.keys().copied().collect();
        be_positions.extend(snap.block_entities.keys().copied());
        game.world.block_entities.clear();
        let mut by_chunk: BTreeMap<(i32, i32), Vec<((i32, i32, i32), BlockEntity)>> =
            BTreeMap::new();
        for (&pos, be) in &snap.block_entities {
            if pos.1 == super::super::minecarts::CART_KEY_Y {
                continue;
            }
            by_chunk
                .entry((pos.0 >> 4, pos.2 >> 4))
                .or_default()
                .push((pos, be.clone()));
        }
        for (_, mut chunk_bes) in by_chunk {
            chunk_bes.sort_unstable_by_key(|(pos, _)| *pos);
            game.world.block_entities.load_chunk(chunk_bes);
        }
        for (pos, att) in &snap.be_attachments {
            game.world
                .block_entities
                .restore_attachments(*pos, att.clone());
        }
        game.world.block_names = snap.block_names.clone();
        game.hoppers = snap.hoppers.clone();
        if let Some(log) = game.world.block_log.as_mut() {
            for pos in be_positions {
                log.push(pos);
            }
        }
        bench_accum(Phase::ChunkBeLoad, t_containers);
    }
    let containers_ms = (std::time::Instant::now() - t_containers).as_secs_f64() * 1000.0;
    let t_entities = std::time::Instant::now();

    for &key in old_chunks.keys().chain(game.world.chunks.keys()) {
        game.random_tick_sections.remove(&key);
        game.portals.sections.remove(&key);
    }
    if game.tick_chunks.synchronize(&game.world.chunks) {
        let added = game.tick_chunks.take_added();
        game.tick_chunks.recycle_added(added);
    }
    game.update_chunk_tickets();
    game.start_ticking_chunks();

    if policy.block_ticks || policy.fluid_ticks {
        let mut old_ticks = FxHashMap::default();
        if !policy.block_ticks || !policy.fluid_ticks {
            for &(cx, cz) in old_chunks.keys() {
                let (old_b, old_f) = game.block_ticks.remove_chunk(ChunkPos::new(cx, cz));
                old_ticks.insert(
                    (cx, cz),
                    (old_b.unwrap_or_default(), old_f.unwrap_or_default()),
                );
            }
        }
        game.block_ticks.blocks = Default::default();
        game.block_ticks.fluids = Default::default();

        let mut snap_chunk_keys: Vec<(i32, i32)> = game.world.chunks.keys().copied().collect();
        snap_chunk_keys.sort_unstable();
        for (cx, cz) in snap_chunk_keys {
            let cpos = ChunkPos::new(cx, cz);
            let (old_b, old_f) = old_ticks.remove(&(cx, cz)).unwrap_or_default();
            let (snap_b, snap_f) = snap
                .chunk_ticks
                .get(&(cx, cz))
                .cloned()
                .unwrap_or_default();
            let b_to_add = if policy.block_ticks {
                let mut b = snap_b;
                b.unpack(snap_game_time);
                b
            } else {
                old_b
            };
            let f_to_add = if policy.fluid_ticks {
                let mut f = snap_f;
                f.unpack(snap_game_time);
                f
            } else {
                old_f
            };
            let _ = game.block_ticks.add_chunk(cpos, b_to_add, f_to_add);
        }
        game.block_ticks.random_tick_value = snap.random_tick_value;
        game.block_ticks.comparator_outputs = snap.comparator_outputs.clone();
        game.block_ticks.torch_toggles = snap.torch_toggles.clone();
    }

    if policy.block_events {
        game.block_ticks.moving_pistons.clear();
        for &(pos, piston) in &snap.moving_pistons {
            game.block_ticks.load_moving_piston(pos, piston);
        }
        game.world.moving_pistons = snap
            .moving_pistons
            .iter()
            .map(|&(pos, piston)| ([pos.x, pos.y, pos.z], piston.shape_state()))
            .collect();
        game.block_ticks.piston_pushes = snap.piston_pushes.clone();
        game.block_ticks.retain_block_events(|_| false);
        for &ev in &snap.block_events {
            game.block_ticks.queue_block_event(ev);
        }
    }

    if policy.world_time {
        game.world.end_clock = snap.end_clock;
    }

    if policy.world_level_rng {
        game.world.random = snap.random.clone();
    }

    if policy.server_world_border {
        game.border = snap.border.clone();
    }

    if policy.server_forced_chunks {
        game.streaming.forced = snap.forced_chunks.clone();
    }

    if policy.entities_presence {
        game.fangs.list = snap.fangs.clone();
        game.ominous_spawners.clear();
        let saved_next = game.next_entity;
        for (oid, onbt) in &snap.ominous_spawners {
            game.load_ominous_item_spawner(onbt.clone());
            let last_idx = game.ominous_spawners.len().saturating_sub(1);
            if !game.ominous_spawners.is_empty() {
                game.ominous_spawners.update(last_idx, |last| {
                    last.id = *oid;
                });
            }
        }
        game.next_entity = saved_next;
        game.portals.entities = snap.portals_entities.clone();
        bench_add_mobs(snap.mobs.len());
        game.mobs.clone_from(&snap.mobs);
        if policy.dropped_items {
            game.dropped.clone_from(&snap.dropped);
        }
        if policy.experience_orbs {
            game.orbs.clone_from(&snap.orbs);
        }
        game.creatures.projectiles.clone_from(&snap.projectiles);
        game.boats.clone_from(&snap.boats);
        game.carts.clone_from(&snap.carts);
        game.tnts.clone_from(&snap.tnts);
        game.falling.clone_from(&snap.falling);
        game.paintings.clone_from(&snap.paintings);
        game.item_frames.clone_from(&snap.item_frames);
        game.armor_stands.clone_from(&snap.armor_stands);
        game.cushions.clone_from(&snap.cushions);
        game.others.clone_from(&snap.others);
        game.crystals.clone_from(&snap.crystals);
        if let Some(d) = snap.dragon.as_ref().and_then(|d| d.to_live(game.level.seed)) {
            game.dragon.replace(d);
        } else {
            game.dragon.discard();
        }
        game.knot_ids.clone_from(&snap.knot_ids);
        game.persist = Default::default();
        for (&id, &uuid) in &snap.persist_uuids {
            game.persist.uuids.insert(id, uuid);
        }
        game.persist.raw.clone_from(&snap.persist_raw);
        game.persist
            .passenger_slots
            .clone_from(&snap.persist_passenger_slots);
        game.tridents.clear();
        let registry = game.pack.assets.registry;
        for ts in &snap.tridents {
            if let Some(n) = horizon_client::entity_nbt::trident_from_nbt(&ts.nbt) {
                let mut t = super::super::trident::ThrownTrident::from_nbt(n, &ts.nbt, registry);
                t.id = ts.id;
                t.previous = ts.previous;
                t.in_ground_time = ts.in_ground_time;
                t.no_physics = ts.no_physics;
                t.owner = ts.owner;
                t.guest = ts.guest;
                game.tridents.push(t);
                if ts.join.is_joined()
                    && let Some(last) = game.tridents.last_mut()
                {
                    last.join = ts.join;
                }
            }
        }
        game.rockets.clear();
        for rs in &snap.rockets {
            if let Some(rn) = horizon_client::entity_nbt::firework_rocket_from_nbt(&rs.nbt, None) {
                let mut r = super::super::fireworks::Rocket::from_nbt(rn);
                r.id = rs.id;
                r.attached = rs.attached;
                r.guest = rs.guest;
                r.owner = rs.owner;
                r.attached_to = rs.attached_to;
                game.rockets.push(r);
            }
        }
        game.bolts.clear();
        game.breath_clouds.clear();
        for bs in &snap.breath_clouds {
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
        game.potions.clear_thrown();
        game.potions.clouds.clear();
    }

    if policy.world_raids {
        let path = raids_dat_path(&game.world_root, dim);
        match &snap.raids_dat {
            Some(bytes) => {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&path, bytes);
            }
            None => {
                let _ = fs::remove_file(&path);
            }
        }
        game.raiding.loaded = None;
        game.load_raids();
    }

    game.spawn_scratch = Default::default();
    game.mob_index = Default::default();
    let saved_tracker = game.sculk.tracker;
    let saved_tracker_loaded = game.sculk.tracker_loaded;
    let saved_darkness = game.sculk.darkness;
    game.sculk = Default::default();
    game.sculk.tracker = saved_tracker;
    game.sculk.tracker_loaded = saved_tracker_loaded;
    game.sculk.darkness = saved_darkness;
    game.sculk_prepare();
    game.sculk_sync_index();
    game.world.loaded_entities.clear();
    game.world.entities_pending.clear();
    game.world.loaded_chunks.clear();
    game.world.modified.clear();
    if policy.blocks {
        game.world.watched_writes.clear();
        if let Some(writes) = game.world.path_writes.as_mut() {
            writes.clear();
        }
        if policy.biomes {
            game.world.light_edits.clear();
            game.world.dirty_light.clear();
        }
    }
    if policy.blocks || policy.biomes || policy.block_entities {
        game.creaking = Default::default();
        game.structure_templates.clear();
        game.test_highlights.clear();
    }
    if policy.block_entities {
        game.world.chest_openers.clear();
        for &(k, v) in &snap.chest_openers {
            game.world.chest_openers.insert(k, v);
        }
        game.world.shulkers.clear();
    }
    if policy.block_events {
        game.world.moving_piston_fallback.clear();
    }
    game.world.attachments = snap.world_attachments.clone();
    game.world.attachment_syncs.clear();

    game.last_hurt_by_player = snap.last_hurt_by_player.clone();
    game.love_cause = snap.love_cause.clone();
    game.died.clear();
    game.combat = Default::default();
    game.guest_combat.clear();
    game.stabbed.clear();
    game.player_stabs.clear();
    game.kinetic_feedback.clear();
    game.brushing = Default::default();
    if policy.block_ticks || policy.block_entities {
        game.lectern_pulses = snap.lectern_pulses.clone();
    }
    let be_anim_kinds = std::mem::take(&mut game.be_anims.kinds);
    game.be_anims = Default::default();
    game.be_anims.kinds = be_anim_kinds;
    if game.be_anims.kinds.is_empty() {
        game.be_anims.classify(game.pack.assets.registry);
    }
    if policy.block_entities {
        game.lid_openers = snap.lid_openers.clone();
        for (pos, shulker) in &snap.shulker_anims {
            game.be_anims.shulkers.insert(*pos, shulker.clone());
        }
        for (pos, gw) in &snap.gateway_anims {
            game.be_anims.gateway_servers.insert(*pos, *gw);
        }
        game.sync_shulkers();
    }
    for (_, sound_id) in std::mem::take(&mut game.jukebox_sounds) {
        game.audio.stop_and_release(sound_id);
    }
    game.lan_songs.clear();
    for be in game.world.block_entities.values_mut() {
        if let BlockEntity::Jukebox(j) = be {
            if j.playing {
                j.loaded_clock = false;
                j.ticks = 0;
            }
        }
    }
    game.lan_cushion_breaks.clear();
    game.lan_global_events.clear();

    game.creatures.wind_source = None;
    game.creatures.late_wind_pushes.clear();
    game.creatures.wind_bursts.clear();
    game.creatures.smash_pushes.clear();
    game.creatures.sweeps.clear();
    game.creatures.ticked.clear();
    game.creatures.deferred_hits = false;
    game.creatures.hook_targets.clear();
    game.creatures.hook_pulls.clear();
    game.creatures.tick_buffer.clear();
    game.creatures.lan_explosions.clear();
    game.creatures.lan_sounds.clear();
    game.creatures.lan_particles.clear();
    game.leash_boats.clear();
    game.spawner_tick_positions.clear();
    game.trial_spawner_tick_positions.clear();
    game.pass_mobs = None;
    game.item_columns = None;
    game.pickup_columns = None;
    game.tnt_refused = None;
    game.mount = Default::default();
    game.posed_rider = game.riding;
    game.use_delay = 0;
    game.forced_hit = None;
    game.tick_hit = None.into();
    game.tick_mob = None;
    game.use_failed = false;

    game.spill = Default::default();
    game.pending = Default::default();
    game.particles.clear();
    game.particles.sounds.clear();
    game.pickups.clear();
    game.transient_blocks.clear();
    game.destroying.clear();
    game.mining = None;
    game.lan_events.clear();
    game.lan_block_events.clear();
    game.lan_entity_events.clear();
    game.lan_critical_hits.clear();
    game.lan_kinetic_hits.clear();
    game.guest_equipment_breaks.clear();
    game.started_riding.clear();
    game.admin_feedback.clear();
    game.cmds.waypoint_dirty.clear();
    game.set_camera(None);
    game.cmds.guest_cameras.clear();
    game.cmds.camera_requests.clear();
    game.cmds.profile_fetches.clear();
    game.cmds.pregen = None;
    game.streaming.set_band(None);
    game.cmds.waypoints = None;
    game.cmds.waypoint_icons.clear();
    game.mount_menu_reset();
    game.ender_open = None;
    game.close_sign_editor();
    game.close_dialogs();
    game.close_book();
    game.close_mod_screen();
    game.mod_container_viewers.clear();
    game.kept_block_entities.get_mut().clear();
    game.close_container();
    game.station = None;
    game.container = None;
    game.craft_table = None;
    game.sign_edit = None;
    game.book = None;
    game.command_block_screen = None;
    game.reset_entity_residency();

    bench_accum(Phase::SpawnNbt, t_entities);
    let entities_ms = (std::time::Instant::now() - t_entities).as_secs_f64() * 1000.0;
    (blocks_ms, containers_ms, entities_ms)
}


pub fn restore_player_motion(
    m: &mut horizon_client::motion::PlayerMotion,
    gs: &GuestSnapshot,
    policy: &RollbackPolicy,
    snap_game_time: i64,
) {
    let keep_pos = (
        m.position,
        m.old_position,
        m.yaw,
        m.head_yaw,
        m.pitch,
        m.ground,
        m.pose,
        m.dimensions,
        m.scale,
    );
    let keep_mot = (
        m.velocity,
        m.fall_distance,
        m.sprinting,
        m.sprint_input,
        m.shift_key_down,
        m.local_crouching,
        m.fall_flying,
        m.fall_fly_ticks,
        m.glide_ending,
        m.swimming,
        m.stuck_multiplier,
        m.piston,
        m.impulse,
    );
    let keep_hp = (
        m.health.health(),
        m.health.absorption(),
        m.fire_ticks,
        m.air,
        m.ticks_frozen,
    );
    let keep_food = m.health.food;
    let keep_abil = (
        m.invulnerable,
        m.may_fly,
        m.spectator,
        m.flying,
        m.flying_speed,
    );

    if let Some(saved_m) = &gs.motion {
        *m = saved_m.clone();
        if !policy.player_position {
            m.position = keep_pos.0;
            m.old_position = keep_pos.1;
            m.yaw = keep_pos.2;
            m.head_yaw = keep_pos.3;
            m.pitch = keep_pos.4;
            m.ground = keep_pos.5;
            m.pose = keep_pos.6;
            m.dimensions = keep_pos.7;
            m.scale = keep_pos.8;
        }
        if !policy.player_motion {
            m.velocity = keep_mot.0;
            m.fall_distance = keep_mot.1;
            m.sprinting = keep_mot.2;
            m.sprint_input = keep_mot.3;
            m.shift_key_down = keep_mot.4;
            m.local_crouching = keep_mot.5;
            m.fall_flying = keep_mot.6;
            m.fall_fly_ticks = keep_mot.7;
            m.glide_ending = keep_mot.8;
            m.swimming = keep_mot.9;
            m.stuck_multiplier = keep_mot.10;
            m.piston = keep_mot.11;
            m.impulse = keep_mot.12;
        }
        if !policy.player_health {
            let _ = m.set_health(keep_hp.0.max(1.0), false);
            let _ = m.internal_set_absorption(keep_hp.1);
            m.fire_ticks = keep_hp.2;
            m.air = keep_hp.3;
            m.ticks_frozen = keep_hp.4;
        } else {
            let _ = m.set_health(gs.health.max(1.0), false);
            let _ = m.internal_set_absorption(gs.absorption);
        }
        if !policy.player_hunger {
            m.health.food = keep_food;
        }
        if !policy.player_abilities {
            m.invulnerable = keep_abil.0;
            m.may_fly = keep_abil.1;
            m.spectator = keep_abil.2;
            m.flying = keep_abil.3;
            m.flying_speed = keep_abil.4;
        }
    } else {
        let is_invuln = gs.game_mode == 1 || gs.game_mode == 3;
        let is_spectator = gs.game_mode == 3;
        if let Some(tag) = &gs.saved {
            let _ = m.apply_saved_attributes(tag.get("attributes"));
        }
        if policy.player_abilities {
            m.invulnerable = is_invuln;
            m.may_fly = is_invuln || gs.allow_flight;
            m.spectator = is_spectator;
            m.flying = gs.flying;
            m.flying_speed = gs.flying_speed;
        }
        if policy.player_position {
            let pos = Vec3::new(gs.position[0], gs.position[1], gs.position[2]);
            m.position = pos;
            m.old_position = pos;
            m.yaw = gs.yaw;
            m.head_yaw = gs.head_yaw;
            m.pitch = gs.pitch;
            m.ground = gs.ground;
            m.sleeping = gs.sleeping;
        }
        if policy.player_motion {
            m.velocity = gs.velocity;
            m.fall_distance = gs.fall_distance;
            let _ = m.set_sprinting(gs.sprinting);
            m.shift_key_down = gs.sneaking;
            m.local_crouching = gs.sneaking;
            m.fall_flying = gs.fall_flying;
            if !gs.fall_flying {
                m.fall_fly_ticks = 0;
            }
            m.glide_ending = false;
            m.swimming = false;
            m.stuck_multiplier = Vec3::ZERO;
            m.piston = Default::default();
            m.impulse = Default::default();
            m.spawn_extra_particles_on_fall = false;
            m.auto_spin_attack_ticks = 0;
            m.auto_spin_attack_dmg = 0.0;
            m.fluids = Default::default();
            m.in_powder_snow = false;
            m.was_in_powder_snow = false;
        }
        if policy.player_health {
            m.fire_ticks = gs.fire_ticks;
            m.air = gs.air;
            m.ticks_frozen = gs.ticks_frozen;
            let _ = m.set_health(gs.health.max(1.0), false);
            let _ = m.internal_set_absorption(gs.absorption);
        }
        if policy.player_hunger {
            m.health.food.food = gs.food;
            m.health.food.saturation = gs.saturation;
            m.health.food.exhaustion = gs.exhaustion;
            m.health.food.timer = gs.food_timer;
        }
    }

    m.events.clear();
    m.health.dead = false;
    m.health.game_time = snap_game_time;
    m.health.hurt.no_action_time = 0;
    m.health.hurt.cooldown = 0;
    m.health.hurt.last_hurt = 0.0;
    m.health.hurt.hurt_duration = 0;
    m.health.hurt.hurt_time = 0;
    m.health.hurt.last_source = None;
    m.health.hurt.last_stamp = 0;
    m.health.hurt_animation_time = 0;
    m.health.hurt_dir = 0.0;
    m.health.indicated_dir = None;
}

pub fn restore_offline_files(saved: &HashMap<PathBuf, Vec<u8>>, restore_stats: bool) {
    for (path, bytes) in saved {
        let is_stat = path.to_string_lossy().contains("stats");
        if is_stat && !restore_stats {
            continue;
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, bytes);
    }
}

pub fn restore_players_on_rollback(
    game: &mut Game<'_>,
    snap: &TimelineSnapshot,
    prev_dim: Dim,
    target_dim: Dim,
) {
    let policy = &snap.policy;
    let net_clock_rate = game.network_clock_rate();
    if let Some(mut host) = game.lan.take() {
        if policy.server_world_border && prev_dim != target_dim {
            host.border_sent = None;
        }
        if policy.entities_presence {
            host.mob_players = snap.mob_players.clone();
        }
        host.states.clear();
        host.states_next_entity = None;
        host.broken.clear();
        host.commands.clear();
        host.poses.clear();
        host.quiet.clear();
        host.waypoints.clear();
        host.waypoint_dead.clear();
        host.idle_states = Default::default();
        host.hub.native_advancement_grants.clear();
        for gi in 0..host.hub.guests.len() {
            if !host.hub.guests[gi].placed {
                continue;
            }
            let uuid = host.hub.guests[gi].uuid;
            let conn = host.hub.guests[gi].conn;
            let tracked = host.hub.guests[gi].tracker.ids();
            if !tracked.is_empty() {
                host.hub.guests[gi]
                    .send(&C::RemoveEntities(play::RemoveEntities { ids: tracked }));
                host.hub.guests.update(gi, |g| g.tracker.clear());
            }

            let mut close_windows = vec![0i32];
            if let Some(m) = &host.hub.guests[gi].menu {
                close_windows.push(m.window);
            }
            if policy.player_inventory {
                if let Some(play_state) = host.play.get_mut(&conn) {
                    if let Some(lec) = play_state.lectern.take() {
                        close_windows.push(lec.window);
                    }
                    if let Some(merch) = play_state.merchant.take() {
                        close_windows.push(merch.window);
                    }
                    if let Some(mw) = play_state.mount_window.take() {
                        close_windows.push(mw.window);
                    }
                }
                host.hub.close_container(conn);
            } else {
                host.hub.close_container(conn);
            }
            close_windows.sort_unstable();
            close_windows.dedup();
            for w in close_windows {
                host.hub.guests[gi]
                    .send(&C::ContainerClose(play::ContainerClose { container: w }));
            }
            if prev_dim != target_dim {
                host.hub.guests[gi].send(&C::StopSound(play::StopSound {
                    source: None,
                    sound: None,
                }));
            }
            host.hub.guests.update(gi, |g| g.landing = false);

            let is_singleplayer = host.hub.guests.len() == 1
                || host.owner.as_ref().is_some_and(|o| o.uuid == uuid);
            let guest_snap = snap.guests.get(&uuid).or_else(|| {
                if is_singleplayer {
                    snap.guests.values().next()
                } else {
                    None
                }
            });

            if let Some(gs) = guest_snap {
                let ent_id = host.hub.guests[gi].entity_id as i32;
                let dim_changed =
                    policy.player_position && host.hub.guests[gi].dimension != gs.dimension;
                let old_effects: Vec<&'static str> = if policy.player_potion_effects {
                    let mut ids = std::collections::HashSet::new();
                    for e in &host.hub.guests[gi].effects.list {
                        ids.insert(e.id);
                    }
                    for s in &host.hub.guests[gi].sent_effects {
                        ids.insert(s.id);
                    }
                    if is_singleplayer {
                        for e in &game.potions.effects.list {
                            ids.insert(e.id);
                        }
                    }
                    ids.into_iter().collect()
                } else {
                    Vec::new()
                };
                host.hub.guests.update(gi, |g| {
                    if policy.player_position {
                        g.dimension = gs.dimension.clone();
                        g.position = gs.position;
                        g.previous_position = gs.position;
                        g.first_good = gs.position;
                        g.yaw = gs.yaw;
                        g.pitch = gs.pitch;
                        g.head_yaw = gs.head_yaw;
                        g.on_ground = gs.on_ground;
                    }
                    if policy.player_abilities {
                        g.game_mode = gs.game_mode;
                        g.previous_game_mode = gs.previous_game_mode;
                        g.abilities.flying = gs.flying;
                        g.abilities.flying_speed = gs.flying_speed;
                        g.abilities.may_fly = gs.allow_flight;
                    }
                    if policy.player_motion {
                        g.sprinting = gs.sprinting;
                        g.client_sprinting = gs.sprinting;
                        g.sneaking = gs.sneaking;
                        g.fall_flying = gs.fall_flying;
                        g.client_is_floating = false;
                        g.above_ground_ticks = 0;
                        g.rests_on_ground = gs.on_ground;
                        g.moves.clear();
                        g.moves_from_ground = gs.on_ground;
                        g.known_movement = [0.0; 3];
                    }
                    g.sleeping = gs.sleeping;
                    g.shoulders = gs.shoulders;
                    g.living_flags = gs.living_flags;
                    g.stuck = gs.stuck;
                    g.bed = gs.bed;
                    g.woke_from = None;
                    g.sent_bed = None;
                    g.sleep_timer = gs.sleep_timer;

                    let prev_vehicle = g.vehicle;
                    g.vehicle = gs.vehicle;
                    g.vehicle_order = gs.vehicle_order;
                    g.vehicle_good = gs.vehicle_good;
                    g.vehicle_rests = false;
                    g.vehicle_floating = false;
                    g.above_ground_vehicle_ticks = 0;
                    if prev_vehicle != gs.vehicle {
                        if let Some(old_v) = prev_vehicle {
                            g.send(&C::SetPassengers(play::SetPassengers {
                                vehicle: old_v as i32,
                                passengers: Vec::new(),
                            }));
                        }
                        if let Some(new_v) = gs.vehicle {
                            g.send(&C::SetPassengers(play::SetPassengers {
                                vehicle: new_v as i32,
                                passengers: vec![g.entity_id],
                            }));
                        }
                    }

                    if policy.player_spawn_point {
                        g.spawn = gs.spawn;
                        g.respawn_elsewhere = gs.respawn_elsewhere.clone();
                    }
                    g.last_death = gs.last_death.clone();
                    if policy.player_health {
                        g.health = gs.health.max(1.0);
                        g.saved_absorption = Some(gs.absorption);
                    }
                    if policy.player_hunger {
                        g.food = gs.food;
                        g.saturation = gs.saturation;
                    }
                    if policy.player_experience {
                        g.xp_level = gs.xp_level;
                        g.xp_progress = gs.xp_progress;
                        g.xp_total = gs.xp_total;
                        g.xp_seed = gs.xp_seed;
                    }
                    if policy.player_inventory {
                        g.inventory = gs.inventory.clone();
                        g.armor = gs.armor.clone();
                        g.offhand = gs.offhand.clone();
                        g.selected = gs.selected;
                        g.craft_grid = gs.craft_grid.clone();
                        g.craft_result = gs.craft_result.clone();
                        g.carried = gs.carried.clone();
                    }
                    if policy.player_ender_chest {
                        g.ender_items = gs.ender_items.clone();
                    }
                    if policy.player_potion_effects {
                        g.effects.list = gs.effects.clone();
                        g.effects.tick_count = gs.effects_tick_count;
                        g.effects.changes = gs.effects_changes;
                    }
                    if policy.player_recipe_book {
                        g.recipe_book = gs.recipe_book.clone();
                        g.send(&C::RecipeBookSettings(recipes::RecipeBookSettings {
                            books: std::array::from_fn(|idx| {
                                (g.recipe_book.open[idx], g.recipe_book.filtering[idx])
                            }),
                        }));
                    }
                    g.advancements = gs.advancements.clone();
                    g.advancements_visible.clear();
                    g.advancements_changed.clear();
                    g.dead = false;
                    g.death_time = 0;
                    g.loading = false;
                    g.load_timeout = 0;
                    g.sent_health = None;
                    g.sent_xp = None;
                    g.awaiting_teleport = None;
                    g.send(&C::SetHealth(play::SetHealth {
                        health: g.health,
                        food: g.food,
                        saturation: g.saturation,
                    }));
                    g.send(&C::GameEvent(play::GameEvent {
                        event: 3,
                        param: gs.game_mode as f32,
                    }));
                    g.send(&g.abilities_packet());
                    g.seen_credits = gs.seen_credits;
                    g.post_effects = gs.post_effects.clone();
                    g.post_effects_dirty = true;
                    g.attachments = gs.attachments.clone();
                    g.saved = gs.saved.clone();
                    g.equipment = Default::default();
                    g.anticheat = Default::default();
                    g.record = None;
                    g.replay = None;
                    if let Ok(mut ch) = g.changes.lock() {
                        ch.clear();
                    }
                    g.sent_air = (-1, -1.0);
                    g.sent_frozen = -1;
                    if snap.policy.world_weather {
                        let rain_level = if snap.weather.raining { 1.0 } else { 0.0 };
                        let thunder_level = if snap.weather.thundering { 1.0 } else { 0.0 };
                        g.send(&C::GameEvent(play::GameEvent {
                            event: if snap.weather.raining { 2 } else { 1 },
                            param: 0.0,
                        }));
                        g.send(&C::GameEvent(play::GameEvent {
                            event: 7,
                            param: rain_level,
                        }));
                        g.send(&C::GameEvent(play::GameEvent {
                            event: 8,
                            param: thunder_level,
                        }));
                    }
                    g.sent_shared ^= -128;
                    g.sent_shoulders = [Some(-999), Some(-999)];
                    g.loaded_body = Some(SavedBody {
                        fire_ticks: gs.fire_ticks,
                        air: gs.air,
                        fall_distance: gs.fall_distance,
                        velocity: [gs.velocity.x, gs.velocity.y, gs.velocity.z],
                        food_exhaustion: gs.exhaustion,
                        food_tick_timer: gs.food_timer,
                        ticks_frozen: gs.ticks_frozen,
                        invulnerable: false,
                        invulnerable_time: 0,
                    });
                    let synced_attrs = g.motion.as_mut().map(|m| {
                        restore_player_motion(m, gs, policy, snap.game_time);
                        horizon_client::lan::attributes::syncable(&m.attributes)
                    });
                    if let Some(values) = synced_attrs
                        && !values.is_empty()
                    {
                        g.send(&C::UpdateAttributes(play::UpdateAttributes {
                            id: g.entity_id,
                            values,
                        }));
                    }
                    let pose_now = g.pose();
                    g.sent_pose = pose_now;
                    let mut shared_flags: i8 = 0;
                    if gs.fire_ticks > 0 {
                        shared_flags |= 0x01;
                    }
                    if gs.sneaking {
                        shared_flags |= 0x02;
                    }
                    if gs.sprinting {
                        shared_flags |= 0x08;
                    }
                    if gs.fall_flying {
                        shared_flags |= 0x80u8 as i8;
                    }

                    g.send(&C::SetEntityData(play::SetEntityData {
                        id: g.entity_id,
                        values: vec![
                            (
                                0,
                                horizon_protocol::entity_data::serializer::BYTE,
                                horizon_protocol::entity_data::Value::Byte(shared_flags),
                            ),
                            (
                                6,
                                horizon_protocol::entity_data::serializer::POSE,
                                horizon_protocol::entity_data::Value::Id(pose_now.id()),
                            ),
                            (
                                7,
                                horizon_protocol::entity_data::serializer::INT,
                                horizon_protocol::entity_data::Value::Int(gs.ticks_frozen),
                            ),
                            (
                                8,
                                horizon_protocol::entity_data::serializer::BYTE,
                                horizon_protocol::entity_data::Value::Byte(g.living_flags),
                            ),
                            (
                                12,
                                horizon_protocol::entity_data::serializer::INT,
                                horizon_protocol::entity_data::Value::Int(g.stuck.0),
                            ),
                            (
                                13,
                                horizon_protocol::entity_data::serializer::INT,
                                horizon_protocol::entity_data::Value::Int(g.stuck.1),
                            ),
                        ],
                    }));
                    if policy.player_inventory {
                        g.send(&C::SetHeldSlot(play::SetHeldSlot {
                            slot: gs.selected as i32,
                        }));
                    }

                    if let Some(active_snap_chunks) = snap.levels.get(&game.dim).map(|l| &l.chunks)
                    {
                        let forget: Vec<_> = g.sent_chunks.iter().copied().collect();
                        for (x, z) in forget {
                            let in_snap = active_snap_chunks.contains_key(&(x, z));
                            if !in_snap {
                                g.sent_chunks.remove(&(x, z));
                                g.send(&C::ForgetLevelChunk(play::ForgetLevelChunk {
                                    pos: [x, z],
                                }));
                            }
                        }
                    }

                    if policy.server_game_rules {
                        g.send(&C::TickingState(play::TickingState {
                            rate: snap.clock_rate,
                            frozen: snap.clock_frozen,
                        }));
                    }
                    if policy.world_difficulty {
                        g.send(&C::ChangeDifficulty(play::ChangeDifficulty {
                            difficulty: i32::from(snap.difficulty),
                            locked: snap.difficulty_locked,
                        }));
                    }
                    if policy.world_spawn
                        && let Some(pos) = snap.world_spawn
                    {
                        g.send(&C::SetDefaultSpawnPosition(play::SetDefaultSpawnPosition {
                            pos: play::GlobalPos {
                                dimension: "minecraft:overworld".to_owned(),
                                pos,
                            },
                            yaw: snap.world_spawn_angle[0],
                            pitch: snap.world_spawn_angle[1],
                        }));
                    }

                    if policy.server_bossbars
                        && dim_changed
                        && Dim::from_id(&gs.dimension) != Dim::End
                    {
                        let dragon_bar_id =
                            horizon_client::bossbar::stable_uuid("minecraft:ender_dragon");
                        g.send(&C::BossEvent(play::BossEvent {
                            id: dragon_bar_id,
                            action: play::BossAction::Remove,
                        }));
                    }
                });

                let mut cd_packets: Vec<(String, i32)> = Vec::new();
                if let Some(play_state) = host.play.get_mut(&conn) {
                    let prev_shield_cd = play_state.shield_cooldown;
                    let prev_cooldowns = std::mem::take(&mut play_state.cooldowns);
                    let prev_stats = std::mem::take(&mut play_state.stats);
                    let prev_hook = play_state.hook.extract();
                    let prev_left_game = play_state.left_game;
                    let prev_menus = if !policy.player_inventory {
                        Some((
                            play_state.lectern.take(),
                            play_state.merchant.take(),
                            play_state.mount_window.take(),
                        ))
                    } else {
                        None
                    };

                    *play_state = Default::default();
                    play_state.left_game = prev_left_game;

                    if let Some((lec, merch, mw)) = prev_menus {
                        play_state.lectern = lec;
                        play_state.merchant = merch;
                        play_state.mount_window = mw;
                    }

                    if policy.player_cooldowns {
                        let old_cds = extract_active_cooldowns(&prev_cooldowns);
                        let new_cds = extract_active_cooldowns(&gs.cooldowns);
                        for (old_grp, _) in &old_cds {
                            if !new_cds.iter().any(|(new_grp, _)| new_grp == old_grp) {
                                cd_packets.push((old_grp.to_string(), 0));
                            }
                        }
                        for (new_grp, ticks) in new_cds {
                            cd_packets.push((new_grp.to_string(), ticks));
                        }
                        play_state.shield_cooldown = gs.shield_cooldown;
                        play_state.cooldowns = gs.cooldowns.clone();
                    } else {
                        play_state.shield_cooldown = prev_shield_cd;
                        play_state.cooldowns = prev_cooldowns;
                    }

                    if policy.player_stats {
                        play_state.stats.values = gs.stats.clone();
                        play_state.stats.mark_all_dirty();
                        play_state.stats.events.clear();
                    } else {
                        play_state.stats = prev_stats;
                    }

                    if policy.entities_presence {
                        if let Some(hs) = &gs.hook {
                            let mut h = prev_hook.unwrap_or_else(|| {
                                fishing::Hook::rreturn_create_full(
                                    hs.id,
                                    hs.position,
                                    hs.previous,
                                    hs.velocity,
                                    hs.rng_state,
                                    hs.nibble,
                                    hs.time_until_lured,
                                    hs.time_until_hooked,
                                    hs.fish_angle,
                                    hs.open_water,
                                    hs.state_bobbing,
                                )
                            });
                            h.position = Vec3::new(hs.position[0], hs.position[1], hs.position[2]);
                            h.previous = Vec3::new(hs.previous[0], hs.previous[1], hs.previous[2]);
                            h.velocity = Vec3::new(hs.velocity[0], hs.velocity[1], hs.velocity[2]);
                            h.nibble = hs.nibble;
                            h.time_until_lured = hs.time_until_lured;
                            h.time_until_hooked = hs.time_until_hooked;
                            h.fish_angle = hs.fish_angle;
                            h.open_water = hs.open_water;
                            if hs.state_bobbing {
                                h.state = fishing::State::Bobbing;
                            }
                            if hs.rng_state != 0 {
                                h.random = horizon_core::random::EntityRandom::own(LegacyRandom::from_state(hs.rng_state));
                            }
                            play_state.hook.replace(h);
                        }
                    } else if let Some(h) = prev_hook {
                        play_state.hook.replace(h);
                    } else {
                        play_state.hook.discard();
                    }
                }
                for (group, duration) in cd_packets {
                    host.hub.guests[gi].send(&C::Cooldown(play::Cooldown { group, duration }));
                }

                if policy.player_abilities {
                    host.hub
                        .set_game_mode(conn, gs.game_mode, &game.world, Some(game.collision));
                }
                if policy.player_position {
                    if dim_changed {
                        let target_dim = Dim::from_id(&gs.dimension);
                        game.level_teleports
                            .push(super::super::server_travel::LevelTeleport {
                                conn,
                                dim: target_dim,
                                at: gs.position,
                                rotation: Some([gs.yaw, gs.pitch]),
                                vehicle: None,
                            });
                    } else {
                        host.hub.teleport(gi, Some(gs.position));
                    }
                }
                if policy.player_inventory {
                    host.hub.send_inventory(gi);
                }
                if policy.player_potion_effects {
                    for old_id in old_effects {
                        if !gs.effects.iter().any(|new_e| new_e.id == old_id) {
                            let full_id = if old_id.contains(':') {
                                old_id.to_string()
                            } else {
                                format!("minecraft:{}", old_id)
                            };
                            if let Some(effect_id) = host.hub.registry_id("minecraft:mob_effect", &full_id) {
                                let target_id = host.hub.guests[gi].entity_id;
                                host.hub.guests[gi].send(&C::RemoveMobEffect(play::RemoveMobEffect {
                                    id: target_id,
                                    effect: effect_id,
                                }));
                            }
                        }
                    }
                    host.hub.send_active_effects(gi);
                }
                if policy.entities_presence {
                    if let Some(hs) = &gs.hook {
                        if is_singleplayer || host.owner.as_ref().is_some_and(|o| o.uuid == uuid) {
                            game.rreturn_restore_hook_full(
                                hs.id,
                                hs.position,
                                hs.previous,
                                hs.velocity,
                                hs.rng_state,
                                hs.nibble,
                                hs.time_until_lured,
                                hs.time_until_hooked,
                                hs.fish_angle,
                                hs.open_water,
                                hs.state_bobbing,
                            );
                        } else if let Some(h) = game.hook.as_mut() {
                            h.position = Vec3::new(hs.position[0], hs.position[1], hs.position[2]);
                            h.previous = Vec3::new(hs.previous[0], hs.previous[1], hs.previous[2]);
                            h.velocity = Vec3::new(hs.velocity[0], hs.velocity[1], hs.velocity[2]);
                            h.nibble = hs.nibble;
                            h.time_until_lured = hs.time_until_lured;
                            h.time_until_hooked = hs.time_until_hooked;
                            h.fish_angle = hs.fish_angle;
                            h.open_water = hs.open_water;
                            if hs.state_bobbing {
                                h.state = fishing::State::Bobbing;
                            }
                            if hs.rng_state != 0 {
                                h.random = horizon_core::random::EntityRandom::own(LegacyRandom::from_state(hs.rng_state));
                            }
                        } else {
                            game.spawn_remote_hook(
                                hs.id,
                                [0; 4],
                                Vec3::new(hs.velocity[0], hs.velocity[1], hs.velocity[2]),
                                Vec3::new(hs.position[0], hs.position[1], hs.position[2]),
                                ent_id,
                            );
                        }
                    } else if let Some(h) = game.hook.extract() {
                        game.remove_remote_hook(h.id());
                    }
                }

                host.hub.flush_progress(conn);
            } else if !policy.preserve_new_player_inventory {
                if let Some(play_state) = host.play.get_mut(&conn) {
                    *play_state = Default::default();
                }
                host.hub.guests.update(gi, |g| {
                    g.inventory = vec![None; 36];
                    g.armor = [None, None, None, None];
                    g.offhand = None;
                    g.ender_items = vec![None; 27];
                    g.carried = None;
                });
                host.hub.send_inventory(gi);
            }
        }
        if !game.level_teleports.is_empty() {
            game.run_level_teleports();
        }
        game.server_trips.clear();
        game.teleport_notes.clear();
        game.spawn_wait.clear();
        for gi in 0..host.hub.guests.len() {
            let conn = host.hub.guests[gi].conn;
            host.hub.guests.update(gi, |g| {
                g.landing = false;
                g.loading = false;
                g.load_timeout = 0;
            });
            if let Some(play_state) = host.play.get_mut(&conn) {
                play_state.portal = Default::default();
            }
        }
        game.portals = Default::default();
        if policy.player_inventory {
            host.hub.native_menu_events.clear();
        }
        host.hub.native_tracking.clear();
        host.hub.attachment_snapshots.clear();
        if let Some(far) = host.hub.far.as_mut() {
            far.forget_level();
        }
        let _ = horizon_client::lan::handoff::take_changed();
        if policy.world_time {
            let overworld_clock = host
                .hub
                .sync_data()
                .id("minecraft:world_clock", "minecraft:overworld")
                .unwrap_or(0);
            let packet = horizon_protocol::packets::play::Clientbound::SetTime(
                horizon_protocol::packets::play::SetTime {
                    game_time: game.world.game_time,
                    clocks: vec![horizon_protocol::packets::play::ClockState {
                        clock: overworld_clock,
                        total_ticks: game.world.day_time,
                        partial_tick: 0.0,
                        rate: net_clock_rate,
                    }],
                },
            );
            host.hub.broadcast_level(&packet);
        }
        host.hub.synced.clear();
        game.lan = Some(host);
    }
    game.guest_packets.clear();
    game.guest_entity_events.clear();
    game.guest_player_sounds.clear();
    game.guest_block_sounds.clear();
    game.guest_cooldowns.clear();
    game.save_after_guest_turn = false;
    game.rotation_forced = false;

    if let Some(gs) = snap.guests.values().next() {
        if policy.player_abilities {
            game.set_mode_id(gs.game_mode);
        }
        if policy.player_cooldowns {
            game.shield_cooldown = gs.shield_cooldown;
            game.cooldowns = gs.cooldowns.clone();
        }
        if let Some(p) = game.player.as_mut() {
            restore_player_motion(p, gs, policy, snap.game_time);
            p.client_side = true;
        }
        if policy.player_inventory {
            game.location_armor = game.armor.clone();
            game.apply_enchantment_attributes();
        }
        if policy.player_potion_effects {
            game.potions.effects.list = gs.effects.clone();
            game.potions.effects.tick_count = gs.effects_tick_count;
            game.potions.effects.changes = gs.effects_changes;
            game.level.active_effects = game.potions.effects.saved();
        }
        if policy.player_potion_effects || policy.player_inventory {
            game.potions.applied.clear();
            game.apply_effect_status();
        }
    }

    game.ui.death_time = 0;
    game.ui.died_at = None;
    game.ui.death_confirm = None;
    game.item_activation.reset();
    game.titles = Default::default();
    game.flash = 0;
    game.screen = super::super::Screen::Playing;
    game.boss_overlay.reset();
    if policy.server_bossbars {
        game.bosses.sent.clear();
        game.publish_boss_bars();
    }
    game.using = snap.client_using;
    game.use_duration = snap.client_use_duration;
    game.crossbow_sounds = snap.client_crossbow_sounds;
}


pub fn apply_client_level_rollback(game: &mut Game<'_>, slot: &str) {
    let t_start = std::time::Instant::now();
    let _ = game.stop_remote_mining();
    if let Some(r) = game.remote.as_mut() {
        r.rreturn_reset_client_state();
    }
    for (_, sound_id) in std::mem::take(&mut game.jukebox_sounds) {
        game.audio.stop_and_release(sound_id);
    }
    game.mount_menu_reset();
    game.mount = Default::default();
    game.ender_open = None;
    game.close_sign_editor();
    game.close_dialogs();
    game.close_book();
    game.close_mod_screen();
    game.kept_block_entities.get_mut().clear();
    game.close_container();
    game.carried = None;
    game.craft_grid.fill(None);
    game.craft_size = 2;
    game.station = None;
    game.container = None;
    game.craft_table = None;
    game.sign_edit = None;
    game.book = None;
    game.command_block_screen = None;
    game.mining = None;
    game.clicks.miss_time = 0;
    game.hotbar_pop = [0; 10];
    game.screen_fx = Default::default();
    game.sculk.darkness = Default::default();
    game.stop_emote();
    game.clear_remote_hooks();
    game.end.clear_remote_eyes();
    game.maps = Default::default();
    game.recipe_ui.reset_screen();
    game.snap_player_model_pitch();
    game.water_plane_scan.set(None);
    game.water_plane_memo.set(None);
    game.adv.toasts.clear();
    game.cart_sounds.clear();
    game.riding_cart_sounds = None;
    game.pad.menu.stop_craft();
    game.pad_clear_focus();
    game.pad_player_replaced();
    game.far_beacons.clear();
    game.test_highlights.clear();
    game.spawner_tick_positions.clear();
    game.trial_spawner_tick_positions.clear();
    game.particles.clear();
    game.particles.sounds.clear();
    game.pickups.clear();
    game.transient_blocks.clear();
    game.destroying.clear();
    game.item_activation.reset();
    game.titles = Default::default();
    game.flash = 0;

    let client_snap = match get_client_slot_snapshot_clone(slot) {
        Some(s) => s,
        None => {
            let (px, pz) = game
                .player
                .as_ref()
                .map(|p| ((p.position.x as i32) >> 4, (p.position.z as i32) >> 4))
                .unwrap_or((0, 0));

            let mut candidate_chunks: HashSet<(i32, i32)> =
                game.world.modified.iter().copied().collect();
            for dz in -3..=3 {
                for dx in -3..=3 {
                    candidate_chunks.insert((px + dx, pz + dz));
                }
            }

            let mut diffs = Vec::new();
            let mut changed_biomes = Vec::new();
            for (cx, cz) in candidate_chunks {
                if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)) {
                    if let Some(disk_chunk) =
                        game.save
                            .load(cx, cz, game.pack.assets.registry, game.res.biomes())
                    {
                        let min_y = disk_chunk.min_y();
                        for (s_idx, disk_sec) in disk_chunk.sections.iter().enumerate() {
                            let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                                continue;
                            };
                            if disk_sec.biomes != live_sec.biomes {
                                changed_biomes.push((cx, cz, s_idx, disk_sec.biomes.clone()));
                            }
                            if disk_sec.blocks == live_sec.blocks {
                                continue;
                            }
                            let y_base = min_y + (s_idx as i32) * 16;
                            let x_base = cx * 16;
                            let z_base = cz * 16;
                            for sy in 0..16 {
                                for sz in 0..16 {
                                    for sx in 0..16 {
                                        let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                                        let want = disk_sec.get(idx);
                                        let current = live_sec.get(idx);
                                        if current != want {
                                            diffs.push((
                                                x_base + sx,
                                                y_base + sy,
                                                z_base + sz,
                                                current,
                                                want,
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let dirty_before = game.world.dirty_sections.len();
            for &(x, y, z, _, want) in &diffs {
                let _ = game.world.set(x, y, z, BlockStateId(want));
            }
            if !changed_biomes.is_empty() {
                let num_sections = game.world.layout().sections() as i32;
                for (cx, cz, s_idx, biomes) in changed_biomes {
                    if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)).cloned() {
                        let mut chunk = (*live_chunk).clone();
                        if let Some(sec_arc) = chunk.sections.get_mut(s_idx) {
                            Arc::make_mut(sec_arc).biomes = biomes;
                        }
                        game.world.chunks.insert((cx, cz), Arc::new(chunk));
                    }
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
                game.world.revision = game.world.revision.wrapping_add(1);
            }
            let dirty_marked = game.world.dirty_sections.len().saturating_sub(dirty_before);
            let elapsed_ms = (t_start.elapsed().as_nanos() as f64) / 1_000_000.0;

            sync_client_epoch();

            let mut report = format!(
                "[RReturn][Client-Spark] === CLIENT ROLLBACK [{slot}]: {elapsed_ms:.2} ms ===\n\
                 |-- Source: Disk Fallback (RAM Cache MISS)\n\
                 |-- Blocks Changed ({}):",
                diffs.len()
            );
            for &(x, y, z, old_id, new_id) in diffs.iter().take(10) {
                let old_name = game.res.facts().get(BlockStateId(old_id)).name.clone();
                let new_name = game.res.facts().get(BlockStateId(new_id)).name.clone();
                report.push_str(&format!(
                    "\n|   |-- [{x}, {y}, {z}]: {old_name} ({old_id}) -> {new_name} ({new_id})"
                ));
            }
            if diffs.len() > 10 {
                report.push_str(&format!("\n|   |-- ... and {} more", diffs.len() - 10));
            }
            report.push_str(&format!("\n|-- Dirty Sections Marked: {dirty_marked}"));
            eprintln!("{report}");
            log::info!("{report}");
            append_to_rreturn_log(&game.world_root, &report);
            return;
        }
    };

    if game.dim != client_snap.dim {
        let elapsed_ms = (t_start.elapsed().as_nanos() as f64) / 1_000_000.0;
        let report = format!(
            "[RReturn][Client-Spark] === CLIENT ROLLBACK [{slot}]: {elapsed_ms:.2} ms ===\n\
             |-- Status: Dimension Mismatch (game: {}, snap: {})\n\
             |-- Blocks Changed: 0",
            game.dim.id(),
            client_snap.dim.id()
        );
        eprintln!("{report}");
        log::info!("{report}");
        append_to_rreturn_log(&game.world_root, &report);
        return;
    }

    let old_chunks: FxHashMap<(i32, i32), Arc<Chunk>> = (&*game.world.chunks).clone();
    let chunks_total = client_snap.chunks.len();
    let mut chunks_skipped_ptr = 0usize;
    let mut chunks_evaluated = 0usize;
    let mut sections_total = 0usize;
    let mut sections_skipped_ptr = 0usize;
    let mut sections_skipped_uniform = 0usize;
    let mut sections_skipped_eq = 0usize;
    let mut sections_evaluated = 0usize;
    let mut changed_coords = Vec::new();
    let mut changed_biomes = Vec::new();

    let (px, pz) = game
        .player
        .as_ref()
        .map(|p| ((p.position.x as i32) >> 4, (p.position.z as i32) >> 4))
        .unwrap_or((0, 0));

    let mut chunk_keys: Vec<(i32, i32)> = client_snap.chunks.keys().copied().collect();
    chunk_keys.sort_unstable_by_key(|&(cx, cz)| (cx - px).abs().max((cz - pz).abs()));

    let t_scan = std::time::Instant::now();
    for &(cx, cz) in &chunk_keys {
        let snap_chunk = &client_snap.chunks[&(cx, cz)];
        if let Some(live_chunk) = old_chunks.get(&(cx, cz)) {
            if Arc::ptr_eq(live_chunk, snap_chunk) {
                chunks_skipped_ptr += 1;
                continue;
            }
            chunks_evaluated += 1;
            let min_y = snap_chunk.min_y();
            for (s_idx, snap_sec) in snap_chunk.sections.iter().enumerate() {
                sections_total += 1;
                let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                    continue;
                };
                if Arc::ptr_eq(live_sec, snap_sec) {
                    sections_skipped_ptr += 1;
                    continue;
                }
                if snap_sec.biomes != live_sec.biomes {
                    changed_biomes.push((cx, cz, s_idx, snap_sec.biomes.clone()));
                }
                if let (Some(u1), Some(u2)) =
                    (snap_sec.blocks.uniform(), live_sec.blocks.uniform())
                {
                    if u1 == u2 {
                        sections_skipped_uniform += 1;
                        continue;
                    }
                }
                if snap_sec.blocks == live_sec.blocks {
                    sections_skipped_eq += 1;
                    continue;
                }
                sections_evaluated += 1;

                let y_base = min_y + (s_idx as i32) * 16;
                let x_base = cx * 16;
                let z_base = cz * 16;
                for sy in 0..16 {
                    for sz in 0..16 {
                        for sx in 0..16 {
                            let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                            let want = snap_sec.get(idx);
                            let current = live_sec.get(idx);
                            if current != want {
                                changed_coords.push((
                                    x_base + sx,
                                    y_base + sy,
                                    z_base + sz,
                                    current,
                                    want,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    for (&(cx, cz), live_chunk) in old_chunks.iter() {
        if !client_snap.chunks.contains_key(&(cx, cz))
            && let Some(disk_chunk) =
                game.save
                    .load(cx, cz, game.pack.assets.registry, game.res.biomes())
        {
            chunks_evaluated += 1;
            let min_y = disk_chunk.min_y();
            for (s_idx, disk_sec) in disk_chunk.sections.iter().enumerate() {
                sections_total += 1;
                let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                    continue;
                };
                if disk_sec.biomes != live_sec.biomes {
                    changed_biomes.push((cx, cz, s_idx, disk_sec.biomes.clone()));
                }
                if disk_sec.blocks == live_sec.blocks {
                    sections_skipped_eq += 1;
                    continue;
                }
                sections_evaluated += 1;
                let y_base = min_y + (s_idx as i32) * 16;
                let x_base = cx * 16;
                let z_base = cz * 16;
                for sy in 0..16 {
                    for sz in 0..16 {
                        for sx in 0..16 {
                            let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                            let want = disk_sec.get(idx);
                            let current = live_sec.get(idx);
                            if current != want {
                                changed_coords.push((
                                    x_base + sx,
                                    y_base + sy,
                                    z_base + sz,
                                    current,
                                    want,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    let scan_ms = (t_scan.elapsed().as_nanos() as f64) / 1_000_000.0;

    let t_set = std::time::Instant::now();
    let dirty_before = game.world.dirty_sections.len();
    for &(wx, wy, wz, _, want) in &changed_coords {
        let _ = game.world.set(wx, wy, wz, BlockStateId(want));
    }
    if !changed_biomes.is_empty() {
        let num_sections = game.world.layout().sections() as i32;
        for (cx, cz, s_idx, biomes) in changed_biomes {
            if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)).cloned() {
                let mut chunk = (*live_chunk).clone();
                if let Some(sec_arc) = chunk.sections.get_mut(s_idx) {
                    Arc::make_mut(sec_arc).biomes = biomes;
                }
                game.world.chunks.insert((cx, cz), Arc::new(chunk));
            }
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
        game.world.revision = game.world.revision.wrapping_add(1);
    }
    let dirty_marked = game.world.dirty_sections.len().saturating_sub(dirty_before);
    let set_ms = (t_set.elapsed().as_nanos() as f64) / 1_000_000.0;
    let elapsed_ms = (t_start.elapsed().as_nanos() as f64) / 1_000_000.0;

    sync_client_epoch();

    let mut report = format!(
        "[RReturn][Client-Spark] === CLIENT ROLLBACK [{slot}]: {elapsed_ms:.2} ms ===\n\
         |-- Phase 1 (Cache Lookup): {:.2} ms [RAM HIT | dim: {}]\n\
         |-- Phase 2 (Chunks & Sections Scan): {scan_ms:.2} ms\n\
         |   |-- Chunks: {chunks_total} total | {chunks_skipped_ptr} ptr_eq | {chunks_evaluated} eval (nearest-first)\n\
         |   |-- Sections: {sections_total} in eval | {sections_skipped_ptr} ptr_eq | {sections_skipped_uniform} uniform | {sections_skipped_eq} content_eq | {sections_evaluated} eval\n\
         |-- Phase 3 (Block World Sets): {set_ms:.2} ms (blocks changed: {})\n\
         |-- Phase 4 (Mesher Invalidation): {dirty_marked} dirty sections marked",
        (t_scan.duration_since(t_start).as_nanos() as f64) / 1_000_000.0,
        game.dim.id(),
        changed_coords.len()
    );
    if !changed_coords.is_empty() {
        report.push_str("\n|-- Changed Blocks Detail:");
        for &(x, y, z, old_id, new_id) in changed_coords.iter().take(10) {
            let old_name = game.res.facts().get(BlockStateId(old_id)).name.clone();
            let new_name = game.res.facts().get(BlockStateId(new_id)).name.clone();
            report.push_str(&format!(
                "\n|   |-- [{x}, {y}, {z}]: {old_name} ({old_id}) -> {new_name} ({new_id})"
            ));
        }
        if changed_coords.len() > 10 {
            report.push_str(&format!(
                "\n|   |-- ... and {} more",
                changed_coords.len() - 10
            ));
        }
    }
    eprintln!("{report}");
    log::info!("{report}");
    append_to_rreturn_log(&game.world_root, &report);
}


pub fn rollback_checkpoint_on_server(game: &mut Game<'_>) -> bool {
    let slot = with_state(|st| st.current_slot.clone());
    rollback_slot_checkpoint_on_server(game, &slot)
}

pub fn rollback_slot_checkpoint_on_server(game: &mut Game<'_>, slot: &str) -> bool {
    let t_start = std::time::Instant::now();
    let prev_dim = game.dim;
    bench_begin(&format!("ROLLBACK RESTORE [{}]", slot), game.world.game_time);

    let t_flush = std::time::Instant::now();
    let _ = game.save.flush();
    for parked in game.levels.parked.values() {
        let _ = parked.save.flush();
    }
    game.world.loaded_entities.clear();
    bench_accum(Phase::FlushSave, t_flush);

    let t_hydrate = std::time::Instant::now();
    let mut snap = match get_slot_snapshot_clone(slot) {
        Some(s) => s,
        None => match hydrate_slot_from_disk_on_server(game, slot) {
            Some(s) => s,
            None => return false,
        },
    };

    let target_dim = snap
        .guests
        .values()
        .next()
        .map(|g| Dim::from_id(&g.dimension))
        .unwrap_or_else(|| {
            snap.levels.keys().copied().next().unwrap_or(Dim::Overworld)
        });

    if !snap.levels.contains_key(&target_dim) {
        if let Some(hydrated) = hydrate_slot_from_disk_on_server(game, slot) {
            snap = hydrated;
        }
    }
    bench_accum(Phase::HydrateSnapshot, t_hydrate);

    let need_force_disk = !snap.disk_restored;
    let is_hot_rollback = slot == "default"
        && game.dim == target_dim
        && snap.levels.contains_key(&game.dim)
        && !need_force_disk;
    let t_disk = std::time::Instant::now();
    let disk_files_changed =
        restore_disk_files_for_slot_opts(&game.world_root, slot, need_force_disk);
    bench_accum(Phase::RestoreDiskFiles, t_disk);

    let t_savedir = std::time::Instant::now();
    if need_force_disk || !is_hot_rollback || disk_files_changed > 0 {
        let new_save = horizon_client::save::SaveDir::new(
            game.save.root.clone(),
            game.save.shape,
        );
        if let Some(tables) = game.save.tables.get() {
            let _ = new_save.tables.set(Arc::clone(tables));
        } else {
            let tables = Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
            let _ = new_save.tables.set(tables);
        }
        game.save = Arc::new(new_save);
    }
    bench_accum(Phase::SaveDirStreaming, t_savedir);

    bench_add_players(snap.guests.len());
    let policy = &snap.policy;

    let t_globals = std::time::Instant::now();
    if policy.world_time {
        game.world.game_time = snap.game_time;
        game.world.day_time = snap.day_time;
        game.ui.clock.rate = snap.time_rate;
        game.ui.clock.partial = snap.time_partial;
        game.ui.clock.paused = snap.time_paused;
        game.ui.command_clocks = snap.command_clocks.clone();
    }
    if policy.world_weather {
        game.world.weather = snap.weather;
    }
    if policy.server_game_rules {
        game.ui.rules = snap.rules.clone();
        game.world.health_rules = snap.health_rules;
        game.sync_rules();
        let _ = game.clock.stop_sprint();
        let _ = game.clock.stop_step();
        game.clock.set_frozen_ticks_to_run(0);
        game.clock.set_rate(snap.clock_rate);
        game.clock.set_frozen(snap.clock_frozen);
    }
    if policy.world_level_rng {
        let _ = horizon_client::random_source::set_thread_uniquifier(
            game.level.seed ^ snap.game_time.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64),
        );
    }
    game.portals = Default::default();
    game.load_portal_arrivals();
    let locator = game.end.locator();
    game.end = Default::default();
    game.end.set_locator(locator);
    if policy.entities_presence {
        game.portals.entities = snap.portals_entities.clone();
        game.end
            .rreturn_set_gateway_cooldowns(&snap.end_gateway_cooldowns);
    }
    game.server_trips.clear();
    game.teleport_notes.clear();
    game.spawn_wait.clear();
    game.watched_vehicles.clear();
    game.clear_game_tests();
    game.xp_delay = 0;
    if policy.world_difficulty {
        game.ui.difficulty = snap.difficulty;
        game.level.set_difficulty(snap.difficulty);
        game.level.difficulty_locked = snap.difficulty_locked;
        game.sync_rules();
    }
    if policy.world_spawn {
        game.level.world_spawn = snap.world_spawn;
        game.level.world_spawn_angle = snap.world_spawn_angle;
    }
    if policy.server_world_border {
        game.border = snap.border.clone();
    }
    if policy.world_scoreboard {
        game.load_scoreboard();
        game.score.board = snap.scoreboard.clone();
        let cp_slot_dir = slot_dir(&game.world_root, slot);
        game.score.storage =
            horizon_client::command_storage::CommandStorage::load(&cp_slot_dir);
        game.score.stopwatches =
            horizon_client::stopwatches::Stopwatches::load(&cp_slot_dir);
        game.sync_score_registries();
    }
    bench_accum(Phase::ServerGlobals, t_globals);
    let globals_ms = (std::time::Instant::now() - t_globals).as_secs_f64() * 1000.0;

    let t_bosses = std::time::Instant::now();
    if policy.server_bossbars {
        game.load_bossbars();
        let bars = BossBars::unpack(&snap.bossbars);
        game.bosses.bars = bars;
        game.ui.registries.bossbars = game.bosses.bars.ids();
    }
    if policy.world_dragon_fight {
        let fight_path = fight_dat_path(&game.world_root);
        match &snap.dragon_fight_dat {
            Some(bytes) => {
                if let Some(parent) = fight_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&fight_path, bytes);
            }
            None => {
                let _ = fs::remove_file(&fight_path);
            }
        }
        if let Some(ftag) = &snap.fight_tag {
            game.fight = super::super::dragon::Fight::rreturn_unpack(ftag);
        } else {
            game.fight = Default::default();
        }
    }

    game.sculk.tracker = snap.sculk_tracker;
    game.sculk.tracker_loaded = true;
    game.sculk.darkness = snap.sculk_darkness;
    game.adv.toasts.clear();
    game.adv.last_inventory = 0;
    game.adv.levitation = None;
    game.adv.marks = Default::default();
    game.adv.changed.clear();
    game.adv.unlocked.clear();
    game.adv.announced.clear();
    game.next_entity = snap.next_entity;
    game.ticked_entities_through = snap.ticked_entities_through;
    game.ordered.announced = snap.ordered_announced;
    game.creatures.cat_tick = snap.cat_tick;
    game.stats.rreturn_set_phantom_tick(snap.phantom_tick);
    game.level.player_extra = snap.player_extra.clone();
    game.creatures.shoulder_time = snap.shoulder_time.clone();
    if policy.entities_presence {
        game.fangs.list = snap.fangs.clone();
        game.ominous_spawners.clear();
        let saved_next = game.next_entity;
        for (oid, onbt) in &snap.ominous_spawners {
            game.load_ominous_item_spawner(onbt.clone());
            let last_idx = game.ominous_spawners.len().saturating_sub(1);
            if !game.ominous_spawners.is_empty() {
                game.ominous_spawners.update(last_idx, |last| {
                    last.id = *oid;
                });
            }
        }
        game.next_entity = saved_next;
    }
    game.contact_drops = 0..0;
    game.lectern_pulses = snap.lectern_pulses.clone();
    game.player_beds = snap.player_beds.clone();
    game.sleep_seen = snap.sleep_seen;
    game.lead_killers = snap.lead_killers.clone();
    if policy.block_entities {
        game.lid_openers = snap.lid_openers.clone();
    }
    game.entity_attachments = snap.entity_attachments.clone();
    game.player_state.attachments.clone_from(&snap.player_attachments);
    if let Some(host) = game.lan.as_mut() {
        host.hub.guests.update_all(|g| {
            if let Some(att) = snap.guest_attachments.get(&g.uuid) {
                *g.attachments = att.clone();
            }
        });
    }
    if policy.player_stats {
        game.stats.events.clear();
    }
    game.fns.schedule = snap.scheduled_events.clone();
    game.sync_schedule_registry();
    game.cmds.post_effects = snap.cmd_post_effects.clone();
    game.exec = Default::default();
    game.maps = Default::default();
    game.rand_sequences = snap.rand_sequences.clone();

    let trader_path = trader_dat_path(&game.world_root);
    match &snap.trader_dat {
        Some(bytes) => {
            if let Some(parent) = trader_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&trader_path, bytes);
        }
        None => {
            let _ = fs::remove_file(&trader_path);
        }
    }
    game.creatures.reset_trader();
    game.load_trader_data();
    bench_accum(Phase::Bosses, t_bosses);

    let t_mca = std::time::Instant::now();
    let subfolders = ["region", "entities", "poi"];
    let dims = [
        "dimensions/minecraft/overworld",
        "dimensions/minecraft/the_nether",
        "dimensions/minecraft/the_end",
    ];
    let mut deleted_region_files = false;
    for dim_path in dims {
        for sub in subfolders {
            let rdir = game.world_root.join(dim_path).join(sub);
            if let Ok(entries) = fs::read_dir(rdir) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.is_file() && !snap.region_files.contains(&p) {
                        let _ = fs::remove_file(&p);
                        deleted_region_files = true;
                    }
                }
            }
        }
    }
    bench_accum(Phase::FutureMca, t_mca);

    if target_dim != game.dim || deleted_region_files || disk_files_changed > 0 || !is_hot_rollback {
        let t_stream = std::time::Instant::now();
        restore_offline_files(&snap.offline_player_data, policy.player_stats);
        game.streaming.wait(std::time::Duration::from_millis(2));
        if let Ok((fresh_streaming, fresh_save, _)) = super::super::world_loading::make_streaming(
            &game.res,
            &game.pack,
            &game.options,
            &game.settings,
            &game.world_root,
            target_dim,
            game.world.shape,
            game.level.seed,
            &game.level.enabled_features,
            game.res.biomes().clone(),
            None,
        ) {
            if let Some(tables) = game.save.tables.get() {
                let _ = fresh_save.tables.set(Arc::clone(tables));
            } else {
                let tables = Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
                let _ = fresh_save.tables.set(tables);
            }
            game.streaming = fresh_streaming;
            game.save = fresh_save;
        }
        bench_accum(Phase::SaveDirStreaming, t_stream);
    }

    if game.dim != target_dim {
        let _ = game.enter_level(target_dim);
    }

    let saved_dims: HashSet<Dim> = snap.levels.keys().copied().collect();
    game.levels.parked.retain(|dim, _| saved_dims.contains(dim));
    for parked in game.levels.parked.values_mut() {
        if policy.server_world_border {
            parked.border_sent = None;
        }
        parked.lan_states.clear();
    }

    let other_dims: Vec<Dim> = snap
        .levels
        .keys()
        .copied()
        .filter(|&d| d != target_dim)
        .collect();

    let mut acc_blocks_ms = 0.0;
    let mut acc_containers_ms = 0.0;
    let mut acc_entities_ms = 0.0;

    for dim in other_dims {
        if let Some(level_snap) = snap.levels.get(&dim) {
            if game.enter_level(dim).is_ok() {
                if need_force_disk || !is_hot_rollback || disk_files_changed > 0 {
                    let new_save = horizon_client::save::SaveDir::new(
                        game.save.root.clone(),
                        game.save.shape,
                    );
                    if let Some(tables) = game.save.tables.get() {
                        let _ = new_save.tables.set(Arc::clone(tables));
                    } else {
                        let tables =
                            Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
                        let _ = new_save.tables.set(tables);
                    }
                    game.save = Arc::new(new_save);
                }
                let (b, c, e) =
                    apply_level_snapshot_in_place(game, dim, level_snap, policy, snap.game_time);
                acc_blocks_ms += b;
                acc_containers_ms += c;
                acc_entities_ms += e;
            }
        }
    }
    if game.dim != target_dim {
        let _ = game.enter_level(target_dim);
        if need_force_disk || !is_hot_rollback || disk_files_changed > 0 {
            let new_save = horizon_client::save::SaveDir::new(
                game.save.root.clone(),
                game.save.shape,
            );
            if let Some(tables) = game.save.tables.get() {
                let _ = new_save.tables.set(Arc::clone(tables));
            } else {
                let tables = Arc::new(horizon_client::save::SaveTables::new(&game.pack.assets));
                let _ = new_save.tables.set(tables);
            }
            game.save = Arc::new(new_save);
        }
    }

    if let Some(level_snap) = snap.levels.get(&target_dim) {
        let (b, c, e) =
            apply_level_snapshot_in_place(game, target_dim, level_snap, policy, snap.game_time);
        acc_blocks_ms += b;
        acc_containers_ms += c;
        acc_entities_ms += e;
    } else {
        let (px, pz) = game
            .player
            .as_ref()
            .map(|p| ((p.position.x as i32) >> 4, (p.position.z as i32) >> 4))
            .or_else(|| {
                game.lan
                    .as_ref()
                    .and_then(|h| h.hub.guests.first())
                    .map(|g| ((g.position[0] as i32) >> 4, (g.position[2] as i32) >> 4))
            })
            .or_else(|| {
                snap.guests.values().next().map(|g| ((g.position[0] as i32) >> 4, (g.position[2] as i32) >> 4))
            })
            .unwrap_or((0, 0));

        let mut candidate_chunks = game.world.modified.clone();
        for dz in -3..=3 {
            for dx in -3..=3 {
                candidate_chunks.insert((px + dx, pz + dz));
            }
        }

        for (cx, cz) in candidate_chunks {
            if let Some(live_chunk) = game.world.chunks.get(&(cx, cz)) {
                if let Some(disk_chunk) = game.save.load(cx, cz, game.pack.assets.registry, game.res.biomes()) {
                    let min_y = disk_chunk.min_y();
                    let mut chunk_diff = false;
                    for (s_idx, disk_sec) in disk_chunk.sections.iter().enumerate() {
                        let Some(live_sec) = live_chunk.sections.get(s_idx) else {
                            continue;
                        };
                        if disk_sec.blocks == live_sec.blocks {
                            continue;
                        }
                        chunk_diff = true;
                        let y_base = min_y + (s_idx as i32) * 16;
                        let x_base = cx * 16;
                        let z_base = cz * 16;
                        for sy in 0..16 {
                            for sz in 0..16 {
                                for sx in 0..16 {
                                    let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                                    let want = disk_sec.get(idx);
                                    if live_sec.get(idx) != want {
                                        let pos = (x_base + sx, y_base + sy, z_base + sz);
                                        if let Some(log) = game.world.block_log.as_mut() {
                                            log.push(pos);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if chunk_diff {
                        game.world.chunks.insert((cx, cz), Arc::new(disk_chunk));
                    }
                }
            }
        }
        game.world.modified.clear();
    }

    let t_player = std::time::Instant::now();
    restore_players_on_rollback(game, &snap, prev_dim, target_dim);
    bench_accum(Phase::PlayersOnline, t_player);
    let player_ms = (std::time::Instant::now() - t_player).as_secs_f64() * 1000.0;
    let total_ms = (std::time::Instant::now() - t_start).as_secs_f64() * 1000.0;

    with_state_mut(|st| {
        st.last_save_tick = snap.game_time;
        st.pending_death_rollback = None;
        st.current_intensity = 0.0;
        st.peak_intensity = 0.0;
        st.relax_ticks = 0;
        st.last_benchmark = Some(RollbackBenchmark {
            total_ms,
            globals_ms,
            blocks_ms: acc_blocks_ms,
            containers_ms: acc_containers_ms,
            entities_ms: acc_entities_ms,
            player_ms,
        });
    });

    let t_cs = std::time::Instant::now();
    if need_force_disk || !snap.levels.contains_key(&target_dim) {
        let mut updated = (*snap).clone();
        updated.disk_restored = true;
        if !updated.levels.contains_key(&target_dim) {
            updated
                .levels
                .insert(target_dim, capture_level_snapshot(game, target_dim));
        }
        set_slot_snapshot(slot, Arc::new(updated));
    }

    let client_snap = ClientBlockSnapshot {
        dim: game.dim,
        chunks: (&*game.world.chunks).clone(),
    };
    set_client_slot_snapshot(slot, client_snap);
    bump_rollback_epoch();
    bench_accum(Phase::ClientSnapUpdate, t_cs);

    let bench_report = bench_end_and_log();
    log::info!("{}", bench_report);
    append_to_rreturn_log(&game.world_root, &bench_report);

    if let Ok(mut tr) = loop_tracker().write() {
        tr.on_rollback(snap.game_time);
    }

    true
}


pub fn handle_bench_subcommand() -> String {
    let bench = with_state(|st| st.last_benchmark);
    match bench {
        Some(b) => {
            format!(
                "[RReturn Benchmark] Last Rollback: {:.2} ms\n  ├── Blocks & Chunks:     {:.2} ms ({:.1}%)\n  ├── Entities & Mobs:     {:.2} ms ({:.1}%)\n  ├── Block Entities:      {:.2} ms ({:.1}%)\n  ├── Player & Network:    {:.2} ms ({:.1}%)\n  └── Globals & Weather:   {:.2} ms ({:.1}%)",
                b.total_ms,
                b.blocks_ms,
                if b.total_ms > 0.0 { (b.blocks_ms / b.total_ms) * 100.0 } else { 0.0 },
                b.entities_ms,
                if b.total_ms > 0.0 { (b.entities_ms / b.total_ms) * 100.0 } else { 0.0 },
                b.containers_ms,
                if b.total_ms > 0.0 { (b.containers_ms / b.total_ms) * 100.0 } else { 0.0 },
                b.player_ms,
                if b.total_ms > 0.0 { (b.player_ms / b.total_ms) * 100.0 } else { 0.0 },
                b.globals_ms,
                if b.total_ms > 0.0 { (b.globals_ms / b.total_ms) * 100.0 } else { 0.0 }
            )
        }
        None => "[RReturn Benchmark] No rollback has been executed yet.".to_owned(),
    }
}

pub fn report_metrics_json(game: &Game<'_>) -> String {
    let (intensity, peak, relax, last_boss) = with_state(|st| {
        (
            st.current_intensity,
            st.peak_intensity,
            st.relax_ticks,
            st.last_boss_death_tick,
        )
    });

    let (player_health, player_food) = game
        .lan
        .as_ref()
        .and_then(|h| h.hub.guests.iter().find(|g| g.placed))
        .map(|g| (g.health, g.food))
        .unwrap_or((20.0, 20));

    let threat_level = (intensity / 10.0).clamp(0.0, 1.0);
    let is_safe = threat_level < 0.25 && player_health >= 15.0;

    let rec = if threat_level > 0.7 || player_health < 6.0 {
        "critical_danger"
    } else if threat_level > 0.25 {
        "combat_active"
    } else {
        "safe_to_save"
    };

    serde_json::to_string_pretty(&serde_json::json!({
        "threat_level": threat_level,
        "combat_intensity": intensity,
        "peak_intensity": peak,
        "relax_ticks": relax,
        "ticks_since_boss_defeat": game.world.game_time - last_boss,
        "player_health": player_health,
        "player_food": player_food,
        "safe_to_save": is_safe,
        "recommended_action": rec,
    }))
    .unwrap_or_else(|_| "{}".to_owned())
}

pub fn report_status_json(game: &Game<'_>) -> String {
    let now = game.world.game_time;
    let online: Vec<u128> = game
        .lan
        .as_ref()
        .map(|h| {
            h.hub
                .guests
                .iter()
                .filter(|g| g.placed)
                .map(|g| g.uuid)
                .collect()
        })
        .unwrap_or_default();

    let (
        has_cp,
        last_save,
        anchor_mode,
        cooldown_sec,
        auto_save,
        adaptive,
        interval_sec,
        intensity,
        peak,
        current_slot,
    ) = with_state(|st| {
        (
            st.has_checkpoint,
            st.last_save_tick,
            st.anchor.mode.id(),
            st.anchor.rollback_cooldown_seconds,
            st.auto_save,
            st.adaptive_mode,
            st.fixed_interval_sec,
            st.current_intensity,
            st.peak_intensity,
            st.current_slot.clone(),
        )
    });

    let player_json = game
        .lan
        .as_ref()
        .and_then(|h| h.hub.guests.iter().find(|g| g.placed))
        .map(|g| {
            serde_json::json!({
                "uuid": format!("{:032x}", g.uuid),
                "name": g.name,
                "dimension": g.dimension,
                "position": g.position,
                "health": g.health,
                "food": g.food,
                "game_mode": match g.game_mode {
                    1 => "creative",
                    2 => "adventure",
                    3 => "spectator",
                    _ => "survival",
                },
                "on_ground": g.on_ground,
                "flying": g.abilities.flying,
            })
        });

    let anchors: Vec<String> = with_state(|st| {
        resolve_anchors(&st.anchor, &online, now, None)
            .into_iter()
            .map(|u| format!("{:032x}", u))
            .collect()
    });

    serde_json::to_string_pretty(&serde_json::json!({
        "rreturn_version": MOD_VERSION,
        "has_checkpoint": has_cp,
        "current_slot": current_slot,
        "game_time": now,
        "day_time": game.world.day_time,
        "last_save_tick": last_save,
        "checkpoint_age_ticks": if has_cp { now - last_save } else { -1 },
        "anchor_mode": anchor_mode,
        "active_anchors": anchors,
        "rollback_cooldown_seconds": cooldown_sec,
        "auto_save": auto_save,
        "adaptive_mode": adaptive,
        "fixed_interval_seconds": interval_sec,
        "combat_intensity": intensity,
        "peak_intensity": peak,
        "player": player_json,
        "last_benchmark": with_state(|st| st.last_benchmark),
    }))
    .unwrap_or_else(|_| "{}".to_owned())
}

pub fn report_status(game: &Game<'_>) -> String {
    let online: Vec<u128> = game
        .lan
        .as_ref()
        .map(|h| {
            h.hub
                .guests
                .iter()
                .filter(|g| g.placed)
                .map(|g| g.uuid)
                .collect()
        })
        .unwrap_or_default();
    let now = game.world.game_time;
    with_state(|st| {
        let anchors = resolve_anchors(&st.anchor, &online, now, None);
        let cd_text = if st.anchor.rollback_cooldown_seconds <= 0 {
            "disabled".to_owned()
        } else {
            let rem = st
                .last_rollback_tick
                .map(|last| {
                    (i64::from(st.anchor.rollback_cooldown_seconds) * 20 - (now - last)).max(0) / 20
                })
                .unwrap_or(0);
            format!(
                "{} s (ready in {} s)",
                st.anchor.rollback_cooldown_seconds, rem
            )
        };
        format!(
            "[RReturn] Slot: {} | Key player mode: {} | rollback cooldown: {} | active key players: {} | has_checkpoint: {}",
            st.current_slot,
            st.anchor.mode.id(),
            cd_text,
            anchors.len(),
            st.has_checkpoint
        )
    })
}


pub fn execute_rreturn_command(
    game: &mut Game<'_>,
    requester: Option<u128>,
    command: &str,
) -> Vec<String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    let sub = words.get(1).copied().unwrap_or("status");
    let reply = match sub {
        "json" => report_status_json(game),
        "status" => {
            if words.get(2).copied() == Some("--json") {
                report_status_json(game)
            } else {
                report_status(game)
            }
        }
        "set" | "save" => {
            let slot = words.get(2).copied().unwrap_or("default");
            if !is_valid_slot_name(slot) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if create_slot_checkpoint_on_server(game, requester, slot) {
                with_state_mut(|st| st.current_slot = slot.to_owned());
                tr_args("command.rreturn.set.slot_success", &[slot])
            } else {
                tr_args("command.rreturn.set.slot_failed", &[slot])
            }
        }
        "load" | "rollback" => {
            if !has_level_4_permission(game, requester) {
                tr("command.rreturn.no_permission")
            } else {
                let slot = words.get(2).copied().unwrap_or("default");
                if !is_valid_slot_name(slot) {
                    "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
                } else if rollback_slot_checkpoint_on_server(game, slot) {
                    with_state_mut(|st| st.current_slot = slot.to_owned());
                    tr_args(
                        "command.rreturn.load.slot_success",
                        &[slot, &game.world.game_time.to_string()],
                    )
                } else {
                    tr_args("command.rreturn.load.slot_failed", &[slot])
                }
            }
        }
        "slot" | "slots" => handle_slot_subcommand(game, &words[2..], requester),
        "policy" => handle_policy_subcommand(game, &words[2..]),
        "metrics" => report_metrics_json(game),
        "rewind" => {
            if !has_level_4_permission(game, requester) {
                tr("command.rreturn.no_permission")
            } else {
                let slot = words.get(2).copied().unwrap_or("default");
                if !is_valid_slot_name(slot) {
                    "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
                } else if rollback_slot_checkpoint_on_server(game, slot) {
                    format!("[RReturn] Immediate rewind executed to slot '{}'.", slot)
                } else {
                    format!("[RReturn] Rewind failed: Slot '{}' not found.", slot)
                }
            }
        }
        "trigger" => handle_trigger_subcommand(game, &words[2..], requester),
        "purge_memory" => {
            clear_all_slots();
            clear_client_snapshots();
            "[RReturn] In-memory snapshots cleared.".to_owned()
        }
        "anchor" => handle_anchor_subcommand(game, &words[2..], requester),
        "autosave" => handle_autosave_subcommand(game, &words[2..]),
        "bench" | "benchmark" => handle_bench_subcommand(),
        "hitboxes" | "f3" => {
            let mode = words.get(2).copied();
            let val = match mode {
                Some("on" | "true" | "1") => true,
                Some("off" | "false" | "0") => false,
                _ => with_state(|st| !st.f3_loop_hitboxes),
            };
            with_state_mut(|st| st.f3_loop_hitboxes = val);
            save_current_settings(game);
            let state_str = if val {
                tr("ui.rreturn.on")
            } else {
                tr("ui.rreturn.off")
            };
            tr_args("command.rreturn.f3.status", &[&state_str])
        }
        "dev" => {
            let mode = words.get(2).copied();
            let val = match mode {
                Some("on" | "true" | "1") => true,
                Some("off" | "false" | "0") => false,
                _ => with_state(|st| !st.dev_mode),
            };
            with_state_mut(|st| st.dev_mode = val);
            save_current_settings(game);
            let state_str = if val {
                tr("ui.rreturn.on")
            } else {
                tr("ui.rreturn.off")
            };
            tr_args("command.rreturn.dev.status", &[&state_str])
        }
        _ => "[RReturn] Usage: /rreturn <set|load|json|status|slot|policy|metrics|anchor|autosave|bench|f3|dev>".to_owned(),
    };
    vec![reply]
}

pub fn handle_slot_subcommand(
    game: &mut Game<'_>,
    args: &[&str],
    requester: Option<u128>,
) -> String {
    let sub = args.first().copied().unwrap_or("list");
    match sub {
        "list" => {
            let disk_slots = list_disk_slots(&game.world_root);
            serde_json::to_string_pretty(&serde_json::json!({
                "current_slot": with_state(|st| st.current_slot.clone()),
                "slots": disk_slots,
            }))
            .unwrap_or_else(|_| "{}".to_owned())
        }
        "save" => {
            let name = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(name) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if create_slot_checkpoint_on_server(game, requester, name) {
                with_state_mut(|st| st.current_slot = name.to_owned());
                format!("[RReturn] Saved timeline to slot '{}'.", name)
            } else {
                tr_args("command.rreturn.set.slot_failed", &[name])
            }
        }
        "load" => {
            if !has_level_4_permission(game, requester) {
                return tr("command.rreturn.no_permission");
            }
            let name = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(name) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if rollback_slot_checkpoint_on_server(game, name) {
                with_state_mut(|st| st.current_slot = name.to_owned());
                format!("[RReturn] Rolled back to slot '{}'.", name)
            } else {
                format!("[RReturn] Slot '{}' not found.", name)
            }
        }
        "select" => {
            let name = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(name) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else {
                with_state_mut(|st| st.current_slot = name.to_owned());
                format!("[RReturn] Active slot set to '{}'.", name)
            }
        }
        "delete" => {
            if !has_level_4_permission(game, requester) {
                return tr("command.rreturn.no_permission");
            }
            let name = args.get(1).copied().unwrap_or("");
            if name.is_empty() || name == "default" {
                "[RReturn] Cannot delete default slot.".to_owned()
            } else if !is_valid_slot_name(name) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else {
                delete_slot_snapshot(name);
                let deleted = delete_disk_slot(&game.world_root, name);
                if deleted {
                    format!("[RReturn] Deleted slot '{}'.", name)
                } else {
                    format!("[RReturn] Slot '{}' not found.", name)
                }
            }
        }
        _ => "[RReturn] Usage: /rreturn slot <list|save|load|select|delete> [name]".to_owned(),
    }
}

pub fn handle_policy_subcommand(game: &mut Game<'_>, args: &[&str]) -> String {
    let sub = args.first().copied().unwrap_or("get");
    match sub {
        "get" => {
            let pol = with_state(|st| st.policy.to_json());
            serde_json::to_string_pretty(&pol).unwrap_or_else(|_| "{}".to_owned())
        }
        "set" => {
            let key = args.get(1).copied().unwrap_or("");
            let val = match args.get(2).copied().unwrap_or("").to_ascii_lowercase().as_str() {
                "true" | "1" | "on" => Some(true),
                "false" | "0" | "off" => Some(false),
                _ => None,
            };
            if key.is_empty() || val.is_none() {
                return "[RReturn] Usage: /rreturn policy set <field> <true|false>".to_owned();
            }
            let success = with_state_mut(|st| st.policy.set_field(key, val.unwrap()));
            if success {
                save_current_settings(game);
                format!("[RReturn] Policy '{}' set to {}.", key, val.unwrap())
            } else {
                format!("[RReturn] Unknown policy field '{}'.", key)
            }
        }
        "reset" => {
            with_state_mut(|st| st.policy = RollbackPolicy::default());
            save_current_settings(game);
            "[RReturn] Rollback policy reset to deterministic standard (100% rollback).".to_owned()
        }
        _ => "[RReturn] Usage: /rreturn policy <get|set|reset> [field] [value]".to_owned(),
    }
}

pub fn handle_trigger_subcommand(
    game: &mut Game<'_>,
    args: &[&str],
    requester: Option<u128>,
) -> String {
    match args.first().copied().unwrap_or("rewind") {
        "rewind" => {
            if !has_level_4_permission(game, requester) {
                return tr("command.rreturn.no_permission");
            }
            let slot = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(slot) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if rollback_slot_checkpoint_on_server(game, slot) {
                format!("[RReturn] Triggered rewind to slot '{}'.", slot)
            } else {
                format!("[RReturn] Trigger failed: Slot '{}' not found.", slot)
            }
        }
        "checkpoint" | "save" => {
            let slot = args.get(1).copied().unwrap_or("default");
            if !is_valid_slot_name(slot) {
                "[RReturn] Invalid slot name: use 1-32 characters of [A-Za-z0-9_-].".to_owned()
            } else if create_slot_checkpoint_on_server(game, requester, slot) {
                format!("[RReturn] Triggered checkpoint creation for slot '{}'.", slot)
            } else {
                tr_args("command.rreturn.set.slot_failed", &[slot])
            }
        }
        _ => "[RReturn] Usage: /rreturn trigger <rewind|checkpoint> [slot]".to_owned(),
    }
}

pub fn handle_anchor_subcommand(
    game: &mut Game<'_>,
    args: &[&str],
    requester: Option<u128>,
) -> String {
    match args.first().copied().unwrap_or("status") {
        "cooldown" => {
            let seconds = args
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(0)
                .max(0);
            with_state_mut(|st| {
                st.anchor.rollback_cooldown_seconds = seconds;
                st.last_rollback_tick = None;
            });
            save_current_settings(game);
            format!("[RReturn] Rollback cooldown set to {} s", seconds)
        }
        "rotate" | "interval" => {
            let seconds = args
                .get(1)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(300)
                .max(10);
            with_state_mut(|st| st.anchor.rotation_seconds = seconds);
            save_current_settings(game);
            format!("[RReturn] Key player rotation interval set to {} s", seconds)
        }
        "pin" => {
            let target = match args.get(1) {
                Some(name) => {
                    let host = game.lan.as_ref();
                    host.and_then(|h| {
                        h.hub
                            .guests
                            .iter()
                            .find(|g| g.name.eq_ignore_ascii_case(name))
                            .map(|g| g.uuid)
                    })
                }
                None => requester,
            };
            if let Some(uuid) = target {
                let added = with_state_mut(|st| {
                    if !st.anchor.pinned.contains(&uuid) {
                        st.anchor.pinned.push(uuid);
                        true
                    } else {
                        false
                    }
                });
                save_current_settings(game);
                if added {
                    format!("[RReturn] Pinned player {:032x} as key player.", uuid)
                } else {
                    format!("[RReturn] Player {:032x} is already pinned.", uuid)
                }
            } else {
                "[RReturn] Player not found.".to_owned()
            }
        }
        "unpin" => {
            let target = match args.get(1) {
                Some(name) => {
                    let host = game.lan.as_ref();
                    host.and_then(|h| {
                        h.hub
                            .guests
                            .iter()
                            .find(|g| g.name.eq_ignore_ascii_case(name))
                            .map(|g| g.uuid)
                    })
                }
                None => requester,
            };
            if let Some(uuid) = target {
                let removed = with_state_mut(|st| {
                    if let Some(idx) = st.anchor.pinned.iter().position(|u| *u == uuid) {
                        st.anchor.pinned.remove(idx);
                        true
                    } else {
                        false
                    }
                });
                save_current_settings(game);
                if removed {
                    format!("[RReturn] Unpinned player {:032x}.", uuid)
                } else {
                    format!("[RReturn] Player {:032x} was not pinned.", uuid)
                }
            } else {
                "[RReturn] Player not found.".to_owned()
            }
        }
        "mode" => {
            let mode_str = args.get(1).copied().unwrap_or("fixed");
            let mode = AnchorMode::by_id(mode_str);
            with_state_mut(|st| {
                st.anchor.mode = mode;
                if mode == AnchorMode::Fixed
                    && st.anchor.pinned.is_empty()
                    && let Some(req) = requester
                {
                    st.anchor.pinned = vec![req];
                }
            });
            save_current_settings(game);
            format!("[RReturn] Key player mode: {}", mode.id())
        }
        _ => report_status(game),
    }
}

pub fn handle_autosave_subcommand(game: &mut Game<'_>, args: &[&str]) -> String {
    match args.first().copied().unwrap_or("status") {
        "on" | "interval" => {
            let sec = args.get(1).and_then(|s| s.parse::<i32>().ok());
            with_state_mut(|st| {
                st.auto_save = true;
                st.adaptive_mode = false;
                if let Some(s) = sec {
                    st.fixed_interval_sec = s.max(5);
                }
                st.current_interval_ticks = i64::from(st.fixed_interval_sec) * 20;
            });
            save_current_settings(game);
            "[RReturn] Interval auto-save enabled.".to_owned()
        }
        "adaptive" => {
            let sec = args.get(1).and_then(|s| s.parse::<i32>().ok());
            with_state_mut(|st| {
                st.auto_save = true;
                st.adaptive_mode = true;
                if let Some(s) = sec {
                    st.adaptive_interval_sec = s.max(60);
                }
            });
            save_current_settings(game);
            "[RReturn] Adaptive auto-save enabled.".to_owned()
        }
        "off" => {
            with_state_mut(|st| st.auto_save = false);
            save_current_settings(game);
            "[RReturn] Auto-save disabled.".to_owned()
        }
        _ => with_state(|st| {
            format!(
                "[RReturn] Auto-save: {} | adaptive: {} | interval: {}s",
                st.auto_save, st.adaptive_mode, st.fixed_interval_sec
            )
        }),
    }
}

pub fn handle_client_command(game: &mut Game<'_>, command: &str, ran: &mut bool) {
    let trimmed = command.trim().trim_start_matches('/');
    let Some(first) = trimmed.split_whitespace().next() else {
        return;
    };
    if first != "rreturn" {
        return;
    }
    *ran = true;
    let is_rollback = trimmed == "rreturn load"
        || trimmed.starts_with("rreturn load ")
        || trimmed == "rreturn rollback"
        || trimmed.starts_with("rreturn rollback ")
        || trimmed == "rreturn rewind"
        || trimmed.starts_with("rreturn rewind ")
        || trimmed.starts_with("rreturn slot load ")
        || trimmed.starts_with("rreturn trigger rewind");
    let needs_level_4 = is_rollback
        || trimmed == "rreturn slot delete"
        || trimmed.starts_with("rreturn slot delete ")
        || trimmed == "rreturn slots delete"
        || trimmed.starts_with("rreturn slots delete ")
        || trimmed == "rreturn dev"
        || trimmed.starts_with("rreturn dev ");
    let cmd_owned = trimmed.to_owned();
    if let Some(server) = &game.integrated {
        if let Some((allowed, lines)) = server.call(move |srv| {
            let permitted = if needs_level_4 {
                has_level_4_permission(srv, None)
            } else {
                has_cheat_permission(srv, None)
            };
            if !permitted {
                return (false, vec![tr("command.rreturn.no_permission")]);
            }
            (true, execute_rreturn_command(srv, None, &cmd_owned))
        }) {
            if allowed && is_rollback {
                game.close_container();
                game.screen = super::super::Screen::Playing;
                let slot = with_state(|st| st.current_slot.clone());
                apply_client_level_rollback(game, &slot);
            }
            for line in lines {
                game.native_chat(&line);
            }
        }
    } else {
        let permitted = if needs_level_4 {
            has_level_4_permission(game, None)
        } else {
            has_cheat_permission(game, None)
        };
        if !permitted {
            game.native_chat(&tr("command.rreturn.no_permission"));
            return;
        }
        if is_rollback {
            game.close_container();
            game.screen = super::super::Screen::Playing;
        }
        let lines = execute_rreturn_command(game, None, &cmd_owned);
        if is_rollback {
            let slot = with_state(|st| st.current_slot.clone());
            apply_client_level_rollback(game, &slot);
        }
        for line in lines {
            game.native_chat(&line);
        }
    }
}

pub fn intercept_owner_console_command(
    game: &mut Game<'_>,
    command: &str,
) -> ControlFlow<Vec<String>> {
    let trimmed = command.trim().trim_start_matches('/');
    if trimmed.split_whitespace().next() != Some("rreturn") {
        return ControlFlow::Continue(());
    }
    let owner = game.serving.as_ref().map(|s| s.owner);
    let lines = execute_rreturn_command(game, owner, trimmed);
    if let Some(host) = &game.lan
        && let Some(owner_uuid) = owner
        && let Some(g) = host.hub.guests.iter().find(|g| g.uuid == owner_uuid)
    {
        for line in &lines {
            g.send(&C::SystemChat(play::SystemChat {
                content: literal(line),
                overlay: false,
            }));
        }
    }
    ControlFlow::Break(lines)
}

pub fn intercept_guest_command(
    game: &mut Game<'_>,
    conn: u32,
    command: &str,
) -> ControlFlow<()> {
    let trimmed = command.trim().trim_start_matches('/');
    if trimmed.split_whitespace().next() != Some("rreturn") {
        return ControlFlow::Continue(());
    }
    let Some(host) = game.lan.as_ref() else {
        return ControlFlow::Continue(());
    };
    let requester = host.hub.guest(conn).map(|g| g.uuid);
    let is_owner = requester.is_some_and(|u| host.owner.as_ref().is_some_and(|o| o.uuid == u));
    let needs_level_4 = trimmed == "rreturn load"
        || trimmed.starts_with("rreturn load ")
        || trimmed == "rreturn rollback"
        || trimmed.starts_with("rreturn rollback ")
        || trimmed == "rreturn rewind"
        || trimmed.starts_with("rreturn rewind ")
        || trimmed == "rreturn slot load"
        || trimmed.starts_with("rreturn slot load ")
        || trimmed == "rreturn slots load"
        || trimmed.starts_with("rreturn slots load ")
        || trimmed == "rreturn slot delete"
        || trimmed.starts_with("rreturn slot delete ")
        || trimmed == "rreturn slots delete"
        || trimmed.starts_with("rreturn slots delete ")
        || trimmed.starts_with("rreturn trigger rewind")
        || trimmed == "rreturn dev"
        || trimmed.starts_with("rreturn dev ");
    let min_op = if needs_level_4 { 4 } else { 2 };
    let has_op = host.hub.guest(conn).is_some_and(|g| {
        g.permission >= min_op || host.hub.ops.get(&g.uuid).copied().unwrap_or(0) >= min_op
    });
    let allowed = has_op || (is_owner && game.allow_commands());
    if !allowed {
        if let Some(g) = host.hub.guest(conn) {
            let msg = tr("command.rreturn.no_permission");
            g.send(&C::SystemChat(play::SystemChat {
                content: literal(&msg),
                overlay: false,
            }));
        }
        return ControlFlow::Break(());
    }
    let lines = execute_rreturn_command(game, requester, trimmed);
    if let Some(host) = game.lan.as_ref()
        && let Some(g) = host.hub.guest(conn)
    {
        for line in lines {
            g.send(&C::SystemChat(play::SystemChat {
                content: literal(&line),
                overlay: false,
            }));
        }
    }
    ControlFlow::Break(())
}


pub fn load_saved_settings(game: &Game<'_>) {
    let auto_save = game
        .setting(MOD_ID, "auto_save")
        .map(|v| v != "false");
    let adaptive = game
        .setting(MOD_ID, "adaptive_mode")
        .map(|v| v != "false");
    let interval = game
        .setting(MOD_ID, "fixed_interval_sec")
        .and_then(|v| v.parse::<i32>().ok());
    let mode = game
        .setting(MOD_ID, "anchor_mode")
        .map(|v| AnchorMode::by_id(&v));
    let cooldown = game
        .setting(MOD_ID, "rollback_cooldown_sec")
        .and_then(|v| v.parse::<i32>().ok());
    let boss_save = game
        .setting(MOD_ID, "save_on_boss_defeat")
        .map(|v| v != "false");
    let logs = game.setting(MOD_ID, "logs").map(|v| v == "true");
    let f3_hitboxes = game
        .setting(MOD_ID, "f3_loop_hitboxes")
        .map(|v| v != "false");
    let dev_mode = game
        .setting(MOD_ID, "dev_mode")
        .map(|v| v == "true");

    let json_cfg = fs::read_to_string(Path::new("config").join("rreturn.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

    with_state_mut(|st| {
        if let Some(v) = &json_cfg {
            let b = |val: &serde_json::Value, d: bool| val.as_bool().unwrap_or(d);
            let rb = &v["checkpointPolicy"]["rollback"];
            if rb.is_object() {
                st.policy.blocks = b(&rb["blocks"], true);
                st.policy.biomes = b(&rb["biomes"], true);
                st.policy.block_entities = b(&rb["block_entities"], true);
                st.policy.block_ticks = b(&rb["block_ticks"], true);
                st.policy.fluid_ticks = b(&rb["fluid_ticks"], true);
                st.policy.block_events = b(&rb["block_events"], true);
                let ent = &rb["entities"];
                if ent.is_object() {
                    st.policy.entities_presence = b(&ent["presence"], true);
                    st.policy.dropped_items = b(&ent["dropped_items"], true);
                    st.policy.experience_orbs = b(&ent["experience_orbs"], true);
                }
                let pl = &rb["players"];
                if pl.is_object() {
                    st.policy.player_position = b(&pl["position"], true);
                    st.policy.player_motion = b(&pl["motion"], true);
                    st.policy.player_health = b(&pl["health"], true);
                    st.policy.player_hunger = b(&pl["hunger"], true);
                    st.policy.player_experience = b(&pl["experience"], true);
                    st.policy.player_inventory = b(&pl["inventory"], true);
                    st.policy.player_ender_chest = b(&pl["ender_chest"], true);
                    st.policy.player_potion_effects = b(&pl["potion_effects"], true);
                    st.policy.player_cooldowns = b(&pl["cooldowns"], true);
                    st.policy.player_abilities = b(&pl["abilities"], true);
                    st.policy.player_recipe_book = b(&pl["recipe_book"], true);
                    st.policy.player_spawn_point = b(&pl["spawn_point"], true);
                    st.policy.player_stats = b(&pl["stats"], true);
                    st.policy.preserve_new_player_inventory =
                        b(&pl["preserve_new_player_inventory"], false);
                }
                let w = &rb["world"];
                if w.is_object() {
                    st.policy.world_time = b(&w["time"], true);
                    st.policy.world_weather = b(&w["weather"], true);
                    st.policy.world_raids = b(&w["raids"], true);
                    st.policy.world_dragon_fight = b(&w["dragon_fight"], true);
                    st.policy.world_scoreboard = b(&w["scoreboard"], true);
                    st.policy.world_level_rng = b(&w["level_rng"], true);
                    st.policy.world_difficulty = b(&w["difficulty"], true);
                    st.policy.world_spawn = b(&w["world_spawn"], true);
                    let sg = &w["server_globals"];
                    if sg.is_object() {
                        st.policy.server_bossbars = b(&sg["bossbars"], true);
                        st.policy.server_forced_chunks = b(&sg["forced_chunks"], true);
                        st.policy.server_world_border = b(&sg["world_border"], true);
                        st.policy.server_game_rules = b(&sg["game_rules"], true);
                    }
                }
            }
            let ad = &v["adaptive"];
            if ad.is_object() {
                if let Some(gap) = ad["min_save_gap_seconds"].as_i64() {
                    st.min_save_gap_sec = (gap as i32).max(5);
                }
                if let Some(rel) = ad["relax_time_seconds"].as_i64() {
                    st.relax_time_sec = (rel as i32).max(1);
                }
                if let Some(pct) = ad["post_combat_save_percent"].as_i64() {
                    st.post_combat_save_pct = (pct as i32).clamp(0, 100);
                }
                st.save_on_boss_defeat = b(&ad["save_on_boss_defeat"], st.save_on_boss_defeat);
            }
        }
        if let Some(v) = auto_save {
            st.auto_save = v;
        }
        if let Some(v) = adaptive {
            st.adaptive_mode = v;
        }
        if let Some(v) = interval {
            st.fixed_interval_sec = v.max(5);
            st.current_interval_ticks = i64::from(st.fixed_interval_sec) * 20;
        }
        if let Some(v) = mode {
            st.anchor.mode = v;
        }
        if let Some(v) = cooldown {
            st.anchor.rollback_cooldown_seconds = v.max(0);
        }
        if let Some(v) = boss_save {
            st.save_on_boss_defeat = v;
        }
        if let Some(v) = logs {
            st.logs = v;
        }
        if let Some(v) = f3_hitboxes {
            st.f3_loop_hitboxes = v;
        }
        if let Some(v) = dev_mode {
            st.dev_mode = v;
        } else if std::env::var_os("RRETURN_DEV").is_some_and(|v| v == "1" || v == "true") {
            st.dev_mode = true;
        }
    });
}

pub fn save_current_settings(game: &Game<'_>) {
    with_state(|st| {
        game.set_setting(
            MOD_ID,
            "auto_save",
            if st.auto_save { "true" } else { "false" },
        );
        game.set_setting(
            MOD_ID,
            "adaptive_mode",
            if st.adaptive_mode { "true" } else { "false" },
        );
        game.set_setting(
            MOD_ID,
            "fixed_interval_sec",
            &st.fixed_interval_sec.to_string(),
        );
        game.set_setting(MOD_ID, "anchor_mode", st.anchor.mode.id());
        game.set_setting(
            MOD_ID,
            "rollback_cooldown_sec",
            &st.anchor.rollback_cooldown_seconds.to_string(),
        );
        game.set_setting(
            MOD_ID,
            "save_on_boss_defeat",
            if st.save_on_boss_defeat {
                "true"
            } else {
                "false"
            },
        );
        game.set_setting(MOD_ID, "logs", if st.logs { "true" } else { "false" });
        game.set_setting(
            MOD_ID,
            "f3_loop_hitboxes",
            if st.f3_loop_hitboxes { "true" } else { "false" },
        );
        game.set_setting(
            MOD_ID,
            "dev_mode",
            if st.dev_mode { "true" } else { "false" },
        );
    });
}


#[derive(Clone, Copy, Debug)]
pub struct MobTimelineState {
    pub pos: [f64; 3],
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub health: f32,
}

pub struct LoopTracker {
    pub checkpoint_tick: i64,
    pub previous_tracks: FxHashMap<u32, Vec<(i64, MobTimelineState)>>,
    pub current_tracks: FxHashMap<u32, Vec<(i64, MobTimelineState)>>,
}

impl LoopTracker {
    pub fn new() -> Self {
        Self {
            checkpoint_tick: 0,
            previous_tracks: FxHashMap::default(),
            current_tracks: FxHashMap::default(),
        }
    }

    pub fn on_checkpoint(&mut self, save_tick: i64) {
        self.checkpoint_tick = save_tick;
        self.previous_tracks.clear();
        self.current_tracks.clear();
    }

    pub fn on_rollback(&mut self, restore_tick: i64) {
        self.checkpoint_tick = restore_tick;
        self.previous_tracks = std::mem::take(&mut self.current_tracks);
        self.current_tracks.clear();
    }

    pub fn record_tick(&mut self, now_tick: i64, mobs: &horizon_client::join_seq::JoinList<Mob>) {
        let delta_t = now_tick - self.checkpoint_tick;
        if delta_t < 0 {
            return;
        }
        for m in mobs.iter() {
            if m.removed {
                continue;
            }
            let track = self.current_tracks.entry(m.id).or_default();
            if track.len() > 3600 {
                track.remove(0);
            }
            track.push((
                delta_t,
                MobTimelineState {
                    pos: [m.position.x, m.position.y, m.position.z],
                    head_yaw: m.head_yaw,
                    head_pitch: m.head_pitch,
                    health: m.health,
                },
            ));
        }
    }

    pub fn get_mob_color(
        &self,
        mob_id: u32,
        now_tick: i64,
        current_pos: [f64; 3],
        current_yaw: f32,
        current_hp: f32,
    ) -> u32 {
        let Some(prev_track) = self.previous_tracks.get(&mob_id) else {
            return 0xFFFF_FFFF;
        };

        if prev_track.is_empty() {
            return 0xFFFF_FFFF;
        }

        let delta_t = (now_tick - self.checkpoint_tick).max(0);
        let frame = match prev_track.binary_search_by_key(&delta_t, |&(t, _)| t) {
            Ok(idx) => Some(&prev_track[idx].1),
            Err(idx) => {
                if idx == 0 {
                    prev_track.first().map(|(_, f)| f)
                } else if idx >= prev_track.len() {
                    prev_track.last().map(|(_, f)| f)
                } else {
                    let (t_left, f_left) = &prev_track[idx - 1];
                    let (t_right, f_right) = &prev_track[idx];
                    if (delta_t - t_left).abs() <= (delta_t - t_right).abs() {
                        Some(f_left)
                    } else {
                        Some(f_right)
                    }
                }
            }
        };

        let Some(prev_state) = frame else {
            return 0xFFFF_FFFF;
        };

        let dx = current_pos[0] - prev_state.pos[0];
        let dy = current_pos[1] - prev_state.pos[1];
        let dz = current_pos[2] - prev_state.pos[2];
        let dist_sq = dx * dx + dy * dy + dz * dz;

        let mut dyaw = (current_yaw - prev_state.head_yaw).abs() % 360.0;
        if dyaw > 180.0 {
            dyaw = 360.0 - dyaw;
        }

        let dhp = (current_hp - prev_state.health).abs();

        if dist_sq <= (0.35 * 0.35) && dyaw <= 40.0 && dhp < 0.5 {
            0xFF22_FF55
        } else {
            0xFFFF_3333
        }
    }
}

static LOOP_TRACKER: OnceLock<RwLock<LoopTracker>> = OnceLock::new();

pub fn loop_tracker() -> &'static RwLock<LoopTracker> {
    LOOP_TRACKER.get_or_init(|| RwLock::new(LoopTracker::new()))
}

pub fn process_hitbox_lines(game: &Game<'_>, lines: &mut Vec<horizon_client::gizmos::Line>) {
    let enabled = with_state(|st| st.f3_loop_hitboxes);
    if !enabled {
        return;
    }

    let tracker_guard = match loop_tracker().read() {
        Ok(g) => g,
        Err(_) => return,
    };

    let now_tick = game.world.game_time;

    for m in game.mobs.iter() {
        if m.removed || m.invisible {
            continue;
        }
        let pos = [m.position.x, m.position.y, m.position.z];
        let color = tracker_guard.get_mob_color(m.id, now_tick, pos, m.head_yaw, m.health);

        if color == 0xFFFF_FFFF {
            continue;
        }

        let half = f64::from(m.width()) / 2.0;
        let min_x = pos[0] - half - 0.08;
        let max_x = pos[0] + half + 0.08;
        let min_y = pos[1] - 0.08;
        let max_y = pos[1] + f64::from(m.height()) + 0.08;
        let min_z = pos[2] - half - 0.08;
        let max_z = pos[2] + half + 0.08;

        for line in lines.iter_mut() {
            if line.color == 0xFFFF_FFFF {
                let inside_start = line.start[0] >= min_x && line.start[0] <= max_x
                    && line.start[1] >= min_y && line.start[1] <= max_y
                    && line.start[2] >= min_z && line.start[2] <= max_z;
                let inside_end = line.end[0] >= min_x && line.end[0] <= max_x
                    && line.end[1] >= min_y && line.end[1] <= max_y
                    && line.end[2] >= min_z && line.end[2] <= max_z;
                if inside_start && inside_end {
                    line.color = color;
                }
            }
        }
    }
}


pub fn handle_key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.playing() {
        return;
    }
    let save_key = game
        .mod_key(MOD_ID, "save")
        .map(|(k, _)| k)
        .unwrap_or_else(|| "key.keyboard.f6".to_owned());
    let load_key = game
        .mod_key(MOD_ID, "load")
        .map(|(k, _)| k)
        .unwrap_or_else(|| "key.keyboard.f7".to_owned());
    let mode_key = game
        .mod_key(MOD_ID, "anchor_menu")
        .map(|(k, _)| k)
        .unwrap_or_else(|| "key.keyboard.f8".to_owned());

    if key == save_key {
        if let Some(server) = &game.integrated {
            let msg = server.call(|srv| {
                if !has_cheat_permission(srv, None) {
                    return tr("command.rreturn.no_permission");
                }
                if create_checkpoint_on_server(srv, None) {
                    tr("command.rreturn.set.success")
                } else {
                    tr("command.rreturn.set.failed")
                }
            });
            if let Some(text) = msg {
                game.native_chat(&text);
            }
        } else if !has_cheat_permission(game, None) {
            game.native_chat(&tr("command.rreturn.no_permission"));
        } else {
            with_state_mut(|st| st.pending_save_request = true);
        }
    } else if key == load_key {
        if let Some(server) = &game.integrated {
            let res = server.call(|srv| {
                if !has_level_4_permission(srv, None) {
                    return (false, tr("command.rreturn.no_permission"));
                }
                if rollback_checkpoint_on_server(srv) {
                    (
                        true,
                        tr_args(
                            "command.rreturn.load.success",
                            &[&srv.world.game_time.to_string()],
                        ),
                    )
                } else {
                    (false, tr("command.rreturn.load.failed"))
                }
            });
            if let Some((did_rollback, text)) = res {
                if did_rollback {
                    game.close_container();
                    game.screen = super::super::Screen::Playing;
                    let slot = with_state(|st| st.current_slot.clone());
                    apply_client_level_rollback(game, &slot);
                }
                game.native_chat(&text);
            }
        } else if !has_level_4_permission(game, None) {
            game.native_chat(&tr("command.rreturn.no_permission"));
        } else {
            game.close_container();
            game.screen = super::super::Screen::Playing;
            with_state_mut(|st| st.pending_load_request = true);
        }
    } else if key == mode_key {
        let allowed = if let Some(server) = &game.integrated {
            server
                .call(|srv| has_cheat_permission(srv, None))
                .unwrap_or(false)
        } else {
            has_cheat_permission(game, None)
        };
        if !allowed {
            game.native_chat(&tr("gui.rreturn.anchor.no_permission"));
            return;
        }
        let next_mode = with_state_mut(|st| {
            st.anchor.mode = st.anchor.mode.next();
            st.anchor.mode
        });
        save_current_settings(game);
        let mode_label = tr(next_mode.lang_key());
        game.native_chat(&tr_args("command.rreturn.anchor.mode", &[&mode_label]));
    }
}

pub fn handle_server_started(game: &mut Game<'_>) {
    load_saved_settings(game);
    clear_all_slots();
    clear_client_snapshots();
    let has_disk_cp = has_disk_checkpoint(&game.world_root);
    let mut restored_anchors = HashSet::new();
    let mut restored_save_tick = game.world.game_time;
    if has_disk_cp {
        if let Some(snap) = load_checkpoint_from_disk(&game.world_root) {
            restored_anchors = snap.anchor_ids.clone();
            restored_save_tick = snap.game_time;
            set_snapshot(Some(Arc::new(snap)));
        }
        with_state_mut(|st| {
            st.has_checkpoint = true;
        });
    } else {
        with_state_mut(|st| {
            st.has_checkpoint = false;
        });
    }
    with_state_mut(|st| {
        st.current_slot = "default".to_owned();
        st.anchor_ids = restored_anchors;
        st.pending_death_rollback = None;
        st.last_save_tick = restored_save_tick;
        st.current_interval_ticks = i64::from(st.fixed_interval_sec.max(5)) * 20;
        st.last_rollback_tick = None;
        st.last_boss_death_tick = -9999;
        st.current_intensity = 0.0;
        st.peak_intensity = 0.0;
        st.relax_ticks = 0;
    });
}

pub fn handle_lifecycle(_game: &mut Game<'_>, phase: WorldPhase) {
    match phase {
        WorldPhase::WorldCloseFinal
        | WorldPhase::WorldCloseFailed
        | WorldPhase::WorldOpenFailed => {
            clear_all_slots();
            clear_client_snapshots();
            with_state_mut(|st| {
                st.pending_death_rollback = None;
            });
        }
        _ => {}
    }
}

pub fn handle_tick(game: &mut Game<'_>) {
    if game.screen == super::super::Screen::Dead {
        let has_cp = with_state(|st| st.has_checkpoint);
        if has_cp {
            game.screen = super::super::Screen::Playing;
            game.ui.death_time = 0;
            game.ui.died_at = None;
            game.ui.death_confirm = None;
            game.boss_overlay.reset();
            if let Some(p) = game.player.as_mut() {
                let _ = p.set_health(20.0, false);
                p.health.dead = false;
                p.invulnerable = false;
                p.may_fly = false;
                p.spectator = false;
                p.flying = false;
                p.fall_distance = 0.0;
                p.health.hurt.no_action_time = 0;
                p.health.hurt.cooldown = 0;
                p.health.hurt.last_hurt = 0.0;
                p.health.hurt.hurt_duration = 0;
                p.health.hurt.hurt_time = 0;
                p.health.hurt.last_source = None;
                p.health.hurt.last_stamp = 0;
                p.health.hurt_animation_time = 0;
                p.health.hurt_dir = 0.0;
                p.health.indicated_dir = None;
            }
            if let Some(server) = &game.integrated {
                server.call(|srv| {
                    rollback_checkpoint_on_server(srv);
                });
            } else {
                rollback_checkpoint_on_server(game);
            }
            let slot = with_state(|st| st.current_slot.clone());
            apply_client_level_rollback(game, &slot);
            return;
        }
    }

    if game.remote.is_some() {
        if client_epoch_needs_sync() && get_rollback_epoch() > 0 {
            let slot = with_state(|st| st.current_slot.clone());
            apply_client_level_rollback(game, &slot);
        }
    }

    if game.native_window() {
        return;
    }

    let (do_save, do_load, dead_rollback) = with_state_mut(|st| {
        (
            std::mem::replace(&mut st.pending_save_request, false),
            std::mem::replace(&mut st.pending_load_request, false),
            st.pending_death_rollback.take(),
        )
    });

    if do_save {
        create_checkpoint_on_server(game, None);
    }
    if do_load || dead_rollback.is_some() {
        rollback_checkpoint_on_server(game);
        let slot = with_state(|st| st.current_slot.clone());
        apply_client_level_rollback(game, &slot);
        return;
    }

    if with_state(|st| st.has_checkpoint) {
        if !game.world.chunks.is_empty() {
            let active_slot = with_state(|st| st.current_slot.clone());
            let needs_hydration = get_slot_snapshot_clone(&active_slot)
                .is_none_or(|s| !s.levels.contains_key(&game.dim));
            if needs_hydration {
                let _ = hydrate_slot_from_disk_on_server(game, &active_slot);
            }
        }
        if let Ok(mut tr) = loop_tracker().write() {
            tr.record_tick(game.world.game_time, &game.mobs);
        }
    }

    tick_autosave_on_server(game);
}

pub fn intercept_guest_die(
    game: &mut Game<'_>,
    gi: usize,
    _conn: u32,
) -> ControlFlow<()> {
    let now = game.world.game_time;
    let Some(host) = game.lan.as_mut() else {
        return ControlFlow::Continue(());
    };
    if gi >= host.hub.guests.len() || host.hub.guests[gi].health > 0.0 || host.hub.guests[gi].dead {
        return ControlFlow::Continue(());
    }
    let player_uuid = host.hub.guests[gi].uuid;
    let online: Vec<u128> = host
        .hub
        .guests
        .iter()
        .filter(|g| g.placed)
        .map(|g| g.uuid)
        .collect();

    let should_intercept = with_state_mut(|st| {
        if !st.has_checkpoint {
            return false;
        }
        let active_anchors = resolve_anchors(&st.anchor, &online, now, None);
        let is_anchor =
            active_anchors.contains(&player_uuid) || st.anchor_ids.contains(&player_uuid);
        if !is_anchor {
            return false;
        }
        let cd = st.anchor.rollback_cooldown_seconds;
        if cd > 0
            && let Some(last) = st.last_rollback_tick
            && now >= last
            && now - last < i64::from(cd) * 20
        {
            return false;
        }
        st.last_rollback_tick = Some(now);
        st.pending_death_rollback = Some(player_uuid);
        true
    });

    if !should_intercept {
        return ControlFlow::Continue(());
    }

    let Some(host) = game.lan.as_mut() else {
        return ControlFlow::Continue(());
    };
    if gi >= host.hub.guests.len() {
        return ControlFlow::Continue(());
    }
    host.hub.guests.update(gi, |g| {
        g.health = 20.0;
        g.dead = false;
        g.death_time = 0;
        g.sent_health = None;
        if let Some(m) = g.motion.as_mut() {
            m.health.dead = false;
            m.invulnerable = false;
            m.may_fly = false;
            m.spectator = false;
            m.flying = false;
            m.fall_distance = 0.0;
            m.health.hurt.no_action_time = 0;
            m.health.hurt.cooldown = 0;
            m.health.hurt.last_hurt = 0.0;
            m.health.hurt.hurt_duration = 0;
            m.health.hurt.hurt_time = 0;
            m.health.hurt.last_source = None;
            m.health.hurt.last_stamp = 0;
            m.health.hurt_animation_time = 0;
            m.health.hurt_dir = 0.0;
            m.health.indicated_dir = None;
            let _ = m.set_health(20.0, false);
        }
    });
    ControlFlow::Break(())
}
