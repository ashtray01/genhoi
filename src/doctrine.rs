use serde::{Deserialize, Serialize};

use crate::learning::EpisodeSummary;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LessonStatus {
    Proposed,
    Active,
    Rejected,
    Obsolete,
}

impl std::fmt::Display for LessonStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Proposed => "proposed",
            Self::Active => "active",
            Self::Rejected => "rejected",
            Self::Obsolete => "obsolete",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LessonEvidence {
    pub comparable_episodes: usize,
    pub successes: usize,
    pub failures: usize,
    pub mean_reward: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LessonDraft {
    pub observation: String,
    pub evidence: LessonEvidence,
    pub proposed_doctrine: String,
    pub confidence: f32,
    pub status: LessonStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredLesson {
    pub id: i64,
    pub draft: LessonDraft,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AfterActionReview {
    pub session_id: String,
    pub successes: usize,
    pub failures: usize,
    pub mean_reward: f32,
    pub report: String,
}

#[derive(Debug, Default)]
pub struct DoctrineEngine;

impl DoctrineEngine {
    #[must_use]
    pub fn derive(&self, episodes: &[EpisodeSummary]) -> Vec<LessonDraft> {
        let mut lessons = Vec::new();
        let low_supply_attacks = episodes
            .iter()
            .filter(|episode| {
                episode.features.get(1).is_some_and(|supply| *supply < 0.55)
                    && episode.action == "ATTACK"
            })
            .collect::<Vec<_>>();
        if low_supply_attacks.len() >= 5 {
            lessons.push(build_lesson(
                &low_supply_attacks,
                "Offensives launched below 55% supply repeatedly underperformed.",
                "Avoid major offensives when supply is below 55% unless a separately validated override applies.",
            ));
        }
        let high_risk_attacks = episodes
            .iter()
            .filter(|episode| {
                episode.features.get(4).is_some_and(|risk| *risk >= 0.70)
                    && episode.action == "ATTACK"
            })
            .collect::<Vec<_>>();
        if high_risk_attacks.len() >= 5 {
            lessons.push(build_lesson(
                &high_risk_attacks,
                "Attacks under high encirclement risk produced poor outcomes.",
                "Stop offensives and reinforce the corridor when encirclement risk is at least 70%.",
            ));
        }
        lessons
    }

    #[must_use]
    pub fn after_action_review(
        &self,
        session_id: &str,
        episodes: &[EpisodeSummary],
    ) -> AfterActionReview {
        let successes = episodes
            .iter()
            .filter(|episode| episode.reward > 0.0)
            .count();
        let failures = episodes.len().saturating_sub(successes);
        let mean_reward = if episodes.is_empty() {
            0.0
        } else {
            episodes.iter().map(|episode| episode.reward).sum::<f32>()
                / usize_as_f32(episodes.len())
        };
        let report = format!(
            "GENHOI GENERAL STAFF REPORT\n\nSession: {session_id}\nEpisodes: {}\nSuccessful outcomes: {successes}\nFailed outcomes: {failures}\nMean reward: {mean_reward:.3}\n\nDoctrine changes remain proposed until explicitly activated.",
            episodes.len()
        );
        AfterActionReview {
            session_id: session_id.to_owned(),
            successes,
            failures,
            mean_reward,
            report,
        }
    }
}

fn build_lesson(episodes: &[&EpisodeSummary], observation: &str, doctrine: &str) -> LessonDraft {
    let successes = episodes
        .iter()
        .filter(|episode| episode.reward > 0.0)
        .count();
    let failures = episodes.len().saturating_sub(successes);
    let mean_reward =
        episodes.iter().map(|episode| episode.reward).sum::<f32>() / usize_as_f32(episodes.len());
    let failure_ratio = usize_as_f32(failures) / usize_as_f32(episodes.len());
    let evidence_factor = usize_as_f32(episodes.len()) / (usize_as_f32(episodes.len()) + 3.0);
    LessonDraft {
        observation: observation.to_owned(),
        evidence: LessonEvidence {
            comparable_episodes: episodes.len(),
            successes,
            failures,
            mean_reward,
        },
        proposed_doctrine: doctrine.to_owned(),
        confidence: (failure_ratio * evidence_factor).clamp(0.0, 1.0),
        status: LessonStatus::Proposed,
    }
}

fn usize_as_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_low_supply_failures_propose_but_do_not_activate_lesson() {
        let episodes = (0..8)
            .map(|id| EpisodeSummary {
                id,
                features: vec![1.0, 0.4, 0.5, 0.6, 0.5],
                action: "ATTACK".to_owned(),
                reward: -0.8,
                outcome_json: "{}".to_owned(),
            })
            .collect::<Vec<_>>();
        let lessons = DoctrineEngine.derive(&episodes);
        assert_eq!(lessons.len(), 1);
        assert_eq!(lessons[0].status, LessonStatus::Proposed);
        assert!(lessons[0].confidence > 0.70);
    }

    #[test]
    fn review_summarizes_session() {
        let episodes = vec![
            EpisodeSummary {
                id: 1,
                features: vec![],
                action: "HOLD".to_owned(),
                reward: 1.0,
                outcome_json: "{}".to_owned(),
            },
            EpisodeSummary {
                id: 2,
                features: vec![],
                action: "ATTACK".to_owned(),
                reward: -0.5,
                outcome_json: "{}".to_owned(),
            },
        ];
        let review = DoctrineEngine.after_action_review("session", &episodes);
        assert_eq!(review.successes, 1);
        assert_eq!(review.failures, 1);
        assert!(review.report.contains("GENERAL STAFF REPORT"));
    }
}
