use super::super::{Dim, Game};
use super::capture::capture_guests_snapshot;
use super::scrub_perf::*;
use super::scrub_server_types::*;
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
    save::block_entity_encode,
    scoreboard::Scoreboard,
    sculk::WardenSpawnTracker,
    world::{HealthRules, Weather},
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::Vec3,
    nbt::Tag,
    position::{BlockPos, ChunkPos},
    random::LevelRandom,
    ticks::ChunkTicks,
};
use horizon_mod_runtime::attachments::Attachments;
use horizon_world::level::{BlockEventData, MovingPiston, Ticks};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{Arc, OnceLock},
};

pub fn mob_unchanged(live: &super::super::Mob, prev: &super::super::Mob) -> bool {
    live.id == prev.id
        && live.removed == prev.removed
        && live.age == prev.age
        && live.position == prev.position
        && live.previous == prev.previous
        && live.velocity == prev.velocity
        && live.yaw.to_bits() == prev.yaw.to_bits()
        && live.body_yaw.to_bits() == prev.body_yaw.to_bits()
        && live.head_yaw.to_bits() == prev.head_yaw.to_bits()
        && live.head_pitch.to_bits() == prev.head_pitch.to_bits()
        && live.health.to_bits() == prev.health.to_bits()
        && live.hurt_time == prev.hurt_time
        && live.invulnerable == prev.invulnerable
        && live.death_time == prev.death_time
        && live.swell == prev.swell
        && live.ambient_timer == prev.ambient_timer
        && live.random.state() == prev.random.state()
        && live.random.cached_gaussian().map(f64::to_bits)
            == prev.random.cached_gaussian().map(f64::to_bits)
        && live.limb_position.to_bits() == prev.limb_position.to_bits()
        && live.limb_speed.to_bits() == prev.limb_speed.to_bits()
        && live.anim.attack_anim.to_bits() == prev.anim.attack_anim.to_bits()
        && live.extra.anger == prev.extra.anger
        && live.extra.anger_end == prev.extra.anger_end
        && live.extra.counter == prev.extra.counter
        && live.extra.counter2 == prev.extra.counter2
        && live.extra.anim_ticks == prev.extra.anim_ticks
        && live.extra.egg_time == prev.extra.egg_time
        && live.extra.flag == prev.extra.flag
        && live.extra.flag2 == prev.extra.flag2
        && live.extra.sheared == prev.extra.sheared
        && live.extra.color == prev.extra.color
        && live.extra.size == prev.extra.size
        && live.extra.mount == prev.extra.mount
        && live.extra.rider == prev.extra.rider
        && live.extra.leader == prev.extra.leader
        && live.ai.aggressive == prev.ai.aggressive
}

pub fn chunk_ticks_eq(live: Option<&ChunkTicks>, cached: &ChunkTicks) -> bool {
    match live {
        None => cached.count() == 0,
        Some(t) => t.count() == cached.count() && t.queued() == cached.queued(),
    }
}

pub fn cow_capture_guests
    game: &Game<'_>,
    prev_map: &mut HashMap<u128, CowGuestPrev>,
    force_full: bool,
) -> (Vec<CowGuestRecord>, Vec<u128>) {
    let (raw_guests, online) = capture_guests_snapshot(game);
    let mut out = Vec::with_capacity(raw_guests.len());
    for (uuid, g) in raw_guests {
        let p = prev_map.entry(uuid).or_default();
        let dimension = if let Some(prev) = &p.dimension
            && &**prev == g.dimension.as_str()
        {
            Arc::clone(prev)
        } else {
            let arc: Arc<str> = Arc::from(g.dimension.as_str());
            p.dimension = Some(Arc::clone(&arc));
            arc
        };
        let inventory = if let Some(prev) = &p.inventory
            && **prev == g.inventory
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.inventory);
            p.inventory = Some(Arc::clone(&arc));
            arc
        };
        let armor = if let Some(prev) = &p.armor
            && **prev == g.armor
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.armor);
            p.armor = Some(Arc::clone(&arc));
            arc
        };
        let offhand = if let Some(prev) = &p.offhand
            && **prev == g.offhand
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.offhand);
            p.offhand = Some(Arc::clone(&arc));
            arc
        };
        let carried = if let Some(prev) = &p.carried
            && **prev == g.carried
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.carried);
            p.carried = Some(Arc::clone(&arc));
            arc
        };
        let ender_items = if let Some(prev) = &p.ender_items
            && **prev == g.ender_items
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.ender_items);
            p.ender_items = Some(Arc::clone(&arc));
            arc
        };
        let craft_grid = if let Some(prev) = &p.craft_grid
            && **prev == g.craft_grid
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.craft_grid);
            p.craft_grid = Some(Arc::clone(&arc));
            arc
        };
        let craft_result = if let Some(prev) = &p.craft_result
            && **prev == g.craft_result
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.craft_result);
            p.craft_result = Some(Arc::clone(&arc));
            arc
        };
        let advancements = if let Some(prev) = &p.advancements
            && prev.criteria == g.advancements.criteria
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.advancements);
            p.advancements = Some(Arc::clone(&arc));
            arc
        };
        let stats = if let Some(prev) = &p.stats
            && **prev == g.stats
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.stats);
            p.stats = Some(Arc::clone(&arc));
            arc
        };
        let post_effects = if let Some(prev) = &p.post_effects
            && **prev == g.post_effects
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.post_effects);
            p.post_effects = Some(Arc::clone(&arc));
            arc
        };
        let saved = if let Some(prev) = &p.saved
            && **prev == g.saved
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.saved);
            p.saved = Some(Arc::clone(&arc));
            arc
        };
        let attachments = if !force_full
            && let Some(prev) = &p.attachments
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.attachments);
            p.attachments = Some(Arc::clone(&arc));
            arc
        };
        let recipe_book = if !force_full
            && let Some(prev) = &p.recipe_book
            && prev.open == g.recipe_book.open
            && prev.filtering == g.recipe_book.filtering
        {
            Arc::clone(prev)
        } else {
            let arc = Arc::new(g.recipe_book);
            p.recipe_book = Some(Arc::clone(&arc));
            arc
        };
        let motion = if force_full || p.motion.is_none() {
            let arc = g.motion.map(Arc::new);
            p.motion = arc.clone();
            arc
        } else {
            p.motion.clone()
        };
        out.push(CowGuestRecord {
            uuid: g.uuid,
            dimension,
            position: g.position,
            velocity: g.velocity,
            yaw: g.yaw,
            pitch: g.pitch,
            head_yaw: g.head_yaw,
            on_ground: g.on_ground,
            ground: g.ground,
            game_mode: g.game_mode,
            previous_game_mode: g.previous_game_mode,
            flying: g.flying,
            flying_speed: g.flying_speed,
            allow_flight: g.allow_flight,
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
            respawn_elsewhere: g.respawn_elsewhere,
            last_death: g.last_death,
            health: g.health,
            absorption: g.absorption,
            food: g.food,
            saturation: g.saturation,
            exhaustion: g.exhaustion,
            food_timer: g.food_timer,
            fire_ticks: g.fire_ticks,
            air: g.air,
            ticks_frozen: g.ticks_frozen,
            fall_distance: g.fall_distance,
            xp_level: g.xp_level,
            xp_progress: g.xp_progress,
            xp_total: g.xp_total,
            xp_seed: g.xp_seed,
            inventory,
            armor,
            offhand,
            carried,
            ender_items,
            selected: g.selected,
            craft_grid,
            craft_result,
            effects: Arc::new(g.effects),
            effects_tick_count: g.effects_tick_count,
            effects_changes: g.effects_changes,
            shield_cooldown: g.shield_cooldown,
            cooldowns: Arc::new(g.cooldowns),
            recipe_book,
            advancements,
            stats,
            seen_credits: g.seen_credits,
            post_effects,
            attachments,
            saved,
            hook: g.hook,
            motion,
        });
    }
    (out, online)
}

fn empty_proj(
) -> Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_boats() -> Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::JoinList<horizon_gameplay::boats::Boat, false>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_carts(
) -> Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::JoinList<horizon_gameplay::minecarts::Minecart, false>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_falling() -> Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>> {
    static E: OnceLock<Arc<horizon_client::join_seq::JoinList<super::super::FallingBlock, false>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_others(
) -> Arc<horizon_client::join_seq::ResidentList<super::super::commands::OtherEntity>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::ResidentList<super::super::commands::OtherEntity>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_fangs() -> Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>> {
    static E: OnceLock<
        Arc<horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>>,
    > = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Default::default())))
}

fn empty_tridents() -> Arc<Vec<TridentSnapshot>> {
    static E: OnceLock<Arc<Vec<TridentSnapshot>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_rockets() -> Arc<Vec<RocketSnapshot>> {
    static E: OnceLock<Arc<Vec<RocketSnapshot>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_breath() -> Arc<Vec<BreathSnapshot>> {
    static E: OnceLock<Arc<Vec<BreathSnapshot>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_thrown_potions() -> Arc<ThrownPotionsSnapshot> {
    static E: OnceLock<Arc<ThrownPotionsSnapshot>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(ThrownPotionsSnapshot::default())))
}

fn empty_ominous() -> Arc<Vec<(u32, Tag)>> {
    static E: OnceLock<Arc<Vec<(u32, Tag)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_block_events() -> Arc<VecDeque<BlockEventData>> {
    static E: OnceLock<Arc<VecDeque<BlockEventData>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(VecDeque::new())))
}

fn empty_moving_pistons() -> Arc<Vec<(BlockPos, MovingPiston)>> {
    static E: OnceLock<Arc<Vec<(BlockPos, MovingPiston)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_piston_pushes() -> Arc<Vec<(BlockPos, MovingPiston, f32)>> {
    static E: OnceLock<Arc<Vec<(BlockPos, MovingPiston, f32)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_torch_toggles() -> Arc<Vec<(BlockPos, i64)>> {
    static E: OnceLock<Arc<Vec<(BlockPos, i64)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_comparators() -> Arc<HashMap<BlockPos, i32>> {
    static E: OnceLock<Arc<HashMap<BlockPos, i32>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_hoppers() -> Arc<HashMap<(i32, i32, i32), super::super::machines::HopperState>> {
    static E: OnceLock<Arc<HashMap<(i32, i32, i32), super::super::machines::HopperState>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_raids() -> Arc<Raids> {
    static E: OnceLock<Arc<Raids>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Raids::default())))
}

fn empty_chest_openers() -> Arc<Vec<((i32, i32, i32), (i32, f64))>> {
    static E: OnceLock<Arc<Vec<((i32, i32, i32), (i32, f64))>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_shulker_anims() -> Arc<Vec<([i32; 3], horizon_client::block_entity_anim::Shulker)>> {
    static E: OnceLock<Arc<Vec<([i32; 3], horizon_client::block_entity_anim::Shulker)>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_gateway_anims() -> Arc<Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)>> {
    static E: OnceLock<Arc<Vec<([i32; 3], horizon_client::block_entity_anim::EndGateway)>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_portals_entities() -> Arc<HashMap<u32, horizon_client::nether_portal::PortalState>> {
    static E: OnceLock<Arc<HashMap<u32, horizon_client::nether_portal::PortalState>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_lectern_pulses() -> Arc<Vec<((i32, i32, i32), i32)>> {
    static E: OnceLock<Arc<Vec<((i32, i32, i32), i32)>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_lid_openers() -> Arc<HashMap<(i32, i32, i32), i32>> {
    static E: OnceLock<Arc<HashMap<(i32, i32, i32), i32>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_passenger_slots() -> Arc<HashMap<u32, Vec<super::super::persist::PassengerSlot>>> {
    static E: OnceLock<Arc<HashMap<u32, Vec<super::super::persist::PassengerSlot>>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_hurt_by_player() -> Arc<HashMap<u32, super::super::combat::PlayerCredit>> {
    static E: OnceLock<Arc<HashMap<u32, super::super::combat::PlayerCredit>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_love_cause() -> Arc<HashMap<u32, Option<u32>>> {
    static E: OnceLock<Arc<HashMap<u32, Option<u32>>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn empty_command_clocks() -> Arc<BTreeMap<String, (i64, super::super::chat::WorldClock)>> {
    static E: OnceLock<Arc<BTreeMap<String, (i64, super::super::chat::WorldClock)>>> =
        OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(BTreeMap::new())))
}

fn empty_scheduled() -> Arc<Vec<super::super::functions::Scheduled>> {
    static E: OnceLock<Arc<Vec<super::super::functions::Scheduled>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(Vec::new())))
}

fn empty_entity_attachments() -> Arc<FxHashMap<u32, Attachments>> {
    static E: OnceLock<Arc<FxHashMap<u32, Attachments>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(FxHashMap::default())))
}

fn empty_guest_attachments() -> Arc<HashMap<u128, Attachments>> {
    static E: OnceLock<Arc<HashMap<u128, Attachments>>> = OnceLock::new();
    Arc::clone(E.get_or_init(|| Arc::new(HashMap::new())))
}

fn diff_chunks_via_touch(
    world: &horizon_client::world::World,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<((i32, i32), Option<Arc<Chunk>>)> {
    let mut changes = Vec::new();
    let touched_opt: Option<Vec<(i32, i32)>> = prev
        .chunk_touch_cursor
        .and_then(|c| world.chunks.touched_since(c).map(|it| it.copied().collect()));
    if let Some(keys) = touched_opt {
        let mut seen = HashSet::with_capacity(keys.len());
        for pos in keys {
            if !seen.insert(pos) {
                continue;
            }
            match world.chunks.get(&pos) {
                Some(live_chunk) => {
                    let same = prev.chunks.get(&pos).is_some_and(|p| Arc::ptr_eq(p, live_chunk));
                    if !same {
                        let c = Arc::clone(live_chunk);
                        prev.chunks.insert(pos, Arc::clone(&c));
                        prev.chunks_dirty_since_hdr = true;
                        if !force_full {
                            changes.push((pos, Some(c)));
                        }
                    }
                }
                None => {
                    if prev.chunks.remove(&pos).is_some() {
                        prev.chunks_dirty_since_hdr = true;
                        if !force_full {
                            changes.push((pos, None));
                        }
                    }
                }
            }
        }
    } else {
        for (&pos, live_chunk) in world.chunks.iter() {
            let same = prev.chunks.get(&pos).is_some_and(|p| Arc::ptr_eq(p, live_chunk));
            if !same {
                let c = Arc::clone(live_chunk);
                prev.chunks.insert(pos, Arc::clone(&c));
                prev.chunks_dirty_since_hdr = true;
                if !force_full {
                    changes.push((pos, Some(c)));
                }
            }
        }
        let mut removed = Vec::new();
        for &pos in prev.chunks.keys() {
            if !world.chunks.contains_key(&pos) {
                removed.push(pos);
            }
        }
        for pos in removed {
            prev.chunks.remove(&pos);
            prev.chunks_dirty_since_hdr = true;
            if !force_full {
                changes.push((pos, None));
            }
        }
    }
    prev.chunk_touch_cursor = Some(world.chunks.touch_cursor());
    changes
}

fn diff_chunk_ticks_cow(
    live_chunks: &FxHashMap<(i32, i32), Arc<Chunk>>,
    block_ticks: &Ticks,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<((i32, i32), Option<Arc<(ChunkTicks, ChunkTicks)>>)> {
    let mut changes = Vec::new();
    for &(cx, cz) in live_chunks.keys() {
        let cpos = ChunkPos::new(cx, cz);
        let b = block_ticks.blocks.chunk(cpos);
        let f = block_ticks.fluids.chunk(cpos);
        let non_empty =
            b.is_some_and(|t| t.count() > 0) || f.is_some_and(|t| t.count() > 0);
        if non_empty {
            let unchanged = prev
                .chunk_ticks
                .get(&(cx, cz))
                .is_some_and(|p| chunk_ticks_eq(b, &p.0) && chunk_ticks_eq(f, &p.1));
            if !unchanged {
                let arc = Arc::new((
                    b.cloned().unwrap_or_default(),
                    f.cloned().unwrap_or_default(),
                ));
                prev.chunk_ticks.insert((cx, cz), Arc::clone(&arc));
                prev.ticks_dirty_since_hdr = true;
                if !force_full {
                    changes.push(((cx, cz), Some(arc)));
                }
            }
        } else if prev.chunk_ticks.remove(&(cx, cz)).is_some() {
            prev.ticks_dirty_since_hdr = true;
            if !force_full {
                changes.push(((cx, cz), None));
            }
        }
    }
    let mut unloaded_keys = Vec::new();
    for &k in prev.chunk_ticks.keys() {
        if !live_chunks.contains_key(&k) {
            unloaded_keys.push(k);
        }
    }
    for k in unloaded_keys {
        prev.chunk_ticks.remove(&k);
        prev.ticks_dirty_since_hdr = true;
        if !force_full {
            changes.push((k, None));
        }
    }
    changes
}

fn diff_mobs_cow(
    live_mobs: &horizon_client::join_seq::JoinList<super::super::Mob>,
    prev_mobs: &mut FxHashMap<u32, Arc<super::super::Mob>>,
    prev_order: &mut Vec<u32>,
    force_full: bool,
) -> (Vec<(u32, Option<Arc<super::super::Mob>>)>, Option<Arc<Vec<u32>>>) {
    let mut changes = Vec::new();
    let mut new_order = Vec::with_capacity(live_mobs.len());
    let mut seen = HashSet::with_capacity(live_mobs.len());
    for m in live_mobs.iter() {
        seen.insert(m.id);
        new_order.push(m.id);
        let unchanged = prev_mobs.get(&m.id).is_some_and(|p| mob_unchanged(m, p));
        if !unchanged {
            let arc = Arc::new(m.clone());
            prev_mobs.insert(m.id, Arc::clone(&arc));
            if !force_full {
                changes.push((m.id, Some(arc)));
            }
        }
    }
    let mut removed_ids = Vec::new();
    for &id in prev_mobs.keys() {
        if !seen.contains(&id) {
            removed_ids.push(id);
        }
    }
    for id in removed_ids {
        prev_mobs.remove(&id);
        if !force_full {
            changes.push((id, None));
        }
    }
    let order_changed = if !force_full && *prev_order != new_order {
        *prev_order = new_order;
        Some(Arc::new(prev_order.clone()))
    } else {
        if *prev_order != new_order {
            *prev_order = new_order;
        }
        None
    };
    (changes, order_changed)
}

fn diff_block_entities_native_cow(
    game: &Game<'_>,
    world: &horizon_client::world::World,
    hoppers: &HashMap<(i32, i32, i32), super::super::machines::HopperState>,
    lid_openers: &HashMap<(i32, i32, i32), i32>,
    chunk_changes: &[((i32, i32), Option<Arc<Chunk>>)],
    player_chunks: &[(i32, i32)],
    any_menu_open: bool,
    is_active_dim: bool,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<((i32, i32, i32), Option<Arc<BlockEntity>>)> {
    let mut changes = Vec::new();

    if prev.block_entities.is_empty() && !world.block_entities.is_empty() {
        for (&pos, be) in world.block_entities.iter() {
            let arc = Arc::new(be.clone());
            prev.block_entities.insert(pos, Arc::clone(&arc));
            prev.be_dirty_since_hdr = true;
            if !force_full {
                changes.push((pos, Some(arc)));
            }
        }
        return changes;
    }

    let mut candidates: HashSet<(i32, i32, i32)> = HashSet::new();

    if prev.block_entities.len() != world.block_entities.len() || !chunk_changes.is_empty() {
        let mut removed_pos = Vec::new();
        for &pos in prev.block_entities.keys() {
            if !(*world.block_entities).contains_key(&pos) {
                removed_pos.push(pos);
            }
        }
        for pos in removed_pos {
            prev.block_entities.remove(&pos);
            prev.be_encoded_tags.remove(&pos);
            prev.be_dirty_since_hdr = true;
            if !force_full {
                changes.push((pos, None));
            }
        }
        for &(cpos, _) in chunk_changes {
            for &pos in world.block_entities.in_chunk(cpos) {
                candidates.insert(pos);
            }
        }
        if prev.block_entities.len() != world.block_entities.len() {
            for &pos in world.block_entities.keys() {
                if !prev.block_entities.contains_key(&pos) {
                    candidates.insert(pos);
                }
            }
        }
    }

    for &cpos in world.modified.iter() {
        for &pos in world.block_entities.in_chunk(cpos) {
            candidates.insert(pos);
        }
    }
    for &pos in hoppers.keys() {
        candidates.insert(pos);
    }
    for &pos in lid_openers.keys() {
        candidates.insert(pos);
    }
    for &pos in world.chest_openers.keys() {
        candidates.insert(pos);
    }
    if any_menu_open {
        for &(px, pz) in player_chunks {
            for dz in -2..=2 {
                for dx in -2..=2 {
                    for &pos in world.block_entities.in_chunk((px + dx, pz + dz)) {
                        candidates.insert(pos);
                    }
                }
            }
        }
    }

    let tickers = world.block_entities.ticker_slots();
    for &(pos, _) in tickers.iter() {
        if world.block_entities.is_asleep(&pos) {
            continue;
        }
        if is_active_dim && !game.block_ticking_at(pos.0, pos.2) {
            continue;
        }
        candidates.insert(pos);
    }

    let facts = game.res.facts();
    for pos in candidates {
        let Some(be) = world.block_entities.get(&pos) else {
            if prev.block_entities.remove(&pos).is_some() {
                prev.be_encoded_tags.remove(&pos);
                prev.be_dirty_since_hdr = true;
                if !force_full {
                    changes.push((pos, None));
                }
            }
            continue;
        };
        let is_new = !prev.block_entities.contains_key(&pos);
        let block_name = &facts.get(world.block(pos.0, pos.1, pos.2)).name;
        let encoded = block_entity_encode::encode(pos, be, block_name.as_ref());
        let unchanged = !is_new
            && prev
                .be_encoded_tags
                .get(&pos)
                .is_some_and(|old_tag| *old_tag == encoded);
        if !unchanged {
            prev.be_encoded_tags.insert(pos, encoded);
            let arc = Arc::new(be.clone());
            prev.block_entities.insert(pos, Arc::clone(&arc));
            prev.be_dirty_since_hdr = true;
            if !force_full {
                changes.push((pos, Some(arc)));
            }
        }
    }

    changes
}

fn diff_persist_raw_cow(
    live: &HashMap<u32, Tag>,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<(u32, Option<Arc<Tag>>)> {
    let mut changes = Vec::new();
    let mut removed = Vec::new();
    for &id in prev.persist_raw.keys() {
        if !live.contains_key(&id) {
            removed.push(id);
        }
    }
    for id in removed {
        prev.persist_raw.remove(&id);
        prev.persist_raw_dirty = true;
        if !force_full {
            changes.push((id, None));
        }
    }
    for (&id, tag) in live.iter() {
        let same = prev.persist_raw.get(&id).is_some_and(|p| **p == *tag);
        if !same {
            let arc = Arc::new(tag.clone());
            prev.persist_raw.insert(id, Arc::clone(&arc));
            prev.persist_raw_dirty = true;
            if !force_full {
                changes.push((id, Some(arc)));
            }
        }
    }
    changes
}

fn diff_persist_uuids_cow(
    live: &HashMap<u32, [i32; 4]>,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<(u32, Option<[i32; 4]>)> {
    let mut changes = Vec::new();
    let mut removed = Vec::new();
    for &id in prev.persist_uuids.keys() {
        if !live.contains_key(&id) {
            removed.push(id);
        }
    }
    for id in removed {
        prev.persist_uuids.remove(&id);
        prev.persist_uuids_dirty = true;
        if !force_full {
            changes.push((id, None));
        }
    }
    for (&id, &u) in live.iter() {
        if prev.persist_uuids.get(&id).copied() != Some(u) {
            prev.persist_uuids.insert(id, u);
            prev.persist_uuids_dirty = true;
            if !force_full {
                changes.push((id, Some(u)));
            }
        }
    }
    changes
}

fn diff_inhabited_cow(
    live: &FxHashMap<(i32, i32), i64>,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> Vec<((i32, i32), Option<i64>)> {
    let mut changes = Vec::new();
    let mut removed = Vec::new();
    for &k in prev.inhabited.keys() {
        if !live.contains_key(&k) {
            removed.push(k);
        }
    }
    for k in removed {
        prev.inhabited.remove(&k);
        prev.inhabited_dirty_since_hdr = true;
        if !force_full {
            changes.push((k, None));
        }
    }
    for (&k, &v) in live.iter() {
        if prev.inhabited.get(&k).copied() != Some(v) {
            prev.inhabited.insert(k, v);
            prev.inhabited_dirty_since_hdr = true;
            if !force_full {
                changes.push((k, Some(v)));
            }
        }
    }
    changes
}

fn collect_dim_player_chunks(game: &Game<'_>, dim: Dim) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let dim_id = dim.id();
    if let Some(host) = &game.lan {
        for g in host.hub.guests.iter() {
            if g.placed && g.dimension == dim_id {
                out.push((
                    g.position[0].floor() as i32 >> 4,
                    g.position[2].floor() as i32 >> 4,
                ));
            }
        }
    }
    if out.is_empty()
        && game.dim == dim
        && let Some(p) = &game.player
    {
        out.push((
            p.position.x.floor() as i32 >> 4,
            p.position.z.floor() as i32 >> 4,
        ));
    }
    out
}

pub fn capture_cow_level_direct(
    game: &Game<'_>,
    dim: Dim,
    world: &horizon_client::world::World,
    prev: &mut CowLevelPrev,
    any_menu_open: bool,
    force_full: bool,
) -> CowLevelRecord {
    let m_chunks_0 = StatmMem::current();
    let live_chunks = &*world.chunks;
    let chunk_changes = diff_chunks_via_touch(world, prev, force_full);
    let m_chunks_1 = StatmMem::current();
    record_mem_cow_chunks(
        game.world.game_time,
        m_chunks_0,
        m_chunks_1,
        live_chunks.len(),
        chunk_changes.len(),
    );

    let player_chunks = collect_dim_player_chunks(game, dim);

    let m_be_0 = StatmMem::current();
    let be_changes = diff_block_entities_native_cow(
        game,
        world,
        &game.hoppers,
        &game.lid_openers,
        &chunk_changes,
        &player_chunks,
        any_menu_open,
        true,
        prev,
        force_full,
    );
    let m_be_1 = StatmMem::current();
    record_mem_cow_be(
        game.world.game_time,
        m_be_0,
        m_be_1,
        be_changes.len(),
        !be_changes.is_empty(),
    );

    let block_names = if !force_full
        && prev.world_revision == world.revision
        && let Some(p) = &prev.block_names
        && p.len() == world.block_names.len()
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.block_names.clone());
        prev.block_names = Some(Arc::clone(&arc));
        arc
    };

    let be_attachments = if !force_full
        && chunk_changes.is_empty()
        && be_changes.is_empty()
        && let Some(p) = &prev.be_attachments
    {
        Arc::clone(p)
    } else {
        let mut atts = Vec::new();
        for &(cx, cz) in live_chunks.keys() {
            atts.extend(world.block_entities.chunk_attachments(cx, cz));
        }
        let arc = Arc::new(atts);
        prev.be_attachments = Some(Arc::clone(&arc));
        arc
    };

    let world_attachments = if !force_full
        && prev.world_revision == world.revision
        && let Some(p) = &prev.world_attachments
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.attachments.clone());
        prev.world_attachments = Some(Arc::clone(&arc));
        arc
    };
    prev.world_revision = world.revision;

    let b_tag = game.border.pack();
    let border = if !force_full
        && prev.border_tag.as_ref() == Some(&b_tag)
        && let Some(p) = &prev.border
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(game.border.clone());
        prev.border_tag = Some(b_tag);
        prev.border = Some(Arc::clone(&arc));
        arc
    };

    let inhabited_changes = diff_inhabited_cow(&world.inhabited, prev, force_full);

    let forced_chunks = if !force_full
        && let Some(p) = &prev.forced_chunks
        && **p == game.streaming.forced
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(game.streaming.forced.clone());
        prev.forced_chunks = Some(Arc::clone(&arc));
        arc
    };

    macro_rules! cow_stamped {
        ($field:ident, $stamp_field:ident, $live:expr) => {{
            let st = $live.stamp();
            if !force_full && prev.$stamp_field == Some(st) && prev.$field.is_some() {
                Arc::clone(prev.$field.as_ref().unwrap())
            } else {
                let arc = Arc::new($live.clone());
                prev.$stamp_field = Some(st);
                prev.$field = Some(Arc::clone(&arc));
                arc
            }
        }};
    }

    macro_rules! cow_stamped_or_empty {
        ($field:ident, $stamp_field:ident, $live:expr, $empty_fn:expr) => {{
            if $live.is_empty() {
                prev.$stamp_field = Some($live.stamp());
                prev.$field = None;
                $empty_fn()
            } else {
                let st = $live.stamp();
                if !force_full && prev.$stamp_field == Some(st) && prev.$field.is_some() {
                    Arc::clone(prev.$field.as_ref().unwrap())
                } else {
                    let arc = Arc::new($live.clone());
                    prev.$stamp_field = Some(st);
                    prev.$field = Some(Arc::clone(&arc));
                    arc
                }
            }
        }};
    }

    let m_mobs_0 = StatmMem::current();
    let (mob_changes, mob_order_changed) =
        diff_mobs_cow(&game.mobs, &mut prev.mobs, &mut prev.mob_order, force_full);
    let m_mobs_1 = StatmMem::current();
    record_mem_cow_mobs(game.world.game_time, m_mobs_0, m_mobs_1, mob_changes.len());

    let dropped = cow_stamped!(dropped, dropped_stamp, game.dropped);
    let orbs = cow_stamped!(orbs, orbs_stamp, game.orbs);
    let tnts = cow_stamped!(tnts, tnts_stamp, game.tnts);
    let paintings = cow_stamped!(paintings, paintings_stamp, game.paintings);
    let item_frames = cow_stamped!(item_frames, frames_stamp, game.item_frames);
    let armor_stands = cow_stamped!(armor_stands, stands_stamp, game.armor_stands);
    let cushions = cow_stamped!(cushions, cushions_stamp, game.cushions);
    let crystals = cow_stamped!(crystals, crystals_stamp, game.crystals);

    let projectiles = cow_stamped_or_empty!(
        projectiles,
        proj_stamp,
        game.creatures.projectiles,
        empty_proj
    );
    let boats = cow_stamped_or_empty!(boats, boats_stamp, game.boats, empty_boats);
    let carts = cow_stamped_or_empty!(carts, carts_stamp, game.carts, empty_carts);
    let falling = cow_stamped_or_empty!(falling, falling_stamp, game.falling, empty_falling);

    let others = if game.others.is_empty() {
        empty_others()
    } else {
        Arc::new(game.others.clone())
    };
    let fangs = if game.fangs.list.is_empty() {
        empty_fangs()
    } else {
        Arc::new(game.fangs.list.clone())
    };

    let tridents = if game.tridents.is_empty() {
        empty_tridents()
    } else {
        let registry = game.pack.assets.registry;
        Arc::new(
            game.tridents
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
                .collect(),
        )
    };

    let rockets = if game.rockets.is_empty() {
        empty_rockets()
    } else {
        Arc::new(
            game.rockets
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
                .collect(),
        )
    };

    let breath_clouds = if game.breath_clouds.is_empty() {
        empty_breath()
    } else {
        Arc::new(
            game.breath_clouds
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
                .collect(),
        )
    };

    let thrown_potions = if game.potions.thrown.is_empty() {
        empty_thrown_potions()
    } else {
        Arc::new(game.potions.rreturn_capture_thrown())
    };

    let ominous_spawners = if game.ominous_spawners.is_empty() {
        empty_ominous()
    } else {
        Arc::new(
            game.ominous_spawners
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
        )
    };

    let m_ticks_0 = StatmMem::current();
    let chunk_tick_changes = diff_chunk_ticks_cow(live_chunks, &game.block_ticks, prev, force_full);
    let m_ticks_1 = StatmMem::current();
    record_mem_cow_ticks(
        game.world.game_time,
        m_ticks_0,
        m_ticks_1,
        chunk_tick_changes.len(),
    );

    let block_events = if game.block_ticks.block_events.is_empty() {
        empty_block_events()
    } else {
        Arc::new(game.block_ticks.block_events.clone())
    };
    let moving_pistons = if game.block_ticks.moving_pistons.is_empty() {
        empty_moving_pistons()
    } else {
        Arc::new(game.block_ticks.moving_pistons.clone())
    };
    let piston_pushes = if game.block_ticks.piston_pushes.is_empty() {
        empty_piston_pushes()
    } else {
        Arc::new(game.block_ticks.piston_pushes.clone())
    };
    let torch_toggles = if game.block_ticks.torch_toggles.is_empty() {
        empty_torch_toggles()
    } else {
        Arc::new(game.block_ticks.torch_toggles.clone())
    };
    let comparator_outputs = if game.block_ticks.comparator_outputs.is_empty() {
        empty_comparators()
    } else {
        Arc::new(game.block_ticks.comparator_outputs.clone())
    };
    let hoppers = if game.hoppers.is_empty() {
        empty_hoppers()
    } else {
        Arc::new(game.hoppers.clone())
    };

    let knot_ids = if !force_full
        && let Some(p) = &prev.knot_ids
        && **p == game.knot_ids
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(game.knot_ids.clone());
        prev.knot_ids = Some(Arc::clone(&arc));
        arc
    };

    let m_raw_0 = StatmMem::current();
    let persist_uuid_changes = diff_persist_uuids_cow(&game.persist.uuids, prev, force_full);
    let persist_raw_changes = diff_persist_raw_cow(&game.persist.raw, prev, force_full);
    let m_raw_1 = StatmMem::current();
    record_mem_cow_persist(
        game.world.game_time,
        m_raw_0,
        m_raw_1,
        persist_raw_changes.len(),
    );

    let raids_mem = if game.raiding.raids.map.is_empty() {
        Some(empty_raids())
    } else {
        let arc = Arc::new(game.raiding.raids.clone());
        prev.raids_mem = Some(Arc::clone(&arc));
        Some(arc)
    };

    let chest_openers = if world.chest_openers.is_empty() {
        empty_chest_openers()
    } else {
        Arc::new(world.chest_openers.iter().map(|(&k, &v)| (k, v)).collect())
    };
    let shulker_anims = if game.be_anims.shulkers.is_empty() {
        empty_shulker_anims()
    } else {
        Arc::new(
            game.be_anims
                .shulkers
                .iter()
                .map(|(&k, v)| (k, v.clone()))
                .collect(),
        )
    };
    let gateway_anims = if game.be_anims.gateway_servers.is_empty() {
        empty_gateway_anims()
    } else {
        Arc::new(
            game.be_anims
                .gateway_servers
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect(),
        )
    };
    let portals_entities = if game.portals.entities.is_empty() {
        empty_portals_entities()
    } else {
        Arc::new(game.portals.entities.clone())
    };
    let lectern_pulses = if game.lectern_pulses.is_empty() {
        empty_lectern_pulses()
    } else {
        Arc::new(game.lectern_pulses.clone())
    };
    let lid_openers = if game.lid_openers.is_empty() {
        empty_lid_openers()
    } else {
        Arc::new(game.lid_openers.clone())
    };
    let persist_passenger_slots = if game.persist.passenger_slots.is_empty() {
        empty_passenger_slots()
    } else {
        Arc::new(game.persist.passenger_slots.clone())
    };
    let last_hurt_by_player = if game.last_hurt_by_player.is_empty() {
        empty_hurt_by_player()
    } else {
        Arc::new(game.last_hurt_by_player.clone())
    };
    let love_cause = if game.love_cause.is_empty() {
        empty_love_cause()
    } else {
        Arc::new(game.love_cause.clone())
    };

    CowLevelRecord {
        chunk_changes,
        be_changes,
        block_names,
        be_attachments,
        world_attachments,
        chest_openers,
        border,
        shulker_anims,
        gateway_anims,
        fangs,
        ominous_spawners,
        portals_entities,
        lectern_pulses,
        lid_openers,
        inhabited_changes,
        end_clock: world.end_clock,
        random: world.random.clone(),
        forced_chunks,
        chunk_tick_changes,
        random_tick_value: game.block_ticks.random_tick_value,
        comparator_outputs,
        torch_toggles,
        moving_pistons,
        piston_pushes,
        block_events,
        hoppers,
        mob_changes,
        mob_order_changed,
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
        dragon: game
            .dragon
            .as_ref()
            .map(DragonSnapshot::from_live)
            .map(Arc::new),
        tridents,
        rockets,
        breath_clouds,
        thrown_potions,
        knot_ids,
        persist_uuid_changes,
        persist_raw_changes,
        persist_passenger_slots,
        raids_mem,
        last_hurt_by_player,
        love_cause,
    }
}

pub fn capture_cow_parked_direct(
    game: &Game<'_>,
    dim: Dim,
    parked: &crate::game::levels::Parked,
    prev: &mut CowLevelPrev,
    force_full: bool,
) -> CowLevelRecord {
    let world = &parked.world;
    let live_chunks = &*world.chunks;
    let chunk_changes = diff_chunks_via_touch(world, prev, force_full);

    let player_chunks = collect_dim_player_chunks(game, dim);
    let be_changes = diff_block_entities_native_cow(
        game,
        world,
        &parked.hoppers,
        &parked.lid_openers,
        &chunk_changes,
        &player_chunks,
        false,
        false,
        prev,
        force_full,
    );

    let block_names = if !force_full
        && prev.world_revision == world.revision
        && let Some(p) = &prev.block_names
        && p.len() == world.block_names.len()
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.block_names.clone());
        prev.block_names = Some(Arc::clone(&arc));
        arc
    };

    let be_attachments = if !force_full
        && chunk_changes.is_empty()
        && be_changes.is_empty()
        && let Some(p) = &prev.be_attachments
    {
        Arc::clone(p)
    } else {
        let mut atts = Vec::new();
        for &(cx, cz) in live_chunks.keys() {
            atts.extend(world.block_entities.chunk_attachments(cx, cz));
        }
        let arc = Arc::new(atts);
        prev.be_attachments = Some(Arc::clone(&arc));
        arc
    };

    let world_attachments = if !force_full
        && prev.world_revision == world.revision
        && let Some(p) = &prev.world_attachments
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(world.attachments.clone());
        prev.world_attachments = Some(Arc::clone(&arc));
        arc
    };
    prev.world_revision = world.revision;

    let b_tag = parked.border.pack();
    let border = if !force_full
        && prev.border_tag.as_ref() == Some(&b_tag)
        && let Some(p) = &prev.border
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(parked.border.clone());
        prev.border_tag = Some(b_tag);
        prev.border = Some(Arc::clone(&arc));
        arc
    };

    let inhabited_changes = diff_inhabited_cow(&world.inhabited, prev, force_full);

    let forced_chunks = if !force_full
        && let Some(p) = &prev.forced_chunks
        && **p == parked.streaming.forced
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(parked.streaming.forced.clone());
        prev.forced_chunks = Some(Arc::clone(&arc));
        arc
    };

    macro_rules! cow_stamped {
        ($field:ident, $stamp_field:ident, $live:expr) => {{
            let st = $live.stamp();
            if !force_full && prev.$stamp_field == Some(st) && prev.$field.is_some() {
                Arc::clone(prev.$field.as_ref().unwrap())
            } else {
                let arc = Arc::new($live.clone());
                prev.$stamp_field = Some(st);
                prev.$field = Some(Arc::clone(&arc));
                arc
            }
        }};
    }

    macro_rules! cow_stamped_or_empty {
        ($field:ident, $stamp_field:ident, $live:expr, $empty_fn:expr) => {{
            if $live.is_empty() {
                prev.$stamp_field = Some($live.stamp());
                prev.$field = None;
                $empty_fn()
            } else {
                let st = $live.stamp();
                if !force_full && prev.$stamp_field == Some(st) && prev.$field.is_some() {
                    Arc::clone(prev.$field.as_ref().unwrap())
                } else {
                    let arc = Arc::new($live.clone());
                    prev.$stamp_field = Some(st);
                    prev.$field = Some(Arc::clone(&arc));
                    arc
                }
            }
        }};
    }

    let (mob_changes, mob_order_changed) =
        diff_mobs_cow(&parked.mobs, &mut prev.mobs, &mut prev.mob_order, force_full);
    let dropped = cow_stamped!(dropped, dropped_stamp, parked.dropped);
    let orbs = cow_stamped!(orbs, orbs_stamp, parked.orbs);
    let tnts = cow_stamped!(tnts, tnts_stamp, parked.tnts);
    let paintings = cow_stamped!(paintings, paintings_stamp, parked.paintings);
    let item_frames = cow_stamped!(item_frames, frames_stamp, parked.item_frames);
    let armor_stands = cow_stamped!(armor_stands, stands_stamp, parked.armor_stands);
    let cushions = cow_stamped!(cushions, cushions_stamp, parked.cushions);
    let crystals = cow_stamped!(crystals, crystals_stamp, parked.crystals);

    let projectiles = cow_stamped_or_empty!(
        projectiles,
        proj_stamp,
        parked.projectiles,
        empty_proj
    );
    let boats = cow_stamped_or_empty!(boats, boats_stamp, parked.boats, empty_boats);
    let carts = cow_stamped_or_empty!(carts, carts_stamp, parked.carts, empty_carts);
    let falling = cow_stamped_or_empty!(falling, falling_stamp, parked.falling, empty_falling);

    let others = if parked.others.is_empty() {
        empty_others()
    } else {
        Arc::new(parked.others.clone())
    };
    let fangs = if parked.fangs.list.is_empty() {
        empty_fangs()
    } else {
        Arc::new(parked.fangs.list.clone())
    };

    let tridents = if parked.tridents.is_empty() {
        empty_tridents()
    } else {
        let registry = game.pack.assets.registry;
        Arc::new(
            parked
                .tridents
                .iter()
                .map(|t| {
                    let uuid = parked.persist.uuids.get(&t.id).copied().unwrap_or([0; 4]);
                    let raw = parked.persist.raw.get(&t.id);
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
                .collect(),
        )
    };
    let rockets = if parked.rockets.is_empty() {
        empty_rockets()
    } else {
        Arc::new(
            parked
                .rockets
                .iter()
                .map(|r| {
                    let uuid = parked.persist.uuids.get(&r.id).copied().unwrap_or([0; 4]);
                    let raw = parked.persist.raw.get(&r.id);
                    RocketSnapshot {
                        id: r.id,
                        nbt: r.to_nbt(uuid, None, raw),
                        attached: r.attached,
                        guest: r.guest,
                        owner: r.owner,
                        attached_to: r.attached_to,
                    }
                })
                .collect(),
        )
    };
    let breath_clouds = if parked.breath_clouds.is_empty() {
        empty_breath()
    } else {
        Arc::new(
            parked
                .breath_clouds
                .iter()
                .map(|c| {
                    let uuid = parked.persist.uuids.get(&c.id).copied().unwrap_or([0; 4]);
                    let raw = parked.persist.raw.get(&c.id);
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
                .collect(),
        )
    };
    let thrown_potions = empty_thrown_potions();
    let ominous_spawners = if parked.ominous_spawners.is_empty() {
        empty_ominous()
    } else {
        Arc::new(
            parked
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
        )
    };

    let chunk_tick_changes = diff_chunk_ticks_cow(live_chunks, &parked.block_ticks, prev, force_full);

    let block_events = if parked.block_ticks.block_events.is_empty() {
        empty_block_events()
    } else {
        Arc::new(parked.block_ticks.block_events.clone())
    };
    let moving_pistons = if parked.block_ticks.moving_pistons.is_empty() {
        empty_moving_pistons()
    } else {
        Arc::new(parked.block_ticks.moving_pistons.clone())
    };
    let piston_pushes = if parked.block_ticks.piston_pushes.is_empty() {
        empty_piston_pushes()
    } else {
        Arc::new(parked.block_ticks.piston_pushes.clone())
    };
    let torch_toggles = if parked.block_ticks.torch_toggles.is_empty() {
        empty_torch_toggles()
    } else {
        Arc::new(parked.block_ticks.torch_toggles.clone())
    };
    let comparator_outputs = if parked.block_ticks.comparator_outputs.is_empty() {
        empty_comparators()
    } else {
        Arc::new(parked.block_ticks.comparator_outputs.clone())
    };
    let hoppers = if parked.hoppers.is_empty() {
        empty_hoppers()
    } else {
        Arc::new(parked.hoppers.clone())
    };

    let knot_ids = if !force_full
        && let Some(p) = &prev.knot_ids
        && **p == parked.knot_ids
    {
        Arc::clone(p)
    } else {
        let arc = Arc::new(parked.knot_ids.clone());
        prev.knot_ids = Some(Arc::clone(&arc));
        arc
    };

    let persist_uuid_changes = diff_persist_uuids_cow(&parked.persist.uuids, prev, force_full);
    let persist_raw_changes = diff_persist_raw_cow(&parked.persist.raw, prev, force_full);

    let raids_mem = if parked.raiding.raids.map.is_empty() {
        Some(empty_raids())
    } else {
        let arc = Arc::new(parked.raiding.raids.clone());
        prev.raids_mem = Some(Arc::clone(&arc));
        Some(arc)
    };

    let chest_openers = if world.chest_openers.is_empty() {
        empty_chest_openers()
    } else {
        Arc::new(world.chest_openers.iter().map(|(&k, &v)| (k, v)).collect())
    };
    let shulker_anims = if parked.anims.shulkers.is_empty() {
        empty_shulker_anims()
    } else {
        Arc::new(
            parked
                .anims
                .shulkers
                .iter()
                .map(|(&k, v)| (k, v.clone()))
                .collect(),
        )
    };
    let gateway_anims = if parked.anims.gateway_servers.is_empty() {
        empty_gateway_anims()
    } else {
        Arc::new(
            parked
                .anims
                .gateway_servers
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect(),
        )
    };
    let portals_entities = if parked.portal_states.is_empty() {
        empty_portals_entities()
    } else {
        Arc::new(parked.portal_states.clone())
    };
    let lectern_pulses = if parked.lectern_pulses.is_empty() {
        empty_lectern_pulses()
    } else {
        Arc::new(parked.lectern_pulses.clone())
    };
    let lid_openers = if parked.lid_openers.is_empty() {
        empty_lid_openers()
    } else {
        Arc::new(parked.lid_openers.clone())
    };
    let persist_passenger_slots = if parked.persist.passenger_slots.is_empty() {
        empty_passenger_slots()
    } else {
        Arc::new(parked.persist.passenger_slots.clone())
    };

    CowLevelRecord {
        chunk_changes,
        be_changes,
        block_names,
        be_attachments,
        world_attachments,
        chest_openers,
        border,
        shulker_anims,
        gateway_anims,
        fangs,
        ominous_spawners,
        portals_entities,
        lectern_pulses,
        lid_openers,
        inhabited_changes,
        end_clock: world.end_clock,
        random: world.random.clone(),
        forced_chunks,
        chunk_tick_changes,
        random_tick_value: parked.block_ticks.random_tick_value,
        comparator_outputs,
        torch_toggles,
        moving_pistons,
        piston_pushes,
        block_events,
        hoppers,
        mob_changes,
        mob_order_changed,
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
        dragon: parked
            .dragon
            .as_ref()
            .map(DragonSnapshot::from_live)
            .map(Arc::new),
        tridents,
        rockets,
        breath_clouds,
        thrown_potions,
        knot_ids,
        persist_uuid_changes,
        persist_raw_changes,
        persist_passenger_slots,
        raids_mem,
        last_hurt_by_player: empty_hurt_by_player(),
        love_cause: empty_love_cause(),
    }
}

pub fn snapshot_level_headers(
    prev: &mut CowLevelPrev,
) -> (
    Arc<FxHashMap<(i32, i32), Arc<Chunk>>>,
    Arc<HashMap<(i32, i32), Arc<(ChunkTicks, ChunkTicks)>>>,
    Arc<HashMap<(i32, i32, i32), Arc<BlockEntity>>>,
    Vec<Arc<super::super::Mob>>,
    Arc<HashMap<u32, Arc<Tag>>>,
    Arc<HashMap<u32, [i32; 4]>>,
    Arc<FxHashMap<(i32, i32), i64>>,
) {
    let h_chunks = if !prev.chunks_dirty_since_hdr
        && let Some(a) = &prev.chunks_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.chunks.clone());
        prev.chunks_hdr_arc = Some(Arc::clone(&a));
        prev.chunks_dirty_since_hdr = false;
        a
    };
    let h_ticks = if !prev.ticks_dirty_since_hdr
        && let Some(a) = &prev.ticks_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.chunk_ticks.clone());
        prev.ticks_hdr_arc = Some(Arc::clone(&a));
        prev.ticks_dirty_since_hdr = false;
        a
    };
    let h_be = if !prev.be_dirty_since_hdr
        && let Some(a) = &prev.be_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.block_entities.clone());
        prev.be_hdr_arc = Some(Arc::clone(&a));
        prev.be_dirty_since_hdr = false;
        a
    };
    let h_mobs: Vec<Arc<super::super::Mob>> = prev
        .mob_order
        .iter()
        .filter_map(|id| prev.mobs.get(id).map(Arc::clone))
        .collect();
    let h_raw = if !prev.persist_raw_dirty
        && let Some(a) = &prev.persist_raw_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.persist_raw.clone());
        prev.persist_raw_hdr_arc = Some(Arc::clone(&a));
        prev.persist_raw_dirty = false;
        a
    };
    let h_uuids = if !prev.persist_uuids_dirty
        && let Some(a) = &prev.persist_uuids_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.persist_uuids.clone());
        prev.persist_uuids_hdr_arc = Some(Arc::clone(&a));
        prev.persist_uuids_dirty = false;
        a
    };
    let h_inh = if !prev.inhabited_dirty_since_hdr
        && let Some(a) = &prev.inhabited_hdr_arc
    {
        Arc::clone(a)
    } else {
        let a = Arc::new(prev.inhabited.clone());
        prev.inhabited_hdr_arc = Some(Arc::clone(&a));
        prev.inhabited_dirty_since_hdr = false;
        a
    };
    (h_chunks, h_ticks, h_be, h_mobs, h_raw, h_uuids, h_inh)
}
