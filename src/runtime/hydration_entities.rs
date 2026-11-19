use super::super::{
    Dim, ExperienceOrb, FallingBlock, Game, ItemEntity, Mob,
    commands::OtherEntity,
    dragon::Crystal,
};
use super::snapshots::*;
use super::types::*;
use horizon_client::entities::{ArmorStand, Cushion, ItemFrame, Painting};
use horizon_core::{
    geometry::Vec3,
    position::BlockPos,
    random::LegacyRandom,
};
use horizon_gameplay::{
    boats::Boat,
    minecarts::Minecart,
    tnt::PrimedTnt,
};
use horizon_world::{
    nbt::Tag,
    registry::BlockStateId,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DiskTnt {
    pub id: u32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub fuse: i32,
    pub power: f32,
    pub no_gravity: bool,
    pub on_ground: bool,
    pub fall_distance: f64,
    pub rot: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct DiskFalling {
    pub id: u32,
    pub state: BlockStateId,
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: i32,
    pub no_gravity: bool,
    pub drop_item: bool,
    pub on_ground: bool,
    pub fall_distance: f64,
    pub rot: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct DiskProjectile {
    pub id: u32,
    pub kind: String,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub age: u32,
    pub stuck: bool,
}

pub struct HydratedEntitiesResult {
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
    pub fangs: horizon_client::join_seq::ResidentList<super::super::fangs::Fangs>,
    pub ominous_spawners: Vec<(u32, Tag)>,
    pub knot_ids: HashMap<[i32; 3], u32>,
    pub persist_uuids: HashMap<u32, [i32; 4]>,
    pub persist_raw: HashMap<u32, Tag>,
    pub persist_passenger_slots: HashMap<u32, Vec<super::super::persist::PassengerSlot>>,
    pub portals_entities: HashMap<u32, horizon_client::nether_portal::PortalState>,
    pub last_hurt_by_player: HashMap<u32, super::super::combat::PlayerCredit>,
    pub love_cause: HashMap<u32, Option<u32>>,
}

pub fn hydrate_entities_from_disk(
    game: &mut Game<'_>,
    cp_save: &horizon_client::save::SaveDir,
    dim: Dim,
    snap: &TimelineSnapshot,
    chunk_keys: &[(i32, i32)],
    meta: Option<&serde_json::Value>,
) -> HydratedEntitiesResult {
    let mut disk_last_hurt_by_player = HashMap::new();
    let mut disk_love_cause = HashMap::new();
    let mut disk_uuid_to_id: HashMap<[i32; 4], u32> = HashMap::new();
    let mut disk_mob_rt: HashMap<u32, DiskMobRuntime> = HashMap::new();
    let mut disk_item_rt: HashMap<u32, DiskItemRuntime> = HashMap::new();
    let mut disk_orb_rt: HashMap<u32, u32> = HashMap::new();
    let mut disk_portals_entities: HashMap<u32, horizon_client::nether_portal::PortalState> =
        HashMap::new();
    let mut disk_tnts: Vec<DiskTnt> = Vec::new();
    let mut disk_falling: Vec<DiskFalling> = Vec::new();
    let mut disk_boat_velocities: HashMap<u32, Vec3> = HashMap::new();
    let mut disk_cart_velocities: HashMap<u32, Vec3> = HashMap::new();
    let mut disk_projectiles: Vec<DiskProjectile> = Vec::new();

    if let Some(meta) = meta {
        let rs = &meta["level_redstone"][dim.id()];
        if let Some(arr) = rs["last_hurt_by_player"].as_array() {
            for row in arr {
                if let Some(a) = row.as_array()
                    && a.len() == 2
                    && let Some(k) = a[0].as_u64()
                {
                    let cred = a[1]
                        .as_str()
                        .and_then(|s| s.parse::<u128>().ok())
                        .unwrap_or_else(|| a[1].as_u64().unwrap_or(0) as u128);
                    disk_last_hurt_by_player.insert(
                        k as u32,
                        super::super::combat::PlayerCredit(cred),
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

    for &(cx, cz) in chunk_keys {
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
                game.tnts.push(horizon_gameplay::tnt::PrimedTnt {
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
    game.world.random = saved_random;
    game.next_entity = saved_next_entity;

    HydratedEntitiesResult {
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
        fangs,
        ominous_spawners,
        knot_ids,
        persist_uuids: (*persist_mem.uuids).clone(),
        persist_raw: persist_mem.raw,
        persist_passenger_slots: persist_mem.passenger_slots,
        portals_entities: disk_portals_entities,
        last_hurt_by_player: disk_last_hurt_by_player,
        love_cause: disk_love_cause,
    }
}
