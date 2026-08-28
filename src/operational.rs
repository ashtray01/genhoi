use serde::{Deserialize, Serialize};

use crate::metrics::FrontMetrics;
use crate::planner::{GameAction, recommend};
use crate::state::GameState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrontAssessment {
    pub front_id: String,
    pub priority: f32,
    pub requested_reserve_divisions: u32,
    pub metrics: FrontMetrics,
    pub actions: Vec<GameAction>,
}

#[derive(Debug, Clone, Copy)]
pub struct OperationalBrain {
    minimum_neck_width_km: f32,
}

impl OperationalBrain {
    #[must_use]
    pub fn new(minimum_neck_width_km: f32) -> Self {
        Self {
            minimum_neck_width_km,
        }
    }

    #[must_use]
    pub fn assess(&self, state: &GameState) -> Vec<FrontAssessment> {
        let mut assessments = state
            .fronts
            .iter()
            .map(|front| {
                let metrics = FrontMetrics::calculate(front, self.minimum_neck_width_km);
                let priority = (0.45 * metrics.encirclement_risk
                    + 0.25 * (1.0 - metrics.front_stability)
                    + 0.20 * (1.0 - metrics.supply_score)
                    + 0.10 * (1.0 - metrics.reserve_strength))
                    .clamp(0.0, 1.0);
                let requested_reserve_divisions = if priority >= 0.80 {
                    8
                } else if priority >= 0.60 {
                    4
                } else if metrics.reserve_strength < 0.10 {
                    2
                } else {
                    0
                };
                FrontAssessment {
                    front_id: front.id.clone(),
                    priority,
                    requested_reserve_divisions,
                    metrics,
                    actions: recommend(front, &metrics),
                }
            })
            .collect::<Vec<_>>();
        assessments.sort_by(|left, right| right.priority.total_cmp(&left.priority));
        assessments
    }
}

#[cfg(test)]
mod tests {
    use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};

    use super::*;

    #[test]
    fn dangerous_front_requests_reserves() {
        let mut adapter = MockGameAdapter::new(MockScenario::DeepSalient);
        let state = adapter.observe().expect("state");
        let assessment = OperationalBrain::new(10.0).assess(&state).remove(0);
        assert!(assessment.priority > 0.70);
        assert!(assessment.requested_reserve_divisions >= 4);
    }
}
