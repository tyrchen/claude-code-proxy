use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Metrics for tool calling performance and reliability
///
/// Thread-safe atomic counters for tracking tool usage patterns.
#[derive(Default)]
pub struct ToolMetrics {
    /// Total number of tool calls processed
    pub total_calls: AtomicU64,

    /// Successful tool transformations
    pub successful_transformations: AtomicU64,

    /// Failed transformations
    pub failed_transformations: AtomicU64,

    /// Tool results processed
    pub tool_results_processed: AtomicU64,

    /// State lookup failures (missing tool_use_id)
    pub state_lookup_failures: AtomicU64,

    /// Total transformation time in microseconds
    pub total_transform_time_us: AtomicU64,
}

impl ToolMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful tool transformation
    pub fn record_transformation(&self, duration: Duration) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.successful_transformations
            .fetch_add(1, Ordering::Relaxed);
        self.total_transform_time_us
            .fetch_add(duration.as_micros() as u64, Ordering::Relaxed);
    }

    /// Record a failed transformation
    pub fn record_failure(&self) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.failed_transformations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a tool result being processed
    pub fn record_tool_result(&self) {
        self.tool_results_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a state lookup failure
    pub fn record_state_lookup_failure(&self) {
        self.state_lookup_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get average transformation time in microseconds
    pub fn avg_transform_time_us(&self) -> u64 {
        let total = self.total_transform_time_us.load(Ordering::Relaxed);
        let count = self.successful_transformations.load(Ordering::Relaxed);
        if count > 0 { total / count } else { 0 }
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.total_calls.load(Ordering::Relaxed);
        let successful = self.successful_transformations.load(Ordering::Relaxed);
        if total > 0 {
            (successful as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Get snapshot of current metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            total_calls: self.total_calls.load(Ordering::Relaxed),
            successful_transformations: self.successful_transformations.load(Ordering::Relaxed),
            failed_transformations: self.failed_transformations.load(Ordering::Relaxed),
            tool_results_processed: self.tool_results_processed.load(Ordering::Relaxed),
            state_lookup_failures: self.state_lookup_failures.load(Ordering::Relaxed),
            avg_transform_time_us: self.avg_transform_time_us(),
            success_rate: self.success_rate(),
        }
    }

    /// Reset all metrics (useful for testing)
    pub fn reset(&self) {
        self.total_calls.store(0, Ordering::Relaxed);
        self.successful_transformations.store(0, Ordering::Relaxed);
        self.failed_transformations.store(0, Ordering::Relaxed);
        self.tool_results_processed.store(0, Ordering::Relaxed);
        self.state_lookup_failures.store(0, Ordering::Relaxed);
        self.total_transform_time_us.store(0, Ordering::Relaxed);
    }
}

/// Immutable snapshot of metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub total_calls: u64,
    pub successful_transformations: u64,
    pub failed_transformations: u64,
    pub tool_results_processed: u64,
    pub state_lookup_failures: u64,
    pub avg_transform_time_us: u64,
    pub success_rate: f64,
}

impl std::fmt::Display for MetricsSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tool Metrics: {} calls ({:.1}% success), {} results, {} state failures, avg {:.2}ms",
            self.total_calls,
            self.success_rate,
            self.tool_results_processed,
            self.state_lookup_failures,
            self.avg_transform_time_us as f64 / 1000.0
        )
    }
}

lazy_static::lazy_static! {
    /// Global metrics instance
    pub static ref TOOL_METRICS: ToolMetrics = ToolMetrics::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_record_transformations() {
        let metrics = ToolMetrics::new();

        metrics.record_transformation(Duration::from_micros(100));
        metrics.record_transformation(Duration::from_micros(200));
        metrics.record_transformation(Duration::from_micros(300));

        assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 3);
        assert_eq!(
            metrics.successful_transformations.load(Ordering::Relaxed),
            3
        );
        assert_eq!(metrics.avg_transform_time_us(), 200); // (100+200+300)/3
    }

    #[test]
    fn test_record_failures() {
        let metrics = ToolMetrics::new();

        metrics.record_transformation(Duration::from_micros(100));
        metrics.record_failure();
        metrics.record_failure();

        assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 3);
        assert_eq!(
            metrics.successful_transformations.load(Ordering::Relaxed),
            1
        );
        assert_eq!(metrics.failed_transformations.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_success_rate() {
        let metrics = ToolMetrics::new();

        metrics.record_transformation(Duration::from_micros(50));
        metrics.record_transformation(Duration::from_micros(50));
        metrics.record_transformation(Duration::from_micros(50));
        metrics.record_failure();

        assert_eq!(metrics.success_rate(), 75.0); // 3/4 = 75%
    }

    #[test]
    fn test_tool_results() {
        let metrics = ToolMetrics::new();

        metrics.record_tool_result();
        metrics.record_tool_result();
        metrics.record_state_lookup_failure();

        assert_eq!(metrics.tool_results_processed.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.state_lookup_failures.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_snapshot() {
        let metrics = ToolMetrics::new();

        metrics.record_transformation(Duration::from_micros(100));
        metrics.record_failure();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_calls, 2);
        assert_eq!(snapshot.successful_transformations, 1);
        assert_eq!(snapshot.success_rate, 50.0);
    }

    #[test]
    fn test_reset() {
        let metrics = ToolMetrics::new();

        metrics.record_transformation(Duration::from_micros(100));
        metrics.record_failure();

        assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 2);

        metrics.reset();

        assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.avg_transform_time_us(), 0);
    }

    #[test]
    fn test_thread_safety() {
        use std::sync::Arc;

        let metrics = Arc::new(ToolMetrics::new());

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let m = Arc::clone(&metrics);
                thread::spawn(move || {
                    m.record_transformation(Duration::from_micros(50));
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(metrics.total_calls.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn test_display_format() {
        let snapshot = MetricsSnapshot {
            total_calls: 100,
            successful_transformations: 95,
            failed_transformations: 5,
            tool_results_processed: 90,
            state_lookup_failures: 2,
            avg_transform_time_us: 1500,
            success_rate: 95.0,
        };

        let output = format!("{}", snapshot);
        assert!(output.contains("100 calls"));
        assert!(output.contains("95.0% success"));
        assert!(output.contains("90 results"));
        assert!(output.contains("1.50ms"));
    }
}
