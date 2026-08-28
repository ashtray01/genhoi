use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub agent: AgentConfig,
    pub llm: LlmConfig,
    pub reasoning: ReasoningConfig,
    pub risk: RiskConfig,
    pub learning: LearningConfig,
    pub performance: PerformanceConfig,
    pub reward: RewardConfig,
    pub paths: PathsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Independent user-facing safety switches are intentional.
pub struct AgentConfig {
    pub enabled: bool,
    pub observer_only: bool,
    pub executor_enabled: bool,
    pub learning_enabled: bool,
    pub dry_run: bool,
    pub observer_interval_seconds: u64,
    pub maximum_actions_per_interval: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub enabled: bool,
    pub model_path: PathBuf,
    pub threads: usize,
    pub context_size: usize,
    pub max_output_tokens: usize,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    pub peace_seconds: u64,
    pub war_seconds: u64,
    pub offensive_seconds: u64,
    pub immediate_cooldown_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    pub high: f32,
    pub critical: f32,
    pub minimum_neck_width_km: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningConfig {
    pub learning_rate: f32,
    pub exploration_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub maximum_ram_mb: u64,
    pub maximum_cpu_threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardConfig {
    pub territory_gain: f32,
    pub enemy_losses: f32,
    pub factory_gain: f32,
    pub own_manpower_loss: f32,
    pub equipment_loss: f32,
    pub supply_penalty: f32,
    pub encirclement_penalty: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub data_dir: PathBuf,
    pub database: PathBuf,
}

impl AppConfig {
    /// Loads the shipped defaults, or a complete replacement TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the supplied file cannot be read, parsed, or fails
    /// semantic validation.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let text = if let Some(path) = path {
            fs::read_to_string(path)
                .with_context(|| format!("failed to read config {}", path.display()))?
        } else {
            include_str!("../config/default.toml").to_owned()
        };
        let config: Self = toml::from_str(&text).context("invalid GenHOI configuration")?;
        config.validate()?;
        Ok(config)
    }

    /// Validates safety gates, thresholds and resource limits.
    ///
    /// # Errors
    ///
    /// Returns an error when values are outside their permitted range or
    /// execution conflicts with observer-only mode.
    pub fn validate(&self) -> Result<()> {
        if self.llm.threads == 0 || self.llm.threads > self.performance.maximum_cpu_threads {
            bail!("llm.threads must be between 1 and performance.maximum_cpu_threads");
        }
        if !(0.0..=1.0).contains(&self.risk.high)
            || !(0.0..=1.0).contains(&self.risk.critical)
            || self.risk.high >= self.risk.critical
        {
            bail!("risk thresholds must satisfy 0 <= high < critical <= 1");
        }
        if self.agent.executor_enabled && self.agent.observer_only {
            bail!("executor_enabled cannot be true while observer_only is true");
        }
        if self.agent.maximum_actions_per_interval == 0 {
            bail!("maximum_actions_per_interval must be greater than zero");
        }
        Ok(())
    }

    #[must_use]
    pub fn data_dir(&self) -> PathBuf {
        if !self.paths.data_dir.as_os_str().is_empty() {
            return self.paths.data_dir.clone();
        }
        native_data_dir()
    }

    #[must_use]
    pub fn database_path(&self) -> PathBuf {
        if self.paths.database.is_absolute() {
            self.paths.database.clone()
        } else {
            self.data_dir().join(&self.paths.database)
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::load(None).expect("embedded default configuration must be valid")
    }
}

#[must_use]
pub fn native_data_dir() -> PathBuf {
    native_data_dir_from(|name| env::var_os(name))
}

fn native_data_dir_from(mut get: impl FnMut(&str) -> Option<std::ffi::OsString>) -> PathBuf {
    if cfg!(windows) {
        get("APPDATA")
            .map_or_else(|| PathBuf::from("."), PathBuf::from)
            .join("GenHOI")
    } else if let Some(path) = get("XDG_DATA_HOME") {
        PathBuf::from(path).join("genhoi")
    } else {
        get("HOME")
            .map_or_else(|| PathBuf::from("."), PathBuf::from)
            .join(".local")
            .join("share")
            .join("genhoi")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_config_is_safe_by_default() {
        let config = AppConfig::default();
        assert!(config.agent.observer_only);
        assert!(!config.agent.executor_enabled);
        assert!(config.agent.dry_run);
        assert!(!config.llm.enabled);
        assert_eq!(config.llm.threads, 2);
    }

    #[test]
    fn linux_fallback_path_follows_xdg_shape() {
        let path = native_data_dir_from(|name| match name {
            "XDG_DATA_HOME" => Some("/tmp/xdg".into()),
            _ => None,
        });
        if cfg!(windows) {
            assert_eq!(path, PathBuf::from(".").join("GenHOI"));
        } else {
            assert_eq!(path, PathBuf::from("/tmp/xdg/genhoi"));
        }
    }
}
