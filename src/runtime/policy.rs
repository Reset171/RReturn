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
    pub reset_new_player_inventory: bool,
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
            reset_new_player_inventory: true,
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
                "reset_new_player_inventory",
                self.reset_new_player_inventory,
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
            "reset_new_player_inventory" | "newcomer_inventory" => {
                self.reset_new_player_inventory = val;
            }
            "preserve_new_player_inventory" => self.reset_new_player_inventory = !val,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OverrideValue {
    #[default]
    Global,
    Allow,
    Deny,
}

impl OverrideValue {
    pub fn resolve(self, global_val: bool) -> bool {
        match self {
            Self::Global => global_val,
            Self::Allow => true,
            Self::Deny => false,
        }
    }

    pub fn from_token(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "global" | "default" | "auto" | "inherit" | "null" => Some(Self::Global),
            "true" | "1" | "on" | "yes" | "allow" => Some(Self::Allow),
            "false" | "0" | "off" | "no" | "deny" => Some(Self::Deny),
            _ => None,
        }
    }

    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        if v.is_null() {
            return Some(Self::Global);
        }
        if let Some(b) = v.as_bool() {
            return Some(if b { Self::Allow } else { Self::Deny });
        }
        v.as_str().and_then(Self::from_token)
    }

    pub fn to_json(self) -> serde_json::Value {
        match self {
            Self::Global => serde_json::Value::String("global".to_owned()),
            Self::Allow => serde_json::Value::Bool(true),
            Self::Deny => serde_json::Value::Bool(false),
        }
    }
}

pub const PLAYER_POLICY_FIELDS: &[&str] = &[
    "player_position",
    "player_motion",
    "player_health",
    "player_hunger",
    "player_experience",
    "player_inventory",
    "player_ender_chest",
    "player_potion_effects",
    "player_cooldowns",
    "player_abilities",
    "player_recipe_book",
    "player_spawn_point",
    "player_stats",
];

pub fn normalize_player_policy_field(key: &str) -> Option<&'static str> {
    let norm = key.trim().to_ascii_lowercase().replace('.', "_");
    match norm.as_str() {
        "player_position" | "position" | "pos" => Some("player_position"),
        "player_motion" | "motion" | "velocity" => Some("player_motion"),
        "player_health" | "health" | "hp" => Some("player_health"),
        "player_hunger" | "hunger" | "food" => Some("player_hunger"),
        "player_experience" | "experience" | "xp" => Some("player_experience"),
        "player_inventory" | "inventory" | "inv" => Some("player_inventory"),
        "player_ender_chest" | "ender_chest" | "ender" => Some("player_ender_chest"),
        "player_potion_effects" | "effects" | "potion_effects" => Some("player_potion_effects"),
        "player_cooldowns" | "cooldowns" => Some("player_cooldowns"),
        "player_abilities" | "abilities" | "gamemode" => Some("player_abilities"),
        "player_recipe_book" | "recipe_book" | "recipes" => Some("player_recipe_book"),
        "player_spawn_point" | "spawn_point" => Some("player_spawn_point"),
        "player_stats" | "stats" => Some("player_stats"),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerPolicyOverrides {
    pub player_position: OverrideValue,
    pub player_motion: OverrideValue,
    pub player_health: OverrideValue,
    pub player_hunger: OverrideValue,
    pub player_experience: OverrideValue,
    pub player_inventory: OverrideValue,
    pub player_ender_chest: OverrideValue,
    pub player_potion_effects: OverrideValue,
    pub player_cooldowns: OverrideValue,
    pub player_abilities: OverrideValue,
    pub player_recipe_book: OverrideValue,
    pub player_spawn_point: OverrideValue,
    pub player_stats: OverrideValue,
}

impl PlayerPolicyOverrides {
    pub fn get(&self, key: &str) -> OverrideValue {
        match normalize_player_policy_field(key) {
            Some("player_position") => self.player_position,
            Some("player_motion") => self.player_motion,
            Some("player_health") => self.player_health,
            Some("player_hunger") => self.player_hunger,
            Some("player_experience") => self.player_experience,
            Some("player_inventory") => self.player_inventory,
            Some("player_ender_chest") => self.player_ender_chest,
            Some("player_potion_effects") => self.player_potion_effects,
            Some("player_cooldowns") => self.player_cooldowns,
            Some("player_abilities") => self.player_abilities,
            Some("player_recipe_book") => self.player_recipe_book,
            Some("player_spawn_point") => self.player_spawn_point,
            Some("player_stats") => self.player_stats,
            _ => OverrideValue::Global,
        }
    }

    pub fn set(&mut self, key: &str, val: OverrideValue) -> bool {
        match normalize_player_policy_field(key) {
            Some("player_position") => self.player_position = val,
            Some("player_motion") => self.player_motion = val,
            Some("player_health") => self.player_health = val,
            Some("player_hunger") => self.player_hunger = val,
            Some("player_experience") => self.player_experience = val,
            Some("player_inventory") => self.player_inventory = val,
            Some("player_ender_chest") => self.player_ender_chest = val,
            Some("player_potion_effects") => self.player_potion_effects = val,
            Some("player_cooldowns") => self.player_cooldowns = val,
            Some("player_abilities") => self.player_abilities = val,
            Some("player_recipe_book") => self.player_recipe_book = val,
            Some("player_spawn_point") => self.player_spawn_point = val,
            Some("player_stats") => self.player_stats = val,
            _ => return false,
        }
        true
    }

    pub fn set_all(&mut self, val: OverrideValue) {
        for &f in PLAYER_POLICY_FIELDS {
            let _ = self.set(f, val);
        }
    }

    pub fn count_custom(&self) -> usize {
        PLAYER_POLICY_FIELDS
            .iter()
            .filter(|&&f| self.get(f) != OverrideValue::Global)
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.count_custom() == 0
    }

    pub fn apply_to_policy(&self, base: &RollbackPolicy) -> RollbackPolicy {
        let mut out = *base;
        out.player_position = self.player_position.resolve(base.player_position);
        out.player_motion = self.player_motion.resolve(base.player_motion);
        out.player_health = self.player_health.resolve(base.player_health);
        out.player_hunger = self.player_hunger.resolve(base.player_hunger);
        out.player_experience = self.player_experience.resolve(base.player_experience);
        out.player_inventory = self.player_inventory.resolve(base.player_inventory);
        out.player_ender_chest = self.player_ender_chest.resolve(base.player_ender_chest);
        out.player_potion_effects = self.player_potion_effects.resolve(base.player_potion_effects);
        out.player_cooldowns = self.player_cooldowns.resolve(base.player_cooldowns);
        out.player_abilities = self.player_abilities.resolve(base.player_abilities);
        out.player_recipe_book = self.player_recipe_book.resolve(base.player_recipe_book);
        out.player_spawn_point = self.player_spawn_point.resolve(base.player_spawn_point);
        out.player_stats = self.player_stats.resolve(base.player_stats);
        out
    }

    pub fn to_sparse_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for &f in PLAYER_POLICY_FIELDS {
            let v = self.get(f);
            if v != OverrideValue::Global {
                map.insert(f.to_owned(), serde_json::Value::Bool(v == OverrideValue::Allow));
            }
        }
        serde_json::Value::Object(map)
    }

    pub fn to_full_json(&self, base: &RollbackPolicy) -> serde_json::Value {
        let resolved = self.apply_to_policy(base).to_json();
        let mut raw = serde_json::Map::new();
        for &f in PLAYER_POLICY_FIELDS {
            raw.insert(f.to_owned(), self.get(f).to_json());
        }
        serde_json::json!({
            "overrides": serde_json::Value::Object(raw),
            "effective": resolved,
        })
    }

    pub fn from_json(v: &serde_json::Value) -> Self {
        let mut out = Self::default();
        let Some(obj) = v.as_object() else {
            return out;
        };
        for (k, val) in obj {
            if let Some(ov) = OverrideValue::from_json(val) {
                let _ = out.set(k, ov);
            }
        }
        out
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerProfileOverrides {
    pub name: String,
    pub rollback: PlayerPolicyOverrides,
    pub scrub: PlayerPolicyOverrides,
}

impl PlayerProfileOverrides {
    pub fn is_empty(&self) -> bool {
        self.rollback.is_empty() && self.scrub.is_empty()
    }
}