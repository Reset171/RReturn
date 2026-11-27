use super::super::{Dim, Game};
use super::det_rng::*;
use super::hydration_chunks::*;
use super::hydration_entities::*;
use super::load_disk::load_slot_from_disk;
use super::snapshots::*;
use super::storage::*;
use super::types::*;
use horizon_client::{
    save::read_dat,
    worldborder::WorldBorder,
};
use horizon_core::random::{LegacyRandom, LevelRandom};
use std::{
    fs,
    sync::Arc,
};

/// Congruence stride applied to hydrated disk entity IDs and `next_entity` so
/// periodic engine checks (`% 2, 4, 10, 20, 40, 120, 300, 1200`) stay identical
/// while preventing ID collisions across loads in the same session.
const DISK_ID_MOD_STRIDE: u32 = 24_000;

pub fn hydrate_level_from_disk(
    game: &mut Game<'_>,
    slot: &str,
    dim: Dim,
    snap: &TimelineSnapshot,
    id_stride: u32,
    fresh_cursor: &mut u32,
    slot_orig_to_fresh: &mut HashMap<u32, u32>,
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

    let meta_val: Option<serde_json::Value> = fs::read_to_string(cp_dir.join("meta.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());

    let chunks_res = hydrate_chunks_from_disk(
        game,
        &cp_dir,
        &cp_save,
        dim,
        snap,
        meta_val.as_ref(),
    )?;

    let mut chunk_keys: Vec<(i32, i32)> = chunks_res.chunks.keys().copied().collect();
    chunk_keys.sort_unstable();

    let entities_res = hydrate_entities_from_disk(
        game,
        &cp_save,
        dim,
        snap,
        &chunk_keys,
        meta_val.as_ref(),
        id_stride,
        fresh_cursor,
        slot_orig_to_fresh,
    );

    let raids_dat = fs::read(raids_dat_path(&cp_dir, dim)).ok();

    let hydrated_random = chunks_res
        .level_rng
        .map(|st| LevelRandom::for_level(LegacyRandom::from((st, chunks_res.level_rng_gaussian))))
        .unwrap_or_else(|| game.world.random.clone());
    let hydrated_rtv = chunks_res
        .random_tick_value
        .unwrap_or(game.block_ticks.random_tick_value);

    let globals_tag = read_dat(&cp_dir.join("rreturn_globals.dat"));
    let (border_key, be_key) = match dim {
        Dim::Overworld => ("border_overworld", "block_entities_overworld"),
        Dim::Nether => ("border_nether", "block_entities_nether"),
        Dim::End => ("border_end", "block_entities_end"),
        Dim::Extra(_) => ("border_extra", "block_entities_extra"),
    };
    let mut block_entities = chunks_res.block_entities;
    if let Some(horizon_core::nbt::Tag::List(be_list)) =
        globals_tag.as_ref().and_then(|t| t.get(be_key))
    {
        let facts = game.res.facts();
        block_entities.clear();
        for tag in be_list {
            let (
                Some(horizon_core::nbt::Tag::Int(x)),
                Some(horizon_core::nbt::Tag::Int(y)),
                Some(horizon_core::nbt::Tag::Int(z)),
            ) = (tag.get("x"), tag.get("y"), tag.get("z"))
            else {
                continue;
            };
            let pos = (*x, *y, *z);
            let block_name = chunks_res
                .chunks
                .get(&(pos.0 >> 4, pos.2 >> 4))
                .map(|c| {
                    facts
                        .get(c.block(pos.0 & 15, pos.1, pos.2 & 15))
                        .name
                        .as_ref()
                })
                .unwrap_or("minecraft:stone");
            if let Some((decoded_pos, be)) =
                horizon_client::save::decode_block_entity_for_block(tag, block_name)
                    .or_else(|| horizon_client::save::decode_block_entity(tag))
            {
                block_entities.insert(decoded_pos, be);
            }
        }
    }
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
        chunks: chunks_res.chunks,
        block_entities,
        block_entity_ticker_order: chunks_res.be_ticker_order,
        block_entity_asleep: chunks_res.be_asleep,
        block_names: chunks_res.block_names,
        be_attachments: chunks_res.be_attachments,
        world_attachments: chunks_res.world_attachments,
        chest_openers: chunks_res.chest_openers,
        border: hydrated_border,
        shulker_anims: Vec::new(),
        gateway_anims: chunks_res.gateway_anims,
        fangs: entities_res.fangs,
        ominous_spawners: entities_res.ominous_spawners,
        portals_entities: entities_res.portals_entities,
        lectern_pulses: chunks_res.lectern_pulses,
        lid_openers: chunks_res.lid_openers,
        inhabited: chunks_res.inhabited,
        end_clock: chunks_res.end_clock,
        random: hydrated_random,
        forced_chunks: chunks_res.forced_chunks,
        chunk_ticks: chunks_res.chunk_ticks,
        random_tick_value: hydrated_rtv,
        comparator_outputs: chunks_res.comparator_outputs,
        torch_toggles: chunks_res.torch_toggles,
        moving_pistons: chunks_res.moving_pistons,
        piston_pushes: chunks_res.piston_pushes,
        block_events: chunks_res.block_events,
        hoppers: chunks_res.hoppers,
        mobs: entities_res.mobs,
        dropped: entities_res.dropped,
        orbs: entities_res.orbs,
        projectiles: entities_res.projectiles,
        boats: entities_res.boats,
        carts: entities_res.carts,
        tnts: entities_res.tnts,
        falling: entities_res.falling,
        paintings: entities_res.paintings,
        item_frames: entities_res.item_frames,
        armor_stands: entities_res.armor_stands,
        cushions: entities_res.cushions,
        others: entities_res.others,
        crystals: entities_res.crystals,
        dragon: entities_res.dragon,
        tridents: entities_res.tridents,
        rockets: entities_res.rockets,
        breath_clouds: entities_res.breath_clouds,
        thrown_potions: ThrownPotionsSnapshot::default(),
        knot_ids: entities_res.knot_ids,
        persist_uuids: entities_res.persist_uuids,
        persist_raw: entities_res.persist_raw,
        persist_passenger_slots: entities_res.persist_passenger_slots,
        raids_dat,
        raids_mem: None,
        raid_runtime: chunks_res.raid_runtime,
        last_hurt_by_player: entities_res.last_hurt_by_player,
        love_cause: entities_res.love_cause,
        died: entities_res.died,
    })
}

fn remap_hydrated_id(
    id: u32,
    orig_to_fresh: &HashMap<u32, u32>,
    session_ceiling: u32,
) -> Option<u32> {
    orig_to_fresh
        .get(&id)
        .copied()
        .or_else(|| (id > session_ceiling).then_some(id))
}

fn remap_id_keyed_map<V>(
    map: &mut HashMap<u32, V>,
    orig_to_fresh: &HashMap<u32, u32>,
    session_ceiling: u32,
) {
    let old = std::mem::take(map);
    for (id, val) in old {
        if let Some(new_id) = remap_hydrated_id(id, orig_to_fresh, session_ceiling) {
            map.insert(new_id, val);
        }
    }
}

fn remap_timeline_snapshot_after_hydration(
    snap: &mut TimelineSnapshot,
    orig_to_fresh: &HashMap<u32, u32>,
    id_stride: u32,
    target_snap_next: u32,
    fresh_cursor: &mut u32,
    session_ceiling: u32,
) {
    for gs in snap.guests.values_mut() {
        gs.vehicle = gs
            .vehicle
            .and_then(|vid| remap_hydrated_id(vid, orig_to_fresh, session_ceiling));
        if let Some(hook) = gs.hook.as_mut() {
            if let Some(shifted) = hook
                .id
                .checked_add(id_stride)
                .filter(|&did| did > session_ceiling && did <= target_snap_next)
            {
                hook.id = shifted;
            } else if hook.id <= *fresh_cursor || hook.id <= session_ceiling {
                *fresh_cursor = fresh_cursor.saturating_add(1);
                hook.id = *fresh_cursor;
            }
        }
    }
    remap_id_keyed_map(&mut snap.portals_entities, orig_to_fresh, session_ceiling);
    remap_id_keyed_map(
        &mut snap.end_gateway_cooldowns,
        orig_to_fresh,
        session_ceiling,
    );
    remap_id_keyed_map(&mut snap.lead_killers, orig_to_fresh, session_ceiling);

    let old_attachments = std::mem::take(&mut snap.entity_attachments);
    for (id, att) in old_attachments {
        if let Some(new_id) = remap_hydrated_id(id, orig_to_fresh, session_ceiling) {
            snap.entity_attachments.insert(new_id, att);
        }
    }

    let old_mob_players = std::mem::take(&mut snap.mob_players);
    for (id, conn) in old_mob_players {
        if let Some(new_id) = remap_hydrated_id(id, orig_to_fresh, session_ceiling) {
            snap.mob_players.insert(new_id, conn);
        }
    }

    let extra = (*fresh_cursor).saturating_sub(target_snap_next);
    let aligned_next = if extra == 0 {
        target_snap_next
    } else {
        target_snap_next.saturating_add(extra.div_ceil(DISK_ID_MOD_STRIDE) * DISK_ID_MOD_STRIDE)
    };
    *fresh_cursor = aligned_next;
    snap.next_entity = snap.next_entity.max(aligned_next);
    snap.ticked_entities_through = snap.ticked_entities_through.max(snap.next_entity);
}

pub fn hydrate_slot_from_disk_on_server(
    game: &mut Game<'_>,
    slot: &str,
) -> Option<Arc<TimelineSnapshot>> {
    let mut snap = match get_slot_snapshot_clone(slot) {
        Some(existing) => (*existing).clone(),
        None => load_slot_from_disk(&game.world_root, slot)?,
    };

    observe_session_entity_ids(game);
    let saved_det_rng = det_rng_raw_state();
    let session_ceiling = session_entity_ceiling();
    let min_safe_base = session_ceiling.max(game.next_entity);
    let id_stride = if min_safe_base == 0 {
        0
    } else {
        (min_safe_base + 1).div_ceil(DISK_ID_MOD_STRIDE) * DISK_ID_MOD_STRIDE
    };
    let target_snap_next = snap.next_entity.saturating_add(id_stride);
    let mut fresh_cursor = target_snap_next.max(min_safe_base);
    let mut slot_orig_to_fresh: HashMap<u32, u32> = HashMap::new();
    let mut any_hydrated = false;

    for dim in [Dim::Overworld, Dim::Nether, Dim::End] {
        if !snap.levels.contains_key(&dim) {
            if let Some(lsnap) = hydrate_level_from_disk(
                game,
                slot,
                dim,
                &snap,
                id_stride,
                &mut fresh_cursor,
                &mut slot_orig_to_fresh,
            ) {
                any_hydrated = true;
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

    if any_hydrated {
        remap_timeline_snapshot_after_hydration(
            &mut snap,
            &slot_orig_to_fresh,
            id_stride,
            target_snap_next,
            &mut fresh_cursor,
            min_safe_base,
        );
    }
    restore_det_rng_raw(saved_det_rng.0, saved_det_rng.1);

    let arc = Arc::new(snap);
    set_slot_snapshot(slot, Arc::clone(&arc));
    Some(arc)
}
