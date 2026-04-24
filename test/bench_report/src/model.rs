use serde::Deserialize;

/// One completed benchmark result from cargo-criterion --message-format=json.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BenchResult {
    /// Full benchmark id, e.g. "tick/idle/entities/10000"
    pub id: String,
    /// Top-level category: "tick", "spawn", "update", "authority", "wire"
    pub category: String,
    /// Sub-benchmark name within category, e.g. "idle/entities/10000"
    pub sub_id: String,
    /// Parameter label extracted from the last path segment, e.g. "10000"
    pub param: String,
    /// Median wall time in nanoseconds
    pub median_ns: f64,
    /// Standard deviation in nanoseconds (may be 0 if unavailable)
    pub std_dev_ns: f64,
    /// Throughput unit if present ("elements" or "bytes")
    pub throughput_unit: Option<String>,
    /// Number of elements/bytes per iteration
    pub throughput_per_iter: Option<u64>,
}

/// Deserialisation target for the cargo-criterion JSON stream.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CriterionMessage {
    pub reason: String,
    pub id: Option<String>,
    pub typical: Option<Estimate>,
    pub median: Option<Estimate>,
    pub mean: Option<Estimate>,
    pub std_dev: Option<Estimate>,
    pub median_abs_dev: Option<Estimate>,
    pub throughput: Option<Vec<ThroughputEntry>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Estimate {
    pub estimate: f64,
    pub unit: String,
}

#[derive(Debug, Deserialize)]
pub struct ThroughputEntry {
    pub per_iteration: u64,
    pub unit: String,
}
