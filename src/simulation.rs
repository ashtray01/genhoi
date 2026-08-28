use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};
use crate::config::AppConfig;
use crate::event::{AgentEvent, derive_events};
use crate::metrics::FrontMetrics;
use crate::planner::{GameAction, RiskLevel, classify_risk, recommend};
use crate::state::GameState;

#[derive(Debug, Serialize)]
pub struct SimulationReport {
    pub version: &'static str,
    pub scenario: String,
    pub observer_only: bool,
    pub state: GameState,
    pub metrics: FrontMetrics,
    pub risk_level: RiskLevel,
    pub events: Vec<AgentEvent>,
    pub recommended_actions: Vec<GameAction>,
}

/// Runs one complete deterministic mock observation and decision cycle.
///
/// # Errors
///
/// Returns an error when the mock observation is invalid or the agent is
/// disabled by configuration.
pub fn run(config: &AppConfig, scenario: MockScenario) -> Result<SimulationReport> {
    let mut adapter = MockGameAdapter::new(scenario);
    let state = adapter
        .observe()
        .context("mock adapter observation failed")?;
    analyze_state(config, state, scenario.title().to_owned())
}

/// Re-runs deterministic analysis for a normalized observation.
///
/// # Errors
///
/// Returns an error if the state contains no front or the agent is disabled.
pub fn analyze_state(
    config: &AppConfig,
    state: GameState,
    scenario: String,
) -> Result<SimulationReport> {
    let front = state
        .fronts
        .first()
        .context("observation contains no fronts")?;
    let metrics = FrontMetrics::calculate(front, config.risk.minimum_neck_width_km);
    let risk_level = classify_risk(metrics.encirclement_risk, &config.risk);
    let events = derive_events(None, &state, &config.risk);
    let mut recommended_actions = recommend(front, &metrics);
    recommended_actions.truncate(config.agent.maximum_actions_per_interval);

    if !config.agent.enabled {
        bail!("agent is disabled in configuration");
    }

    Ok(SimulationReport {
        version: env!("CARGO_PKG_VERSION"),
        scenario,
        observer_only: config.agent.observer_only,
        state,
        metrics,
        risk_level,
        events,
        recommended_actions,
    })
}

#[must_use]
pub fn format_human(report: &SimulationReport) -> String {
    let front = &report.state.fronts[0];
    let mut output = format!(
        "GENHOI {}\n\nMode: {}\n\nScenario:\n{}\n\nFriendly divisions: {}\nEnemy divisions: {}\n\nSupply: {:.0}%\nSalient depth: {:.0} km\nNeck width: {:.0} km\nSalient ratio: {:.2}\n\nEnemy pressure:\nNorth: {:.0}%\nSouth: {:.0}%\n\nCalculated encirclement risk:\n{:.0}% {}\n\nRecommended deterministic actions:\n",
        report.version,
        if report.observer_only {
            "OBSERVER-ONLY"
        } else {
            "EXECUTION ENABLED"
        },
        report.scenario,
        front.friendly_divisions,
        front.enemy_estimated_divisions,
        front.supply * 100.0,
        front.salient_depth_km,
        front.salient_neck_width_km,
        report.metrics.salient_ratio,
        front.enemy_pressure_north * 100.0,
        front.enemy_pressure_south * 100.0,
        report.metrics.encirclement_risk * 100.0,
        report.risk_level,
    );
    for (index, action) in report.recommended_actions.iter().enumerate() {
        write!(&mut output, "\n{}. {}", index + 1, action.kind)
            .expect("writing to a String cannot fail");
    }
    output.push('\n');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_simulation_matches_acceptance_scenario() {
        let report = run(&AppConfig::default(), MockScenario::DeepSalient).expect("simulation");
        let output = format_human(&report);
        assert!(output.contains("GENHOI 0.1.0-alpha.1"));
        assert!(output.contains("Supply: 43%"));
        assert!(output.contains("Salient ratio: 6.73"));
        assert!(output.contains("CRITICAL"));
        assert!(output.contains("1. STOP_OFFENSIVE"));
        assert!(output.contains("2. REINFORCE_CORRIDOR"));
        assert!(output.contains("3. WIDEN_FRONT"));
    }
}
