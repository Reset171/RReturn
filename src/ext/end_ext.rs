use std::collections::HashMap;
use super::*;

pub trait EndExt {
    fn rreturn_gateway_cooldowns(&self) -> HashMap<u32, i32>;
    fn rreturn_set_gateway_cooldowns(&mut self, cooldowns: &HashMap<u32, i32>);
}

impl EndExt for EndState {
    fn rreturn_gateway_cooldowns(&self) -> HashMap<u32, i32> {
        self.gateways
            .iter()
            .map(|(&k, v)| (k, v.cooldown()))
            .collect()
    }

    fn rreturn_set_gateway_cooldowns(&mut self, cooldowns: &HashMap<u32, i32>) {
        for (&id, &cd) in cooldowns {
            let mut tr = horizon_client::end_gateway::Traveller::default();
            tr.set_cooldown(cd);
            self.gateways.insert(id, tr);
        }
    }
}
