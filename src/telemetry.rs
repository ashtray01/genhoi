use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::adapter::{
    ActionResult, AdapterError, AdapterHealth, AdapterResult, AdapterStatus, GameAdapter,
};
use crate::planner::GameAction;
use crate::state::GameState;

pub const TELEMETRY_SCHEMA_VERSION: u32 = 1;
pub const TELEMETRY_SENTINEL: &str = "GENHOI_TELEMETRY ";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TelemetryEnvelope {
    pub schema: u32,
    pub sequence: u64,
    pub state: GameState,
}

pub struct TelemetryGameAdapter {
    path: PathBuf,
    reader: Option<BufReader<File>>,
    last_sequence: Option<u64>,
}

impl TelemetryGameAdapter {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            reader: None,
            last_sequence: None,
        }
    }

    fn ensure_open(&mut self) -> AdapterResult<()> {
        if self.reader.is_none() {
            let file = File::open(&self.path).map_err(|error| {
                AdapterError::Other(format!(
                    "failed to open telemetry {}: {error}",
                    self.path.display()
                ))
            })?;
            self.reader = Some(BufReader::new(file));
        }
        Ok(())
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl GameAdapter for TelemetryGameAdapter {
    fn observe(&mut self) -> AdapterResult<GameState> {
        self.ensure_open()?;
        let reader = self.reader.as_mut().ok_or_else(|| {
            AdapterError::Other("telemetry reader was not initialized".to_owned())
        })?;
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|error| AdapterError::Other(error.to_string()))?;
            if read == 0 {
                return Err(AdapterError::Exhausted);
            }
            if line.len() > 1_048_576 {
                return Err(AdapterError::Other(
                    "telemetry line exceeds 1 MiB safety limit".to_owned(),
                ));
            }
            let Some(envelope) = parse_telemetry_line(&line)? else {
                continue;
            };
            if self
                .last_sequence
                .is_some_and(|sequence| envelope.sequence <= sequence)
            {
                continue;
            }
            self.last_sequence = Some(envelope.sequence);
            return Ok(envelope.state);
        }
    }

    fn execute(&mut self, _action: &GameAction) -> AdapterResult<ActionResult> {
        Err(AdapterError::ExecutionDisabled(
            "telemetry adapter is permanently read-only".to_owned(),
        ))
    }

    fn health(&self) -> AdapterHealth {
        let (status, detail) = if !self.path.is_file() {
            (
                AdapterStatus::Offline,
                "telemetry file not found".to_owned(),
            )
        } else if let Some(sequence) = self.last_sequence {
            (
                AdapterStatus::Ready,
                format!("last accepted sequence {sequence}"),
            )
        } else {
            (
                AdapterStatus::Degraded,
                "waiting for first valid snapshot".to_owned(),
            )
        };
        AdapterHealth {
            status,
            name: "hoi4-telemetry".to_owned(),
            detail,
        }
    }
}

/// Parses a versioned telemetry record embedded in an HOI4 log line.
///
/// Non-telemetry lines return `Ok(None)` so normal game logs can be tailed.
///
/// # Errors
///
/// Rejects malformed JSON, unknown schema versions and invalid normalized
/// state ranges.
pub fn parse_telemetry_line(line: &str) -> AdapterResult<Option<TelemetryEnvelope>> {
    let Some(index) = line.find(TELEMETRY_SENTINEL) else {
        return Ok(None);
    };
    let payload = line[index + TELEMETRY_SENTINEL.len()..].trim();
    let envelope: TelemetryEnvelope = serde_json::from_str(payload)
        .map_err(|error| AdapterError::Other(format!("invalid telemetry JSON: {error}")))?;
    if envelope.schema != TELEMETRY_SCHEMA_VERSION {
        return Err(AdapterError::Other(format!(
            "unsupported telemetry schema {}",
            envelope.schema
        )));
    }
    validate_state(&envelope.state)?;
    Ok(Some(envelope))
}

fn validate_state(state: &GameState) -> AdapterResult<()> {
    if state.country.tag.is_empty() || state.country.tag.len() > 8 {
        return Err(AdapterError::Other("invalid country tag".to_owned()));
    }
    if state.fronts.len() > 128 || state.armies.len() > 512 {
        return Err(AdapterError::Other(
            "telemetry collection exceeds safety limit".to_owned(),
        ));
    }
    let country_ratios = [
        state.country.stability,
        state.country.war_support,
        state.economy.fuel_ratio,
    ];
    if country_ratios
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(AdapterError::Other(
            "country ratio is outside [0, 1]".to_owned(),
        ));
    }
    for front in &state.fronts {
        let ratios = [
            front.friendly_strength,
            front.enemy_strength,
            front.organization,
            front.supply,
            front.enemy_pressure,
            front.enemy_pressure_north,
            front.enemy_pressure_south,
            front.friendly_pressure,
            front.air_superiority,
            front.equipment_ratio,
        ];
        if ratios
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        {
            return Err(AdapterError::Other(format!(
                "front {} contains a ratio outside [0, 1]",
                front.id
            )));
        }
        if [
            front.front_width_km,
            front.depth_km,
            front.salient_depth_km,
            front.salient_neck_width_km,
        ]
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(AdapterError::Other(format!(
                "front {} contains invalid geometry",
                front.id
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::adapter::{GameAdapter, MockGameAdapter, MockScenario};

    use super::*;

    #[test]
    fn parses_prefixed_snapshot_and_ignores_other_lines() {
        let mut mock = MockGameAdapter::new(MockScenario::StableFront);
        let state = mock.observe().expect("mock state");
        let envelope = TelemetryEnvelope {
            schema: TELEMETRY_SCHEMA_VERSION,
            sequence: 7,
            state: state.clone(),
        };
        let line = format!(
            "[12:00:00][effectbase.cpp] {TELEMETRY_SENTINEL}{}",
            serde_json::to_string(&envelope).expect("serialize")
        );
        assert_eq!(
            parse_telemetry_line(&line)
                .expect("parse")
                .expect("envelope")
                .state,
            state
        );
        assert!(
            parse_telemetry_line("ordinary log line")
                .expect("ignore")
                .is_none()
        );
    }

    #[test]
    fn file_adapter_is_read_only_and_deduplicates_sequences() {
        let mut mock = MockGameAdapter::new(MockScenario::StableFront);
        let state = mock.observe().expect("mock state");
        let envelope = TelemetryEnvelope {
            schema: TELEMETRY_SCHEMA_VERSION,
            sequence: 1,
            state,
        };
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("genhoi-telemetry-{unique}.log"));
        let record = format!(
            "{TELEMETRY_SENTINEL}{}\n{TELEMETRY_SENTINEL}{}\n",
            serde_json::to_string(&envelope).expect("serialize"),
            serde_json::to_string(&envelope).expect("serialize")
        );
        fs::write(&path, record).expect("fixture");
        let mut adapter = TelemetryGameAdapter::new(&path);
        adapter.observe().expect("first state");
        assert!(matches!(adapter.observe(), Err(AdapterError::Exhausted)));
        assert_eq!(adapter.health().status, AdapterStatus::Ready);
        let _ = fs::remove_file(path);
    }
}
