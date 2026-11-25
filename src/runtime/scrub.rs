use super::super::{
    Dim, ExperienceOrb, FallingBlock, Game, ItemEntity, Mob,
    commands::OtherEntity,
    dragon::Crystal,
};
use super::policy::RollbackPolicy;
use super::restore::apply_timeline_snapshot_on_server;
use super::restore_client::apply_client_level_rollback;
use super::scrub_hud::*;
use super::scrub_interpolate::{
    apply_block_rewind_to_game, apply_interpolated_poses_to_game, apply_timeline_sounds_to_game,
    reset_scrub_ui_preview_cache,
};
use super::scrub_perf::*;
use super::scrub_server::{
    materialize_server_scrub, server_scrub_time_range, truncate_server_scrub_after,
};
use super::scrub_timeline::{with_scrub_ring, with_scrub_ring_mut};
use super::snapshots::{BreathSnapshot, DragonSnapshot, RocketSnapshot, TridentSnapshot};
use super::types::*;
use super::ui_snapshot::*;
use horizon_client::{
    block_entity_anim::BlockEntityAnims,
    boss_overlay::BossOverlay,
    chunk::Chunk,
    containers::BlockEntity,
    entities::{ArmorStand, Cushion, ItemFrame, Painting},
    fast_hash::FxHashMap,
    items::ItemStack,
    world::Weather,
    worldborder::WorldBorder,
};
use horizon_gameplay::{
    boats::Boat,
    minecarts::Minecart,
    tnt::PrimedTnt,
};
use horizon_protocol::packets::play::{self, Clientbound as C};
use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub struct AnchorClientState {
    pub dim: Dim,
    pub chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
    pub day_time: i64,
    pub weather: Weather,
    pub border: WorldBorder,
    pub boss_overlay: BossOverlay,
    pub sculk_darkness: (f32, f32),
    pub flash: u32,
    pub be_anims: BlockEntityAnims,
    pub moving_pistons: Vec<([i32; 3], horizon_world::piston_shape::MovingPistonState)>,
    pub mobs: horizon_client::join_seq::JoinList<Mob>,
    pub dropped: horizon_client::join_seq::JoinList<ItemEntity>,
    pub orbs: horizon_client::join_seq::JoinList<ExperienceOrb>,
    pub tnts: horizon_client::join_seq::JoinList<PrimedTnt>,
    pub falling: horizon_client::join_seq::JoinList<FallingBlock, false>,
    pub boats: horizon_client::join_seq::JoinList<Boat, false>,
    pub carts: horizon_client::join_seq::JoinList<Minecart, false>,
    pub projectiles:
        horizon_client::join_seq::JoinList<horizon_client::projectiles::Projectile, false>,
    pub crystals: horizon_client::join_seq::JoinList<Crystal>,
    pub paintings: horizon_client::join_seq::JoinList<Painting>,
    pub item_frames: horizon_client::join_seq::JoinList<ItemFrame>,
    pub armor_stands: horizon_client::join_seq::JoinList<ArmorStand>,
    pub cushions: horizon_client::join_seq::JoinList<Cushion>,
    pub others: horizon_client::join_seq::ResidentList<OtherEntity>,
    pub dragon: Option<DragonSnapshot>,
    pub tridents: Vec<TridentSnapshot>,
    pub rockets: Vec<RocketSnapshot>,
    pub breath_clouds: Vec<BreathSnapshot>,
    pub fangs: horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>,
    pub thrown_potions: ThrownPotionsSnapshot,
    pub particles: Vec<horizon_client::particles::Particle>,
    pub elder_guardians: Vec<i32>,
    pub player: Option<horizon_client::motion::PlayerMotion>,
    pub yaw: f32,
    pub pitch: f32,
    pub selected: usize,
    pub inventory: Vec<Option<ItemStack>>,
    pub armor: [Option<ItemStack>; 4],
    pub offhand: Option<ItemStack>,
    pub using: Option<(usize, u32)>,
    pub use_duration: i32,
    pub fov_modifier: f32,
    pub walk_dist: f32,
    pub bob: f32,
    pub carried: Option<ItemStack>,
    pub craft_grid: Vec<Option<ItemStack>>,
    pub ender_items: Vec<Option<ItemStack>>,
    pub ui: ClientUiSnapshot,
    pub open_container_bes: Vec<((i32, i32, i32), BlockEntity)>,
}

thread_local! {
    static ANCHOR_STATE: RefCell<Option<AnchorClientState>> = const { RefCell::new(None) };
}

fn capture_anchor_client_state(game: &mut Game<'_>) -> AnchorClientState {
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
    AnchorClientState {
        dim: game.dim,
        chunks: (&*game.world.chunks).clone(),
        day_time: game.world.day_time,
        weather: game.world.weather,
        border: game.border.clone(),
        boss_overlay: game.boss_overlay.clone(),
        sculk_darkness: game.sculk.darkness,
        flash: game.flash,
        be_anims: game.be_anims.clone(),
        moving_pistons: game.world.moving_pistons.clone(),
        mobs: game.mobs.clone(),
        dropped: game.dropped.clone(),
        orbs: game.orbs.clone(),
        tnts: game.tnts.clone(),
        falling: game.falling.clone(),
        boats: game.boats.clone(),
        carts: game.carts.clone(),
        projectiles: game.creatures.projectiles.clone(),
        crystals: game.crystals.clone(),
        paintings: game.paintings.clone(),
        item_frames: game.item_frames.clone(),
        armor_stands: game.armor_stands.clone(),
        cushions: game.cushions.clone(),
        others: game.others.clone(),
        dragon: game.dragon.as_ref().map(DragonSnapshot::from_live),
        tridents,
        rockets,
        breath_clouds,
        fangs: game.fangs.list.clone(),
        thrown_potions: game.potions.rreturn_capture_thrown(),
        particles: game.particles.live().cloned().collect(),
        elder_guardians: game.particles.elder_guardians().to_vec(),
        player: game.player.clone(),
        yaw: game.yaw,
        pitch: game.pitch,
        selected: game.selected,
        inventory: game.inventory.clone(),
        armor: game.armor.clone(),
        offhand: game.offhand.clone(),
        using: game.using,
        use_duration: game.use_duration,
        fov_modifier: game.view.fov_modifier,
        walk_dist: game.avatar.walk_dist,
        bob: game.avatar.bob,
        carried: game.carried.clone(),
        craft_grid: game.craft_grid.clone(),
        ender_items: game.ender_items.clone(),
        ui: capture_client_ui_snapshot(game),
        open_container_bes: if game.container.is_some() {
            game.container_parts()
                .into_iter()
                .filter_map(|(pos, _)| game.world.block_entities.get(&pos).map(|be| (pos, be.clone())))
                .collect()
        } else {
            Vec::new()
        },
    }
}

fn restore_anchor_client_state(game: &mut Game<'_>, anchor: AnchorClientState) {
    if game.dim != anchor.dim {
        game.dim = anchor.dim;
        game.world.chunks = Default::default();
        let num_sections = game.world.layout().sections() as i32;
        for ((cx, cz), chunk) in anchor.chunks {
            game.world.chunks.insert((cx, cz), chunk);
            for s in 0..num_sections {
                game.world.dirty_sections.insert((cx, s, cz));
            }
        }
        game.world.revision = game.world.revision.wrapping_add(1);
    }
    game.world.day_time = anchor.day_time;
    game.world.weather = anchor.weather;
    game.border = anchor.border;
    game.boss_overlay = anchor.boss_overlay;
    game.sculk.darkness = anchor.sculk_darkness;
    game.flash = anchor.flash;
    let kinds = std::mem::take(&mut game.be_anims.kinds);
    game.be_anims = anchor.be_anims;
    if game.be_anims.kinds.is_empty() {
        game.be_anims.kinds = kinds;
    }
    game.world.moving_pistons = anchor.moving_pistons;
    game.mobs = anchor.mobs;
    game.dropped = anchor.dropped;
    game.orbs = anchor.orbs;
    game.tnts = anchor.tnts;
    game.falling = anchor.falling;
    game.boats = anchor.boats;
    game.carts = anchor.carts;
    game.creatures.projectiles = anchor.projectiles;
    game.crystals = anchor.crystals;
    game.paintings = anchor.paintings;
    game.item_frames = anchor.item_frames;
    game.armor_stands = anchor.armor_stands;
    game.cushions = anchor.cushions;
    game.others = anchor.others;
    if let Some(d) = anchor.dragon.and_then(|ds| ds.to_live(game.level.seed)) {
        game.dragon.replace(d);
    } else {
        game.dragon.discard();
    }
    let registry = game.pack.assets.registry;
    game.tridents.clear();
    for ts in &anchor.tridents {
        if let Some(n) = horizon_client::entity_nbt::trident_from_nbt(&ts.nbt) {
            let mut t = horizon_gameplay::trident::ThrownTrident::from_nbt(n, &ts.nbt, registry);
            t.id = ts.id;
            t.previous = ts.previous;
            t.in_ground_time = ts.in_ground_time;
            t.no_physics = ts.no_physics;
            t.owner = ts.owner;
            t.guest = ts.guest;
            game.tridents.push(t);
        }
    }
    game.rockets.clear();
    for rs in &anchor.rockets {
        if let Some(rn) = horizon_client::entity_nbt::firework_rocket_from_nbt(&rs.nbt, None) {
            let mut r = horizon_gameplay::fireworks::Rocket::from_nbt(rn);
            r.id = rs.id;
            r.attached = rs.attached;
            r.guest = rs.guest;
            r.owner = rs.owner;
            r.attached_to = rs.attached_to;
            game.rockets.push(r);
        }
    }
    game.breath_clouds.clear();
    for bs in &anchor.breath_clouds {
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
    game.fangs.list = anchor.fangs;
    game.potions.rreturn_restore_thrown(&anchor.thrown_potions);
    game.particles.clear();
    for p in anchor.particles {
        game.particles.push(p);
    }
    for _ in 0..anchor.elder_guardians.len() {
        game.particles.elder_guardian();
    }
    let level = horizon_client::particles::ParticleLevel {
        world: &game.world,
        facts: game.res.facts(),
        collision: game.res.collision(),
        player: anchor
            .player
            .as_ref()
            .map(|p| ([p.position.x, p.position.y, p.position.z], p.velocity.y)),
        cached_chunk: std::cell::Cell::new(None),
    };
    game.particles.tick(&level);
    game.particles.sounds.clear();
    game.player = anchor.player;
    game.yaw = anchor.yaw;
    game.pitch = anchor.pitch;
    game.selected = anchor.selected;
    game.inventory = anchor.inventory;
    game.armor = anchor.armor;
    game.offhand = anchor.offhand;
    game.ender_items = anchor.ender_items;
    for (pos, be) in anchor.open_container_bes {
        game.world.block_entities.insert(pos, be);
    }
    game.using = anchor.using;
    game.use_duration = anchor.use_duration;
    game.view.fov_modifier = anchor.fov_modifier;
    game.view.old_fov_modifier = anchor.fov_modifier;
    game.avatar.walk_dist = anchor.walk_dist;
    game.avatar.walk_dist_o = anchor.walk_dist;
    game.avatar.bob = anchor.bob;
    game.avatar.bob_o = anchor.bob;
    game.snap_player_model_pitch();
    restore_client_ui_on_rollback(game, &anchor.ui, &RollbackPolicy::default());
}

pub struct ScrubSession {
    pub active: bool,
    pub anchor_tick: i64,
    pub min_tick: i64,
    pub current_time: f64,
    pub target_time: f64,
    pub last_scroll_time: Option<Instant>,
    pub jog_accel: f64,
    pub start_time: Option<Instant>,
    pub applied_block_tick: i64,
    pub was_frozen_before_scrub: bool,
    pub server_was_frozen_before_scrub: bool,
}

impl Default for ScrubSession {
    fn default() -> Self {
        Self {
            active: false,
            anchor_tick: 0,
            min_tick: 0,
            current_time: 0.0,
            target_time: 0.0,
            last_scroll_time: None,
            jog_accel: 1.0,
            start_time: None,
            applied_block_tick: 0,
            was_frozen_before_scrub: false,
            server_was_frozen_before_scrub: false,
        }
    }
}

static SESSION: OnceLock<Mutex<ScrubSession>> = OnceLock::new();
static SCRUB_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn session_lock() -> &'static Mutex<ScrubSession> {
    SESSION.get_or_init(|| Mutex::new(ScrubSession::default()))
}

pub fn is_scrubbing() -> bool {
    SCRUB_ACTIVE.load(std::sync::atomic::Ordering::Acquire)
}

pub fn snap_scrub_time_to_target() {
    if let Ok(mut s) = session_lock().lock() {
        s.current_time = s.target_time;
    }
}

pub fn is_held_scrub(min_ms: u128) -> bool {
    let s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    if !s.active {
        return false;
    }
    s.start_time
        .map(|t| t.elapsed().as_millis() >= min_ms)
        .unwrap_or(false)
}

pub fn get_scrub_info() -> Option<(f64, i64, i64)> {
    let s = session_lock().lock().ok()?;
    if !s.active {
        return None;
    }
    Some((s.current_time, s.min_tick, s.anchor_tick))
}

pub fn step_scrub_ticks(ticks: f64) {
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if !s.active {
        return;
    }
    s.target_time = (s.target_time + ticks).clamp(s.min_tick as f64, s.anchor_tick as f64);
}

pub fn set_scrub_target_fraction(frac: f64, snap_now: bool) {
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if !s.active {
        return;
    }
    let span = (s.anchor_tick - s.min_tick).max(1) as f64;
    let target = (s.min_tick as f64 + span * frac.clamp(0.0, 1.0))
        .clamp(s.min_tick as f64, s.anchor_tick as f64);
    s.target_time = target;
    if snap_now {
        s.current_time = target;
    }
}

pub fn start_scrub(game: &mut Game<'_>) {
    if !with_state(|st| st.scrub_enabled) {
        mod_chat(game, &tr("command.rreturn.scrub.disabled"));
        return;
    }
    if !has_level_4_permission(game, None) {
        mod_chat(game, &tr("command.rreturn.no_permission"));
        return;
    }
    if game.remote.is_some() && game.integrated.is_none() {
        mod_chat(game, &tr("command.rreturn.no_permission"));
        return;
    }
    let server_range = if let Some(server) = &game.integrated {
        let (rng, players) = server
            .call(|srv| (server_scrub_time_range(), collect_online_players(srv)))
            .unwrap_or((None, Vec::new()));
        if !players.is_empty() {
            with_state_mut(|st| st.online_players = players);
        }
        rng
    } else {
        server_scrub_time_range()
    };

    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if s.active {
        return;
    }

    let now_tick = game.world.game_time;
    let client_range = with_scrub_ring(|r| r.time_range());

    let (min_tick, max_tick) = match (client_range, server_range) {
        (Some((cmin, cmax)), Some((smin, smax))) => (cmin.max(smin), cmax.max(smax)),
        (Some(cr), None) => cr,
        (None, Some(sr)) => sr,
        (None, None) => (now_tick, now_tick),
    };

    s.active = true;
    s.anchor_tick = now_tick.max(max_tick);
    s.min_tick = min_tick.min(s.anchor_tick);
    s.current_time = s.anchor_tick as f64;
    s.target_time = s.current_time;
    s.last_scroll_time = None;
    s.jog_accel = 1.0;
    s.start_time = Some(Instant::now());
    s.applied_block_tick = s.anchor_tick;
    s.was_frozen_before_scrub = game.clock.frozen();
    let was_frozen = s.was_frozen_before_scrub;
    let start_t = s.current_time;
    SCRUB_ACTIVE.store(true, std::sync::atomic::Ordering::Release);
    drop(s);

    let anchor_snapshot = capture_anchor_client_state(game);
    ANCHOR_STATE.with(|cell| {
        *cell.borrow_mut() = Some(anchor_snapshot);
    });
    reset_scrub_ui_preview_cache();
    reset_scrub_hud_state();

    with_scrub_ring_mut(|r| {
        if r.reference_chunks.is_empty() {
            r.reference_chunks = (&*game.world.chunks).clone();
            r.last_world_revision = game.world.revision;
        }
    });

    game.stop_mining();
    if let Some(c) = game.audio.capture.as_mut() {
        c.clear();
    }
    game.clock.set_frozen(true);

    let srv_frozen = if let Some(server) = &game.integrated {
        server
            .call(|srv| {
                let prev = srv.clock.frozen();
                srv.clock.set_frozen(true);
                if let Some(host) = srv.lan.as_ref() {
                    let pkt = C::TickingState(play::TickingState {
                        rate: srv.clock.rate(),
                        frozen: true,
                    });
                    host.hub.broadcast_level(&pkt);
                }
                prev
            })
            .unwrap_or(false)
    } else {
        was_frozen
    };
    if let Ok(mut s) = session_lock().lock() {
        s.server_was_frozen_before_scrub = srv_frozen;
    }

    if let Some(r) = game.remote.as_mut() {
        r.stepped.clear();
        r.lerps.clear();
        r.cart_lerps.clear();
        r.head_lerps.clear();
    }

    apply_interpolated_poses_to_game(game, start_t);
}

pub fn apply_scrub_wheel_delta(wheel_y: f64) -> bool {
    if !with_state(|st| st.scrub_enabled) {
        return false;
    }
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    if !s.active {
        return false;
    }
    if wheel_y.abs() <= 1e-6 {
        return true;
    }

    let now = Instant::now();
    if let Some(prev) = s.last_scroll_time {
        let dt_ms = now.duration_since(prev).as_millis();
        if dt_ms < 60 {
            s.jog_accel = (s.jog_accel + 0.8).min(20.0);
        } else if dt_ms > 200 {
            s.jog_accel = 1.0;
        }
    } else {
        s.jog_accel = 1.0;
    }
    s.last_scroll_time = Some(now);

    let step = wheel_y * s.jog_accel * scrub_wheel_sens_factor();
    s.target_time = (s.target_time + step).clamp(s.min_tick as f64, s.anchor_tick as f64);
    true
}

pub fn on_mouse_scroll(_game: &mut Game<'_>, scroll: [f64; 2], accumulated: [i32; 2]) -> bool {
    if !with_state(|st| st.scrub_enabled) || !is_scrubbing() {
        return false;
    }
    let wheel_y = if scroll[1].abs() > 1e-4 {
        scroll[1]
    } else if accumulated[1] != 0 {
        f64::from(accumulated[1])
    } else {
        return true;
    };
    apply_scrub_wheel_delta(wheel_y)
}

pub fn on_frame_render_pre(game: &mut Game<'_>, _world_partial: f32, _partial: f32) {
    if !with_state(|st| st.scrub_enabled) {
        return;
    }
    sync_scrub_hud_drag(game);
    if let Some(action) = take_pending_scrub_hud_action() {
        match action {
            ScrubHudAction::Apply => {
                commit_scrub(game);
                return;
            }
            ScrubHudAction::Cancel => {
                cancel_scrub(game);
                return;
            }
        }
    }
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if !s.active {
        return;
    }

    let diff = s.target_time - s.current_time;
    if diff.abs() > 1e-3 {
        s.current_time += diff * 0.35;
    } else {
        s.current_time = s.target_time;
    }

    let target_t = s.current_time;
    let old_applied = s.applied_block_tick;
    let new_applied = target_t.round() as i64;
    let accel = s.jog_accel;
    s.applied_block_tick = new_applied;
    drop(s);

    let t_frame_start = std::time::Instant::now();
    apply_interpolated_poses_to_game(game, target_t);
    let mut block_rewind_us = 0;
    if old_applied != new_applied {
        let t_br_0 = std::time::Instant::now();
        apply_block_rewind_to_game(game, old_applied, new_applied);
        block_rewind_us = t_br_0.elapsed().as_micros() as u64;
        if scrub_reverse_audio_enabled() {
            apply_timeline_sounds_to_game(game, old_applied, new_applied, accel);
        }
    }
    let total_render_us = t_frame_start.elapsed().as_micros() as u64;
    record_frame_render_perf(
        total_render_us,
        0,
        0,
        0,
        0,
        block_rewind_us,
        0,
    );
}

fn clear_sprint_on_commit(game: &mut Game<'_>) {
    if let Some(p) = game.player.as_mut() {
        p.sprint_input = Default::default();
        let _ = p.set_sprinting(false);
        if let Ok(spd) = p.attribute("minecraft:movement_speed") {
            p.speed = spd as f32;
        }
    }
    game.view.fov_modifier = 1.0;
    game.view.old_fov_modifier = 1.0;
    game.avatar.walk_dist = 0.0;
    game.avatar.walk_dist_o = 0.0;
    game.avatar.bob = 0.0;
    game.avatar.bob_o = 0.0;
}

pub fn commit_scrub(game: &mut Game<'_>) {
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if !s.active {
        return;
    }

    let final_tick = s.current_time.round() as i64;
    let anchor = s.anchor_tick;
    let final_time = s.current_time;
    let was_frozen = s.was_frozen_before_scrub;
    let srv_was_frozen = s.server_was_frozen_before_scrub;
    s.active = false;
    SCRUB_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    s.start_time = None;
    s.jog_accel = 1.0;
    s.applied_block_tick = final_tick;
    drop(s);

    game.extras.frame_partial_ticks = None;
    let saved_anchor = ANCHOR_STATE.with(|cell| cell.borrow_mut().take());
    reset_scrub_ui_preview_cache();
    reset_scrub_hud_state();

    if final_tick < anchor {
        let target_ui: Option<ClientUiSnapshot> = with_scrub_ring(|r| {
            r.frames
                .iter()
                .min_by_key(|f| (f.game_time - final_tick).abs())
                .map(|f| (*f.ui).clone())
        });
        let t_commit_start = std::time::Instant::now();
        apply_interpolated_poses_to_game(game, final_time);
        game.extras.frame_partial_ticks = None;

        let (did_rollback, restored_frozen, mat_us, srb_us) = if let Some(server) = &game.integrated {
            server
                .call(move |srv| {
                    let t_mat_0 = std::time::Instant::now();
                    if let Some((snap, touched_unloaded, future_region_files)) =
                        materialize_server_scrub(srv, final_tick)
                    {
                        let mat_us = t_mat_0.elapsed().as_micros() as u64;
                        let t_srb_0 = std::time::Instant::now();
                        let ok = apply_timeline_snapshot_on_server(
                            srv,
                            &snap,
                            RollbackOrigin::ScrubSnapshot {
                                target_tick: final_tick,
                                touched_unloaded,
                                future_region_files,
                            },
                        );
                        let srb_us = t_srb_0.elapsed().as_micros() as u64;
                        if ok {
                            truncate_server_scrub_after(snap.game_time);
                        }
                        let frozen = srv.clock.frozen();
                        if let Some(host) = srv.lan.as_ref() {
                            let pkt = C::TickingState(play::TickingState {
                                rate: srv.clock.rate(),
                                frozen,
                            });
                            host.hub.broadcast_level(&pkt);
                        }
                        (ok, frozen, mat_us, srb_us)
                    } else {
                        srv.clock.set_frozen(srv_was_frozen);
                        (false, srv_was_frozen, 0, 0)
                    }
                })
                .unwrap_or((false, was_frozen, 0, 0))
        } else {
            let t_mat_0 = std::time::Instant::now();
            if let Some((snap, touched_unloaded, future_region_files)) =
                materialize_server_scrub(game, final_tick)
            {
                let mat_us = t_mat_0.elapsed().as_micros() as u64;
                let t_srb_0 = std::time::Instant::now();
                let ok = apply_timeline_snapshot_on_server(
                    game,
                    &snap,
                    RollbackOrigin::ScrubSnapshot {
                        target_tick: final_tick,
                        touched_unloaded,
                        future_region_files,
                    },
                );
                let srb_us = t_srb_0.elapsed().as_micros() as u64;
                if ok {
                    truncate_server_scrub_after(snap.game_time);
                }
                (ok, game.clock.frozen(), mat_us, srb_us)
            } else {
                (false, was_frozen, 0, 0)
            }
        };

        let mut crb_us = 0;
        if did_rollback {
            close_client_container_local(game);
            game.screen = super::super::Screen::Playing;
            let t_crb_0 = std::time::Instant::now();
            apply_client_level_rollback(game, "scrub");
            if let Some(ui) = target_ui {
                let pol = resolve_local_client_policy(game, true);
                restore_client_ui_on_rollback(game, &ui, &pol);
            }
            crb_us = t_crb_0.elapsed().as_micros() as u64;

            let player_breaker = game.remote.as_ref().map(|r| r.entity_id);
            with_scrub_ring(|ring| {
                if let Some(frame) = ring.frames.iter().rfind(|f| f.game_time == final_tick) {
                    if let Some(bp) = frame
                        .break_progress
                        .iter()
                        .find(|bp| Some(bp.breaker) == player_breaker || bp.breaker == -1)
                    {
                        let progress = (bp.stage as f32 + 0.5) / 10.0;
                        if let Some(r) = game.remote.as_mut() {
                            r.destroying = Some((bp.pos, 0, progress));
                            r.is_destroying = true;
                        }
                    }
                }
            });
        } else if let Some(anchor_state) = saved_anchor {
            restore_anchor_client_state(game, anchor_state);
        }

        let total_commit_us = t_commit_start.elapsed().as_micros() as u64;
        record_commit_perf(total_commit_us, mat_us, srb_us, crb_us);

        game.clock.set_frozen(restored_frozen);

        with_scrub_ring_mut(|r| {
            r.frames.retain(|f| f.game_time <= final_tick);
            r.checkpoint_markers.retain(|&t| t <= final_tick);
            r.dim_markers.retain(|&(t, _)| t <= final_tick);
            r.damage_markers.retain(|&t| t <= final_tick);
            r.reference_chunks = (&*game.world.chunks).clone();
            r.last_world_revision = game.world.revision;
            r.last_dim = Some(game.dim);
        });
        rrtl_truncate_client_after(final_tick);
        horizon_threads::release_free_memory();

        let dt = (anchor - final_tick) as f64 / 20.0;
        let dt_str = format!("{:.1}", dt);
        mod_chat(game, &tr_args("message.rreturn.scrub.commit", &[&dt_str]));
    } else {
        if let Some(anchor_state) = saved_anchor {
            restore_anchor_client_state(game, anchor_state);
        }
        game.clock.set_frozen(was_frozen);
        if let Some(server) = &game.integrated {
            server.call(move |srv| {
                srv.clock.set_frozen(srv_was_frozen);
                if let Some(host) = srv.lan.as_ref() {
                    let pkt = C::TickingState(play::TickingState {
                        rate: srv.clock.rate(),
                        frozen: srv_was_frozen,
                    });
                    host.hub.broadcast_level(&pkt);
                }
            });
        }
        with_scrub_ring_mut(|r| {
            r.reference_chunks = (&*game.world.chunks).clone();
            r.last_world_revision = game.world.revision;
        });
    }
    clear_sprint_on_commit(game);
}

pub fn cancel_scrub(game: &mut Game<'_>) {
    let mut s = match session_lock().lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    if !s.active {
        return;
    }

    let anchor = s.anchor_tick;
    let applied = s.applied_block_tick;
    let was_frozen = s.was_frozen_before_scrub;
    let srv_was_frozen = s.server_was_frozen_before_scrub;
    s.active = false;
    SCRUB_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    s.start_time = None;
    s.current_time = anchor as f64;
    s.target_time = anchor as f64;
    s.applied_block_tick = anchor;
    drop(s);

    game.extras.frame_partial_ticks = None;

    if applied != anchor {
        apply_block_rewind_to_game(game, applied, anchor);
    }

    let saved_anchor = ANCHOR_STATE.with(|cell| cell.borrow_mut().take());
    reset_scrub_ui_preview_cache();
    reset_scrub_hud_state();
    if let Some(anchor_state) = saved_anchor {
        restore_anchor_client_state(game, anchor_state);
    } else {
        apply_interpolated_poses_to_game(game, anchor as f64);
        game.extras.frame_partial_ticks = None;
    }

    with_scrub_ring_mut(|r| {
        r.reference_chunks = (&*game.world.chunks).clone();
        r.last_world_revision = game.world.revision;
    });

    game.clock.set_frozen(was_frozen);
    if let Some(server) = &game.integrated {
        server.call(move |srv| {
            srv.clock.set_frozen(srv_was_frozen);
            if let Some(host) = srv.lan.as_ref() {
                let pkt = C::TickingState(play::TickingState {
                    rate: srv.clock.rate(),
                    frozen: srv_was_frozen,
                });
                host.hub.broadcast_level(&pkt);
            }
        });
    }
    mod_chat(game, &tr("message.rreturn.scrub.cancel"));
}

pub fn render_hud(game: &Game<'_>, draw: &mut horizon_client::hud::HudBatch) {
    if game.screen == super::super::Screen::Playing {
        draw_scrub_hud(game, draw);
    }
}