use std::time::Duration;

use crate::config::ReasoningConfig;
use crate::event::AgentEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategicSituation {
    Peace,
    War,
    ActiveOffensive,
    Critical,
}

#[derive(Debug, Clone)]
pub struct ReasoningScheduler {
    config: ReasoningConfig,
    last_reasoning: Option<Duration>,
}

impl ReasoningScheduler {
    #[must_use]
    pub fn new(config: ReasoningConfig) -> Self {
        Self {
            config,
            last_reasoning: None,
        }
    }

    #[must_use]
    pub fn interval(&self, situation: StrategicSituation) -> Duration {
        Duration::from_secs(match situation {
            StrategicSituation::Peace => self.config.peace_seconds,
            StrategicSituation::War => self.config.war_seconds,
            StrategicSituation::ActiveOffensive => self.config.offensive_seconds,
            StrategicSituation::Critical => 0,
        })
    }

    #[must_use]
    pub fn due(&self, now: Duration, situation: StrategicSituation, events: &[AgentEvent]) -> bool {
        let Some(last) = self.last_reasoning else {
            return true;
        };
        let elapsed = now.saturating_sub(last);
        let immediate = situation == StrategicSituation::Critical
            || events.iter().any(requires_immediate_reasoning);
        if immediate {
            elapsed >= Duration::from_secs(self.config.immediate_cooldown_seconds)
        } else {
            elapsed >= self.interval(situation)
        }
    }

    pub fn mark_reasoned(&mut self, now: Duration) {
        self.last_reasoning = Some(now);
    }
}

#[must_use]
pub fn requires_immediate_reasoning(event: &AgentEvent) -> bool {
    matches!(
        event,
        AgentEvent::WarStarted { .. }
            | AgentEvent::FrontCollapsed { .. }
            | AgentEvent::SupplyCritical { .. }
            | AgentEvent::EncirclementRiskHigh { .. }
            | AgentEvent::MajorTerritoryLoss { .. }
            | AgentEvent::EquipmentCritical { .. }
            | AgentEvent::ManpowerCritical { .. }
            | AgentEvent::AirSuperiorityLost { .. }
    )
}

#[cfg(test)]
mod tests {
    use crate::config::AppConfig;

    use super::*;

    #[test]
    fn immediate_events_obey_cooldown() {
        let mut scheduler = ReasoningScheduler::new(AppConfig::default().reasoning);
        assert!(scheduler.due(Duration::ZERO, StrategicSituation::War, &[]));
        scheduler.mark_reasoned(Duration::ZERO);
        let event = AgentEvent::SupplyCritical {
            front_id: "front".to_owned(),
            supply: 0.3,
        };
        assert!(!scheduler.due(
            Duration::from_secs(4),
            StrategicSituation::Critical,
            std::slice::from_ref(&event)
        ));
        assert!(scheduler.due(
            Duration::from_secs(5),
            StrategicSituation::Critical,
            &[event]
        ));
    }

    #[test]
    fn peace_uses_slowest_interval() {
        let mut scheduler = ReasoningScheduler::new(AppConfig::default().reasoning);
        scheduler.mark_reasoned(Duration::ZERO);
        assert!(!scheduler.due(Duration::from_secs(59), StrategicSituation::Peace, &[]));
        assert!(scheduler.due(Duration::from_secs(60), StrategicSituation::Peace, &[]));
    }
}
