use super::*;

/// Extension helpers to snapshot and synchronize first-person visible hand state.
pub trait HandExt {
    /// Returns the current first-person visible main/off-hand items (`visible_hands`) and replacement flags.
    fn rreturn_hand_visual_state(
        &self,
    ) -> ([(usize, Option<ItemStack>); 2], [bool; 2]);
    /// Restores captured first-person visible hand stacks and replacement flags immediately.
    fn rreturn_set_hand_visual_state(
        &mut self,
        visible: [(usize, Option<ItemStack>); 2],
        replaced: [bool; 2],
    );
    /// Synchronizes `visible_hands` directly to current `inventory[selected]` and `offhand` with full heights.
    fn rreturn_sync_hands_to_inventory(&mut self);
}

impl HandExt for Game<'_> {
    fn rreturn_hand_visual_state(
        &self,
    ) -> ([(usize, Option<ItemStack>); 2], [bool; 2]) {
        (self.visible_hands.clone(), self.hands_replaced)
    }

    fn rreturn_set_hand_visual_state(
        &mut self,
        visible: [(usize, Option<ItemStack>); 2],
        replaced: [bool; 2],
    ) {
        self.visible_hands = visible;
        self.hands_replaced = replaced;
    }

    fn rreturn_sync_hands_to_inventory(&mut self) {
        self.reset_studio_hand_state();
    }
}
