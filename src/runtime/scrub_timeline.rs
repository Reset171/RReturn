use super::super::{
    Dim, ExperienceOrb, FallingBlock, Game, ItemEntity, Mob,
    commands::OtherEntity,
    dragon::Crystal,
};
use super::scrub_audio::prewarm_reverse_sound;
use super::scrub_perf::*;
use super::snapshots::{
    BreathSnapshot, DragonSnapshot, RocketSnapshot, TridentSnapshot, get_rollback_epoch,
};
use super::types::scrub_max_history_ticks;
use super::ui_snapshot::*;
use horizon_client::{
    block_entity_anim::BlockEntityAnims,
    boss_overlay::BossOverlay,
    chunk::{Chunk, Section},
    containers::BlockEntity,
    entities::{ArmorStand, Cushion, Experience, ItemFrame, Painting},
    fast_hash::FxHashMap,
    items::ItemStack,
    lan::abilities::PlayerAbilities,
    particles::Particle,
    world::Weather,
    worldborder::WorldBorder,
};
use horizon_core::geometry::Vec3;
use horizon_gameplay::{
    boats::Boat,
    minecarts::Minecart,
    tnt::PrimedTnt,
};
use horizon_world::piston_shape::MovingPistonState;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    sync::Arc,
};

pub const MAX_SCRUB_TICKS: usize = 24_000;
const SECTION_SWAP_THRESHOLD: usize = 256;

#[derive(Clone, Copy, Debug)]
pub struct BlockDeltaRecord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub old_state: u32,
    pub new_state: u32,
}

#[derive(Clone)]
pub struct SectionSwapDelta {
    pub cx: i32,
    pub cz: i32,
    pub s_idx: usize,
    pub old_section: Arc<Section>,
    pub new_section: Arc<Section>,
}

#[derive(Clone)]
pub struct BiomeDeltaRecord {
    pub cx: i32,
    pub cz: i32,
    pub s_idx: usize,
    pub old_section: Arc<Section>,
    pub new_section: Arc<Section>,
}

#[derive(Clone, Copy, Debug)]
pub struct BreakProgressRecord {
    pub breaker: i32,
    pub pos: [i32; 3],
    pub stage: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct ArmSwingRecord {
    pub swinging: bool,
    pub hand: u8,
    pub animation: f32,
    pub old_animation: f32,
    pub ticks: i32,
    pub duration: i32,
    pub height_main: f32,
    pub height_off: f32,
}

#[derive(Clone)]
pub enum MobPoseFrameRef {
    Inline(Vec<MobPoseRecord>),
    RrtlSealed { segment_id: u64, offset: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct EntityPoseRecord {
    pub id: u32,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub extra_i32: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct HookVisualRecord {
    pub id: u32,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub nibble: i32,
    pub bobbing: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct SpawnerSpinRecord {
    pub pos: [i32; 3],
    pub spin: f64,
    pub o_spin: f64,
    pub client_delay: i32,
}

#[derive(Clone, Debug)]
pub struct SoundRecord {
    pub identifier: String,
    pub position: [f64; 3],
    pub relative: bool,
}

#[derive(Clone)]
pub enum ParticleFrameRef {
    Inline(Vec<ParticleTickState>),
    RrtlSealed { segment_id: u64, offset: usize },
}

#[derive(Clone)]
pub struct TickHistoryFrame {
    pub game_time: i64,
    pub dim: Dim,
    pub day_time: i64,
    pub weather: Weather,
    pub border: Arc<WorldBorder>,
    pub boss_overlay: BossOverlay,
    pub sculk_darkness: (f32, f32),
    pub flash: u32,
    pub be_anims: Arc<BlockEntityAnims>,
    pub spawner_spins: Vec<SpawnerSpinRecord>,
    pub moving_pistons: Vec<([i32; 3], MovingPistonState)>,
    pub player_pos: [f64; 3],
    pub player_yaw: f32,
    pub player_pitch: f32,
    pub player_selected: usize,
    pub player_inventory: Arc<Vec<Option<ItemStack>>>,
    pub player_armor: Arc<[Option<ItemStack>; 4]>,
    pub player_offhand: Arc<Option<ItemStack>>,
    pub player_using: Option<(usize, u32)>,
    pub player_use_duration: i32,
    pub player_health: f32,
    pub player_absorption: f32,
    pub player_food: i32,
    pub player_fire_ticks: i32,
    pub player_air: i32,
    pub player_ticks_frozen: i32,
    pub player_sneaking: bool,
    pub player_fall_flying: bool,
    pub player_game_mode: i32,
    pub player_previous_game_mode: Option<i32>,
    pub player_experience: Experience,
    pub player_acting_abilities: Option<PlayerAbilities>,
    pub player_abilities: (bool, bool, bool, bool, f32),
    pub mobs: MobPoseFrameRef,
    pub items: Vec<EntityPoseRecord>,
    pub orbs: Vec<EntityPoseRecord>,
    pub tnts: Vec<EntityPoseRecord>,
    pub falling: Vec<EntityPoseRecord>,
    pub boats: Vec<EntityPoseRecord>,
    pub carts: Vec<EntityPoseRecord>,
    pub projectiles: Vec<EntityPoseRecord>,
    pub tridents: Vec<EntityPoseRecord>,
    pub rockets: Vec<EntityPoseRecord>,
    pub crystals: Vec<EntityPoseRecord>,
    pub paintings: Arc<Vec<Painting>>,
    pub item_frames: Arc<Vec<ItemFrame>>,
    pub armor_stands: Arc<Vec<ArmorStand>>,
    pub cushions: Arc<Vec<Cushion>>,
    pub others: Arc<Vec<OtherEntity>>,
    pub dragon: Option<DragonSnapshot>,
    pub breath_clouds: Arc<Vec<BreathSnapshot>>,
    pub fangs: Arc<Vec<super::super::fangs::Fangs>>,
    pub thrown_potions: Vec<(u32, [f64; 3], [f64; 3], u32)>,
    pub hook: Option<HookVisualRecord>,
    pub block_deltas: Vec<BlockDeltaRecord>,
    pub section_swaps: Vec<SectionSwapDelta>,
    pub biome_deltas: Vec<BiomeDeltaRecord>,
    pub break_progress: Vec<BreakProgressRecord>,
    pub arm_swing: ArmSwingRecord,
    pub particles: ParticleFrameRef,
    pub elder_guardians: Vec<i32>,
    pub sounds: Vec<SoundRecord>,
    pub fov_modifier: f32,
    pub avatar_walk_dist: f32,
    pub avatar_bob: f32,
    pub player_sprinting: bool,
    pub player_hurt_time: i32,
    pub player_hurt_dir: f32,
    pub carried: Arc<Option<ItemStack>>,
    pub craft_grid: Arc<Vec<Option<ItemStack>>>,
    pub ender_items: Arc<Vec<Option<ItemStack>>>,
    pub ui: Arc<ClientUiSnapshot>,
    pub open_container_bes: Option<Arc<Vec<((i32, i32, i32), BlockEntity)>>>,
}

fn opt_arc_ptr_eq<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => Arc::ptr_eq(x, y),
        _ => false,
    }
}

pub fn mob_template_visuals_match(a: &Mob, b: &Mob) -> bool {
    (Arc::ptr_eq(&a.kind, &b.kind) || a.kind.id == b.kind.id)
        && a.skin == b.skin
        && a.slim == b.slim
        && a.cape == b.cape
        && opt_arc_ptr_eq(&a.skin_3d, &b.skin_3d)
        && a.profile == b.profile
        && a.custom_name == b.custom_name
        && a.name_visible == b.name_visible
        && a.custom_name_text.as_ref().map(|t| t.plain())
            == b.custom_name_text.as_ref().map(|t| t.plain())
        && a.invisible == b.invisible
        && a.glowing == b.glowing
        && (a.fire_ticks > 0) == (b.fire_ticks > 0)
        && (a.ticks_frozen > 0) == (b.ticks_frozen > 0)
        && a.standing_on == b.standing_on
        && a.extra.model_scale.to_bits() == b.extra.model_scale.to_bits()
        && a.extra.crouching == b.extra.crouching
        && a.extra.avatar_pose == b.extra.avatar_pose
        && a.extra.pose == b.extra.pose
        && a.extra.synced_pose == b.extra.synced_pose
        && a.extra.sprinting == b.extra.sprinting
        && a.extra.passenger == b.extra.passenger
        && (a.extra.age < 0) == (b.extra.age < 0)
        && a.extra.age_locked == b.extra.age_locked
        && a.extra.variant == b.extra.variant
        && a.extra.variant2 == b.extra.variant2
        && a.extra.variant3 == b.extra.variant3
        && a.extra.profession == b.extra.profession
        && a.extra.villager_level == b.extra.villager_level
        && a.extra.color == b.extra.color
        && a.extra.sheared == b.extra.sheared
        && a.extra.equipment == b.extra.equipment
        && a.extra.gear == b.extra.gear
        && a.extra.chested == b.extra.chested
        && a.extra.tame == b.extra.tame
        && a.extra.sitting == b.extra.sitting
        && a.extra.sitting_pose == b.extra.sitting_pose
        && a.extra.owner == b.extra.owner
        && a.extra.size == b.extra.size
        && a.extra.carried == b.extra.carried
        && a.extra.charged == b.extra.charged
        && a.extra.adopted_body == b.extra.adopted_body
        && a.extra.using_item == b.extra.using_item
        && a.extra.using_off_hand == b.extra.using_off_hand
        && a.extra.use_animation == b.extra.use_animation
        && a.extra.held_arm_pose == b.extra.held_arm_pose
        && a.extra.fall_flying == b.extra.fall_flying
        && a.extra.auto_spin_attack == b.extra.auto_spin_attack
        && a.extra.stuck == b.extra.stuck
        && a.extra.shoulder_parrots == b.extra.shoulder_parrots
        && a.extra.main_hand_left == b.extra.main_hand_left
        && a.extra.hidden_model_parts == b.extra.hidden_model_parts
        && a.extra.spectator == b.extra.spectator
        && a.extra.swing_right_arm == b.extra.swing_right_arm
        && a.extra.swing_stab == b.extra.swing_stab
        && a.extra.flag == b.extra.flag
        && a.extra.flag2 == b.extra.flag2
        && a.extra.ints == b.extra.ints
        && a.extra.scratch == b.extra.scratch
        && a.extra.counter == b.extra.counter
        && a.extra.counter2 == b.extra.counter2
        && a.extra.mount == b.extra.mount
        && a.extra.rider == b.extra.rider
        && a.extra.left_handed == b.extra.left_handed
        && a.extra.screaming == b.extra.screaming
        && a.extra.lost_horns == b.extra.lost_horns
        && a.extra.axolotl.playing_dead == b.extra.axolotl.playing_dead
        && a.extra.description == b.extra.description
        && a.extra.immovable == b.extra.immovable
        && a.extra.effect_particles.len() == b.extra.effect_particles.len()
        && a.extra.effect_ambient == b.extra.effect_ambient
}

pub fn sync_mob_appearance_from_template(dst: &mut Mob, tpl: &Mob) {
    if mob_template_visuals_match(dst, tpl) {
        return;
    }
    dst.kind = Arc::clone(&tpl.kind);
    dst.skin = tpl.skin.clone();
    dst.slim = tpl.slim;
    dst.cape = tpl.cape.clone();
    dst.skin_3d = tpl.skin_3d.clone();
    dst.profile = tpl.profile.clone();
    dst.custom_name = tpl.custom_name.clone();
    dst.custom_name_text = tpl.custom_name_text.clone();
    dst.name_visible = tpl.name_visible;
    dst.invisible = tpl.invisible;
    dst.glowing = tpl.glowing;
    dst.fire_ticks = tpl.fire_ticks;
    dst.ticks_frozen = tpl.ticks_frozen;
    dst.standing_on = tpl.standing_on.clone();
    dst.extra.model_scale = tpl.extra.model_scale;
    dst.extra.crouching = tpl.extra.crouching;
    dst.extra.avatar_pose = tpl.extra.avatar_pose;
    dst.extra.pose = tpl.extra.pose;
    dst.extra.synced_pose = tpl.extra.synced_pose;
    dst.extra.sprinting = tpl.extra.sprinting;
    dst.extra.passenger = tpl.extra.passenger;
    dst.extra.age = tpl.extra.age;
    dst.extra.age_locked = tpl.extra.age_locked;
    dst.extra.variant = tpl.extra.variant;
    dst.extra.variant2 = tpl.extra.variant2;
    dst.extra.variant3 = tpl.extra.variant3;
    dst.extra.profession = tpl.extra.profession.clone();
    dst.extra.villager_level = tpl.extra.villager_level;
    dst.extra.color = tpl.extra.color;
    dst.extra.sheared = tpl.extra.sheared;
    dst.extra.equipment = tpl.extra.equipment.clone();
    dst.extra.gear = tpl.extra.gear.clone();
    dst.extra.chested = tpl.extra.chested;
    dst.extra.tame = tpl.extra.tame;
    dst.extra.sitting = tpl.extra.sitting;
    dst.extra.sitting_pose = tpl.extra.sitting_pose;
    dst.extra.owner = tpl.extra.owner;
    dst.extra.size = tpl.extra.size;
    dst.extra.carried = tpl.extra.carried;
    dst.extra.charged = tpl.extra.charged;
    dst.extra.adopted_body = tpl.extra.adopted_body.clone();
    dst.extra.using_item = tpl.extra.using_item;
    dst.extra.using_off_hand = tpl.extra.using_off_hand;
    dst.extra.use_animation = tpl.extra.use_animation;
    dst.extra.held_arm_pose = tpl.extra.held_arm_pose.clone();
    dst.extra.fall_flying = tpl.extra.fall_flying;
    dst.extra.auto_spin_attack = tpl.extra.auto_spin_attack;
    dst.extra.stuck = tpl.extra.stuck.clone();
    dst.extra.shoulder_parrots = tpl.extra.shoulder_parrots;
    dst.extra.main_hand_left = tpl.extra.main_hand_left;
    dst.extra.hidden_model_parts = tpl.extra.hidden_model_parts;
    dst.extra.spectator = tpl.extra.spectator;
    dst.extra.swing_right_arm = tpl.extra.swing_right_arm;
    dst.extra.swing_stab = tpl.extra.swing_stab;
    dst.extra.flag = tpl.extra.flag;
    dst.extra.flag2 = tpl.extra.flag2;
    dst.extra.ints = tpl.extra.ints;
    dst.extra.scratch = tpl.extra.scratch;
    dst.extra.counter = tpl.extra.counter;
    dst.extra.counter2 = tpl.extra.counter2;
    dst.extra.mount = tpl.extra.mount;
    dst.extra.rider = tpl.extra.rider;
    dst.extra.left_handed = tpl.extra.left_handed;
    dst.extra.screaming = tpl.extra.screaming;
    dst.extra.lost_horns = tpl.extra.lost_horns;
    dst.extra.axolotl.playing_dead = tpl.extra.axolotl.playing_dead;
    dst.extra.description = tpl.extra.description.clone();
    dst.extra.immovable = tpl.extra.immovable;
    dst.extra.effect_particles = tpl.extra.effect_particles.clone();
    dst.extra.effect_ambient = tpl.extra.effect_ambient;
}

#[derive(Default)]
pub struct PreviewEntityPool {
    pub mob_templates: Vec<Mob>,
    pub mob_latest: FxHashMap<u32, u32>,
    pub dropped: FxHashMap<u32, ItemEntity>,
    pub orbs: FxHashMap<u32, ExperienceOrb>,
    pub tnts: FxHashMap<u32, PrimedTnt>,
    pub falling: FxHashMap<u32, FallingBlock>,
    pub boats: FxHashMap<u32, Boat>,
    pub carts: FxHashMap<u32, Minecart>,
    pub projectiles: FxHashMap<u32, horizon_client::projectiles::Projectile>,
    pub crystals: FxHashMap<u32, Crystal>,
    pub tridents: FxHashMap<u32, TridentSnapshot>,
    pub rockets: FxHashMap<u32, RocketSnapshot>,
    pub thrown_potions: ThrownPotionsSnapshot,
}

impl PreviewEntityPool {
    pub fn clear(&mut self) {
        self.mob_templates.clear();
        self.mob_latest.clear();
        self.dropped.clear();
        self.orbs.clear();
        self.tnts.clear();
        self.falling.clear();
        self.boats.clear();
        self.carts.clear();
        self.projectiles.clear();
        self.crystals.clear();
        self.tridents.clear();
        self.rockets.clear();
        self.thrown_potions.clear();
    }

    pub fn observe_mob(&mut self, m: &Mob) -> u32 {
        if let Some(&idx) = self.mob_latest.get(&m.id)
            && let Some(existing) = self.mob_templates.get(idx as usize)
            && mob_template_visuals_match(existing, m)
        {
            return idx;
        }
        let idx = self.mob_templates.len() as u32;
        self.mob_templates.push(m.clone());
        self.mob_latest.insert(m.id, idx);
        idx
    }

    pub fn observe(&mut self, game: &Game<'_>) {
        for m in game.mobs.iter() {
            if !m.removed {
                self.observe_mob(m);
            }
        }
        for d in game.dropped.iter() {
            self.dropped.entry(d.id).or_insert_with(|| d.clone());
        }
        for o in game.orbs.iter() {
            self.orbs.entry(o.id).or_insert_with(|| o.clone());
        }
        for t in game.tnts.iter() {
            self.tnts.entry(t.id).or_insert_with(|| t.clone());
        }
        for f in game.falling.iter() {
            self.falling.entry(f.id).or_insert_with(|| f.clone());
        }
        for b in game.boats.iter() {
            self.boats.entry(b.id).or_insert_with(|| b.clone());
        }
        for c in game.carts.iter() {
            self.carts.entry(c.id).or_insert_with(|| c.clone());
        }
        for p in game.creatures.projectiles.iter() {
            self.projectiles.entry(p.id).or_insert_with(|| p.clone());
        }
        for c in game.crystals.iter() {
            self.crystals.entry(c.id).or_insert_with(|| c.clone());
        }
        let registry = game.pack.assets.registry;
        for t in game.tridents.iter() {
            self.tridents.entry(t.id).or_insert_with(|| {
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
            });
        }
        for r in game.rockets.iter() {
            self.rockets.entry(r.id).or_insert_with(|| {
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
            });
        }
        let current_potions = game.potions.rreturn_capture_thrown();
        if !current_potions.is_empty() {
            self.thrown_potions.merge_from(&current_potions);
        }
    }
}

pub struct ScrubHistoryRing {
    pub frames: VecDeque<TickHistoryFrame>,
    pub last_world_revision: u64,
    pub last_chunks_membership_rev: u64,
    pub last_rollback_epoch: u64,
    pub last_dim: Option<Dim>,
    pub reference_chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
    pub dim_snapshots: HashMap<Dim, FxHashMap<(i32, i32), Arc<Chunk>>>,
    pub carried_sounds: Vec<SoundRecord>,
    pub entity_pool: PreviewEntityPool,
    pub particle_templates: Vec<Particle>,
    pub prev_particle_positions:
        Vec<(u32, u32, horizon_client::particles::ParticleType, i32, [u64; 3])>,
    pub next_particle_id: u32,
    pub next_client_seg_id: u64,
    pub unsealed_inline_frames: usize,
    pub checkpoint_markers: Vec<i64>,
    pub dim_markers: Vec<(i64, Dim)>,
    pub damage_markers: Vec<i64>,
    last_border_tag: Option<Tag>,
    last_border: Option<Arc<WorldBorder>>,
    last_inventory: Option<Arc<Vec<Option<ItemStack>>>>,
    last_armor: Option<Arc<[Option<ItemStack>; 4]>>,
    last_offhand: Option<Arc<Option<ItemStack>>>,
    last_paintings_stamp: Option<horizon_client::join_seq::ListStamp>,
    last_paintings: Arc<Vec<Painting>>,
    last_frames_stamp: Option<horizon_client::join_seq::ListStamp>,
    last_item_frames: Arc<Vec<ItemFrame>>,
    last_stands_stamp: Option<horizon_client::join_seq::ListStamp>,
    last_armor_stands: Arc<Vec<ArmorStand>>,
    last_cushions_stamp: Option<horizon_client::join_seq::ListStamp>,
    last_cushions: Arc<Vec<Cushion>>,
    last_others: Arc<Vec<OtherEntity>>,
    last_breath: Arc<Vec<BreathSnapshot>>,
    last_fangs: Arc<Vec<super::super::fangs::Fangs>>,
    last_carried: Option<Arc<Option<ItemStack>>>,
    last_craft_grid: Option<Arc<Vec<Option<ItemStack>>>>,
    last_ender_items: Option<Arc<Vec<Option<ItemStack>>>>,
    last_ui: Option<Arc<ClientUiSnapshot>>,
    last_open_bes: Option<Arc<Vec<((i32, i32, i32), BlockEntity)>>>,
}

pub fn extract_chunk_deltas(
    old_chunks: &FxHashMap<(i32, i32), Arc<Chunk>>,
    new_chunks: &FxHashMap<(i32, i32), Arc<Chunk>>,
) -> (
    Vec<BlockDeltaRecord>,
    Vec<SectionSwapDelta>,
    Vec<BiomeDeltaRecord>,
) {
    let mut deltas = Vec::new();
    let mut swaps = Vec::new();
    let mut biome_deltas = Vec::new();
    for (&(cx, cz), live_chunk) in new_chunks.iter() {
        let Some(old_chunk) = old_chunks.get(&(cx, cz)) else {
            continue;
        };
        if Arc::ptr_eq(live_chunk, old_chunk) {
            continue;
        }
        let min_y = live_chunk.min_y();
        for (s_idx, live_sec) in live_chunk.sections.iter().enumerate() {
            let Some(old_sec) = old_chunk.sections.get(s_idx) else {
                continue;
            };
            if Arc::ptr_eq(live_sec, old_sec) {
                continue;
            }
            if old_sec.biomes != live_sec.biomes {
                biome_deltas.push(BiomeDeltaRecord {
                    cx,
                    cz,
                    s_idx,
                    old_section: Arc::clone(old_sec),
                    new_section: Arc::clone(live_sec),
                });
            }
            if let (Some(u1), Some(u2)) = (old_sec.blocks.uniform(), live_sec.blocks.uniform()) {
                if u1 == u2 {
                    continue;
                }
            }
            if old_sec.blocks == live_sec.blocks {
                continue;
            }
            let y_base = min_y + (s_idx as i32) * 16;
            let x_base = cx * 16;
            let z_base = cz * 16;
            let mut sec_changes = Vec::new();
            let mut overflow = false;
            'scan: for sy in 0..16 {
                for sz in 0..16 {
                    for sx in 0..16 {
                        let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                        let want = live_sec.get(idx);
                        let prev = old_sec.get(idx);
                        if prev != want {
                            if sec_changes.len() >= SECTION_SWAP_THRESHOLD {
                                overflow = true;
                                break 'scan;
                            }
                            sec_changes.push(BlockDeltaRecord {
                                x: x_base + sx,
                                y: y_base + sy,
                                z: z_base + sz,
                                old_state: prev,
                                new_state: want,
                            });
                        }
                    }
                }
            }
            if overflow {
                swaps.push(SectionSwapDelta {
                    cx,
                    cz,
                    s_idx,
                    old_section: Arc::clone(old_sec),
                    new_section: Arc::clone(live_sec),
                });
            } else {
                deltas.extend(sec_changes);
            }
        }
    }
    (deltas, swaps, biome_deltas)
}

fn open_container_be_equal(a: &BlockEntity, b: &BlockEntity) -> bool {
    match (a, b) {
        (BlockEntity::Chest(x), BlockEntity::Chest(y)) => x == y,
        (BlockEntity::Furnace(x), BlockEntity::Furnace(y)) => {
            x.kind == y.kind
                && x.lit_remaining == y.lit_remaining
                && x.lit_total == y.lit_total
                && x.cooking_timer == y.cooking_timer
                && x.cooking_total == y.cooking_total
                && x.speed == y.speed
                && x.items == y.items
        }
        (BlockEntity::Brewing(x), BlockEntity::Brewing(y)) => {
            x.brew_time == y.brew_time
                && x.total_brew_time == y.total_brew_time
                && x.fuel == y.fuel
                && x.total_fuel == y.total_fuel
                && x.speed == y.speed
                && x.ingredient == y.ingredient
                && x.last_bottles == y.last_bottles
                && x.items == y.items
        }
        (BlockEntity::Crafter(x), BlockEntity::Crafter(y)) => {
            x.items == y.items
                && x.disabled == y.disabled
                && x.triggered == y.triggered
                && x.crafting_ticks == y.crafting_ticks
                && x.state_triggered == y.state_triggered
        }
        _ => false,
    }
}

fn open_container_bes_equal(
    a: &[((i32, i32, i32), BlockEntity)],
    b: &[((i32, i32, i32), BlockEntity)],
) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|((pa, ba), (pb, bb))| pa == pb && open_container_be_equal(ba, bb))
}

impl ScrubHistoryRing {
    pub fn new() -> Self {
        Self {
            frames: VecDeque::with_capacity(1200),
            last_world_revision: 0,
            last_chunks_membership_rev: 0,
            last_rollback_epoch: get_rollback_epoch(),
            last_dim: None,
            reference_chunks: FxHashMap::default(),
            dim_snapshots: HashMap::new(),
            carried_sounds: Vec::new(),
            entity_pool: PreviewEntityPool::default(),
            particle_templates: Vec::new(),
            prev_particle_positions: Vec::new(),
            next_particle_id: 1,
            next_client_seg_id: 1,
            unsealed_inline_frames: 0,
            checkpoint_markers: Vec::new(),
            dim_markers: Vec::new(),
            damage_markers: Vec::new(),
            last_border_tag: None,
            last_border: None,
            last_inventory: None,
            last_armor: None,
            last_offhand: None,
            last_paintings_stamp: None,
            last_paintings: Arc::new(Vec::new()),
            last_frames_stamp: None,
            last_item_frames: Arc::new(Vec::new()),
            last_stands_stamp: None,
            last_armor_stands: Arc::new(Vec::new()),
            last_cushions_stamp: None,
            last_cushions: Arc::new(Vec::new()),
            last_others: Arc::new(Vec::new()),
            last_breath: Arc::new(Vec::new()),
            last_fangs: Arc::new(Vec::new()),
            last_carried: None,
            last_craft_grid: None,
            last_ender_items: None,
            last_ui: None,
            last_open_bes: None,
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.last_world_revision = 0;
        self.last_chunks_membership_rev = 0;
        self.last_rollback_epoch = get_rollback_epoch();
        self.last_dim = None;
        self.reference_chunks.clear();
        self.dim_snapshots.clear();
        self.carried_sounds.clear();
        self.entity_pool.clear();
        self.particle_templates.clear();
        self.prev_particle_positions.clear();
        self.next_particle_id = 1;
        self.next_client_seg_id = 1;
        self.unsealed_inline_frames = 0;
        rrtl_clear_client();
        self.checkpoint_markers.clear();
        self.dim_markers.clear();
        self.damage_markers.clear();
        self.last_border_tag = None;
        self.last_border = None;
        self.last_inventory = None;
        self.last_armor = None;
        self.last_offhand = None;
        self.last_paintings_stamp = None;
        self.last_paintings = Arc::new(Vec::new());
        self.last_frames_stamp = None;
        self.last_item_frames = Arc::new(Vec::new());
        self.last_stands_stamp = None;
        self.last_armor_stands = Arc::new(Vec::new());
        self.last_cushions_stamp = None;
        self.last_cushions = Arc::new(Vec::new());
        self.last_others = Arc::new(Vec::new());
        self.last_breath = Arc::new(Vec::new());
        self.last_fangs = Arc::new(Vec::new());
        self.last_carried = None;
        self.last_craft_grid = None;
        self.last_ender_items = None;
        self.last_ui = None;
        self.last_open_bes = None;
    }

    pub fn time_range(&self) -> Option<(i64, i64)> {
        let first = self.frames.front()?.game_time;
        let last = self.frames.back()?.game_time;
        Some((first, last))
    }

    pub fn resolve_particles(&self, pref: &ParticleFrameRef) -> Vec<ParticleTickState> {
        match pref {
            ParticleFrameRef::Inline(v) => v.clone(),
            ParticleFrameRef::RrtlSealed { segment_id, offset } => {
                rrtl_load_particle_frame(*segment_id, *offset)
            }
        }
    }

    pub fn resolve_mobs(&self, mref: &MobPoseFrameRef) -> Vec<MobPoseRecord> {
        match mref {
            MobPoseFrameRef::Inline(v) => v.clone(),
            MobPoseFrameRef::RrtlSealed { segment_id, offset } => {
                rrtl_load_client_mobs_frame(*segment_id, *offset)
            }
        }
    }

    fn intern_particle_template(&mut self, p: &Particle) -> u32 {
        let g_bits = p.gravity.to_bits();
        let uv_bits = [
            p.uv[0].to_bits(),
            p.uv[1].to_bits(),
            p.uv[2].to_bits(),
            p.uv[3].to_bits(),
        ];
        for (idx, t) in self.particle_templates.iter().enumerate() {
            if t.ty == p.ty
                && t.lifetime == p.lifetime
                && t.gravity.to_bits() == g_bits
                && t.uv[0].to_bits() == uv_bits[0]
                && t.uv[1].to_bits() == uv_bits[1]
                && t.uv[2].to_bits() == uv_bits[2]
                && t.uv[3].to_bits() == uv_bits[3]
            {
                return idx as u32;
            }
        }
        let idx = self.particle_templates.len() as u32;
        self.particle_templates.push(p.clone());
        idx
    }

    fn track_live_particles(&mut self, game: &Game<'_>) -> Vec<ParticleTickState> {
        let mut prev_map: FxHashMap<
            (u64, u64, u64),
            VecDeque<(u32, u32, horizon_client::particles::ParticleType, i32)>,
        > = FxHashMap::default();
        for &(id, tpl_id, ty, lifetime, pos_bits) in &self.prev_particle_positions {
            prev_map
                .entry((pos_bits[0], pos_bits[1], pos_bits[2]))
                .or_default()
                .push_back((id, tpl_id, ty, lifetime));
        }

        let mut next_prev = Vec::new();
        let mut states = Vec::new();
        for p in game.particles.live() {
            let prev_key = (
                p.previous[0].to_bits(),
                p.previous[1].to_bits(),
                p.previous[2].to_bits(),
            );
            let mut matched = None;
            if let Some(bucket) = prev_map.get_mut(&prev_key)
                && let Some(pos) = bucket
                    .iter()
                    .position(|&(_, _, ty, lt)| ty == p.ty && lt == p.lifetime)
            {
                let (id, tpl_id, _, _) = bucket.remove(pos).unwrap();
                matched = Some((id, tpl_id));
            }
            let (id, template_id) = match matched {
                Some(existing) => existing,
                None => {
                    let id = self.next_particle_id;
                    self.next_particle_id = self.next_particle_id.wrapping_add(1).max(1);
                    let tpl_id = self.intern_particle_template(p);
                    (id, tpl_id)
                }
            };
            let cur_bits = [
                p.position[0].to_bits(),
                p.position[1].to_bits(),
                p.position[2].to_bits(),
            ];
            next_prev.push((id, template_id, p.ty, p.lifetime, cur_bits));
            states.push(ParticleTickState {
                id,
                template_id,
                pos: p.position,
                vel: p.velocity,
                size: p.size,
                rgba: p.rgba,
                age: p.age,
            });
        }
        self.prev_particle_positions = next_prev;
        states
    }

    fn maybe_seal_client_segment(&mut self) {
        if self.unsealed_inline_frames < RRTL_SEGMENT_TICKS * 2 {
            return;
        }
        let mut indices = Vec::with_capacity(RRTL_SEGMENT_TICKS);
        for (i, f) in self.frames.iter().enumerate() {
            if matches!(f.particles, ParticleFrameRef::Inline(_)) {
                indices.push(i);
                if indices.len() >= RRTL_SEGMENT_TICKS {
                    break;
                }
            }
        }
        if indices.len() < RRTL_SEGMENT_TICKS {
            return;
        }
        let seg_id = self.next_client_seg_id;
        self.next_client_seg_id += 1;
        let start_tick = self.frames[indices[0]].game_time;
        let end_tick = self.frames[*indices.last().unwrap()].game_time;
        let mut p_batch = Vec::with_capacity(RRTL_SEGMENT_TICKS);
        let mut m_batch = Vec::with_capacity(RRTL_SEGMENT_TICKS);
        for (offset, &frame_idx) in indices.iter().enumerate() {
            let old_p = std::mem::replace(
                &mut self.frames[frame_idx].particles,
                ParticleFrameRef::RrtlSealed {
                    segment_id: seg_id,
                    offset,
                },
            );
            if let ParticleFrameRef::Inline(states) = old_p {
                p_batch.push(states);
            } else {
                p_batch.push(Vec::new());
            }
            let old_m = std::mem::replace(
                &mut self.frames[frame_idx].mobs,
                MobPoseFrameRef::RrtlSealed {
                    segment_id: seg_id,
                    offset,
                },
            );
            if let MobPoseFrameRef::Inline(mobs) = old_m {
                m_batch.push(mobs);
            } else {
                m_batch.push(Vec::new());
            }
        }
        self.unsealed_inline_frames = self.unsealed_inline_frames.saturating_sub(indices.len());
        rrtl_seal_client_segment(seg_id, start_tick, end_tick, p_batch, m_batch);
    }

    fn rewind_reference_for_dropped_future(&mut self, active_dim: Dim, from_tick_inclusive: i64) {
        let mut section_swaps: FxHashMap<(Dim, i32, i32, usize), Arc<Section>> =
            FxHashMap::default();
        let mut swap_ticks: FxHashMap<(Dim, i32, i32, usize), i64> = FxHashMap::default();
        let mut biome_swaps: FxHashMap<(Dim, i32, i32, usize), Arc<Section>> = FxHashMap::default();
        let mut coalesced_blocks: FxHashMap<(Dim, i32, i32, i32), (u32, i64)> =
            FxHashMap::default();

        for frame in self.frames.iter() {
            if frame.game_time >= from_tick_inclusive {
                for s in &frame.sounds {
                    if self.carried_sounds.len() < 128
                        && !self.carried_sounds.iter().any(|c| c.identifier == s.identifier)
                    {
                        self.carried_sounds.push(s.clone());
                    }
                }
            }
        }

        for frame in self.frames.iter().rev() {
            if frame.game_time < from_tick_inclusive {
                break;
            }
            for swap in frame.section_swaps.iter().rev() {
                let key = (frame.dim, swap.cx, swap.cz, swap.s_idx);
                section_swaps.insert(key, Arc::clone(&swap.old_section));
                swap_ticks.insert(key, frame.game_time);
            }
            for b in frame.biome_deltas.iter().rev() {
                biome_swaps.insert((frame.dim, b.cx, b.cz, b.s_idx), Arc::clone(&b.old_section));
            }
            for d in frame.block_deltas.iter().rev() {
                coalesced_blocks.insert(
                    (frame.dim, d.x, d.y, d.z),
                    (d.old_state, frame.game_time),
                );
            }
        }

        let mut swaps_by_dim_chunk: FxHashMap<(Dim, i32, i32), Vec<(usize, Arc<Section>)>> =
            FxHashMap::default();
        for ((dim, cx, cz, s_idx), target_sec) in section_swaps {
            swaps_by_dim_chunk
                .entry((dim, cx, cz))
                .or_default()
                .push((s_idx, target_sec));
        }

        for ((dim, cx, cz), secs) in swaps_by_dim_chunk {
            let chunk_map = if dim == active_dim {
                Some(&mut self.reference_chunks)
            } else {
                self.dim_snapshots.get_mut(&dim)
            };
            if let Some(map) = chunk_map
                && let Some(chunk_arc) = map.get_mut(&(cx, cz))
            {
                let chunk = Arc::make_mut(chunk_arc);
                for (s_idx, target_sec) in secs {
                    if let Some(cur_sec) = chunk.sections.get(s_idx)
                        && !Arc::ptr_eq(cur_sec, &target_sec)
                    {
                        let mut values = [0u32; 4096];
                        target_sec.unpack(&mut values);
                        chunk.replace_blocks(s_idx, &values);
                        chunk.sections[s_idx] = target_sec;
                    }
                }
            }
        }

        for ((dim, x, y, z), (old_state, delta_tick)) in coalesced_blocks {
            let chunk_map = if dim == active_dim {
                Some(&mut self.reference_chunks)
            } else {
                self.dim_snapshots.get_mut(&dim)
            };
            if let Some(map) = chunk_map
                && let Some(chunk_arc) = map.get_mut(&(x >> 4, z >> 4))
            {
                let cx = x >> 4;
                let cz = z >> 4;
                if let Some(s_idx) = chunk_arc.section_index(y)
                    && let Some(&swap_tick) = swap_ticks.get(&(dim, cx, cz, s_idx))
                    && delta_tick >= swap_tick
                {
                    continue;
                }
                Arc::make_mut(chunk_arc).set(x & 15, y, z & 15, old_state);
            }
        }

        for ((dim, cx, cz, s_idx), target_sec) in biome_swaps {
            let chunk_map = if dim == active_dim {
                Some(&mut self.reference_chunks)
            } else {
                self.dim_snapshots.get_mut(&dim)
            };
            if let Some(map) = chunk_map
                && let Some(chunk_arc) = map.get_mut(&(cx, cz))
            {
                let chunk = Arc::make_mut(chunk_arc);
                if let Some(sec_arc) = chunk.sections.get_mut(s_idx) {
                    Arc::make_mut(sec_arc).biomes = target_sec.biomes.clone();
                }
            }
        }
    }

    pub fn record_tick(&mut self, game: &mut Game<'_>) {
        if !with_state(|st| st.scrub_enabled) {
            return;
        }
        if !game.clock.runs_normally() {
            return;
        }
        let t_client_start = std::time::Instant::now();
        let m_cli_0 = StatmMem::current();
        let now = game.world.game_time;
        let current_epoch = get_rollback_epoch();
        let chunks_membership_rev = game.world.chunks.membership_revision();
        let mut force_extract = false;

        if let Some(back) = self.frames.back() {
            if now == back.game_time {
                return;
            }
            if now < back.game_time {
                if current_epoch != self.last_rollback_epoch {
                    self.frames.retain(|f| f.game_time < now);
                    self.unsealed_inline_frames = self
                        .frames
                        .iter()
                        .filter(|f| matches!(f.particles, ParticleFrameRef::Inline(_)))
                        .count();
                    rrtl_truncate_client_after(now - 1);
                    self.checkpoint_markers.retain(|&t| t < now);
                    self.dim_markers.retain(|&(t, _)| t < now);
                    self.damage_markers.retain(|&t| t < now);
                    self.carried_sounds.clear();
                    self.reference_chunks = (&*game.world.chunks).clone();
                    self.last_world_revision = game.world.revision;
                    self.last_chunks_membership_rev = chunks_membership_rev;
                    self.last_dim = Some(game.dim);
                    self.last_rollback_epoch = current_epoch;
                } else {
                    // C::SetTime can rewind client game_time back toward server time while chunks
                    // already hold received server edits; rewind reference_chunks across dropped
                    // frames instead of overwriting the baseline with post-edit world chunks.
                    self.rewind_reference_for_dropped_future(game.dim, now);
                    self.frames.retain(|f| f.game_time < now);
                    self.unsealed_inline_frames = self
                        .frames
                        .iter()
                        .filter(|f| matches!(f.particles, ParticleFrameRef::Inline(_)))
                        .count();
                    rrtl_truncate_client_after(now - 1);
                    self.checkpoint_markers.retain(|&t| t < now);
                    self.dim_markers.retain(|&(t, _)| t < now);
                    self.damage_markers.retain(|&t| t < now);
                    force_extract = true;
                }
            }
        }

        if self.last_dim != Some(game.dim) {
            if let Some(prev_dim) = self.last_dim {
                self.dim_snapshots
                    .insert(prev_dim, self.reference_chunks.clone());
                self.dim_markers.push((now, game.dim));
            }
            self.reference_chunks = (&*game.world.chunks).clone();
            self.last_world_revision = game.world.revision;
            self.last_chunks_membership_rev = chunks_membership_rev;
            self.last_dim = Some(game.dim);
            self.last_rollback_epoch = current_epoch;
            force_extract = false;
        }

        let t_deltas_0 = std::time::Instant::now();
        let (mut block_deltas, mut section_swaps, mut biome_deltas) =
            (Vec::new(), Vec::new(), Vec::new());
        if self.reference_chunks.is_empty() {
            self.reference_chunks = (&*game.world.chunks).clone();
            self.last_world_revision = game.world.revision;
            self.last_chunks_membership_rev = chunks_membership_rev;
            self.last_rollback_epoch = current_epoch;
        } else if force_extract
            || game.world.revision != self.last_world_revision
            || chunks_membership_rev != self.last_chunks_membership_rev
        {
            let extracted = extract_chunk_deltas(&self.reference_chunks, &game.world.chunks);
            block_deltas = extracted.0;
            section_swaps = extracted.1;
            biome_deltas = extracted.2;
            for (&coord, live_chunk) in game.world.chunks.iter() {
                if let Some(old_chunk) = self.reference_chunks.get(&coord) {
                    if !Arc::ptr_eq(live_chunk, old_chunk) {
                        self.reference_chunks.insert(coord, Arc::clone(live_chunk));
                    }
                } else {
                    self.reference_chunks.insert(coord, Arc::clone(live_chunk));
                }
            }
            if self.reference_chunks.len() != game.world.chunks.len() {
                self.reference_chunks.retain(|k, _| game.world.chunks.contains_key(k));
            }
            self.last_world_revision = game.world.revision;
            self.last_chunks_membership_rev = chunks_membership_rev;
        }
        let t_deltas = t_deltas_0.elapsed();

        let t_entity_0 = std::time::Instant::now();
        self.entity_pool.observe(game);
        let t_entity_pool = t_entity_0.elapsed();

        let mut break_progress = Vec::new();
        for (&breaker, &(pos, stage, _)) in game.destroying.iter() {
            break_progress.push(BreakProgressRecord {
                breaker,
                pos,
                stage,
            });
        }

        let mut spawner_spins = Vec::new();
        for (&pos, be) in game.world.block_entities.iter() {
            if let BlockEntity::Spawner(s) = be {
                spawner_spins.push(SpawnerSpinRecord {
                    pos: [pos.0, pos.1, pos.2],
                    spin: s.spin,
                    o_spin: s.o_spin,
                    client_delay: s.client_delay,
                });
            }
        }

        let p_pos = game
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z])
            .unwrap_or([0.0; 3]);
        let p_yaw = game.yaw;
        let p_pitch = game.pitch;

        let player_inventory = if let Some(prev) = &self.last_inventory
            && **prev == game.inventory
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(game.inventory.clone());
            self.last_inventory = Some(Arc::clone(&arc));
            arc
        };
        let player_armor = if let Some(prev) = &self.last_armor
            && **prev == game.armor
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(game.armor.clone());
            self.last_armor = Some(Arc::clone(&arc));
            arc
        };
        let player_offhand = if let Some(prev) = &self.last_offhand
            && **prev == game.offhand
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(game.offhand.clone());
            self.last_offhand = Some(Arc::clone(&arc));
            arc
        };
        let carried = if let Some(prev) = &self.last_carried
            && **prev == game.carried
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(game.carried.clone());
            self.last_carried = Some(Arc::clone(&arc));
            arc
        };
        let craft_grid = if let Some(prev) = &self.last_craft_grid
            && **prev == game.craft_grid
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(game.craft_grid.clone());
            self.last_craft_grid = Some(Arc::clone(&arc));
            arc
        };
        let ender_items = if let Some(prev) = &self.last_ender_items
            && **prev == game.ender_items
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(game.ender_items.clone());
            self.last_ender_items = Some(Arc::clone(&arc));
            arc
        };
        let live_ui = capture_client_ui_snapshot(game);
        let ui = if let Some(prev) = &self.last_ui
            && **prev == live_ui
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(live_ui);
            self.last_ui = Some(Arc::clone(&arc));
            arc
        };
        let open_container_bes = if game.container.is_some() {
            let parts = game.container_parts();
            let mut vec = Vec::with_capacity(parts.len());
            for (pos, _) in parts {
                if let Some(be) = game.world.block_entities.get(&pos) {
                    vec.push((pos, be.clone()));
                }
            }
            if vec.is_empty() {
                self.last_open_bes = None;
                None
            } else if let Some(prev) = &self.last_open_bes
                && open_container_bes_equal(prev, &vec)
            {
                Some(Arc::clone(prev))
            } else {
                let arc = Arc::new(vec);
                self.last_open_bes = Some(Arc::clone(&arc));
                Some(arc)
            }
        } else {
            self.last_open_bes = None;
            None
        };
        let player_game_mode = game.mode_id();
        let player_previous_game_mode = game.previous_mode.map(|m| m.id());
        let player_experience = game.experience;
        let player_acting_abilities = game.acting_abilities;
        let player_abilities = game
            .player
            .as_ref()
            .map(|p| (p.invulnerable, p.may_fly, p.spectator, p.flying, p.flying_speed))
            .unwrap_or((false, false, false, false, 0.05));

        let fov_modifier = if game.view.fov_modifier > 0.05 {
            game.view.fov_modifier
        } else {
            1.0
        };
        let avatar_walk_dist = game.avatar.walk_dist;
        let avatar_bob = game.avatar.bob;
        let (
            player_sprinting,
            player_sneaking,
            player_fall_flying,
            player_hurt_time,
            player_hurt_dir,
            player_health,
            player_absorption,
            player_food,
            player_fire_ticks,
            player_air,
            player_ticks_frozen,
        ) = game
            .player
            .as_ref()
            .map(|p| {
                (
                    p.sprinting,
                    p.local_crouching,
                    p.fall_flying,
                    p.health.hurt_animation_time,
                    p.health.hurt_dir,
                    p.health.health(),
                    p.health.absorption(),
                    p.health.food.food,
                    p.fire_ticks,
                    p.air,
                    p.ticks_frozen,
                )
            })
            .unwrap_or((false, false, false, 0, 0.0, 20.0, 0.0, 20, 0, 300, 0));

        if player_hurt_time >= 9 && self.damage_markers.last().copied() != Some(now) {
            self.damage_markers.push(now);
        }

        let t_poses_0 = std::time::Instant::now();
        let mut mobs = Vec::with_capacity(game.mobs.len());
        for m in game.mobs.iter() {
            if m.removed {
                continue;
            }
            let template_idx = self
                .entity_pool
                .mob_latest
                .get(&m.id)
                .copied()
                .unwrap_or(0);
            mobs.push(MobPoseRecord {
                id: m.id,
                template_idx,
                pos: [m.position.x, m.position.y, m.position.z],
                yaw: m.yaw,
                body_yaw: m.body_yaw,
                head_yaw: m.head_yaw,
                head_pitch: m.head_pitch,
                limb_position: m.limb_position,
                limb_speed: m.limb_speed,
                attack_anim: m.anim.attack_anim,
                swell: m.swell as f32,
                health: m.health,
                alive: m.alive(),
                hurt_time: m.hurt_time,
                death_time: m.death_time,
                sheared: m.extra.sheared,
                color: m.extra.color,
                size: m.extra.size,
                profession: m.extra.profession.index().unwrap_or(0),
                villager_level: m.extra.villager_level,
                variant: m.extra.variant,
                variant2: m.extra.variant2,
                variant3: m.extra.variant3,
                age: m.extra.age,
                sitting: m.extra.sitting,
                sitting_pose: m.extra.sitting_pose,
                charged: m.extra.charged,
                invisible: m.invisible,
                glowing: m.glowing,
                on_fire: m.fire_ticks > 0,
            });
        }

        let items = game
            .dropped
            .iter()
            .map(|d| EntityPoseRecord {
                id: d.id,
                pos: [d.position.x, d.position.y, d.position.z],
                vel: [d.velocity.x, d.velocity.y, d.velocity.z],
                yaw: 0.0,
                pitch: 0.0,
                extra_i32: d.ticks as i32,
            })
            .collect();

        let orbs = game
            .orbs
            .iter()
            .map(|o| EntityPoseRecord {
                id: o.id,
                pos: [o.position.x, o.position.y, o.position.z],
                vel: [o.velocity.x, o.velocity.y, o.velocity.z],
                yaw: 0.0,
                pitch: 0.0,
                extra_i32: o.ticks as i32,
            })
            .collect();

        let tnts = game
            .tnts
            .iter()
            .map(|t| EntityPoseRecord {
                id: t.id,
                pos: [t.position.x, t.position.y, t.position.z],
                vel: [t.velocity.x, t.velocity.y, t.velocity.z],
                yaw: t.rotation[0],
                pitch: t.rotation[1],
                extra_i32: t.fuse,
            })
            .collect();

        let falling = game
            .falling
            .iter()
            .map(|f| EntityPoseRecord {
                id: f.id,
                pos: [f.position.x, f.position.y, f.position.z],
                vel: [f.velocity.x, f.velocity.y, f.velocity.z],
                yaw: f.rotation[0],
                pitch: f.rotation[1],
                extra_i32: f.age,
            })
            .collect();

        let boats = game
            .boats
            .iter()
            .map(|b| EntityPoseRecord {
                id: b.id,
                pos: [b.position.x, b.position.y, b.position.z],
                vel: [b.velocity.x, b.velocity.y, b.velocity.z],
                yaw: b.yaw,
                pitch: 0.0,
                extra_i32: b.hurt_time,
            })
            .collect();

        let carts = game
            .carts
            .iter()
            .map(|c| EntityPoseRecord {
                id: c.id,
                pos: [c.position.x, c.position.y, c.position.z],
                vel: [c.velocity.x, c.velocity.y, c.velocity.z],
                yaw: c.yaw,
                pitch: c.pitch,
                extra_i32: c.hurt_time,
            })
            .collect();

        let projectiles = game
            .creatures
            .projectiles
            .iter()
            .map(|p| EntityPoseRecord {
                id: p.id,
                pos: [p.position.x, p.position.y, p.position.z],
                vel: [p.velocity.x, p.velocity.y, p.velocity.z],
                yaw: p.yaw,
                pitch: p.pitch,
                extra_i32: p.age as i32,
            })
            .collect();

        let tridents = game
            .tridents
            .iter()
            .map(|tr| EntityPoseRecord {
                id: tr.id,
                pos: [tr.position.x, tr.position.y, tr.position.z],
                vel: [tr.velocity.x, tr.velocity.y, tr.velocity.z],
                yaw: tr.yaw,
                pitch: tr.pitch,
                extra_i32: tr.in_ground_time,
            })
            .collect();

        let rockets = game
            .rockets
            .iter()
            .map(|r| EntityPoseRecord {
                id: r.id,
                pos: [r.position.x, r.position.y, r.position.z],
                vel: [r.velocity.x, r.velocity.y, r.velocity.z],
                yaw: r.rotation[0],
                pitch: r.rotation[1],
                extra_i32: r.life,
            })
            .collect();

        let crystals = game
            .crystals
            .iter()
            .map(|c| EntityPoseRecord {
                id: c.id,
                pos: [c.position.x, c.position.y, c.position.z],
                vel: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
                extra_i32: 0,
            })
            .collect();

        let p_stamp = game.paintings.stamp();
        let paintings = if self.last_paintings_stamp == Some(p_stamp) {
            Arc::clone(&self.last_paintings)
        } else {
            let arc = Arc::new(game.paintings.iter().cloned().collect());
            self.last_paintings_stamp = Some(p_stamp);
            self.last_paintings = Arc::clone(&arc);
            arc
        };

        let f_stamp = game.item_frames.stamp();
        let item_frames = if self.last_frames_stamp == Some(f_stamp) {
            Arc::clone(&self.last_item_frames)
        } else {
            let arc = Arc::new(game.item_frames.iter().cloned().collect());
            self.last_frames_stamp = Some(f_stamp);
            self.last_item_frames = Arc::clone(&arc);
            arc
        };

        let a_stamp = game.armor_stands.stamp();
        let armor_stands = if self.last_stands_stamp == Some(a_stamp) {
            Arc::clone(&self.last_armor_stands)
        } else {
            let arc = Arc::new(game.armor_stands.iter().cloned().collect());
            self.last_stands_stamp = Some(a_stamp);
            self.last_armor_stands = Arc::clone(&arc);
            arc
        };

        let c_stamp = game.cushions.stamp();
        let cushions = if self.last_cushions_stamp == Some(c_stamp) {
            Arc::clone(&self.last_cushions)
        } else {
            let arc = Arc::new(game.cushions.iter().cloned().collect());
            self.last_cushions_stamp = Some(c_stamp);
            self.last_cushions = Arc::clone(&arc);
            arc
        };

        let others = if game.others.is_empty() && self.last_others.is_empty() {
            Arc::clone(&self.last_others)
        } else {
            let arc = Arc::new(game.others.iter().cloned().collect());
            self.last_others = Arc::clone(&arc);
            arc
        };

        let dragon = game.dragon.as_ref().map(DragonSnapshot::from_live);

        let breath_clouds = if game.breath_clouds.is_empty() && self.last_breath.is_empty() {
            Arc::clone(&self.last_breath)
        } else {
            let v: Vec<BreathSnapshot> = game
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
            let arc = Arc::new(v);
            self.last_breath = Arc::clone(&arc);
            arc
        };

        let fangs = if game.fangs.list.is_empty() && self.last_fangs.is_empty() {
            Arc::clone(&self.last_fangs)
        } else {
            let arc = Arc::new(game.fangs.list.iter().cloned().collect());
            self.last_fangs = Arc::clone(&arc);
            arc
        };

        let thrown_potions = game.potions.rreturn_thrown_poses();

        let hook = game.hook.as_ref().map(|h| {
            let (nibble, _, _, _, _, bobbing) = h.rreturn_fishing_state();
            HookVisualRecord {
                id: h.id(),
                pos: [h.position.x, h.position.y, h.position.z],
                vel: h.rreturn_velocity(),
                nibble,
                bobbing,
            }
        });

        let (is_swinging, swing_hand, swing_dur) = if let Some(c) = game.swing.current {
            (
                true,
                if c.hand == horizon_client::first_person::Hand::Off {
                    1
                } else {
                    0
                },
                c.duration_ticks,
            )
        } else {
            (false, 0, 0)
        };

        let arm_swing = ArmSwingRecord {
            swinging: is_swinging,
            hand: swing_hand,
            animation: game.swing.animation,
            old_animation: game.swing.old_animation,
            ticks: game.swing.ticks,
            duration: swing_dur,
            height_main: game.heights.main,
            height_off: game.heights.off,
        };
        let t_poses = t_poses_0.elapsed();

        let t_part_0 = std::time::Instant::now();
        let particle_states = self.track_live_particles(game);
        let live_particle_count = particle_states.len();
        let elder_guardians = game.particles.elder_guardians().to_vec();

        game.audio.record_heard = true;
        let mut sounds = std::mem::take(&mut self.carried_sounds);
        let captured = std::mem::take(game.audio.capture.get_or_insert_with(Vec::new));
        for sound in captured {
            let norm = if sound.event.contains(':') {
                sound.event
            } else {
                format!("minecraft:{}", sound.event)
            };
            prewarm_reverse_sound(game, &norm);
            sounds.push(SoundRecord {
                identifier: norm,
                position: sound.position,
                relative: false,
            });
        }
        let notifications = std::mem::take(&mut game.audio.engine.notifications);
        for notif in notifications {
            let norm = if notif.identifier.contains(':') {
                notif.identifier
            } else {
                format!("minecraft:{}", notif.identifier)
            };
            if !sounds.iter().any(|s| s.identifier == norm) {
                prewarm_reverse_sound(game, &norm);
                sounds.push(SoundRecord {
                    identifier: norm,
                    position: notif.position,
                    relative: notif.relative,
                });
            }
        }

        let max_ticks = scrub_max_history_ticks();
        while self.frames.len() >= max_ticks {
            if let Some(evicted) = self.frames.pop_front()
                && matches!(evicted.particles, ParticleFrameRef::Inline(_))
            {
                self.unsealed_inline_frames = self.unsealed_inline_frames.saturating_sub(1);
            }
        }

        let b_tag = game.border.pack();
        let border = if self.last_border_tag.as_ref() == Some(&b_tag)
            && let Some(prev) = &self.last_border
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(game.border.clone());
            self.last_border_tag = Some(b_tag);
            self.last_border = Some(Arc::clone(&arc));
            arc
        };

        let boss_overlay = game.boss_overlay.clone();
        let saved_kinds = std::mem::take(&mut game.be_anims.kinds);
        let be_anims = if game.be_anims.chests.is_empty()
            && game.be_anims.books.is_empty()
            && game.be_anims.shulkers.is_empty()
            && game.be_anims.bells.is_empty()
            && game.be_anims.conduits.is_empty()
            && game.be_anims.skulls.is_empty()
            && game.be_anims.pots.is_empty()
            && game.be_anims.gateways.is_empty()
        {
            empty_client_be_anims()
        } else {
            Arc::new(game.be_anims.clone())
        };
        game.be_anims.kinds = saved_kinds;

        self.frames.push_back(TickHistoryFrame {
            game_time: now,
            dim: game.dim,
            day_time: game.world.day_time,
            weather: game.world.weather,
            border,
            boss_overlay,
            sculk_darkness: game.sculk.darkness,
            flash: game.flash,
            be_anims,
            spawner_spins,
            moving_pistons: game.world.moving_pistons.clone(),
            player_pos: p_pos,
            player_yaw: p_yaw,
            player_pitch: p_pitch,
            player_selected: game.selected,
            player_inventory,
            player_armor,
            player_offhand,
            player_using: game.using,
            player_use_duration: game.use_duration,
            player_health,
            player_absorption,
            player_food,
            player_fire_ticks,
            player_air,
            player_ticks_frozen,
            player_sneaking,
            player_fall_flying,
            player_game_mode,
            player_previous_game_mode,
            player_experience,
            player_acting_abilities,
            player_abilities,
            mobs: MobPoseFrameRef::Inline(mobs),
            items,
            orbs,
            tnts,
            falling,
            boats,
            carts,
            projectiles,
            tridents,
            rockets,
            crystals,
            paintings,
            item_frames,
            armor_stands,
            cushions,
            others,
            dragon,
            breath_clouds,
            fangs,
            thrown_potions,
            hook,
            block_deltas,
            section_swaps,
            biome_deltas,
            break_progress,
            arm_swing,
            particles: ParticleFrameRef::Inline(particle_states),
            elder_guardians,
            sounds,
            fov_modifier,
            avatar_walk_dist,
            avatar_bob,
            player_sprinting,
            player_hurt_time,
            player_hurt_dir,
            carried,
            craft_grid,
            ender_items,
            ui,
            open_container_bes,
        });
        self.unsealed_inline_frames += 1;
        self.maybe_seal_client_segment();
        let t_part = t_part_0.elapsed();
        let t_total = t_client_start.elapsed();
        let t_sub = t_deltas + t_entity_pool + t_poses + t_part;
        let t_misc = if t_total > t_sub { t_total - t_sub } else { std::time::Duration::ZERO };
        record_client_tick_perf(
            t_total.as_micros() as u64,
            t_deltas.as_micros() as u64,
            t_entity_pool.as_micros() as u64,
            t_poses.as_micros() as u64,
            t_part.as_micros() as u64,
            t_misc.as_micros() as u64,
        );
        let m_cli_1 = StatmMem::current();
        record_mem_client_tick(now, m_cli_0, m_cli_1, self.frames.len(), live_particle_count);
    }

    pub fn find_frame_pair(
        &self,
        target_time: f64,
    ) -> Option<(&TickHistoryFrame, &TickHistoryFrame, f32)> {
        if self.frames.is_empty() {
            return None;
        }
        if self.frames.len() == 1 {
            let f = &self.frames[0];
            return Some((f, f, 0.0));
        }

        let target_tick = target_time.floor() as i64;
        let first_tick = self.frames.front()?.game_time;
        let last_tick = self.frames.back()?.game_time;

        if target_tick <= first_tick {
            let f = self.frames.front()?;
            return Some((f, f, 0.0));
        }
        if target_tick >= last_tick {
            let f = self.frames.back()?;
            return Some((f, f, 1.0));
        }

        let idx = match self
            .frames
            .binary_search_by_key(&target_tick, |f| f.game_time)
        {
            Ok(i) => i,
            Err(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
        };

        let next_idx = (idx + 1).min(self.frames.len() - 1);
        let left = &self.frames[idx];
        let right = &self.frames[next_idx];

        let tick_delta = right.game_time - left.game_time;
        let alpha = if tick_delta <= 0 {
            0.0
        } else {
            let progress = (target_time - left.game_time as f64) / (tick_delta as f64);
            (progress as f32).clamp(0.0, 1.0)
        };

        Some((left, right, alpha))
    }
}

fn empty_client_be_anims() -> Arc<BlockEntityAnims> {
    static E: OnceLock<Arc<BlockEntityAnims>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(BlockEntityAnims::default())))
}

thread_local! {
    static SCRUB_RING: RefCell<ScrubHistoryRing> = RefCell::new(ScrubHistoryRing::new());
}

pub fn with_scrub_ring<R>(f: impl FnOnce(&ScrubHistoryRing) -> R) -> R {
    SCRUB_RING.with(|cell| f(&cell.borrow()))
}

pub fn with_scrub_ring_mut<R>(f: impl FnOnce(&mut ScrubHistoryRing) -> R) -> R {
    SCRUB_RING.with(|cell| f(&mut cell.borrow_mut()))
}

pub fn record_scrub_checkpoint_marker(tick: i64) {
    if !with_state(|st| st.scrub_enabled) {
        return;
    }
    with_scrub_ring_mut(|r| {
        if r.checkpoint_markers.last().copied() != Some(tick) {
            r.checkpoint_markers.push(tick);
        }
    });
}

pub fn handle_client_chunk_unload(_game: &mut Game<'_>, _dimension: &str, chunk: [i32; 2]) {
    if !with_state(|st| st.scrub_enabled) {
        return;
    }
    with_scrub_ring_mut(|r| {
        r.reference_chunks.remove(&(chunk[0], chunk[1]));
    });
}