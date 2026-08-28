use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::LearningConfig;
use crate::memory::MemoryStore;
use crate::metrics::FrontMetrics;
use crate::planner::GameAction;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SimilarEpisode {
    pub id: i64,
    pub action: String,
    pub reward: f32,
    pub outcome_json: String,
    pub similarity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodeSummary {
    pub id: i64,
    pub features: Vec<f32>,
    pub action: String,
    pub reward: f32,
    pub outcome_json: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QValue {
    pub value: f32,
    pub visits: u64,
}

#[derive(Debug, Clone)]
pub struct QLearner {
    settings: LearningConfig,
}

impl QLearner {
    #[must_use]
    pub fn new(settings: LearningConfig) -> Self {
        Self { settings }
    }

    /// Applies one bounded tabular Q-learning update in SQLite.
    ///
    /// # Errors
    ///
    /// Returns an error if the existing value cannot be read or persisted.
    pub fn update(
        &self,
        store: &MemoryStore,
        state_key: &str,
        action: &str,
        reward: f32,
        next_best: f32,
    ) -> Result<QValue> {
        let current = store.q_value(state_key, action)?;
        let target = reward + self.settings.discount_factor * next_best;
        let value = current.value + self.settings.learning_rate * (target - current.value);
        let updated = QValue {
            value,
            visits: current.visits.saturating_add(1),
        };
        store.set_q_value(state_key, action, updated)?;
        Ok(updated)
    }

    /// Chooses the best known constrained action, with bounded exploration.
    ///
    /// `exploration_sample` is supplied by the caller so replay stays fully
    /// deterministic. Values below the configured rate choose the least-visited
    /// candidate; otherwise the highest Q-value wins.
    ///
    /// # Errors
    ///
    /// Returns an error if Q-values cannot be read from SQLite.
    pub fn select<'a>(
        &self,
        store: &MemoryStore,
        state_key: &str,
        candidates: &'a [GameAction],
        exploration_sample: f32,
    ) -> Result<Option<&'a GameAction>> {
        let mut scored = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            scored.push((
                candidate,
                store.q_value(state_key, &candidate.kind.to_string())?,
            ));
        }
        let selected = if exploration_sample.clamp(0.0, 1.0) < self.settings.exploration_rate {
            scored.into_iter().min_by_key(|(_, q)| q.visits)
        } else {
            scored.into_iter().max_by(|(_, left), (_, right)| {
                left.value
                    .total_cmp(&right.value)
                    .then_with(|| left.visits.cmp(&right.visits))
            })
        };
        Ok(selected.map(|(action, _)| action))
    }
}

#[must_use]
pub fn abstract_state(metrics: &FrontMetrics) -> String {
    let mut labels = Vec::new();
    if metrics.supply_score < 0.55 {
        labels.push("LOW_SUPPLY");
    }
    if metrics.encirclement_risk >= 0.70 {
        labels.push("HIGH_ENCIRCLEMENT_RISK");
    }
    if metrics.force_ratio >= 1.35 {
        labels.push("ENEMY_WEAK");
    }
    if metrics.offensive_potential >= 0.72 {
        labels.push("BREAKTHROUGH_AVAILABLE");
    }
    if metrics.salient_ratio >= 3.0 {
        labels.push("FRONT_OVEREXTENDED");
    }
    if labels.is_empty() {
        "BALANCED".to_owned()
    } else {
        labels.join("|")
    }
}

#[must_use]
pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let dot = left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm <= f32::EPSILON || right_norm <= f32::EPSILON {
        0.0
    } else {
        (dot / (left_norm * right_norm)).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};
    use crate::config::AppConfig;
    use crate::metrics::FrontMetrics;

    use super::*;

    #[test]
    fn salient_state_has_expected_labels() {
        let mut adapter = MockGameAdapter::new(MockScenario::DeepSalient);
        let state = adapter.observe().expect("state");
        let metrics = FrontMetrics::calculate(&state.fronts[0], 10.0);
        assert_eq!(
            abstract_state(&metrics),
            "LOW_SUPPLY|HIGH_ENCIRCLEMENT_RISK|FRONT_OVEREXTENDED"
        );
    }

    #[test]
    fn q_learning_moves_toward_reward() {
        let store = MemoryStore::in_memory().expect("database");
        let learner = QLearner::new(AppConfig::default().learning);
        let first = learner
            .update(&store, "LOW_SUPPLY", "HOLD", 1.0, 0.0)
            .expect("first update");
        let second = learner
            .update(&store, "LOW_SUPPLY", "HOLD", 1.0, 0.0)
            .expect("second update");
        assert!(first.value > 0.0);
        assert!(second.value > first.value);
        assert_eq!(second.visits, 2);
    }

    #[test]
    fn cosine_handles_identical_and_empty_vectors() {
        assert!((cosine_similarity(&[1.0, 2.0], &[1.0, 2.0]) - 1.0).abs() < 0.000_1);
        assert!(cosine_similarity(&[], &[]).abs() < f32::EPSILON);
    }

    #[test]
    fn retrieves_most_similar_episode_first() {
        let store = MemoryStore::in_memory().expect("database");
        let session = store.begin_session("test").expect("session");
        store
            .record_episode(&session, &[1.0, 0.0], "HOLD", 0.5, "{}")
            .expect("first episode");
        store
            .record_episode(&session, &[0.0, 1.0], "ATTACK", -0.5, "{}")
            .expect("second episode");
        let similar = store
            .similar_episodes(&[0.9, 0.1], 2)
            .expect("similar episodes");
        assert_eq!(similar[0].action, "HOLD");
        assert!(similar[0].similarity > similar[1].similarity);
    }
}
