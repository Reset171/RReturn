use super::*;

/// Extension helpers for cleaning up server-side `LanHost.menus` entries during rollback.
pub trait LanMenusExt {
    /// Removes all tracked guest menus for `conn` and returns their open window IDs.
    fn rreturn_clear_guest_menus(&mut self, conn: u32) -> Vec<i32>;
}

impl LanMenusExt for LanHost {
    fn rreturn_clear_guest_menus(&mut self, conn: u32) -> Vec<i32> {
        let mut windows = Vec::new();
        if let Some(mut menus) = self.forget_menus(conn) {
            if let Some(table) = menus.crafting.table.take() {
                windows.push(table.0);
            }
            if let Some(machine) = menus.crafting.machine.take() {
                windows.push(machine.0);
            }
            if let Some(st) = menus.station.take() {
                windows.push(st.window);
            }
        }
        windows
    }
}
