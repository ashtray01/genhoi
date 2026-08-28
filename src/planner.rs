use serde::{Deserialize, Serialize};

use crate::config::RiskConfig;
use crate::metrics::FrontMetrics;
use crate::state::FrontState;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GameActionKind {
    StopOffensive,
    ReinforceCorridor,
    WidenFront,
    Hold,
    Redeploy,
    ConcentrateArmor,
    Attack,
}

impl std::fmt::Display for GameActionKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = serde_json::to_value(self).map_err(|_| std::fmt::Error)?;
        formatter.write_str(value.as_str().ok_or(std::fmt::Error)?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameAction {
    pub kind: GameActionKind,
    pub target_front: String,
    pub confidence: f32,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum RiskLevel {
    Low,
    Elevated,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Low => "LOW",
            Self::Elevated => "ELEVATED",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        })
    }
}

#[must_use]
pub fn classify_risk(value: f32, thresholds: &RiskConfig) -> RiskLevel {
    if value >= thresholds.critical {
        RiskLevel::Critical
    } else if value >= thresholds.high {
        RiskLevel::High
    } else if value >= thresholds.high * 0.6 {
        RiskLevel::Elevated
    } else {
        RiskLevel::Low
    }
}

#[must_use]
pub fn recommend(front: &FrontState, metrics: &FrontMetrics) -> Vec<GameAction> {
    let mut actions = Vec::new();
    if metrics.encirclement_risk >= 0.70 {
        actions.push(action(
            GameActionKind::StopOffensive,
            front,
            metrics.encirclement_risk,
            "Encirclement risk exceeds the deterministic safety threshold.",
        ));
        actions.push(action(
            GameActionKind::ReinforceCorridor,
            front,
            (metrics.encirclement_risk + 0.05).min(1.0),
            "Both shoulders are pressured and nearby reserves are insufficient.",
        ));
        if metrics.salient_ratio >= 3.0 {
            actions.push(action(
                GameActionKind::WidenFront,
                front,
                metrics.salient_risk,
                "The salient is deep relative to its neck width.",
            ));
        }
    } else if metrics.supply_score < 0.55 {
        actions.push(action(
            GameActionKind::Hold,
            front,
            1.0 - metrics.supply_score,
            "Supply is too low for a sustainable offensive.",
        ));
        actions.push(action(
            GameActionKind::Redeploy,
            front,
            0.65,
            "Reduce local demand and improve supply efficiency.",
        ));
    } else if metrics.offensive_potential >= 0.72 && metrics.force_ratio >= 1.20 {
        actions.push(action(
            GameActionKind::ConcentrateArmor,
            front,
            metrics.offensive_potential,
            "The sector has sufficient force, organization, equipment and supply.",
        ));
        actions.push(action(
            GameActionKind::Attack,
            front,
            metrics.offensive_potential,
            "A bounded offensive is favored by the deterministic metrics.",
        ));
    } else {
        actions.push(action(
            GameActionKind::Hold,
            front,
            metrics.defensive_potential,
            "No decisive low-risk opportunity is present.",
        ));
    }
    actions
}

fn action(
    kind: GameActionKind,
    front: &FrontState,
    confidence: f32,
    rationale: &str,
) -> GameAction {
    GameAction {
        kind,
        target_front: front.id.clone(),
        confidence: confidence.clamp(0.0, 1.0),
        rationale: rationale.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};

    use super::*;

    #[test]
    fn dangerous_offensive_is_stopped() {
        let mut adapter = MockGameAdapter::new(MockScenario::EncirclementRisk);
        let state = adapter.observe().expect("mock observation");
        let metrics = FrontMetrics::calculate(&state.fronts[0], 10.0);
        let actions = recommend(&state.fronts[0], &metrics);
        assert_eq!(actions[0].kind, GameActionKind::StopOffensive);
        assert!(
            !actions
                .iter()
                .any(|item| item.kind == GameActionKind::Attack)
        );
    }
}
