use super::*;

impl Remote {
    pub fn rreturn_reset_client_state(&mut self) {
        self.camera = None;
        self.use_item = None;
        self.predictions.clear();
        self.menu = None;
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
        self.destroying_item.clear();
        self.is_destroying = false;
        self.destroy_delay = 0;
        self.pending_keys.clear();
        self.swap_answer = [false; 2];
        self.stop_mining = false;
        self.placing = None;
        self.death_message = None;
    }
}