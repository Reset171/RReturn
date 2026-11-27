use super::*;
use horizon_core::nbt::Tag;

fn build_trader_data(spawn_delay: i32, chance: i32) -> Tag {
    Tag::compound([
        ("spawn_delay", Tag::Int(spawn_delay)),
        ("spawn_chance", Tag::Int(chance)),
    ])
}

fn extract_trader_pair(tag: Option<&Tag>) -> (i32, i32) {
    let data = tag.and_then(|t| t.get("data")).or(tag);
    let get = |k: &str, def: i32| match data.and_then(|d| d.get(k)) {
        Some(Tag::Int(v)) => *v,
        _ => def,
    };
    (get("spawn_delay", 24000), get("spawn_chance", 25))
}

fn advance_trader_tick_delay(spawner: &mut Spawner, target_tick_delay: i32) {
    let clamped = target_tick_delay.clamp(1, 1200);
    let steps = 1200 - clamped;
    for _ in 0..steps {
        let _ = spawner.due();
    }
}

/// Extension helpers to inspect and deterministically restore `creatures.trader` state.
pub trait WanderingTraderExt {
    fn rreturn_should_tick_trader(&self) -> bool;
    fn rreturn_restore_trader_disk(&mut self, tick_delay: i32);
    fn rreturn_restore_trader_scrub(
        &mut self,
        tag: Option<&Tag>,
        disk_tag: Option<&Tag>,
        tick_delay: i32,
        dirty: bool,
    );
}

impl WanderingTraderExt for Game<'_> {
    fn rreturn_should_tick_trader(&self) -> bool {
        self.dim == Dim::Overworld
            && self
                .ui
                .rules
                .bool_at(horizon_client::gamerules::vanilla::SPAWN_WANDERING_TRADERS)
    }

    fn rreturn_restore_trader_disk(&mut self, tick_delay: i32) {
        self.creatures.reset_trader();
        self.load_trader_data();
        advance_trader_tick_delay(&mut self.creatures.trader, tick_delay);
    }

    fn rreturn_restore_trader_scrub(
        &mut self,
        tag: Option<&Tag>,
        disk_tag: Option<&Tag>,
        tick_delay: i32,
        dirty: bool,
    ) {
        self.creatures.reset_trader();
        let effective = tag.or(disk_tag);
        if let Some(raw) = effective {
            let (spawn_delay, chance) = extract_trader_pair(Some(raw));
            if dirty && spawn_delay > 0 && spawn_delay <= 22800 {
                // Reloading with spawn_delay + 1200 and stepping 1200 dues restores the
                // exact internal `dirty = true` flag cleanly via public APIs.
                let pre = build_trader_data(spawn_delay + 1200, chance);
                self.creatures.trader.load(Some(&pre));
                for _ in 0..1200 {
                    let _ = self.creatures.trader.due();
                }
            } else {
                let exact = build_trader_data(spawn_delay, chance);
                self.creatures.trader.load(Some(&exact));
            }
        } else if dirty || tick_delay < 1200 {
            self.creatures.trader.load(None);
        }
        advance_trader_tick_delay(&mut self.creatures.trader, tick_delay);
    }
}
