use super::super::{Dim, Game};
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
    );

    let raids_dat = fs::read(raids_dat_path(&cp_dir, dim)).ok();

    let hydrated_random = chunks_res
        .level_rng
        .map(|st| LevelRandom::for_level(LegacyRandom::from_state(st)))
        .unwrap_or_else(|| game.world.random.clone());
    let hydrated_rtv = chunks_res
        .random_tick_value
        .unwrap_or(game.block_ticks.random_tick_value);

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
        chunks: chunks_res.chunks,
        block_entities: chunks_res.block_entities,
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
        knot_ids: entities_res.knot_ids,
        persist_uuids: entities_res.persist_uuids,
        persist_raw: entities_res.persist_raw,
        persist_passenger_slots: entities_res.persist_passenger_slots,
        raids_dat,
        last_hurt_by_player: entities_res.last_hurt_by_player,
        love_cause: entities_res.love_cause,
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
