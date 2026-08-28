use std::sync::mpsc::{self, Receiver, Sender};

use serde::{Deserialize, Serialize};

use crate::config::RiskConfig;
use crate::metrics::FrontMetrics;
use crate::state::GameState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentEvent {
    WarStarted { war_id: String },
    WarEnded { war_id: String },
    FrontChanged { front_id: String },
    FrontCollapsed { front_id: String },
    SupplyCritical { front_id: String, supply: f32 },
    EncirclementRiskHigh { front_id: String, risk: f32 },
    MajorTerritoryGain { square_km: u64 },
    MajorTerritoryLoss { square_km: u64 },
    EquipmentCritical { fulfillment: f32 },
    ManpowerCritical { manpower: u64 },
    AirSuperiorityLost { region_id: String },
    DoctrineUpdated { doctrine_id: i64 },
    PeriodicStrategicReview,
}

/// Small in-process fan-out bus. Slow consumers cannot block state analysis.
#[derive(Debug, Default)]
pub struct EventBus {
    subscribers: Vec<Sender<AgentEvent>>,
}

impl EventBus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&mut self) -> Receiver<AgentEvent> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers.push(sender);
        receiver
    }

    pub fn publish(&mut self, event: &AgentEvent) {
        self.subscribers
            .retain(|subscriber| subscriber.send(event.clone()).is_ok());
    }
}

#[must_use]
pub fn derive_events(
    previous: Option<&GameState>,
    current: &GameState,
    risk: &RiskConfig,
) -> Vec<AgentEvent> {
    let mut events = Vec::new();

    if let Some(previous) = previous {
        for war in &current.wars {
            let was_active = previous
                .wars
                .iter()
                .find(|old| old.id == war.id)
                .is_some_and(|old| old.active);
            if war.active && !was_active {
                events.push(AgentEvent::WarStarted {
                    war_id: war.id.clone(),
                });
            } else if !war.active && was_active {
                events.push(AgentEvent::WarEnded {
                    war_id: war.id.clone(),
                });
            }
        }
    }

    for front in &current.fronts {
        let metrics = FrontMetrics::calculate(front, risk.minimum_neck_width_km);
        if front.supply < 0.50 {
            events.push(AgentEvent::SupplyCritical {
                front_id: front.id.clone(),
                supply: front.supply,
            });
        }
        if metrics.encirclement_risk >= risk.high {
            events.push(AgentEvent::EncirclementRiskHigh {
                front_id: front.id.clone(),
                risk: metrics.encirclement_risk,
            });
        }
        if let Some(old_front) = previous.and_then(|state| {
            state
                .fronts
                .iter()
                .find(|old_front| old_front.id == front.id)
        }) && (old_front.front_width_km - front.front_width_km).abs() >= 25.0
        {
            events.push(AgentEvent::FrontChanged {
                front_id: front.id.clone(),
            });
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};
    use crate::config::AppConfig;

    use super::*;

    #[test]
    fn critical_scenario_emits_immediate_events() {
        let mut adapter = MockGameAdapter::new(MockScenario::DeepSalient);
        let state = adapter.observe().expect("mock observation");
        let events = derive_events(None, &state, &AppConfig::default().risk);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::EncirclementRiskHigh { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::SupplyCritical { .. }))
        );
    }

    #[test]
    fn event_bus_fans_out() {
        let mut bus = EventBus::new();
        let first = bus.subscribe();
        let second = bus.subscribe();
        bus.publish(&AgentEvent::PeriodicStrategicReview);
        assert_eq!(
            first.recv().expect("first subscriber"),
            AgentEvent::PeriodicStrategicReview
        );
        assert_eq!(
            second.recv().expect("second subscriber"),
            AgentEvent::PeriodicStrategicReview
        );
    }
}
