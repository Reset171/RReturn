use super::policy::RollbackPolicy;
use super::types::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiTab {
    Rollback = 0,
    Scrub = 1,
    Players = 2,
    System = 3,
}

impl UiTab {
    pub const ALL: [Self; 4] = [Self::Rollback, Self::Scrub, Self::Players, Self::System];
    pub const GLOBAL: [Self; 3] = [Self::Rollback, Self::Scrub, Self::System];

    pub fn title_key(self) -> &'static str {
        match self {
            Self::Rollback => "rreturn.ui.tab.rollback",
            Self::Scrub => "rreturn.ui.tab.scrub",
            Self::Players => "rreturn.ui.tab.players",
            Self::System => "rreturn.ui.tab.system",
        }
    }

    pub fn subtitle_key(self) -> &'static str {
        match self {
            Self::Rollback => "rreturn.ui.tab.rollback.sub",
            Self::Scrub => "rreturn.ui.tab.scrub.sub",
            Self::Players => "rreturn.ui.tab.players.sub",
            Self::System => "rreturn.ui.tab.system.sub",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiGroup {
    RbAuto,
    RbWorld,
    RbEntities,
    RbPlayer,
    RbRules,
    ScrubGeneral,
    ScrubWorld,
    ScrubEntities,
    ScrubPlayer,
    ScrubRules,
    SysDiag,
}

impl UiGroup {
    pub fn jump_id(self) -> &'static str {
        match self {
            Self::RbAuto => "rb_auto",
            Self::RbWorld => "rb_world",
            Self::RbEntities => "rb_entities",
            Self::RbPlayer => "rb_player",
            Self::RbRules => "rb_rules",
            Self::ScrubGeneral => "scrub_general",
            Self::ScrubWorld => "scrub_world",
            Self::ScrubEntities => "scrub_entities",
            Self::ScrubPlayer => "scrub_player",
            Self::ScrubRules => "scrub_rules",
            Self::SysDiag => "sys_diag",
        }
    }

    pub fn title_key(self) -> &'static str {
        match self {
            Self::RbAuto => "rreturn.ui.group.rb_auto",
            Self::RbWorld => "rreturn.ui.group.rb_world",
            Self::RbEntities => "rreturn.ui.group.rb_entities",
            Self::RbPlayer => "rreturn.ui.group.rb_player",
            Self::RbRules => "rreturn.ui.group.rb_rules",
            Self::ScrubGeneral => "rreturn.ui.group.scrub_general",
            Self::ScrubWorld => "rreturn.ui.group.scrub_world",
            Self::ScrubEntities => "rreturn.ui.group.scrub_entities",
            Self::ScrubPlayer => "rreturn.ui.group.scrub_player",
            Self::ScrubRules => "rreturn.ui.group.scrub_rules",
            Self::SysDiag => "rreturn.ui.group.sys_diag",
        }
    }

    pub fn is_policy_group(self) -> bool {
        matches!(
            self,
            Self::RbWorld
                | Self::RbEntities
                | Self::RbPlayer
                | Self::RbRules
                | Self::ScrubWorld
                | Self::ScrubEntities
                | Self::ScrubPlayer
                | Self::ScrubRules
        )
    }
}

pub const ROLLBACK_GROUPS: &[UiGroup] = &[
    UiGroup::RbAuto,
    UiGroup::RbWorld,
    UiGroup::RbEntities,
    UiGroup::RbPlayer,
    UiGroup::RbRules,
];

pub const SCRUB_GROUPS: &[UiGroup] = &[
    UiGroup::ScrubGeneral,
    UiGroup::ScrubWorld,
    UiGroup::ScrubEntities,
    UiGroup::ScrubPlayer,
    UiGroup::ScrubRules,
];

#[derive(Clone, Copy)]
pub enum SliderUnit {
    Sec,
    Pct,
    TenthsX,
}

#[derive(Clone, Copy)]
pub enum ControlKind {
    Bool,
    Slider { min: i32, max: i32, step: i32, unit: SliderUnit },
}

#[derive(Clone, Copy)]
pub struct SettingEntry {
    pub id: &'static str,
    pub tab: UiTab,
    pub group: UiGroup,
    pub label_key: &'static str,
    pub desc_key: &'static str,
    pub kind: ControlKind,
}

const fn opt_b(id: &'static str, tab: UiTab, group: UiGroup, label: &'static str, desc: &'static str) -> SettingEntry {
    SettingEntry { id, tab, group, label_key: label, desc_key: desc, kind: ControlKind::Bool }
}

const fn opt_s(
    id: &'static str,
    tab: UiTab,
    group: UiGroup,
    label: &'static str,
    desc: &'static str,
    min: i32,
    max: i32,
    step: i32,
    unit: SliderUnit,
) -> SettingEntry {
    SettingEntry { id, tab, group, label_key: label, desc_key: desc, kind: ControlKind::Slider { min, max, step, unit } }
}

const POLICY_SPECS: &[(&str, &str, &str)] = &[
    ("blocks", "rreturn.ui.opt.blocks", "rreturn.ui.opt.blocks.desc"),
    ("biomes", "rreturn.ui.opt.biomes", "rreturn.ui.opt.biomes.desc"),
    ("block_entities", "rreturn.ui.opt.block_entities", "rreturn.ui.opt.block_entities.desc"),
    ("block_ticks", "rreturn.ui.opt.block_ticks", "rreturn.ui.opt.block_ticks.desc"),
    ("fluid_ticks", "rreturn.ui.opt.fluid_ticks", "rreturn.ui.opt.fluid_ticks.desc"),
    ("block_events", "rreturn.ui.opt.block_events", "rreturn.ui.opt.block_events.desc"),
    ("entities_presence", "rreturn.ui.opt.entities_presence", "rreturn.ui.opt.entities_presence.desc"),
    ("dropped_items", "rreturn.ui.opt.dropped_items", "rreturn.ui.opt.dropped_items.desc"),
    ("experience_orbs", "rreturn.ui.opt.experience_orbs", "rreturn.ui.opt.experience_orbs.desc"),
    ("player_position", "rreturn.ui.opt.player_position", "rreturn.ui.opt.player_position.desc"),
    ("player_motion", "rreturn.ui.opt.player_motion", "rreturn.ui.opt.player_motion.desc"),
    ("player_health", "rreturn.ui.opt.player_health", "rreturn.ui.opt.player_health.desc"),
    ("player_hunger", "rreturn.ui.opt.player_hunger", "rreturn.ui.opt.player_hunger.desc"),
    ("player_experience", "rreturn.ui.opt.player_experience", "rreturn.ui.opt.player_experience.desc"),
    ("player_inventory", "rreturn.ui.opt.player_inventory", "rreturn.ui.opt.player_inventory.desc"),
    ("player_ender_chest", "rreturn.ui.opt.player_ender_chest", "rreturn.ui.opt.player_ender_chest.desc"),
    ("player_potion_effects", "rreturn.ui.opt.player_potion_effects", "rreturn.ui.opt.player_potion_effects.desc"),
    ("player_cooldowns", "rreturn.ui.opt.player_cooldowns", "rreturn.ui.opt.player_cooldowns.desc"),
    ("player_abilities", "rreturn.ui.opt.player_abilities", "rreturn.ui.opt.player_abilities.desc"),
    ("player_recipe_book", "rreturn.ui.opt.player_recipe_book", "rreturn.ui.opt.player_recipe_book.desc"),
    ("player_spawn_point", "rreturn.ui.opt.player_spawn_point", "rreturn.ui.opt.player_spawn_point.desc"),
    ("player_stats", "rreturn.ui.opt.player_stats", "rreturn.ui.opt.player_stats.desc"),
    ("reset_new_player_inventory", "rreturn.ui.opt.reset_new_player_inventory", "rreturn.ui.opt.reset_new_player_inventory.desc"),
    ("world_time", "rreturn.ui.opt.world_time", "rreturn.ui.opt.world_time.desc"),
    ("world_weather", "rreturn.ui.opt.world_weather", "rreturn.ui.opt.world_weather.desc"),
    ("world_raids", "rreturn.ui.opt.world_raids", "rreturn.ui.opt.world_raids.desc"),
    ("world_dragon_fight", "rreturn.ui.opt.world_dragon_fight", "rreturn.ui.opt.world_dragon_fight.desc"),
    ("world_scoreboard", "rreturn.ui.opt.world_scoreboard", "rreturn.ui.opt.world_scoreboard.desc"),
    ("world_level_rng", "rreturn.ui.opt.world_level_rng", "rreturn.ui.opt.world_level_rng.desc"),
    ("world_difficulty", "rreturn.ui.opt.world_difficulty", "rreturn.ui.opt.world_difficulty.desc"),
    ("world_spawn", "rreturn.ui.opt.world_spawn", "rreturn.ui.opt.world_spawn.desc"),
    ("server_bossbars", "rreturn.ui.opt.server_bossbars", "rreturn.ui.opt.server_bossbars.desc"),
    ("server_forced_chunks", "rreturn.ui.opt.server_forced_chunks", "rreturn.ui.opt.server_forced_chunks.desc"),
    ("server_world_border", "rreturn.ui.opt.server_world_border", "rreturn.ui.opt.server_world_border.desc"),
    ("server_game_rules", "rreturn.ui.opt.server_game_rules", "rreturn.ui.opt.server_game_rules.desc"),
];

pub const PLAYER_OVERRIDE_SPECS: &[(&str, &str, &str)] = &[
    POLICY_SPECS[9],
    POLICY_SPECS[10],
    POLICY_SPECS[11],
    POLICY_SPECS[12],
    POLICY_SPECS[13],
    POLICY_SPECS[14],
    POLICY_SPECS[15],
    POLICY_SPECS[16],
    POLICY_SPECS[17],
    POLICY_SPECS[18],
    POLICY_SPECS[19],
    POLICY_SPECS[20],
    POLICY_SPECS[21],
];

pub const SETTINGS_CATALOG: &[SettingEntry] = &[
    // Rollback Autosave & Timing (13)
    opt_b("death_rollback", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.death_rollback", "rreturn.ui.opt.death_rollback.desc"),
    opt_b("auto_save", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.auto_save", "rreturn.ui.opt.auto_save.desc"),
    opt_b("adaptive_mode", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.adaptive_mode", "rreturn.ui.opt.adaptive_mode.desc"),
    opt_b("use_random_interval", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.use_random_interval", "rreturn.ui.opt.use_random_interval.desc"),
    opt_b("save_on_boss_defeat", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.save_on_boss_defeat", "rreturn.ui.opt.save_on_boss_defeat.desc"),
    opt_s("fixed_interval_sec", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.fixed_interval_sec", "rreturn.ui.opt.fixed_interval_sec.desc", 5, 600, 5, SliderUnit::Sec),
    opt_s("random_min_sec", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.random_min_sec", "rreturn.ui.opt.random_min_sec.desc", 5, 600, 5, SliderUnit::Sec),
    opt_s("random_max_sec", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.random_max_sec", "rreturn.ui.opt.random_max_sec.desc", 10, 1200, 10, SliderUnit::Sec),
    opt_s("adaptive_interval_sec", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.adaptive_interval_sec", "rreturn.ui.opt.adaptive_interval_sec.desc", 60, 900, 15, SliderUnit::Sec),
    opt_s("min_save_gap_sec", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.min_save_gap_sec", "rreturn.ui.opt.min_save_gap_sec.desc", 5, 120, 5, SliderUnit::Sec),
    opt_s("relax_time_sec", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.relax_time_sec", "rreturn.ui.opt.relax_time_sec.desc", 1, 30, 1, SliderUnit::Sec),
    opt_s("post_combat_save_pct", UiTab::Rollback, UiGroup::RbAuto, "rreturn.ui.opt.post_combat_save_pct", "rreturn.ui.opt.post_combat_save_pct.desc", 0, 100, 5, SliderUnit::Pct),
    opt_s("rollback_cooldown_sec", UiTab::Players, UiGroup::RbAuto, "rreturn.ui.opt.rollback_cooldown_sec", "rreturn.ui.opt.rollback_cooldown_sec.desc", 0, 60, 1, SliderUnit::Sec),
    // Rollback World (6)
    opt_b("blocks", UiTab::Rollback, UiGroup::RbWorld, POLICY_SPECS[0].1, POLICY_SPECS[0].2),
    opt_b("biomes", UiTab::Rollback, UiGroup::RbWorld, POLICY_SPECS[1].1, POLICY_SPECS[1].2),
    opt_b("block_entities", UiTab::Rollback, UiGroup::RbWorld, POLICY_SPECS[2].1, POLICY_SPECS[2].2),
    opt_b("block_ticks", UiTab::Rollback, UiGroup::RbWorld, POLICY_SPECS[3].1, POLICY_SPECS[3].2),
    opt_b("fluid_ticks", UiTab::Rollback, UiGroup::RbWorld, POLICY_SPECS[4].1, POLICY_SPECS[4].2),
    opt_b("block_events", UiTab::Rollback, UiGroup::RbWorld, POLICY_SPECS[5].1, POLICY_SPECS[5].2),
    // Rollback Entities (3)
    opt_b("entities_presence", UiTab::Rollback, UiGroup::RbEntities, POLICY_SPECS[6].1, POLICY_SPECS[6].2),
    opt_b("dropped_items", UiTab::Rollback, UiGroup::RbEntities, POLICY_SPECS[7].1, POLICY_SPECS[7].2),
    opt_b("experience_orbs", UiTab::Rollback, UiGroup::RbEntities, POLICY_SPECS[8].1, POLICY_SPECS[8].2),
    // Rollback Player (14)
    opt_b("player_position", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[9].1, POLICY_SPECS[9].2),
    opt_b("player_motion", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[10].1, POLICY_SPECS[10].2),
    opt_b("player_health", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[11].1, POLICY_SPECS[11].2),
    opt_b("player_hunger", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[12].1, POLICY_SPECS[12].2),
    opt_b("player_experience", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[13].1, POLICY_SPECS[13].2),
    opt_b("player_inventory", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[14].1, POLICY_SPECS[14].2),
    opt_b("player_ender_chest", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[15].1, POLICY_SPECS[15].2),
    opt_b("player_potion_effects", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[16].1, POLICY_SPECS[16].2),
    opt_b("player_cooldowns", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[17].1, POLICY_SPECS[17].2),
    opt_b("player_abilities", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[18].1, POLICY_SPECS[18].2),
    opt_b("player_recipe_book", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[19].1, POLICY_SPECS[19].2),
    opt_b("player_spawn_point", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[20].1, POLICY_SPECS[20].2),
    opt_b("player_stats", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[21].1, POLICY_SPECS[21].2),
    opt_b("reset_new_player_inventory", UiTab::Rollback, UiGroup::RbPlayer, POLICY_SPECS[22].1, POLICY_SPECS[22].2),
    // Rollback Rules (12)
    opt_b("world_time", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[23].1, POLICY_SPECS[23].2),
    opt_b("world_weather", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[24].1, POLICY_SPECS[24].2),
    opt_b("world_raids", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[25].1, POLICY_SPECS[25].2),
    opt_b("world_dragon_fight", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[26].1, POLICY_SPECS[26].2),
    opt_b("world_scoreboard", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[27].1, POLICY_SPECS[27].2),
    opt_b("world_level_rng", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[28].1, POLICY_SPECS[28].2),
    opt_b("world_difficulty", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[29].1, POLICY_SPECS[29].2),
    opt_b("world_spawn", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[30].1, POLICY_SPECS[30].2),
    opt_b("server_bossbars", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[31].1, POLICY_SPECS[31].2),
    opt_b("server_forced_chunks", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[32].1, POLICY_SPECS[32].2),
    opt_b("server_world_border", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[33].1, POLICY_SPECS[33].2),
    opt_b("server_game_rules", UiTab::Rollback, UiGroup::RbRules, POLICY_SPECS[34].1, POLICY_SPECS[34].2),

    // Scrub Own Behavior Settings (5)
    opt_b("scrub_enabled", UiTab::Scrub, UiGroup::ScrubGeneral, "rreturn.ui.opt.scrub_enabled", "rreturn.ui.opt.scrub_enabled.desc"),
    opt_b("scrub_reverse_audio", UiTab::Scrub, UiGroup::ScrubGeneral, "rreturn.ui.opt.scrub_reverse_audio", "rreturn.ui.opt.scrub_reverse_audio.desc"),
    opt_b("scrub_show_hud", UiTab::Scrub, UiGroup::ScrubGeneral, "rreturn.ui.opt.scrub_show_hud", "rreturn.ui.opt.scrub_show_hud.desc"),
    opt_s("scrub_history_seconds", UiTab::Scrub, UiGroup::ScrubGeneral, "rreturn.ui.opt.scrub_history_seconds", "rreturn.ui.opt.scrub_history_seconds.desc", 30, 1200, 30, SliderUnit::Sec),
    opt_s("scrub_wheel_sensitivity", UiTab::Scrub, UiGroup::ScrubGeneral, "rreturn.ui.opt.scrub_wheel_sensitivity", "rreturn.ui.opt.scrub_wheel_sensitivity.desc", 5, 40, 1, SliderUnit::TenthsX),
    // Scrub World (6)
    opt_b("scrub.blocks", UiTab::Scrub, UiGroup::ScrubWorld, POLICY_SPECS[0].1, POLICY_SPECS[0].2),
    opt_b("scrub.biomes", UiTab::Scrub, UiGroup::ScrubWorld, POLICY_SPECS[1].1, POLICY_SPECS[1].2),
    opt_b("scrub.block_entities", UiTab::Scrub, UiGroup::ScrubWorld, POLICY_SPECS[2].1, POLICY_SPECS[2].2),
    opt_b("scrub.block_ticks", UiTab::Scrub, UiGroup::ScrubWorld, POLICY_SPECS[3].1, POLICY_SPECS[3].2),
    opt_b("scrub.fluid_ticks", UiTab::Scrub, UiGroup::ScrubWorld, POLICY_SPECS[4].1, POLICY_SPECS[4].2),
    opt_b("scrub.block_events", UiTab::Scrub, UiGroup::ScrubWorld, POLICY_SPECS[5].1, POLICY_SPECS[5].2),
    // Scrub Entities (3)
    opt_b("scrub.entities_presence", UiTab::Scrub, UiGroup::ScrubEntities, POLICY_SPECS[6].1, POLICY_SPECS[6].2),
    opt_b("scrub.dropped_items", UiTab::Scrub, UiGroup::ScrubEntities, POLICY_SPECS[7].1, POLICY_SPECS[7].2),
    opt_b("scrub.experience_orbs", UiTab::Scrub, UiGroup::ScrubEntities, POLICY_SPECS[8].1, POLICY_SPECS[8].2),
    // Scrub Player (14)
    opt_b("scrub.player_position", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[9].1, POLICY_SPECS[9].2),
    opt_b("scrub.player_motion", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[10].1, POLICY_SPECS[10].2),
    opt_b("scrub.player_health", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[11].1, POLICY_SPECS[11].2),
    opt_b("scrub.player_hunger", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[12].1, POLICY_SPECS[12].2),
    opt_b("scrub.player_experience", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[13].1, POLICY_SPECS[13].2),
    opt_b("scrub.player_inventory", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[14].1, POLICY_SPECS[14].2),
    opt_b("scrub.player_ender_chest", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[15].1, POLICY_SPECS[15].2),
    opt_b("scrub.player_potion_effects", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[16].1, POLICY_SPECS[16].2),
    opt_b("scrub.player_cooldowns", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[17].1, POLICY_SPECS[17].2),
    opt_b("scrub.player_abilities", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[18].1, POLICY_SPECS[18].2),
    opt_b("scrub.player_recipe_book", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[19].1, POLICY_SPECS[19].2),
    opt_b("scrub.player_spawn_point", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[20].1, POLICY_SPECS[20].2),
    opt_b("scrub.player_stats", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[21].1, POLICY_SPECS[21].2),
    opt_b("scrub.reset_new_player_inventory", UiTab::Scrub, UiGroup::ScrubPlayer, POLICY_SPECS[22].1, POLICY_SPECS[22].2),
    // Scrub Rules (12)
    opt_b("scrub.world_time", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[23].1, POLICY_SPECS[23].2),
    opt_b("scrub.world_weather", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[24].1, POLICY_SPECS[24].2),
    opt_b("scrub.world_raids", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[25].1, POLICY_SPECS[25].2),
    opt_b("scrub.world_dragon_fight", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[26].1, POLICY_SPECS[26].2),
    opt_b("scrub.world_scoreboard", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[27].1, POLICY_SPECS[27].2),
    opt_b("scrub.world_level_rng", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[28].1, POLICY_SPECS[28].2),
    opt_b("scrub.world_difficulty", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[29].1, POLICY_SPECS[29].2),
    opt_b("scrub.world_spawn", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[30].1, POLICY_SPECS[30].2),
    opt_b("scrub.server_bossbars", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[31].1, POLICY_SPECS[31].2),
    opt_b("scrub.server_forced_chunks", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[32].1, POLICY_SPECS[32].2),
    opt_b("scrub.server_world_border", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[33].1, POLICY_SPECS[33].2),
    opt_b("scrub.server_game_rules", UiTab::Scrub, UiGroup::ScrubRules, POLICY_SPECS[34].1, POLICY_SPECS[34].2),

    // System & Diagnostics (3)
    opt_b("f3_loop_hitboxes", UiTab::System, UiGroup::SysDiag, "rreturn.ui.opt.f3_loop_hitboxes", "rreturn.ui.opt.f3_loop_hitboxes.desc"),
    opt_b("logs", UiTab::System, UiGroup::SysDiag, "rreturn.ui.opt.logs", "rreturn.ui.opt.logs.desc"),
    opt_b("dev_mode", UiTab::System, UiGroup::SysDiag, "rreturn.ui.opt.dev_mode", "rreturn.ui.opt.dev_mode.desc"),
];

pub fn policy_field(p: &RollbackPolicy, id: &str) -> bool {
    match id {
        "blocks" => p.blocks,
        "biomes" => p.biomes,
        "block_entities" => p.block_entities,
        "block_ticks" => p.block_ticks,
        "fluid_ticks" => p.fluid_ticks,
        "block_events" => p.block_events,
        "entities_presence" => p.entities_presence,
        "dropped_items" => p.dropped_items,
        "experience_orbs" => p.experience_orbs,
        "player_position" => p.player_position,
        "player_motion" => p.player_motion,
        "player_health" => p.player_health,
        "player_hunger" => p.player_hunger,
        "player_experience" => p.player_experience,
        "player_inventory" => p.player_inventory,
        "player_ender_chest" => p.player_ender_chest,
        "player_potion_effects" => p.player_potion_effects,
        "player_cooldowns" => p.player_cooldowns,
        "player_abilities" => p.player_abilities,
        "player_recipe_book" => p.player_recipe_book,
        "player_spawn_point" => p.player_spawn_point,
        "player_stats" => p.player_stats,
        "reset_new_player_inventory" => p.reset_new_player_inventory,
        "preserve_new_player_inventory" => !p.reset_new_player_inventory,
        "world_time" => p.world_time,
        "world_weather" => p.world_weather,
        "world_raids" => p.world_raids,
        "world_dragon_fight" => p.world_dragon_fight,
        "world_scoreboard" => p.world_scoreboard,
        "world_level_rng" => p.world_level_rng,
        "world_difficulty" => p.world_difficulty,
        "world_spawn" => p.world_spawn,
        "server_bossbars" => p.server_bossbars,
        "server_forced_chunks" => p.server_forced_chunks,
        "server_world_border" => p.server_world_border,
        "server_game_rules" => p.server_game_rules,
        _ => false,
    }
}

pub fn read_bool_setting(st: &RReturnState, id: &str) -> bool {
    if let Some(sub) = id.strip_prefix("scrub.") {
        return policy_field(&st.scrub_policy, sub);
    }
    match id {
        "death_rollback" => st.death_rollback,
        "auto_save" => st.auto_save,
        "adaptive_mode" => st.adaptive_mode,
        "use_random_interval" => st.use_random_interval,
        "save_on_boss_defeat" => st.save_on_boss_defeat,
        "scrub_enabled" => st.scrub_enabled,
        "scrub_reverse_audio" => st.scrub_reverse_audio,
        "scrub_show_hud" => st.scrub_show_hud,
        "f3_loop_hitboxes" => st.f3_loop_hitboxes,
        "logs" => st.logs,
        "dev_mode" => st.dev_mode,
        _ => policy_field(&st.policy, id),
    }
}

pub fn write_bool_setting(st: &mut RReturnState, id: &str, val: bool) {
    if let Some(sub) = id.strip_prefix("scrub.") {
        let _ = st.scrub_policy.set_field(sub, val);
        return;
    }
    if st.policy.set_field(id, val) {
        return;
    }
    match id {
        "death_rollback" => st.death_rollback = val,
        "auto_save" => st.auto_save = val,
        "adaptive_mode" => st.adaptive_mode = val,
        "use_random_interval" => st.use_random_interval = val,
        "save_on_boss_defeat" => st.save_on_boss_defeat = val,
        "scrub_enabled" => st.scrub_enabled = val,
        "scrub_reverse_audio" => st.scrub_reverse_audio = val,
        "scrub_show_hud" => st.scrub_show_hud = val,
        "f3_loop_hitboxes" => st.f3_loop_hitboxes = val,
        "logs" => st.logs = val,
        "dev_mode" => st.dev_mode = val,
        _ => {}
    }
    sync_scrub_atomics_from_state(st);
}

pub fn read_slider_setting(st: &RReturnState, id: &str) -> i32 {
    match id {
        "fixed_interval_sec" => st.fixed_interval_sec,
        "random_min_sec" => st.random_min_sec,
        "random_max_sec" => st.random_max_sec,
        "adaptive_interval_sec" => st.adaptive_interval_sec,
        "min_save_gap_sec" => st.min_save_gap_sec,
        "relax_time_sec" => st.relax_time_sec,
        "post_combat_save_pct" => st.post_combat_save_pct,
        "anchor_rotation_sec" => st.anchor.rotation_seconds,
        "rollback_cooldown_sec" => st.anchor.rollback_cooldown_seconds,
        "scrub_history_seconds" => st.scrub_history_seconds,
        "scrub_wheel_sensitivity" => st.scrub_wheel_sensitivity,
        _ => 0,
    }
}

pub fn write_slider_setting(st: &mut RReturnState, id: &str, val: i32) {
    match id {
        "fixed_interval_sec" => {
            st.fixed_interval_sec = val.clamp(5, 600);
            st.current_interval_ticks = i64::from(st.fixed_interval_sec) * 20;
        }
        "random_min_sec" => {
            st.random_min_sec = val.clamp(5, 600);
            if st.random_max_sec <= st.random_min_sec {
                st.random_max_sec = (st.random_min_sec + 5).clamp(10, 1200);
            }
        }
        "random_max_sec" => st.random_max_sec = val.clamp(st.random_min_sec + 5, 1200),
        "adaptive_interval_sec" => st.adaptive_interval_sec = val.clamp(60, 900),
        "min_save_gap_sec" => st.min_save_gap_sec = val.clamp(5, 120),
        "relax_time_sec" => st.relax_time_sec = val.clamp(1, 30),
        "post_combat_save_pct" => st.post_combat_save_pct = val.clamp(0, 100),
        "anchor_rotation_sec" => st.anchor.rotation_seconds = val.clamp(10, 900),
        "rollback_cooldown_sec" => {
            st.anchor.rollback_cooldown_seconds = val.clamp(0, 60);
            st.last_rollback_tick = None;
        }
        "scrub_history_seconds" => st.scrub_history_seconds = val.clamp(30, 1200),
        "scrub_wheel_sensitivity" => st.scrub_wheel_sensitivity = val.clamp(5, 40),
        _ => {}
    }
    sync_scrub_atomics_from_state(st);
}

pub fn format_slider_value(val: i32, unit: SliderUnit) -> String {
    match unit {
        SliderUnit::Sec => format!("{} {}", val, tr("rreturn.ui.sec_unit")),
        SliderUnit::Pct => format!("{}{}", val, tr("rreturn.ui.pct_unit")),
        SliderUnit::TenthsX => format!("{:.1}x", val as f32 / 10.0),
    }
}

pub fn count_tab_policies(st: &RReturnState, tab: UiTab) -> (usize, usize) {
    let mut on = 0usize;
    let mut total = 0usize;
    for e in SETTINGS_CATALOG {
        if e.tab == tab && e.group.is_policy_group() {
            total += 1;
            if read_bool_setting(st, e.id) {
                on += 1;
            }
        }
    }
    (on, total)
}
