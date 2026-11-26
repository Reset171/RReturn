use super::*;
use horizon_client::items::ItemStack;
use horizon_protocol::packets::play::OpenScreen;
use std::cell::RefCell;

thread_local! {
    static REMOTE_CRAFT_TABLE_POS: RefCell<Option<(i32, [i32; 3])>> = const { RefCell::new(None) };
}

fn clear_remote_craft_table_pos() {
    REMOTE_CRAFT_TABLE_POS.with(|c| {
        *c.borrow_mut() = None;
    });
}

pub trait RemoteExt {
    fn rreturn_reset_client_state(&mut self);
    fn rreturn_clear_remote_menu(&mut self);
    fn rreturn_clear_transient_menu_state(&mut self);
    fn rreturn_set_use_target(&mut self, target: Option<[i32; 3]>);
}

pub trait GameRemoteContainerExt {
    fn rreturn_remote_close_local_only(&mut self);
    fn rreturn_on_remote_open_screen_pre(&self, p: &OpenScreen);
    fn rreturn_craft_table_pos(&self) -> Option<[i32; 3]>;
    fn rreturn_craft_result(&self) -> Option<ItemStack>;
    fn rreturn_set_craft_result(&mut self, item: Option<ItemStack>);
}

impl GameRemoteContainerExt for super::Game<'_> {
    fn rreturn_remote_close_local_only(&mut self) {
        clear_remote_craft_table_pos();
        self.remote_close_local();
    }

    fn rreturn_on_remote_open_screen_pre(&self, p: &OpenScreen) {
        let menu_name = self.remote.as_ref().and_then(|r| {
            r.builtin
                .get("minecraft:menu")?
                .name(p.menu)
                .map(str::to_owned)
        });
        if menu_name.as_deref() == Some("minecraft:crafting") {
            let pos = self.remote.as_ref().and_then(|r| {
                r.menu_extra
                    .as_ref()
                    .filter(|e| e.window == p.container)
                    .and_then(|e| e.pos)
                    .or(r.use_target)
            });
            REMOTE_CRAFT_TABLE_POS.with(|c| {
                *c.borrow_mut() = pos.map(|coords| (p.container, coords));
            });
        } else {
            clear_remote_craft_table_pos();
        }
    }

    fn rreturn_craft_table_pos(&self) -> Option<[i32; 3]> {
        if self.craft_table.is_some() {
            return self.craft_table;
        }
        if self.craft_size < 3 {
            return None;
        }
        let win = self
            .remote
            .as_ref()
            .and_then(|r| r.menu.as_ref())
            .filter(|(_, name)| name == "minecraft:crafting")
            .map(|(w, _)| *w)?;
        REMOTE_CRAFT_TABLE_POS.with(|c| {
            c.borrow()
                .filter(|(stored_w, _)| *stored_w == win)
                .map(|(_, pos)| pos)
        })
    }

    fn rreturn_craft_result(&self) -> Option<ItemStack> {
        self.remote.as_ref().and_then(|r| r.craft_result.clone())
    }

    fn rreturn_set_craft_result(&mut self, item: Option<ItemStack>) {
        if let Some(r) = self.remote.as_mut() {
            r.craft_result = item;
        }
    }
}

impl RemoteExt for Remote {
    fn rreturn_set_use_target(&mut self, target: Option<[i32; 3]>) {
        self.use_target = target;
    }

    fn rreturn_clear_remote_menu(&mut self) {
        clear_remote_craft_table_pos();
        self.menu = ClientMenu::default();
        self.craft_result = None;
        self.menu_title = None;
        self.menu_extra = None;
    }

    fn rreturn_clear_transient_menu_state(&mut self) {
        clear_remote_craft_table_pos();
        self.menu = ClientMenu::default();
        self.craft_result = None;
        self.menu_title = None;
        self.menu_extra = None;
        self.use_target = None;
        self.pending_keys.clear();
    }

    fn rreturn_reset_client_state(&mut self) {
        clear_remote_craft_table_pos();
        self.camera = None;
        self.use_item = None;
        self.predictions.clear();
        self.menu = ClientMenu::default();
        self.craft_result = None;
        self.menu_title = None;
        self.use_target = None;
        self.menu_extra = None;
        self.lerps.clear();
        self.cart_lerps.clear();
        self.stepped.clear();
        self.head_lerps.clear();
        self.ghast_dismount = None;
        self.pending_items.clear();
        self.flash_on_set_health = false;
        self.destroying = None;
        self.destroying_item = None;
        self.is_destroying = false;
        self.destroy_delay = 0;
        self.pending_keys.clear();
        self.swap_answer = [false; 2];
        self.stop_mining = false;
        self.placing = None;
        self.death_message = None;
        self.splash_sound = Default::default();
    }
}
