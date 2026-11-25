use super::*;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct StationUiSnapshot {
    pub kind: String,
    pub pos: [i32; 3],
    pub inputs: Vec<Option<ItemStack>>,
    pub result: Option<ItemStack>,
    pub anvil_name: String,
    pub rename_cursor: usize,
    pub rename_highlight: usize,
    pub selected: Option<usize>,
    pub start: usize,
    pub scroll: f32,
    pub beacon: [Option<usize>; 2],
    pub beacon_levels: Option<i32>,
    pub loom_selected: Option<usize>,
    pub loom_scroll: f32,
    pub loom_start_row: usize,
    pub merchant_mob: Option<u32>,
    pub merchant_scroll: usize,
    pub merchant_focused: Option<usize>,
    pub enchant_costs: [i32; 3],
    pub enchant_clues: [Option<(String, u32)>; 3],
}

fn kind_to_str(k: Kind) -> &'static str {
    match k {
        Kind::Enchanting => "enchanting",
        Kind::Anvil => "anvil",
        Kind::Grindstone => "grindstone",
        Kind::Smithing => "smithing",
        Kind::Stonecutter => "stonecutter",
        Kind::Merchant => "merchant",
        Kind::Beacon => "beacon",
        Kind::Cartography => "cartography",
        Kind::Loom => "loom",
    }
}

fn str_to_kind(s: &str) -> Option<Kind> {
    Some(match s {
        "enchanting" => Kind::Enchanting,
        "anvil" => Kind::Anvil,
        "grindstone" => Kind::Grindstone,
        "smithing" => Kind::Smithing,
        "stonecutter" => Kind::Stonecutter,
        "merchant" => Kind::Merchant,
        "beacon" => Kind::Beacon,
        "cartography" => Kind::Cartography,
        "loom" => Kind::Loom,
        _ => return None,
    })
}

pub trait StationsExt {
    fn rreturn_capture_station_ui(&self) -> Option<StationUiSnapshot>;
    fn rreturn_apply_station_client_ui(&mut self, snap: &StationUiSnapshot);
    fn rreturn_lan_reopen_station(&mut self, conn: u32, snap: &StationUiSnapshot) -> bool;
}

impl StationsExt for Game<'_> {
    fn rreturn_capture_station_ui(&self) -> Option<StationUiSnapshot> {
        let s = self.station.as_ref()?;
        let merchant = s.merchant.as_ref();
        Some(StationUiSnapshot {
            kind: kind_to_str(s.kind).to_owned(),
            pos: s.pos,
            inputs: s.inputs.clone(),
            result: s.result.clone(),
            anvil_name: s.name.clone(),
            rename_cursor: s.rename.edit.cursor,
            rename_highlight: s.rename.edit.highlight,
            selected: s.selected,
            start: s.start,
            scroll: s.scroll,
            beacon: s.beacon,
            beacon_levels: s.beacon_levels,
            loom_selected: s.loom.selected,
            loom_scroll: s.loom.scroll,
            loom_start_row: s.loom.start_row,
            merchant_mob: merchant.and_then(|m| m.mob),
            merchant_scroll: merchant.map(|m| m.scroll).unwrap_or(0),
            merchant_focused: merchant.and_then(|m| m.focused),
            enchant_costs: s.offers.costs,
            enchant_clues: [
                s.offers.clues[0].as_ref().map(|(id, lvl)| (id.to_string(), *lvl)),
                s.offers.clues[1].as_ref().map(|(id, lvl)| (id.to_string(), *lvl)),
                s.offers.clues[2].as_ref().map(|(id, lvl)| (id.to_string(), *lvl)),
            ],
        })
    }

    fn rreturn_apply_station_client_ui(&mut self, snap: &StationUiSnapshot) {
        let Some(kind) = str_to_kind(&snap.kind) else {
            return;
        };
        let needs_new = self
            .station
            .as_ref()
            .is_none_or(|s| s.kind != kind || s.pos != snap.pos);
        if needs_new {
            self.station = Some(Station::new_kind(kind, snap.pos));
        }
        if let Some(s) = self.station.as_mut() {
            let len = s.inputs.len();
            for i in 0..len {
                s.inputs[i] = snap.inputs.get(i).cloned().flatten();
            }
            s.result = snap.result.clone();
            s.name = snap.anvil_name.clone();
            s.rename.set_value(&snap.anvil_name);
            let max_c = s.rename.edit.value.chars().count();
            s.rename.edit.cursor = snap.rename_cursor.min(max_c);
            s.rename.edit.highlight = snap.rename_highlight.min(max_c);
            s.anvil_last = s.inputs.first().cloned().flatten();
            if kind == Kind::Anvil && s.inputs.first().is_some_and(Option::is_some) {
                s.rename.focused = true;
            }
            s.offers.costs = snap.enchant_costs;
            s.offers.clues = [
                snap.enchant_clues[0].as_ref().map(|(id, lvl)| (Arc::from(id.as_str()), *lvl)),
                snap.enchant_clues[1].as_ref().map(|(id, lvl)| (Arc::from(id.as_str()), *lvl)),
                snap.enchant_clues[2].as_ref().map(|(id, lvl)| (Arc::from(id.as_str()), *lvl)),
            ];
            s.beacon = snap.beacon;
            if snap.beacon_levels.is_some() {
                s.beacon_levels = snap.beacon_levels;
            }
            if let Some(m) = s.merchant.as_mut() {
                m.scroll = snap.merchant_scroll;
                m.focused = snap.merchant_focused;
            }
        }
        if kind != Kind::Enchanting && kind != Kind::Merchant {
            self.station_changed();
        }
        if let Some(s) = self.station.as_mut() {
            s.selected = snap.selected;
            s.start = snap.start;
            s.scroll = snap.scroll;
            s.loom.selected = snap.loom_selected;
            s.loom.scroll = snap.loom_scroll;
            s.loom.start_row = snap.loom_start_row;
            if snap.result.is_some() && kind != Kind::Anvil {
                s.result = snap.result.clone();
            }
        }
    }

    fn rreturn_lan_reopen_station(&mut self, conn: u32, snap: &StationUiSnapshot) -> bool {
        let Some(kind) = str_to_kind(&snap.kind) else {
            return false;
        };
        if kind == Kind::Merchant {
            let Some(mob_id) = snap.merchant_mob else {
                return false;
            };
            let Some(index) = self.mobs.iter().position(|m| m.id == mob_id) else {
                return false;
            };
            let saved_station = self.station.take();
            let saved_screen = self.screen;
            let opened = self.interact_trader(index, "").unwrap_or(false);
            let generated = if opened { self.station.take() } else { None };
            self.station = saved_station;
            self.screen = saved_screen;
            if let Some(st) = generated {
                self.lan_station_opened(conn, st);
                return true;
            }
            return false;
        }
        let Some(Some(window)) = self.lan_open_station_at(conn, kind, snap.pos, snap.inputs.clone()) else {
            return false;
        };
        if kind == Kind::Anvil && !snap.anvil_name.is_empty() {
            self.lan_station_rename(conn, &snap.anvil_name);
        }
        if kind == Kind::Stonecutter
            && let Some(sel) = snap.selected
        {
            self.lan_station_button(conn, window, sel as i32);
        }
        if kind == Kind::Loom
            && let Some(sel) = snap.loom_selected
        {
            self.lan_station_button(conn, window, sel as i32);
        }
        true
    }
}
