use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::adapter::{ActionResult, AdapterError, AdapterResult, GameAdapter};
use crate::config::AgentConfig;
use crate::metrics::FrontMetrics;
use crate::planner::{GameAction, GameActionKind};
use crate::state::GameState;

#[derive(Debug, Clone, Default)]
pub struct PauseSwitch {
    paused: Arc<AtomicBool>,
}

impl PauseSwitch {
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }
}

#[derive(Debug)]
pub struct SafetyExecutor {
    config: AgentConfig,
    pause: PauseSwitch,
    actions_this_interval: usize,
}

impl SafetyExecutor {
    #[must_use]
    pub fn new(config: AgentConfig, pause: PauseSwitch) -> Self {
        Self {
            config,
            pause,
            actions_this_interval: 0,
        }
    }

    pub fn begin_interval(&mut self) {
        self.actions_this_interval = 0;
    }

    /// Validates all global and tactical safety gates, then delegates one action.
    ///
    /// # Errors
    ///
    /// Rejects paused, observer-only, disabled, rate-limited or tactically unsafe
    /// actions. Adapter errors are propagated unchanged.
    pub fn execute<A: GameAdapter>(
        &mut self,
        adapter: &mut A,
        action: &GameAction,
        state: &GameState,
        minimum_neck_width_km: f32,
    ) -> AdapterResult<ActionResult> {
        self.validate_global_gates()?;
        validate_tactical_gate(action, state, minimum_neck_width_km)?;
        if self.config.dry_run {
            self.actions_this_interval = self.actions_this_interval.saturating_add(1);
            return Ok(ActionResult {
                accepted: false,
                dry_run: true,
                message: format!("validated dry-run action {}", action.kind),
            });
        }
        let result = adapter.execute(action)?;
        self.actions_this_interval = self.actions_this_interval.saturating_add(1);
        Ok(result)
    }

    fn validate_global_gates(&self) -> AdapterResult<()> {
        let reason = if self.pause.is_paused() {
            Some("global pause is active")
        } else if self.config.observer_only {
            Some("observer-only mode is active")
        } else if !self.config.executor_enabled {
            Some("executor is disabled")
        } else if self.actions_this_interval >= self.config.maximum_actions_per_interval {
            Some("maximum actions per interval reached")
        } else {
            None
        };
        if let Some(reason) = reason {
            Err(AdapterError::ExecutionDisabled(reason.to_owned()))
        } else {
            Ok(())
        }
    }
}

fn validate_tactical_gate(
    action: &GameAction,
    state: &GameState,
    minimum_neck_width_km: f32,
) -> AdapterResult<()> {
    let front = state
        .fronts
        .iter()
        .find(|front| front.id == action.target_front)
        .ok_or_else(|| AdapterError::Other(format!("unknown front {}", action.target_front)))?;
    let metrics = FrontMetrics::calculate(front, minimum_neck_width_km);
    if action.kind == GameActionKind::Attack
        && (metrics.supply_score < 0.55 || metrics.encirclement_risk >= 0.70)
    {
        return Err(AdapterError::ExecutionDisabled(
            "hard tactical gate rejected unsafe attack".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};
    use crate::config::AppConfig;
    use crate::metrics::FrontMetrics;
    use crate::planner::{GameAction, GameActionKind, recommend};

    use super::*;

    #[test]
    fn default_observer_mode_rejects_every_action() {
        let config = AppConfig::default();
        let mut adapter = MockGameAdapter::new(MockScenario::StableFront);
        let state = adapter.observe().expect("state");
        let metrics = FrontMetrics::calculate(&state.fronts[0], 10.0);
        let action = recommend(&state.fronts[0], &metrics).remove(0);
        let mut executor = SafetyExecutor::new(config.agent, PauseSwitch::default());
        assert!(
            executor
                .execute(&mut adapter, &action, &state, 10.0)
                .is_err()
        );
    }

    #[test]
    fn hard_gate_rejects_attack_in_dangerous_salient() {
        let mut adapter = MockGameAdapter::new(MockScenario::DeepSalient);
        let state = adapter.observe().expect("state");
        let front = &state.fronts[0];
        let action = GameAction {
            kind: GameActionKind::Attack,
            target_front: front.id.clone(),
            confidence: 1.0,
            rationale: "test".to_owned(),
        };
        let mut agent = AppConfig::default().agent;
        agent.observer_only = false;
        agent.executor_enabled = true;
        agent.dry_run = false;
        let mut executor = SafetyExecutor::new(agent, PauseSwitch::default());
        assert!(
            executor
                .execute(&mut adapter, &action, &state, 10.0)
                .is_err()
        );
    }

    #[test]
    fn pause_switch_is_shared_and_immediate() {
        let pause = PauseSwitch::default();
        let copy = pause.clone();
        pause.pause();
        assert!(copy.is_paused());
        copy.resume();
        assert!(!pause.is_paused());
    }
}
