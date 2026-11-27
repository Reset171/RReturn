use super::*;

/// Extension helper to run `tick_trial_spawner` with deterministic UUID allocation.
pub trait TrialSpawnerDetExt {
    fn rreturn_det_tick_trial_spawner(&mut self, key: (i32, i32, i32)) -> Option<Result<()>>;
}

impl TrialSpawnerDetExt for Game<'_> {
    fn rreturn_det_tick_trial_spawner(&mut self, key: (i32, i32, i32)) -> Option<Result<()>> {
        if !crate::game::autosave::rreturn::det_rng_active() {
            return None;
        }
        let Some((current, ominous)) = self.trial_spawner_block(key) else {
            return Some(Ok(()));
        };
        let Some((lease, BlockEntity::TrialSpawner(mut spawner))) =
            self.world.block_entities.take(&key)
        else {
            return Some(Ok(()));
        };
        let result = det_server_tick_trial_spawner(self, key, &mut spawner, current, ominous);
        self.world
            .block_entities
            .put(lease, BlockEntity::TrialSpawner(spawner));
        Some(result)
    }
}

fn next_det_uuid() -> Uuid {
    crate::game::autosave::rreturn::det_uuid().unwrap_or_else(horizon_client::save::random_uuid)
}

fn det_server_tick_trial_spawner(
    game: &mut Game<'_>,
    key: Pos,
    spawner: &mut TrialSpawner,
    current: TrialState,
    ominous: bool,
) -> Result<()> {
    let pos = [key.0, key.1, key.2];
    let mut random = std::mem::replace(
        &mut game.world.random,
        LevelRandom::for_level(LegacyRandom::new(0)),
    );
    let mut sequences = std::mem::take(&mut game.rand_sequences);
    let mut events = Vec::new();
    let mut planned: Vec<Planned> = Vec::new();
    let source = Some(mod_api::SpawnSource::Block(mod_api::BlockPos::new(
        key.0, key.1, key.2,
    )));
    let mut uuids: Vec<(u32, Uuid)> = Vec::new();
    let mut ejected: Vec<(ItemStack, [f64; 3])> = Vec::new();
    let extras: (Vec<Mob>, Vec<(u32, u32)>);
    let next = {
        let mut level = game.natural_level();
        level.spawner = Some(mod_api::Spawner::Trial {
            pos: mod_api::BlockPos::new(key.0, key.1, key.2),
        });
        let loot = game.server_loot.get();
        let seed = game.level.seed;
        let mut spawn = |l: &GameLevel, rng: &mut LevelRandom, r: &SpawnerSpawn| {
            let added = l.game.plan_spawner_mob(l, r, source, &mut planned, rng)?;
            let uuid = match planned.last_mut() {
                Some(Planned::Loaded(Tag::Compound(tag), _, _)) => {
                    let tag_uuid =
                        horizon_client::entity_nbt::uuid(&Tag::Compound(tag.clone()));
                    tag_uuid.unwrap_or_else(|| {
                        let uuid = next_det_uuid();
                        tag.insert("UUID".into(), Tag::IntArray(uuid.to_vec()));
                        uuid
                    })
                }
                _ => next_det_uuid(),
            };
            uuids.push((added.id, uuid));
            Some(uuid)
        };
        let mut eject = |table: &str, random: &mut LevelRandom| {
            let Some(loot) = &loot else {
                return false;
            };
            let drops = match loot.random_sequence(table) {
                Some(s) => loot.roll_split(table, sequences.get(s, seed)),
                None => loot.roll_split(table, random),
            };
            for stack in &drops {
                random.next_double();
                let spread = 0.017_227_5 * 2.0;
                let v = [
                    random.triangle(0.0, spread),
                    random.triangle(0.2, spread),
                    random.triangle(0.0, spread),
                ];
                ejected.push((
                    horizon_client::structure_blocks::loot_item_with_components(stack),
                    v,
                ));
            }
            !drops.is_empty()
        };
        let state = spawner.server_tick(
            &level,
            &mut random,
            pos,
            current,
            ominous,
            &mut spawn,
            &mut eject,
            &mut events,
        );
        extras = (
            std::mem::take(&mut *level.spawned.borrow_mut()),
            std::mem::take(&mut *level.rides.borrow_mut()),
        );
        state
    };
    game.world.random = random;
    game.rand_sequences = sequences;
    let (created, rides) = extras;
    let ids = game.add_planned(planned, created, rides);
    let mut by_uuid = HashMap::new();
    for (provisional, uuid) in uuids {
        if let Some(id) = ids.get(&provisional) {
            game.persist.uuids.insert(*id, uuid);
            by_uuid.insert(uuid, *id);
        }
    }
    let center = pos.map(|v| f64::from(v) + 0.5);
    let above = [center[0], center[1] + 0.7 - 0.125, center[2]];
    for (stack, v) in ejected {
        game.spawn_item_thrown(stack, above, v);
    }
    if next != current || ominous != spawner.is_ominous {
        game.set_trial_spawner_state(key, next, spawner.is_ominous)?;
    }
    for event in events {
        match event {
            TrialEvent::BlockUpdated => game.mark_block_entity_updated(key),
            TrialEvent::LevelEvent(id, at, data) => game.level_event_at(id, at, data),
            TrialEvent::Sound(name) => game
                .audio
                .play(&format!("minecraft:{name}"), Some(center), 1.0, 1.0),
            TrialEvent::OminousSet(_) => {}
            TrialEvent::EntityPlace(uuid, at) => {
                let who = by_uuid.get(&uuid).map(|id| Who::Mob(*id));
                game.post_block_game_event("minecraft:entity_place", at, who, None);
            }
            TrialEvent::TrialOmen(uuid) => game.bad_omen_to_trial_omen(uuid),
            TrialEvent::Discard(uuid) => {
                let (at, dropped) = {
                    let id = game.persist.uuids.iter().find(|(_, u)| **u == uuid);
                    if let Some((id, _)) = id
                        && let Some(m) = game.mobs.iter_mut().find(|m| m.id == *id)
                    {
                        let at = m.position;
                        let dropped = horizon_client::mobs::drops::drop_preserved_equipment(m);
                        m.removed = true;
                        (Some(at), dropped)
                    } else {
                        (None, Vec::new())
                    }
                };
                if let Some(at) = at {
                    for stack in dropped {
                        game.spawn_at_location(at, 0.0, stack);
                    }
                }
            }
            TrialEvent::SoundAt(name, at, pitch) => game.audio.play(
                &format!("minecraft:{name}"),
                Some(at.map(|v| f64::from(v) + 0.5)),
                1.0,
                pitch,
            ),
            TrialEvent::ItemSpawner {
                item,
                position,
                delay,
            } => game.add_ominous_item_spawner(item, position, delay),
        }
    }
    Ok(())
}
