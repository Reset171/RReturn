fn hook_det_monster_brain(
    mob: &mut horizon_client::mobs::Mob,
    seed: i64,
    op: impl FnOnce(&mut horizon_client::mobs::Mob, i64) -> (),
) {
    let final_seed = if crate::game::autosave::rreturn::det_rng_active() {
        crate::game::autosave::rreturn::det_mob_brain_seed(mob.id)
    } else {
        seed
    };
    op(mob, final_seed);
}
