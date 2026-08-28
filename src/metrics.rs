use serde::{Deserialize, Serialize};

use crate::state::FrontState;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct FrontMetrics {
    pub force_ratio: f32,
    pub supply_score: f32,
    pub front_stability: f32,
    pub offensive_potential: f32,
    pub defensive_potential: f32,
    pub encirclement_risk: f32,
    pub salient_risk: f32,
    pub salient_ratio: f32,
    pub reserve_strength: f32,
    pub air_support_score: f32,
    pub equipment_shortage_score: f32,
}

impl FrontMetrics {
    #[must_use]
    pub fn calculate(front: &FrontState, minimum_neck_width_km: f32) -> Self {
        let friendly_strength = front.friendly_strength.max(0.01);
        let enemy_strength = front.enemy_strength.max(0.01);
        let force_ratio = friendly_strength / enemy_strength;
        let supply_score = unit(front.supply);
        let organization = unit(front.organization);
        let air_support_score = unit(front.air_superiority);
        let equipment_ratio = unit(front.equipment_ratio);
        let equipment_shortage_score = 1.0 - equipment_ratio;
        let reserve_strength = unit(
            count_as_f32(front.nearby_reserve_divisions)
                / count_as_f32(front.friendly_divisions.max(1)),
        );
        let salient_ratio = if front.salient_depth_km <= 0.0 {
            0.0
        } else {
            front.salient_depth_km / front.salient_neck_width_km.max(minimum_neck_width_km)
        };
        let salient_risk = unit((salient_ratio - 1.0) / 4.0);
        let flank_pressure = unit(f32::midpoint(
            front.enemy_pressure_north,
            front.enemy_pressure_south,
        ));
        let general_pressure = unit(front.enemy_pressure);

        // The interaction bonus is deliberate: a deep, poorly supplied salient under
        // pressure on both shoulders is much more dangerous than the sum of its parts.
        let interaction = if salient_ratio >= 4.0
            && supply_score < 0.55
            && front.enemy_pressure_north > 0.70
            && front.enemy_pressure_south > 0.70
        {
            0.12
        } else {
            0.0
        };
        let encirclement_risk = unit(
            0.45 * salient_risk
                + 0.25 * flank_pressure
                + 0.10 * general_pressure
                + 0.15 * (1.0 - supply_score)
                + 0.05 * (1.0 - organization)
                + interaction,
        );

        let normalized_force = unit(force_ratio / 1.5);
        let offensive_potential = unit(
            0.30 * normalized_force
                + 0.25 * supply_score
                + 0.20 * organization
                + 0.15 * equipment_ratio
                + 0.10 * air_support_score
                - 0.35 * encirclement_risk,
        );
        let defensive_potential = unit(
            0.25 * normalized_force
                + 0.20 * supply_score
                + 0.25 * organization
                + 0.15 * equipment_ratio
                + 0.15 * reserve_strength,
        );
        let front_stability = unit(
            1.0 - 0.35 * encirclement_risk
                - 0.25 * general_pressure
                - 0.20 * (1.0 - supply_score)
                - 0.10 * (1.0 - organization)
                - 0.10 * equipment_shortage_score,
        );

        Self {
            force_ratio,
            supply_score,
            front_stability,
            offensive_potential,
            defensive_potential,
            encirclement_risk,
            salient_risk,
            salient_ratio,
            reserve_strength,
            air_support_score,
            equipment_shortage_score,
        }
    }

    #[must_use]
    pub fn feature_vector(self) -> [f32; 9] {
        [
            unit(self.force_ratio / 2.0),
            self.supply_score,
            self.front_stability,
            unit(self.salient_ratio / 8.0),
            self.encirclement_risk,
            self.defensive_potential,
            self.air_support_score,
            self.reserve_strength,
            1.0 - self.equipment_shortage_score,
        ]
    }
}

fn unit(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn count_as_f32(value: u32) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

#[cfg(test)]
mod tests {
    use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};

    use super::*;

    #[test]
    fn deep_salient_is_critical() {
        let mut adapter = MockGameAdapter::new(MockScenario::DeepSalient);
        let state = adapter.observe().expect("mock observation");
        let metrics = FrontMetrics::calculate(&state.fronts[0], 10.0);
        assert!(metrics.salient_ratio > 6.7);
        assert!(metrics.encirclement_risk >= 0.90);
        assert!(metrics.offensive_potential < metrics.defensive_potential);
    }

    #[test]
    fn stable_front_has_low_risk() {
        let mut adapter = MockGameAdapter::new(MockScenario::StableFront);
        let state = adapter.observe().expect("mock observation");
        let metrics = FrontMetrics::calculate(&state.fronts[0], 10.0);
        assert!(metrics.encirclement_risk < 0.35);
    }
}
