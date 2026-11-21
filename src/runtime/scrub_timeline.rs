use super::super::{
    Dim, ExperienceOrb, FallingBlock, Game, ItemEntity, Mob,
    commands::OtherEntity,
    dragon::Crystal,
};
use super::scrub_audio::prewarm_reverse_sound;
use super::scrub_perf::*;
use super::snapshots::{BreathSnapshot, DragonSnapshot, RocketSnapshot, TridentSnapshot};
use horizon_client::{
    block_entity_anim::BlockEntityAnims,
    boss_overlay::BossOverlay,
    chunk::{Chunk, Section},
    containers::BlockEntity,
    entities::{ArmorStand, Cushion, ItemFrame, Painting},
    fast_hash::FxHashMap,
    items::ItemStack,
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

#[derive(Clone, Copy, Debug)]
pub struct MobPoseRecord {
    pub id: u32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub body_yaw: f32,
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub limb_position: f32,
    pub limb_speed: f32,
    pub attack_anim: f32,
    pub swell: f32,
    pub health: f32,
    pub alive: bool,
    pub hurt_time: i32,
    pub death_time: i32,
    pub sheared: bool,
    pub color: u8,
    pub size: i32,
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
pub struct TickHistoryFrame {
    pub game_time: i64,
    pub dim: Dim,
    pub day_time: i64,
    pub weather: Weather,
    pub border: WorldBorder,
    pub boss_overlay: BossOverlay,
    pub sculk_darkness: (f32, f32),
    pub flash: u32,
    pub be_anims: BlockEntityAnims,
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
    pub mobs: Vec<MobPoseRecord>,
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
    pub particles: Vec<Particle>,
    pub elder_guardians: Vec<i32>,
    pub sounds: Vec<SoundRecord>,
    pub fov_modifier: f32,
    pub avatar_walk_dist: f32,
    pub avatar_bob: f32,
    pub player_sprinting: bool,
    pub player_hurt_time: i32,
    pub player_hurt_dir: f32,
}

#[derive(Default)]
pub struct PreviewEntityPool {
    pub mobs: FxHashMap<u32, Mob>,
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
        self.mobs.clear();
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

    pub fn observe(&mut self, game: &Game<'_>) {
        for m in game.mobs.iter() {
            if !m.removed {
                self.mobs.entry(m.id).or_insert_with(|| m.clone());
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
    pub last_dim: Option<Dim>,
    pub reference_chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
    pub dim_snapshots: HashMap<Dim, FxHashMap<(i32, i32), Arc<Chunk>>>,
    pub entity_pool: PreviewEntityPool,
    pub checkpoint_markers: Vec<i64>,
    pub dim_markers: Vec<(i64, Dim)>,
    pub damage_markers: Vec<i64>,
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

impl ScrubHistoryRing {
    pub fn new() -> Self {
        Self {
            frames: VecDeque::with_capacity(1200),
            last_world_revision: 0,
            last_dim: None,
            reference_chunks: FxHashMap::default(),
            dim_snapshots: HashMap::new(),
            entity_pool: PreviewEntityPool::default(),
            checkpoint_markers: Vec::new(),
            dim_markers: Vec::new(),
            damage_markers: Vec::new(),
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
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.last_world_revision = 0;
        self.last_dim = None;
        self.reference_chunks.clear();
        self.dim_snapshots.clear();
        self.entity_pool.clear();
        self.checkpoint_markers.clear();
        self.dim_markers.clear();
        self.damage_markers.clear();
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
    }

    pub fn time_range(&self) -> Option<(i64, i64)> {
        let first = self.frames.front()?.game_time;
        let last = self.frames.back()?.game_time;
        Some((first, last))
    }

    pub fn record_tick(&mut self, game: &mut Game<'_>) {
        if !game.clock.runs_normally() {
            return;
        }
        let t_client_start = std::time::Instant::now();
        let now = game.world.game_time;

        if let Some(back) = self.frames.back() {
            if now == back.game_time {
                return;
            }
            if now < back.game_time {
                self.frames.retain(|f| f.game_time < now);
                self.checkpoint_markers.retain(|&t| t < now);
                self.dim_markers.retain(|&(t, _)| t < now);
                self.damage_markers.retain(|&t| t < now);
                self.reference_chunks = (&*game.world.chunks).clone();
                self.last_world_revision = game.world.revision;
                self.last_dim = Some(game.dim);
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
            self.last_dim = Some(game.dim);
        }

        let t_deltas_0 = std::time::Instant::now();
        let (mut block_deltas, mut section_swaps, mut biome_deltas) =
            (Vec::new(), Vec::new(), Vec::new());
        if self.reference_chunks.is_empty() {
            self.reference_chunks = (&*game.world.chunks).clone();
            self.last_world_revision = game.world.revision;
        } else if game.world.revision != self.last_world_revision {
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
            mobs.push(MobPoseRecord {
                id: m.id,
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
        let particles = game.particles.live().cloned().collect::<Vec<_>>();
        let elder_guardians = game.particles.elder_guardians().to_vec();

        game.audio.record_heard = true;
        let mut sounds = Vec::new();
        if let Some(captured) = game.audio.capture.as_mut() {
            for sound in captured.drain(..) {
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
        } else {
            game.audio.capture = Some(Vec::new());
        }
        for notif in std::mem::take(&mut game.audio.engine.notifications) {
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

        if self.frames.len() >= MAX_SCRUB_TICKS {
            self.frames.pop_front();
        }

        let boss_overlay = game.boss_overlay.clone();

        self.frames.push_back(TickHistoryFrame {
            game_time: now,
            dim: game.dim,
            day_time: game.world.day_time,
            weather: game.world.weather,
            border: game.border.clone(),
            boss_overlay,
            sculk_darkness: game.sculk.darkness,
            flash: game.flash,
            be_anims: game.be_anims.clone(),
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
            mobs,
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
            particles,
            elder_guardians,
            sounds,
            fov_modifier,
            avatar_walk_dist,
            avatar_bob,
            player_sprinting,
            player_hurt_time,
            player_hurt_dir,
        });
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
    with_scrub_ring_mut(|r| {
        if r.checkpoint_markers.last().copied() != Some(tick) {
            r.checkpoint_markers.push(tick);
        }
    });
}

pub fn handle_client_chunk_unload(_game: &mut Game<'_>, _dimension: &str, chunk: [i32; 2]) {
    with_scrub_ring_mut(|r| {
        r.reference_chunks.remove(&(chunk[0], chunk[1]));
    });
}