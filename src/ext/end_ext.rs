use super::*;

impl EndState {
    pub fn rreturn_gateway_cooldowns(&self) -> HashMap<u32, i32> {
        self.gateways
            .iter()
            .map(|(&k, v)| (k, v.cooldown()))
            .collect()
    }

    pub fn rreturn_set_gateway_cooldowns(&mut self, cooldowns: &HashMap<u32, i32>) {
        for (&id, &cd) in cooldowns {
            let mut tr = horizon_client::end_gateway::Traveller::default();
            tr.set_cooldown(cd);
            self.gateways.insert(id, tr);
        }
    }
}