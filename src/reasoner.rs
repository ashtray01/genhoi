use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::LlmConfig;
use crate::learning::SimilarEpisode;
use crate::memory::MemoryStore;
use crate::metrics::FrontMetrics;
use crate::planner::GameActionKind;

#[derive(Debug, Error)]
pub enum ReasonerError {
    #[error("invalid strategic decision: {0}")]
    InvalidDecision(String),
    #[error("strategic reasoner is disabled")]
    Disabled,
    #[error("strategic inference failed: {0}")]
    Inference(String),
}

pub type ReasonerResult<T> = Result<T, ReasonerError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategicContext {
    pub at_war: bool,
    pub front_id: String,
    pub front_name: String,
    pub metrics: FrontMetrics,
    pub recent_experiences: Vec<SimilarEpisode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StrategicIntent {
    StabilizeFront,
    ImproveSupply,
    ExploitBreakthrough,
    BuildReserves,
    HoldFront,
    MaintainPeace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StrategicDecision {
    pub intent: StrategicIntent,
    pub priority: f32,
    pub target_front: Option<String>,
    pub actions: Vec<GameActionKind>,
    pub reason: String,
}

impl StrategicDecision {
    /// Validates a decision before it can reach an executor.
    ///
    /// # Errors
    ///
    /// Rejects non-finite priorities, out-of-range values, excessive action
    /// counts, empty/oversized reasons, or an unexpected target front.
    pub fn validate(&self, expected_front: Option<&str>) -> ReasonerResult<()> {
        if !self.priority.is_finite() || !(0.0..=1.0).contains(&self.priority) {
            return Err(ReasonerError::InvalidDecision(
                "priority must be finite and between zero and one".to_owned(),
            ));
        }
        if self.actions.len() > 5 {
            return Err(ReasonerError::InvalidDecision(
                "no more than five actions are permitted".to_owned(),
            ));
        }
        let reason_length = self.reason.chars().count();
        if reason_length == 0 || reason_length > 512 {
            return Err(ReasonerError::InvalidDecision(
                "reason must contain 1 to 512 characters".to_owned(),
            ));
        }
        if let (Some(expected), Some(actual)) = (expected_front, self.target_front.as_deref())
            && expected != actual
        {
            return Err(ReasonerError::InvalidDecision(format!(
                "unexpected target front {actual}"
            )));
        }
        Ok(())
    }
}

pub trait StrategicReasoner {
    /// Produces one constrained strategic decision.
    ///
    /// # Errors
    ///
    /// Returns an error when reasoning fails or the constrained result does not
    /// pass validation.
    fn reason(&mut self, context: &StrategicContext) -> ReasonerResult<StrategicDecision>;
}

#[derive(Debug, Default)]
pub struct RuleBasedReasoner;

impl StrategicReasoner for RuleBasedReasoner {
    fn reason(&mut self, context: &StrategicContext) -> ReasonerResult<StrategicDecision> {
        let metrics = context.metrics;
        let (intent, priority, actions, reason) = if !context.at_war {
            (
                StrategicIntent::MaintainPeace,
                0.25,
                vec![],
                "No active war requires an immediate strategic commitment.",
            )
        } else if metrics.encirclement_risk >= 0.70 {
            (
                StrategicIntent::StabilizeFront,
                metrics.encirclement_risk,
                vec![
                    GameActionKind::StopOffensive,
                    GameActionKind::ReinforceCorridor,
                    GameActionKind::WidenFront,
                ],
                "High encirclement risk requires stopping the offensive and securing the salient shoulders.",
            )
        } else if metrics.supply_score < 0.55 {
            (
                StrategicIntent::ImproveSupply,
                1.0 - metrics.supply_score,
                vec![GameActionKind::Hold, GameActionKind::Redeploy],
                "Supply is below the safe threshold for sustained offensive operations.",
            )
        } else if metrics.offensive_potential >= 0.72 && metrics.force_ratio >= 1.20 {
            (
                StrategicIntent::ExploitBreakthrough,
                metrics.offensive_potential,
                vec![GameActionKind::ConcentrateArmor, GameActionKind::Attack],
                "Force, supply, organization and air support favor a bounded offensive.",
            )
        } else if metrics.reserve_strength < 0.10 {
            (
                StrategicIntent::BuildReserves,
                0.65,
                vec![GameActionKind::Hold, GameActionKind::Redeploy],
                "The theater lacks a sufficient operational reserve.",
            )
        } else {
            (
                StrategicIntent::HoldFront,
                metrics.defensive_potential,
                vec![GameActionKind::Hold],
                "No decisive low-risk strategic opportunity is present.",
            )
        };
        let decision = StrategicDecision {
            intent,
            priority: priority.clamp(0.0, 1.0),
            target_front: context.at_war.then(|| context.front_id.clone()),
            actions,
            reason: reason.to_owned(),
        };
        decision.validate(context.at_war.then_some(context.front_id.as_str()))?;
        Ok(decision)
    }
}

#[derive(Debug, Clone)]
pub struct LlamaReasoner {
    config: LlmConfig,
    last_inference_duration: Option<Duration>,
}

impl LlamaReasoner {
    #[must_use]
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config,
            last_inference_duration: None,
        }
    }

    pub fn take_last_inference_duration(&mut self) -> Option<Duration> {
        self.last_inference_duration.take()
    }

    fn prompt(context: &StrategicContext) -> ReasonerResult<String> {
        let experiences = context
            .recent_experiences
            .iter()
            .take(3)
            .collect::<Vec<_>>();
        let compact = serde_json::json!({
            "at_war": context.at_war,
            "front_id": &context.front_id,
            "front_name": &context.front_name,
            "metrics": context.metrics,
            "past_similar_experiences": experiences,
        });
        let prompt = format!(
            "You are the strategic layer of a single-player HOI4 research agent. \
             Return exactly one JSON object matching the supplied schema. Choose only \
             the listed actions. Never emit commands or prose outside JSON. State: {compact}"
        );
        if prompt.len() > 16_384 {
            return Err(ReasonerError::Inference(
                "strategic prompt exceeds local safety limit".to_owned(),
            ));
        }
        Ok(prompt)
    }

    fn invoke(&self, prompt: &str) -> ReasonerResult<String> {
        let mut command = Command::new(&self.config.executable);
        command
            .arg("-m")
            .arg(&self.config.model_path)
            .arg("-p")
            .arg(prompt)
            .arg("-t")
            .arg(self.config.threads.to_string())
            .arg("-c")
            .arg(self.config.context_size.to_string())
            .arg("-n")
            .arg(self.config.max_output_tokens.to_string())
            .arg("--temp")
            .arg(self.config.temperature.to_string())
            .arg("--json-schema")
            .arg(DECISION_JSON_SCHEMA)
            .args([
                "--no-display-prompt",
                "--no-show-timings",
                "--no-conversation",
                "--single-turn",
                "--log-disable",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| ReasonerError::Inference(error.to_string()))?;
        let timeout = Duration::from_secs(self.config.timeout_seconds);
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ReasonerError::Inference(format!(
                        "llama.cpp exceeded {} second timeout",
                        self.config.timeout_seconds
                    )));
                }
                Err(error) => return Err(ReasonerError::Inference(error.to_string())),
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|error| ReasonerError::Inference(error.to_string()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ReasonerError::Inference(format!(
                "llama.cpp exited with {}: {}",
                output.status,
                truncate(&stderr, 512)
            )));
        }
        String::from_utf8(output.stdout)
            .map_err(|error| ReasonerError::Inference(error.to_string()))
    }
}

impl StrategicReasoner for LlamaReasoner {
    fn reason(&mut self, context: &StrategicContext) -> ReasonerResult<StrategicDecision> {
        if !self.config.enabled {
            return Err(ReasonerError::Disabled);
        }
        if !self.config.model_path.is_file() {
            return Err(ReasonerError::Inference(format!(
                "GGUF model not found: {}",
                self.config.model_path.display()
            )));
        }
        let prompt = Self::prompt(context)?;
        let started = Instant::now();
        let output = self.invoke(&prompt);
        self.last_inference_duration = Some(started.elapsed());
        let output = output?;
        parse_decision(&output, context.at_war.then_some(context.front_id.as_str()))
    }
}

#[derive(Debug)]
pub enum ConfiguredReasoner {
    Rule(RuleBasedReasoner),
    Llama(LlamaReasoner),
}

impl ConfiguredReasoner {
    #[must_use]
    pub fn new(config: &LlmConfig) -> Self {
        if config.enabled {
            Self::Llama(LlamaReasoner::new(config.clone()))
        } else {
            Self::Rule(RuleBasedReasoner)
        }
    }

    pub fn take_last_inference_duration(&mut self) -> Option<Duration> {
        match self {
            Self::Rule(_) => None,
            Self::Llama(reasoner) => reasoner.take_last_inference_duration(),
        }
    }
}

impl StrategicReasoner for ConfiguredReasoner {
    fn reason(&mut self, context: &StrategicContext) -> ReasonerResult<StrategicDecision> {
        match self {
            Self::Rule(reasoner) => reasoner.reason(context),
            Self::Llama(reasoner) => reasoner.reason(context),
        }
    }
}

pub struct MemoryAwareReasoner<R> {
    inner: R,
    memory: MemoryStore,
    learning_enabled: bool,
}

impl<R> MemoryAwareReasoner<R> {
    #[must_use]
    pub fn new(inner: R, memory: MemoryStore, learning_enabled: bool) -> Self {
        Self {
            inner,
            memory,
            learning_enabled,
        }
    }

    pub fn inner_mut(&mut self) -> &mut R {
        &mut self.inner
    }
}

impl<R: StrategicReasoner> StrategicReasoner for MemoryAwareReasoner<R> {
    fn reason(&mut self, context: &StrategicContext) -> ReasonerResult<StrategicDecision> {
        let mut enriched = context.clone();
        enriched.recent_experiences = if self.learning_enabled {
            self.memory
                .similar_episodes(&context.metrics.feature_vector(), 3)
                .map_err(|error| ReasonerError::Inference(error.to_string()))?
        } else {
            vec![]
        };
        self.inner.reason(&enriched)
    }
}

const DECISION_JSON_SCHEMA: &str = r#"{
  "type":"object",
  "properties":{
    "intent":{"type":"string","enum":["stabilize_front","improve_supply","exploit_breakthrough","build_reserves","hold_front","maintain_peace"]},
    "priority":{"type":"number","minimum":0,"maximum":1},
    "target_front":{"type":["string","null"]},
    "actions":{"type":"array","maxItems":5,"items":{"type":"string","enum":["STOP_OFFENSIVE","REINFORCE_CORRIDOR","WIDEN_FRONT","HOLD","REDEPLOY","CONCENTRATE_ARMOR","ATTACK"]}},
    "reason":{"type":"string","minLength":1,"maxLength":512}
  },
  "required":["intent","priority","target_front","actions","reason"],
  "additionalProperties":false
}"#;

fn parse_decision(raw: &str, expected_front: Option<&str>) -> ReasonerResult<StrategicDecision> {
    let decision: StrategicDecision = serde_json::from_str(raw.trim())
        .map_err(|error| ReasonerError::InvalidDecision(error.to_string()))?;
    decision.validate(expected_front)?;
    Ok(decision)
}

fn truncate(value: &str, maximum_chars: usize) -> String {
    value.chars().take(maximum_chars).collect()
}

#[cfg(test)]
mod tests {
    use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};
    use crate::config::AppConfig;

    use super::*;

    #[test]
    fn rule_reasoner_stabilizes_dangerous_salient() {
        let mut adapter = MockGameAdapter::new(MockScenario::DeepSalient);
        let state = adapter.observe().expect("state");
        let context = StrategicContext {
            at_war: true,
            front_id: state.fronts[0].id.clone(),
            front_name: state.fronts[0].name.clone(),
            metrics: FrontMetrics::calculate(&state.fronts[0], 10.0),
            recent_experiences: vec![],
        };
        let decision = RuleBasedReasoner.reason(&context).expect("decision");
        assert_eq!(decision.intent, StrategicIntent::StabilizeFront);
        assert_eq!(decision.actions[0], GameActionKind::StopOffensive);
    }

    #[test]
    fn validation_rejects_wrong_front() {
        let decision = StrategicDecision {
            intent: StrategicIntent::HoldFront,
            priority: 0.5,
            target_front: Some("wrong".to_owned()),
            actions: vec![GameActionKind::Hold],
            reason: "Hold while reserves arrive.".to_owned(),
        };
        assert!(decision.validate(Some("expected")).is_err());
    }

    #[test]
    fn strict_json_parser_accepts_only_constrained_decision() {
        let raw = r#"{
            "intent":"stabilize_front",
            "priority":0.92,
            "target_front":"eastern_front_3",
            "actions":["STOP_OFFENSIVE","REINFORCE_CORRIDOR"],
            "reason":"High encirclement risk."
        }"#;
        let decision = parse_decision(raw, Some("eastern_front_3")).expect("valid decision");
        assert_eq!(decision.intent, StrategicIntent::StabilizeFront);
        assert!(parse_decision(&format!("prefix {raw}"), Some("eastern_front_3")).is_err());
    }

    #[test]
    fn disabled_llama_never_starts_a_process() {
        let config = AppConfig::default().llm;
        let mut reasoner = LlamaReasoner::new(config);
        let context = StrategicContext {
            at_war: false,
            front_id: "front".to_owned(),
            front_name: "Front".to_owned(),
            metrics: FrontMetrics {
                force_ratio: 1.0,
                supply_score: 1.0,
                front_stability: 1.0,
                offensive_potential: 0.0,
                defensive_potential: 1.0,
                encirclement_risk: 0.0,
                salient_risk: 0.0,
                salient_ratio: 0.0,
                reserve_strength: 1.0,
                air_support_score: 0.5,
                equipment_shortage_score: 0.0,
            },
            recent_experiences: vec![],
        };
        assert!(matches!(
            reasoner.reason(&context),
            Err(ReasonerError::Disabled)
        ));
    }
}
