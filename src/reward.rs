use serde::{Deserialize, Serialize};

use crate::config::RewardConfig;
use crate::state::GameState;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OutcomeDelta {
    pub territory_delta_km2: i64,
    pub victory_points_delta: i32,
    pub factory_delta: i32,
    pub enemy_casualties: u64,
    pub own_casualties: u64,
    pub equipment_losses: u64,
    pub enemy_divisions_destroyed: u32,
    pub own_divisions_destroyed: u32,
    pub successful_encirclements: u32,
    pub wars_won: u32,
    pub wars_lost: u32,
    pub equipment_efficiency_delta: f32,
    pub supply_ratio: f32,
    pub failed_offensive: bool,
    pub manpower_ratio: f32,
}

impl OutcomeDelta {
    #[must_use]
    pub fn between(previous: &GameState, current: &GameState) -> Self {
        let previous_own_casualties = previous
            .wars
            .iter()
            .map(|war| war.own_casualties)
            .sum::<u64>();
        let current_own_casualties = current
            .wars
            .iter()
            .map(|war| war.own_casualties)
            .sum::<u64>();
        let previous_enemy_casualties = previous
            .wars
            .iter()
            .map(|war| war.enemy_casualties)
            .sum::<u64>();
        let current_enemy_casualties = current
            .wars
            .iter()
            .map(|war| war.enemy_casualties)
            .sum::<u64>();
        let territory_delta_km2 = current
            .fronts
            .iter()
            .map(|front| front.recent_territory_delta_km2)
            .sum();
        let previous_factories = total_factories(previous);
        let current_factories = total_factories(current);
        let supply_ratio = if current.fronts.is_empty() {
            1.0
        } else {
            current.fronts.iter().map(|front| front.supply).sum::<f32>()
                / usize_as_f32(current.fronts.len())
        };
        let equipment_losses = previous
            .economy
            .equipment
            .iter()
            .map(|old| {
                current
                    .economy
                    .equipment
                    .iter()
                    .find(|new| new.kind == old.kind)
                    .map_or(0, |new| old.stockpile.saturating_sub(new.stockpile).max(0))
            })
            .map(|loss| u64::try_from(loss).unwrap_or(u64::MAX))
            .sum();
        let manpower_ratio = if previous.country.manpower == 0 {
            1.0
        } else {
            ratio_u64(current.country.manpower, previous.country.manpower)
        };
        Self {
            territory_delta_km2,
            factory_delta: current_factories.saturating_sub(previous_factories),
            enemy_casualties: current_enemy_casualties.saturating_sub(previous_enemy_casualties),
            own_casualties: current_own_casualties.saturating_sub(previous_own_casualties),
            equipment_losses,
            supply_ratio,
            failed_offensive: previous.fronts.iter().any(|front| front.offensive_active)
                && territory_delta_km2 < 0,
            manpower_ratio,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RewardBreakdown {
    pub territory: f32,
    pub victory_points: f32,
    pub factories: f32,
    pub casualties: f32,
    pub equipment: f32,
    pub divisions: f32,
    pub encirclements: f32,
    pub wars: f32,
    pub logistics: f32,
    pub failed_offensive: f32,
    pub manpower: f32,
    pub total: f32,
}

#[must_use]
pub fn calculate(delta: &OutcomeDelta, weights: &RewardConfig) -> RewardBreakdown {
    let territory = if delta.territory_delta_km2 >= 0 {
        normalized(delta.territory_delta_km2.unsigned_abs(), 10_000) * weights.territory_gain
    } else {
        -normalized(delta.territory_delta_km2.unsigned_abs(), 10_000) * weights.territory_loss
    };
    let victory_points = signed_normalized(delta.victory_points_delta, 50) * weights.victory_points;
    let factories = if delta.factory_delta >= 0 {
        signed_normalized(delta.factory_delta, 20) * weights.factory_gain
    } else {
        signed_normalized(delta.factory_delta, 20) * weights.factory_loss
    };
    let casualties = normalized(delta.enemy_casualties, 100_000) * weights.enemy_losses
        - normalized(delta.own_casualties, 100_000) * weights.own_manpower_loss;
    let equipment = delta.equipment_efficiency_delta.clamp(-1.0, 1.0)
        * weights.equipment_efficiency
        - normalized(delta.equipment_losses, 50_000) * weights.equipment_loss;
    let divisions = normalized(u64::from(delta.enemy_divisions_destroyed), 10)
        * weights.enemy_divisions_destroyed
        - normalized(u64::from(delta.own_divisions_destroyed), 10)
            * weights.own_divisions_destroyed;
    let encirclements = normalized(u64::from(delta.successful_encirclements), 3)
        * weights.successful_encirclement
        - normalized(u64::from(delta.own_divisions_destroyed), 5) * weights.encirclement_penalty;
    let wars = normalized(u64::from(delta.wars_won), 1) * weights.war_won
        - normalized(u64::from(delta.wars_lost), 1) * weights.war_lost;
    let supply = delta.supply_ratio.clamp(0.0, 1.0);
    let logistics = supply * weights.stable_supply
        - if supply < 0.60 {
            (0.60 - supply) / 0.60 * weights.supply_penalty
        } else {
            0.0
        };
    let failed_offensive = if delta.failed_offensive {
        -weights.failed_offensive
    } else {
        0.0
    };
    let manpower = if delta.manpower_ratio < 0.10 {
        -(0.10 - delta.manpower_ratio.max(0.0)) / 0.10 * weights.manpower_exhaustion
    } else {
        0.0
    };
    let total = territory
        + victory_points
        + factories
        + casualties
        + equipment
        + divisions
        + encirclements
        + wars
        + logistics
        + failed_offensive
        + manpower;
    RewardBreakdown {
        territory,
        victory_points,
        factories,
        casualties,
        equipment,
        divisions,
        encirclements,
        wars,
        logistics,
        failed_offensive,
        manpower,
        total,
    }
}

fn normalized(value: u64, scale: u64) -> f32 {
    let capped = value.min(scale);
    let capped = u32::try_from(capped).unwrap_or(u32::MAX);
    let scale = u32::try_from(scale).unwrap_or(u32::MAX).max(1);
    f64_to_f32(f64::from(capped) / f64::from(scale))
}

fn signed_normalized(value: i32, scale: i32) -> f32 {
    let capped = value.clamp(-scale, scale);
    f64_to_f32(f64::from(capped) / f64::from(scale.max(1)))
}

fn f64_to_f32(value: f64) -> f32 {
    // All callers clamp values to [-1, 1], which is exactly representable enough
    // for the configurable heuristic reward model.
    #[allow(clippy::cast_possible_truncation)]
    let result = value as f32;
    result
}

fn total_factories(state: &GameState) -> i32 {
    let total = state
        .economy
        .civilian_factories
        .saturating_add(state.economy.military_factories)
        .saturating_add(state.economy.dockyards);
    i32::try_from(total).unwrap_or(i32::MAX)
}

fn usize_as_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn ratio_u64(numerator: u64, denominator: u64) -> f32 {
    f64_to_f32(u64_as_f64(numerator) / u64_as_f64(denominator.max(1)))
}

fn u64_as_f64(value: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let converted = value as f64;
    converted
}

#[cfg(test)]
mod tests {
    use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};
    use crate::config::AppConfig;

    use super::*;

    #[test]
    fn successful_breakthrough_is_positive() {
        let delta = OutcomeDelta {
            territory_delta_km2: 8_000,
            victory_points_delta: 20,
            factory_delta: 4,
            enemy_casualties: 80_000,
            own_casualties: 20_000,
            enemy_divisions_destroyed: 4,
            successful_encirclements: 1,
            equipment_efficiency_delta: 0.3,
            supply_ratio: 0.8,
            manpower_ratio: 0.5,
            ..OutcomeDelta::default()
        };
        assert!(calculate(&delta, &AppConfig::default().reward).total > 1.0);
    }

    #[test]
    fn encircled_failed_offensive_is_strongly_negative() {
        let delta = OutcomeDelta {
            territory_delta_km2: -6_000,
            own_casualties: 190_000,
            equipment_losses: 12_000,
            own_divisions_destroyed: 14,
            supply_ratio: 0.4,
            failed_offensive: true,
            manpower_ratio: 0.08,
            ..OutcomeDelta::default()
        };
        assert!(calculate(&delta, &AppConfig::default().reward).total < -3.0);
    }

    #[test]
    fn derives_casualty_and_factory_deltas_from_states() {
        let mut adapter = MockGameAdapter::new(MockScenario::StableFront);
        let previous = adapter.observe().expect("state");
        let mut current = previous.clone();
        current.wars[0].own_casualties += 10_000;
        current.wars[0].enemy_casualties += 20_000;
        current.economy.military_factories += 2;
        let delta = OutcomeDelta::between(&previous, &current);
        assert_eq!(delta.own_casualties, 10_000);
        assert_eq!(delta.enemy_casualties, 20_000);
        assert_eq!(delta.factory_delta, 2);
    }
}
