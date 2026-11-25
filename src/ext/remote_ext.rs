use super::*;

pub trait RemoteExt {
    fn rreturn_reset_client_state(&mut self);
    fn rreturn_clear_remote_menu(&mut self);
    fn rreturn_set_use_target(&mut self, target: Option<[i32; 3]>);
}

pub trait GameRemoteContainerExt {
    fn rreturn_remote_close_local_only(&mut self);
}

impl GameRemoteContainerExt for super::Game<'_> {
    fn rreturn_remote_close_local_only(&mut self) {
        self.remote_close_local();
    }
}

impl RemoteExt for Remote {
    fn rreturn_set_use_target(&mut self, target: Option<[i32; 3]>) {
        self.use_target = target;
    }

    fn rreturn_clear_remote_menu(&mut self) {
        self.menu = ClientMenu::default();
        self.craft_result = None;
        self.menu_title = None;
        self.menu_extra = None;
    }

    fn rreturn_reset_client_state(&mut self) {
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
