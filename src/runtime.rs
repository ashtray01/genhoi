use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::adapter::{AdapterError, AdapterResult, GameAdapter};
use crate::config::AppConfig;
use crate::event::{AgentEvent, derive_events};
use crate::operational::{FrontAssessment, OperationalBrain};
use crate::reasoner::{StrategicContext, StrategicDecision, StrategicReasoner};
use crate::reward::{OutcomeDelta, RewardBreakdown, calculate};
use crate::scheduler::{ReasoningScheduler, StrategicSituation};
use crate::state::GameState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeCycle {
    pub state: GameState,
    pub events: Vec<AgentEvent>,
    pub assessments: Vec<FrontAssessment>,
    pub strategic_decision: Option<StrategicDecision>,
    pub reward: Option<RewardBreakdown>,
}

pub struct AgentRuntime<A, R> {
    adapter: A,
    reasoner: R,
    config: AppConfig,
    scheduler: ReasoningScheduler,
    previous: Option<GameState>,
}

impl<A: GameAdapter, R: StrategicReasoner> AgentRuntime<A, R> {
    #[must_use]
    pub fn new(adapter: A, reasoner: R, config: AppConfig) -> Self {
        let scheduler = ReasoningScheduler::new(config.reasoning.clone());
        Self {
            adapter,
            reasoner,
            config,
            scheduler,
            previous: None,
        }
    }

    /// Processes one observation and invokes strategic reasoning only when due.
    ///
    /// # Errors
    ///
    /// Propagates adapter errors and converts reasoner failures into an adapter
    /// failure so the caller can stop or fall back explicitly.
    pub fn tick(&mut self, elapsed: Duration) -> AdapterResult<RuntimeCycle> {
        let state = self.adapter.observe()?;
        let events = derive_events(self.previous.as_ref(), &state, &self.config.risk);
        let reward = self.previous.as_ref().map(|previous| {
            calculate(
                &OutcomeDelta::between(previous, &state),
                &self.config.reward,
            )
        });
        let assessments =
            OperationalBrain::new(self.config.risk.minimum_neck_width_km).assess(&state);
        let situation = classify_situation(&state, &assessments);
        let strategic_decision = if self.scheduler.due(elapsed, situation, &events) {
            let decision = if let Some(front) = assessments.first() {
                let name = state
                    .fronts
                    .iter()
                    .find(|candidate| candidate.id == front.front_id)
                    .map_or_else(
                        || front.front_id.clone(),
                        |candidate| candidate.name.clone(),
                    );
                let context = StrategicContext {
                    at_war: state.at_war(),
                    front_id: front.front_id.clone(),
                    front_name: name,
                    metrics: front.metrics,
                    recent_experiences: vec![],
                };
                Some(
                    self.reasoner
                        .reason(&context)
                        .map_err(|error| AdapterError::Other(error.to_string()))?,
                )
            } else {
                None
            };
            self.scheduler.mark_reasoned(elapsed);
            decision
        } else {
            None
        };
        self.previous = Some(state.clone());
        Ok(RuntimeCycle {
            state,
            events,
            assessments,
            strategic_decision,
            reward,
        })
    }

    #[must_use]
    pub fn adapter(&self) -> &A {
        &self.adapter
    }

    pub fn reasoner_mut(&mut self) -> &mut R {
        &mut self.reasoner
    }
}

fn classify_situation(state: &GameState, assessments: &[FrontAssessment]) -> StrategicSituation {
    if assessments
        .iter()
        .any(|front| front.metrics.encirclement_risk >= 0.85)
    {
        StrategicSituation::Critical
    } else if state.fronts.iter().any(|front| front.offensive_active) {
        StrategicSituation::ActiveOffensive
    } else if state.at_war() {
        StrategicSituation::War
    } else {
        StrategicSituation::Peace
    }
}

#[cfg(test)]
mod tests {
    use crate::adapter::{MockGameAdapter, MockScenario};
    use crate::reasoner::{RuleBasedReasoner, StrategicIntent};

    use super::*;

    #[test]
    fn first_critical_tick_reasons_immediately() {
        let adapter = MockGameAdapter::new(MockScenario::DeepSalient);
        let mut runtime = AgentRuntime::new(adapter, RuleBasedReasoner, AppConfig::default());
        let cycle = runtime.tick(Duration::ZERO).expect("cycle");
        assert_eq!(
            cycle.strategic_decision.expect("decision").intent,
            StrategicIntent::StabilizeFront
        );
        assert!(!cycle.events.is_empty());
    }
}
