use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessesToUpdate, System, get_current_pid};

use crate::config::PerformanceConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceSnapshot {
    pub ram_mb: f64,
    pub cpu_percent: f32,
    pub llm_inference_ms_average: f64,
    pub llm_calls_per_hour: f64,
    pub sqlite_size_bytes: u64,
    pub decision_latency_ms_average: f64,
    pub warnings: Vec<String>,
}

pub struct PerformanceMonitor {
    system: System,
    pid: Pid,
    started: Instant,
    llm_calls: u64,
    llm_duration: Duration,
    decisions: u64,
    decision_duration: Duration,
}

impl PerformanceMonitor {
    /// Creates a monitor scoped only to the current process.
    ///
    /// # Errors
    ///
    /// Returns an error if the operating system cannot identify this process.
    pub fn new() -> Result<Self> {
        let pid = get_current_pid()
            .map_err(|error| anyhow::anyhow!("failed to determine current process id: {error}"))?;
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        Ok(Self {
            system,
            pid,
            started: Instant::now(),
            llm_calls: 0,
            llm_duration: Duration::ZERO,
            decisions: 0,
            decision_duration: Duration::ZERO,
        })
    }

    pub fn record_llm_inference(&mut self, duration: Duration) {
        self.llm_calls = self.llm_calls.saturating_add(1);
        self.llm_duration = self.llm_duration.saturating_add(duration);
    }

    pub fn record_decision(&mut self, duration: Duration) {
        self.decisions = self.decisions.saturating_add(1);
        self.decision_duration = self.decision_duration.saturating_add(duration);
    }

    #[must_use]
    pub fn snapshot(
        &mut self,
        sqlite_size_bytes: u64,
        limits: &PerformanceConfig,
    ) -> PerformanceSnapshot {
        self.system
            .refresh_processes(ProcessesToUpdate::Some(&[self.pid]), true);
        let (memory, process_cpu) = self
            .system
            .process(self.pid)
            .map_or((0, 0.0), |process| (process.memory(), process.cpu_usage()));
        let cpu_count = self.system.cpus().len().max(1);
        let cpu_percent = process_cpu / usize_as_f32(cpu_count);
        let ram_mb = u64_as_f64(memory) / (1024.0 * 1024.0);
        let elapsed_hours = (self.started.elapsed().as_secs_f64() / 3600.0).max(1.0 / 3600.0);
        let llm_calls_per_hour = u64_as_f64(self.llm_calls) / elapsed_hours;
        let llm_inference_ms_average = average_duration(self.llm_duration, self.llm_calls);
        let decision_latency_ms_average = average_duration(self.decision_duration, self.decisions);
        let mut warnings = Vec::new();
        if ram_mb > u64_as_f64(limits.maximum_ram_mb) {
            warnings.push(format!(
                "RAM {:.0} MB exceeds configured {} MB limit",
                ram_mb, limits.maximum_ram_mb
            ));
        }
        if llm_inference_ms_average > 30_000.0 {
            warnings.push("average LLM inference exceeds 30 seconds".to_owned());
        }
        if llm_calls_per_hour > 240.0 {
            warnings.push("LLM call frequency exceeds 240 per hour".to_owned());
        }
        PerformanceSnapshot {
            ram_mb,
            cpu_percent,
            llm_inference_ms_average,
            llm_calls_per_hour,
            sqlite_size_bytes,
            decision_latency_ms_average,
            warnings,
        }
    }
}

fn average_duration(total: Duration, count: u64) -> f64 {
    if count == 0 {
        0.0
    } else {
        total.as_secs_f64() * 1000.0 / u64_as_f64(count)
    }
}

fn usize_as_f32(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap_or(u16::MAX))
}

fn u64_as_f64(value: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let converted = value as f64;
    converted
}

#[cfg(test)]
mod tests {
    use crate::config::AppConfig;

    use super::*;

    #[test]
    fn monitor_tracks_latency_and_database_size() {
        let mut monitor = PerformanceMonitor::new().expect("monitor");
        monitor.record_llm_inference(Duration::from_millis(200));
        monitor.record_decision(Duration::from_millis(10));
        let snapshot = monitor.snapshot(42, &AppConfig::default().performance);
        assert!((snapshot.llm_inference_ms_average - 200.0).abs() < 0.1);
        assert!((snapshot.decision_latency_ms_average - 10.0).abs() < 0.1);
        assert_eq!(snapshot.sqlite_size_bytes, 42);
    }
}
