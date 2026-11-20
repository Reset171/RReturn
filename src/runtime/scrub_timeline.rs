use super::super::{Game, Mob};
use super::capture::capture_level_snapshot;
use super::snapshots::*;
use horizon_client::{chunk::Chunk, fast_hash::FxHashMap, particles::Particle};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

pub const MAX_SCRUB_TICKS: usize = 24_000; // 20 minutes at 20 TPS
pub const KEYFRAME_INTERVAL_TICKS: i64 = 100; // 5 seconds per keyframe
pub const MAX_KEYFRAMES: usize = 240; // 240 * 100 = 24,000 ticks

#[derive(Clone, Copy, Debug)]
pub struct BlockDeltaRecord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub old_state: u32,
    pub new_state: u32,
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
}

#[derive(Clone, Copy, Debug)]
pub struct ItemPoseRecord {
    pub id: u32,
    pub pos: [f64; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectilePoseRecord {
    pub id: u32,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
}

#[derive(Clone, Debug)]
pub struct SoundRecord {
    pub identifier: String,
    pub position: [f64; 3],
    pub relative: bool,
}

#[derive(Clone, Debug)]
pub struct TickHistoryFrame {
    pub game_time: i64,
    pub player_pos: [f64; 3],
    pub player_yaw: f32,
    pub player_pitch: f32,
    pub mobs: Vec<MobPoseRecord>,
    pub items: Vec<ItemPoseRecord>,
    pub projectiles: Vec<ProjectilePoseRecord>,
    pub block_deltas: Vec<BlockDeltaRecord>,
    pub break_progress: Vec<BreakProgressRecord>,
    pub arm_swing: ArmSwingRecord,
    pub particles: Vec<horizon_client::particles::Particle>,
    pub sounds: Vec<SoundRecord>,
}

pub struct ScrubHistoryRing {
    pub frames: VecDeque<TickHistoryFrame>,
    pub keyframes: VecDeque<(i64, LevelSnapshot)>,
    pub last_keyframe_tick: i64,
    pub last_world_revision: u64,
    pub reference_chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
}

pub fn extract_chunk_block_deltas(
    old_chunks: &FxHashMap<(i32, i32), Arc<Chunk>>,
    new_chunks: &FxHashMap<(i32, i32), Arc<Chunk>>,
) -> Vec<BlockDeltaRecord> {
    let mut deltas = Vec::new();
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
            for sy in 0..16 {
                for sz in 0..16 {
                    for sx in 0..16 {
                        let idx = ((sy << 8) | (sz << 4) | sx) as usize;
                        let want = live_sec.get(idx);
                        let prev = old_sec.get(idx);
                        if prev != want {
                            deltas.push(BlockDeltaRecord {
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
        }
    }
    deltas
}

impl ScrubHistoryRing {
    pub fn new() -> Self {
        Self {
            frames: VecDeque::with_capacity(1200),
            keyframes: VecDeque::with_capacity(MAX_KEYFRAMES),
            last_keyframe_tick: -99999,
            last_world_revision: 0,
            reference_chunks: FxHashMap::default(),
        }
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.keyframes.clear();
        self.last_keyframe_tick = -99999;
        self.last_world_revision = 0;
        self.reference_chunks.clear();
    }

    pub fn time_range(&self) -> Option<(i64, i64)> {
        let first = self.frames.front()?.game_time;
        let last = self.frames.back()?.game_time;
        Some((first, last))
    }

    pub fn record_tick(&mut self, game: &mut Game<'_>) {
        let now = game.world.game_time;

        // If the game time jumped backwards, clear future history
        if let Some(back) = self.frames.back() {
            if now <= back.game_time {
                self.frames.retain(|f| f.game_time < now);
                self.keyframes.retain(|(t, _)| *t < now);
                self.reference_chunks = (&*game.world.chunks).clone();
                self.last_world_revision = game.world.revision;
            }
        }

        // Detect block changes since last tick
        let mut block_deltas = Vec::new();
        if self.reference_chunks.is_empty() {
            self.reference_chunks = (&*game.world.chunks).clone();
            self.last_world_revision = game.world.revision;
        } else if game.world.revision != self.last_world_revision {
            let live_chunks = (&*game.world.chunks).clone();
            block_deltas = extract_chunk_block_deltas(&self.reference_chunks, &live_chunks);
            self.reference_chunks = live_chunks;
            self.last_world_revision = game.world.revision;
        }

        // Record any active block breaking progress
        let mut break_progress = Vec::new();
        for (&breaker, &(pos, stage, _)) in game.destroying.iter() {
            break_progress.push(BreakProgressRecord {
                breaker,
                pos,
                stage,
            });
        }

        let p_pos = game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]).unwrap_or([0.0; 3]);
        let p_yaw = game.yaw;
        let p_pitch = game.pitch;

        let mut mobs = Vec::with_capacity(game.mobs.len().min(64));
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
            });
        }

        let mut items = Vec::new();
        for item in game.dropped.iter() {
            items.push(ItemPoseRecord {
                id: item.id,
                pos: [item.position.x, item.position.y, item.position.z],
            });
        }

        let mut projectiles = Vec::new();
        for tr in game.tridents.iter() {
            projectiles.push(ProjectilePoseRecord {
                id: tr.id,
                pos: [tr.position.x, tr.position.y, tr.position.z],
                vel: [tr.velocity.x, tr.velocity.y, tr.velocity.z],
            });
        }

        let (is_swinging, swing_hand, swing_dur) = if let Some(c) = game.swing.current {
            (
                true,
                if c.hand == horizon_client::first_person::Hand::Off { 1 } else { 0 },
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

        // Capture live particles (up to 128 particles per tick)
        let particles: Vec<horizon_client::particles::Particle> = game.particles.live().take(128).cloned().collect();

        // Ensure audio notification capture is enabled and drain sounds from the last tick
        game.audio.record_heard = true;
        let mut sounds = Vec::new();
        for notif in std::mem::take(&mut game.audio.engine.notifications) {
            sounds.push(SoundRecord {
                identifier: notif.identifier,
                position: notif.position,
                relative: notif.relative,
            });
        }

        if self.frames.len() >= MAX_SCRUB_TICKS {
            self.frames.pop_front();
        }

        self.frames.push_back(TickHistoryFrame {
            game_time: now,
            player_pos: p_pos,
            player_yaw: p_yaw,
            player_pitch: p_pitch,
            mobs,
            items,
            projectiles,
            block_deltas,
            break_progress,
            arm_swing,
            particles,
            sounds,
        });

        // Periodic in-memory keyframe snapshot (every 5 seconds)
        if now - self.last_keyframe_tick >= KEYFRAME_INTERVAL_TICKS {
            let snap = capture_level_snapshot(game, game.dim);
            if self.keyframes.len() >= MAX_KEYFRAMES {
                self.keyframes.pop_front();
            }
            self.keyframes.push_back((now, snap));
            self.last_keyframe_tick = now;
        }
    }

    pub fn find_frame_pair(&self, target_time: f64) -> Option<(&TickHistoryFrame, &TickHistoryFrame, f32)> {
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

        let idx = match self.frames.binary_search_by_key(&target_tick, |f| f.game_time) {
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

    pub fn get_nearest_keyframe(&self, target_tick: i64) -> Option<&LevelSnapshot> {
        if self.keyframes.is_empty() {
            return None;
        }
        let mut best: Option<&LevelSnapshot> = None;
        for (t, snap) in self.keyframes.iter() {
            if *t <= target_tick {
                best = Some(snap);
            } else {
                break;
            }
        }
        best.or_else(|| self.keyframes.front().map(|(_, s)| s))
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
