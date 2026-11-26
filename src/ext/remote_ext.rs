use super::*;
use horizon_client::items::ItemStack;
use horizon_protocol::packets::play::OpenScreen;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

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
    fn rreturn_sync_player_heads(&mut self, uuids: &[u128]) -> HashMap<u128, String>;
    fn rreturn_set_remote_chat_lines(&mut self, lines: &[String]);
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

    fn rreturn_sync_player_heads(&mut self, uuids: &[u128]) -> HashMap<u128, String> {
        self.flush_skin_slots();
        let tab_skins: HashMap<u128, String> = self
            .remote_tab_players()
            .map(|(players, _, _)| players.into_iter().map(|p| (p.uuid, p.skin)).collect())
            .unwrap_or_default();
        let local_uuid = self.level.player_uuid.map_or(0, |u| {
            u.iter()
                .fold(0u128, |a, &w| (a << 32) | u128::from(w as u32))
        });

        let mut out = HashMap::with_capacity(uuids.len());
        for &uuid in uuids {
            let mut skin = if let Some(raw) = tab_skins.get(&uuid) {
                self.face_skin(uuid, raw)
            } else if uuid == local_uuid {
                if let Some((rgba, _)) = horizon_client::skins::offline_skin() {
                    const OFFLINE: &str = "horizon/offline_skin";
                    if !self.skin_slots.contains(OFFLINE)
                        || !self.gui.atlas.sprites.contains_key(OFFLINE)
                    {
                        self.store_skin(OFFLINE.into(), rgba.clone());
                    }
                    if self.gui.atlas.sprites.contains_key(OFFLINE) {
                        OFFLINE.to_owned()
                    } else {
                        horizon_client::tab_list::default_skin(uuid).to_owned()
                    }
                } else {
                    horizon_client::tab_list::default_skin(uuid).to_owned()
                }
            } else {
                horizon_client::tab_list::default_skin(uuid).to_owned()
            };
            if !self.gui.atlas.sprites.contains_key(&skin) {
                skin = horizon_client::tab_list::default_skin(uuid).to_owned();
            }
            out.insert(uuid, skin);
        }
        out
    }

    fn rreturn_set_remote_chat_lines(&mut self, lines: &[String]) {
        if let Some(r) = self.remote.as_mut() {
            let now = Instant::now();
            r.chat = lines.iter().map(|s| (s.clone(), now)).collect();
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
