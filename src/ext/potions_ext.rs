use super::*;

#[derive(Clone, Debug, Default)]
pub struct ThrownPotionsSnapshot {
    thrown: Vec<ThrownPotion>,
}

impl ThrownPotionsSnapshot {
    pub fn is_empty(&self) -> bool {
        self.thrown.is_empty()
    }

    pub fn merge_from(&mut self, current: &Self) {
        for t in &current.thrown {
            if let Some(slot) = self.thrown.iter_mut().find(|existing| existing.id == t.id) {
                *slot = t.clone();
            } else {
                self.thrown.push(t.clone());
            }
        }
    }

    pub fn clear(&mut self) {
        self.thrown.clear();
    }
}

pub trait PotionStateExt {
    fn rreturn_capture_thrown(&self) -> ThrownPotionsSnapshot;
    fn rreturn_restore_thrown(&mut self, snap: &ThrownPotionsSnapshot);
    fn rreturn_thrown_poses(&self) -> Vec<(u32, [f64; 3], [f64; 3], u32)>;
    fn rreturn_apply_thrown_preview(
        &mut self,
        pool: &ThrownPotionsSnapshot,
        poses: &[(u32, [f64; 3], u32)],
    );
}

impl PotionStateExt for PotionState {
    fn rreturn_capture_thrown(&self) -> ThrownPotionsSnapshot {
        ThrownPotionsSnapshot {
            thrown: self.thrown.clone(),
        }
    }

    fn rreturn_restore_thrown(&mut self, snap: &ThrownPotionsSnapshot) {
        self.thrown = snap.thrown.clone();
    }

    fn rreturn_thrown_poses(&self) -> Vec<(u32, [f64; 3], [f64; 3], u32)> {
        self.thrown
            .iter()
            .map(|t| {
                (
                    t.id,
                    [t.position.x, t.position.y, t.position.z],
                    [t.velocity.x, t.velocity.y, t.velocity.z],
                    t.age,
                )
            })
            .collect()
    }

    fn rreturn_apply_thrown_preview(
        &mut self,
        pool: &ThrownPotionsSnapshot,
        poses: &[(u32, [f64; 3], u32)],
    ) {
        self.thrown.clear();
        for &(id, pos, age) in poses {
            if let Some(template) = pool.thrown.iter().find(|t| t.id == id) {
                let mut t = template.clone();
                let v = Vec3::new(pos[0], pos[1], pos[2]);
                t.position = v;
                t.previous = v;
                t.velocity = Vec3::ZERO;
                t.age = age;
                self.thrown.push(t);
            }
        }
    }
}