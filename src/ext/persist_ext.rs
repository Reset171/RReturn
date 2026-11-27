use super::*;

/// Extension helpers for deterministic entity UUID generation and loading in `world::persist`.
pub trait PersistDetExt {
    fn rreturn_det_uuid_for(&mut self, id: u32) -> Option<[i32; 4]>;
    fn rreturn_det_saved_uuid_or_random(&self, id: u32) -> Option<[i32; 4]>;
    fn rreturn_det_load_dragon(&mut self, tag: &Tag, uuid: Option<[i32; 4]>) -> bool;
}

impl PersistDetExt for Game<'_> {
    fn rreturn_det_uuid_for(&mut self, id: u32) -> Option<[i32; 4]> {
        if !crate::game::autosave::rreturn::det_rng_active() {
            return None;
        }
        if let Some(uuid) = self.known_resident_entity_uuid_at(self.dim.key(), id) {
            return Some(uuid);
        }
        if let Some(&uuid) = self.persist.uuids.get(&id) {
            return Some(uuid);
        }
        let uuid = crate::game::autosave::rreturn::det_uuid().unwrap_or_else(random_uuid);
        self.persist.uuids.insert(id, uuid);
        Some(uuid)
    }

    fn rreturn_det_saved_uuid_or_random(&self, id: u32) -> Option<[i32; 4]> {
        if !crate::game::autosave::rreturn::det_rng_active() {
            return None;
        }
        Some(
            self.persist
                .uuids
                .get(&id)
                .copied()
                .unwrap_or_else(|| {
                    crate::game::autosave::rreturn::det_uuid().unwrap_or_else(random_uuid)
                }),
        )
    }

    fn rreturn_det_load_dragon(&mut self, tag: &Tag, uuid: Option<[i32; 4]>) -> bool {
        let Some(seed) = crate::game::autosave::rreturn::det_unique_seed() else {
            return false;
        };
        if self.dragon.is_none()
            && let Some(mut d) = crate::game::dragon::Dragon::from_nbt(tag, seed)
        {
            d.live
                .retain_supplier(&self.motion.attributes, "minecraft:ender_dragon");
            d.id = self.next_entity + 1;
            self.next_entity += 9;
            self.remember_tags(d.id, tag);
            self.remember_uuid(d.id, uuid, tag);
            d.join = horizon_client::join_seq::JoinSeq::next();
            assert!(self.dragon.install(d).is_ok());
        }
        true
    }
}
