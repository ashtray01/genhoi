use genhoi::adapter::{GameAdapter, MockGameAdapter, MockScenario};
use genhoi::config::AppConfig;
use genhoi::event::{AgentEvent, derive_events};
use genhoi::metrics::FrontMetrics;
use genhoi::planner::{GameActionKind, RiskLevel, classify_risk, recommend};

#[test]
fn all_six_mock_scenarios_are_observable() {
    let scenarios = [
        MockScenario::StableFront,
        MockScenario::LowSupply,
        MockScenario::Breakthrough,
        MockScenario::DeepSalient,
        MockScenario::EncirclementRisk,
        MockScenario::EnemyCollapse,
    ];
    for scenario in scenarios {
        let mut adapter = MockGameAdapter::new(scenario);
        let state = adapter.observe().expect("scenario must be observable");
        assert_eq!(state.fronts.len(), 1);
        assert!(state.at_war());
    }
}

#[test]
fn specified_encirclement_case_is_critical_and_never_attacks() {
    let config = AppConfig::default();
    let mut adapter = MockGameAdapter::new(MockScenario::DeepSalient);
    let state = adapter.observe().expect("scenario must be observable");
    let front = &state.fronts[0];
    let metrics = FrontMetrics::calculate(front, config.risk.minimum_neck_width_km);
    let actions = recommend(front, &metrics);
    let events = derive_events(None, &state, &config.risk);

    assert_eq!(
        classify_risk(metrics.encirclement_risk, &config.risk),
        RiskLevel::Critical
    );
    assert!(metrics.encirclement_risk >= 0.90);
    assert_eq!(actions[0].kind, GameActionKind::StopOffensive);
    assert!(
        !actions
            .iter()
            .any(|action| action.kind == GameActionKind::Attack)
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::EncirclementRiskHigh { .. }))
    );
}
