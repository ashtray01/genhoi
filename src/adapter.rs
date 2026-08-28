use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::planner::GameAction;
use crate::state::{
    ArmyState, CountryState, DiplomacyState, EconomyState, FrontState, GameState, Terrain, WarState,
};

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("adapter has no observations remaining")]
    Exhausted,
    #[error("action execution is disabled: {0}")]
    ExecutionDisabled(String),
    #[error("adapter failure: {0}")]
    Other(String),
}

pub type AdapterResult<T> = Result<T, AdapterError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdapterStatus {
    Ready,
    Degraded,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdapterHealth {
    pub status: AdapterStatus,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionResult {
    pub accepted: bool,
    pub dry_run: bool,
    pub message: String,
}

pub trait GameAdapter {
    /// Returns the next normalized observation.
    ///
    /// # Errors
    ///
    /// Returns an adapter-specific error when the source is unavailable,
    /// exhausted, stale, or malformed.
    fn observe(&mut self) -> AdapterResult<GameState>;

    /// Attempts a constrained action through the adapter's safety gate.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::ExecutionDisabled`] unless execution was
    /// explicitly enabled, or an adapter-specific error if delivery fails.
    fn execute(&mut self, action: &GameAction) -> AdapterResult<ActionResult>;

    fn health(&self) -> AdapterHealth;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum MockScenario {
    StableFront,
    LowSupply,
    Breakthrough,
    DeepSalient,
    EncirclementRisk,
    EnemyCollapse,
}

impl MockScenario {
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::StableFront => "Eastern Front / Stable Front",
            Self::LowSupply => "Eastern Front / Low Supply",
            Self::Breakthrough => "Central Front / Breakthrough",
            Self::DeepSalient => "Eastern Front / Deep Salient",
            Self::EncirclementRisk => "Eastern Front / Encirclement Risk",
            Self::EnemyCollapse => "Western Front / Enemy Collapse",
        }
    }
}

pub struct MockGameAdapter {
    observations: VecDeque<GameState>,
    scenario: MockScenario,
    allow_execution: bool,
}

impl MockGameAdapter {
    #[must_use]
    pub fn new(scenario: MockScenario) -> Self {
        let observations = VecDeque::from([scenario_state(scenario)]);
        Self {
            observations,
            scenario,
            allow_execution: false,
        }
    }

    #[must_use]
    pub fn with_observations(observations: Vec<GameState>) -> Self {
        Self {
            observations: observations.into(),
            scenario: MockScenario::StableFront,
            allow_execution: false,
        }
    }

    pub fn set_execution_enabled(&mut self, enabled: bool) {
        self.allow_execution = enabled;
    }
}

impl GameAdapter for MockGameAdapter {
    fn observe(&mut self) -> AdapterResult<GameState> {
        self.observations.pop_front().ok_or(AdapterError::Exhausted)
    }

    fn execute(&mut self, action: &GameAction) -> AdapterResult<ActionResult> {
        if !self.allow_execution {
            return Err(AdapterError::ExecutionDisabled(
                "MockGameAdapter is observer-only by default".to_owned(),
            ));
        }
        Ok(ActionResult {
            accepted: true,
            dry_run: true,
            message: format!("mocked action {} for {}", action.kind, action.target_front),
        })
    }

    fn health(&self) -> AdapterHealth {
        AdapterHealth {
            status: AdapterStatus::Ready,
            name: "mock".to_owned(),
            detail: format!("synthetic scenario: {}", self.scenario.title()),
        }
    }
}

fn scenario_state(scenario: MockScenario) -> GameState {
    let mut front = base_front();
    match scenario {
        MockScenario::StableFront => {}
        MockScenario::LowSupply => {
            front.supply = 0.38;
            front.organization = 0.57;
            front.offensive_active = true;
        }
        MockScenario::Breakthrough => {
            "Central Front / Breakthrough".clone_into(&mut front.name);
            front.friendly_divisions = 48;
            front.enemy_estimated_divisions = 29;
            front.friendly_strength = 0.95;
            front.enemy_strength = 0.57;
            front.supply = 0.88;
            front.organization = 0.86;
            front.air_superiority = 0.77;
            front.enemy_pressure = 0.25;
            front.friendly_pressure = 0.84;
        }
        MockScenario::DeepSalient | MockScenario::EncirclementRisk => {
            scenario.title().clone_into(&mut front.name);
            front.friendly_divisions = 41;
            front.enemy_estimated_divisions = 36;
            front.friendly_strength = 0.76;
            front.enemy_strength = 0.79;
            front.organization = 0.66;
            front.supply = 0.43;
            front.salient_depth_km = 370.0;
            front.salient_neck_width_km = 55.0;
            front.enemy_pressure = 0.79;
            front.enemy_pressure_north = 0.81;
            front.enemy_pressure_south = 0.77;
            front.friendly_pressure = 0.52;
            front.nearby_reserve_divisions = 4;
            front.offensive_active = true;
        }
        MockScenario::EnemyCollapse => {
            "Western Front / Enemy Collapse".clone_into(&mut front.name);
            front.friendly_divisions = 52;
            front.enemy_estimated_divisions = 21;
            front.friendly_strength = 0.93;
            front.enemy_strength = 0.39;
            front.organization = 0.91;
            front.supply = 0.92;
            front.enemy_pressure = 0.08;
            front.enemy_pressure_north = 0.10;
            front.enemy_pressure_south = 0.12;
            front.friendly_pressure = 0.91;
            front.air_superiority = 0.86;
            front.equipment_ratio = 0.94;
        }
    }

    GameState {
        game_hour: 24,
        country: CountryState {
            tag: "GEN".to_owned(),
            name: "GenHOI Test Country".to_owned(),
            manpower: 1_250_000,
            political_power: 112.0,
            stability: 0.72,
            war_support: 0.81,
        },
        economy: EconomyState {
            civilian_factories: 46,
            military_factories: 72,
            dockyards: 12,
            fuel_ratio: 0.71,
            equipment: Vec::new(),
        },
        wars: vec![WarState {
            id: "mock_war".to_owned(),
            name: "Synthetic War".to_owned(),
            active: true,
            war_score: 0.08,
            own_casualties: 82_000,
            enemy_casualties: 103_000,
        }],
        armies: vec![ArmyState {
            id: "army_1".to_owned(),
            name: "First Army Group".to_owned(),
            divisions: front.friendly_divisions,
            reserve: false,
            average_strength: front.friendly_strength,
            average_organization: front.organization,
        }],
        fronts: vec![front],
        air_regions: Vec::new(),
        naval_regions: Vec::new(),
        diplomacy: DiplomacyState::default(),
        strategic_summary: scenario.title().to_owned(),
    }
}

fn base_front() -> FrontState {
    FrontState {
        id: "eastern_front_3".to_owned(),
        name: "Eastern Front / Stable Front".to_owned(),
        friendly_divisions: 40,
        enemy_estimated_divisions: 38,
        friendly_strength: 0.88,
        enemy_strength: 0.83,
        organization: 0.79,
        supply: 0.82,
        terrain: Terrain::Plains,
        front_width_km: 640.0,
        depth_km: 80.0,
        enemy_pressure: 0.38,
        enemy_pressure_north: 0.35,
        enemy_pressure_south: 0.41,
        friendly_pressure: 0.42,
        salient_depth_km: 35.0,
        salient_neck_width_km: 120.0,
        nearby_reserve_divisions: 8,
        recent_friendly_casualties: 3_200,
        recent_enemy_casualties: 3_600,
        recent_territory_delta_km2: 12,
        air_superiority: 0.58,
        equipment_ratio: 0.87,
        offensive_active: false,
    }
}

#[cfg(test)]
mod tests {
    use crate::metrics::FrontMetrics;
    use crate::planner::recommend;

    use super::*;

    #[test]
    fn execution_requires_explicit_enable() {
        let mut adapter = MockGameAdapter::new(MockScenario::StableFront);
        let state = adapter.observe().expect("mock observation");
        let metrics = FrontMetrics::calculate(&state.fronts[0], 10.0);
        let action = recommend(&state.fronts[0], &metrics).remove(0);
        let result = adapter.execute(&action);
        assert!(matches!(result, Err(AdapterError::ExecutionDisabled(_))));
    }
}
