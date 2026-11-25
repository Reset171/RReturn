use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct MountMenuSnapshot {
    pub mob: u32,
    pub columns: usize,
    pub saddle: Option<ItemStack>,
    pub armor: Option<ItemStack>,
    pub items: Vec<Option<ItemStack>>,
}

pub trait MountMenuExt {
    fn rreturn_capture_mount_menu(&self) -> Option<MountMenuSnapshot>;
    fn rreturn_apply_mount_client_ui(&mut self, snap: Option<&MountMenuSnapshot>);
}

impl MountMenuExt for Game<'_> {
    fn rreturn_capture_mount_menu(&self) -> Option<MountMenuSnapshot> {
        let open = self.mount_menu.open.as_ref()?;
        Some(MountMenuSnapshot {
            mob: open.mob,
            columns: open.columns,
            saddle: open.saddle.clone(),
            armor: open.armor.clone(),
            items: open.items.clone(),
        })
    }

    fn rreturn_apply_mount_client_ui(&mut self, snap: Option<&MountMenuSnapshot>) {
        let Some(snap) = snap else {
            self.mount_menu.open = None;
            return;
        };
        if let Some(open) = self.mount_menu.open.as_mut()
            && open.mob == snap.mob
        {
            open.saddle = snap.saddle.clone();
            open.armor = snap.armor.clone();
            open.items = snap.items.clone();
            return;
        }
        if self.open_remote_mount_menu(snap.mob, snap.columns)
            && let Some(open) = self.mount_menu.open.as_mut()
        {
            open.saddle = snap.saddle.clone();
            open.armor = snap.armor.clone();
            open.items = snap.items.clone();
        }
    }
}
