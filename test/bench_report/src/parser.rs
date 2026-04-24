use crate::model::{BenchResult, CriterionMessage};

/// Parse line-delimited JSON from `cargo criterion --message-format=json`
/// on stdin. Returns one BenchResult per "benchmark-complete" message.
pub fn parse_stdin() -> Vec<BenchResult> {
    use std::io::{self, BufRead};
    let stdin = io::stdin();
    let mut results = Vec::new();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };
        if let Ok(msg) = serde_json::from_str::<CriterionMessage>(&line) {
            if msg.reason != "benchmark-complete" {
                continue;
            }
            if let Some(result) = extract_result(msg) {
                results.push(result);
            }
        }
    }
    results
}

fn extract_result(msg: CriterionMessage) -> Option<BenchResult> {
    let id = msg.id?;

    let median_ns = msg
        .median
        .as_ref()
        .or(msg.typical.as_ref())
        .or(msg.mean.as_ref())
        .map(|e| to_ns(&e.estimate, &e.unit))?;

    let std_dev_ns = msg
        .std_dev
        .as_ref()
        .or(msg.median_abs_dev.as_ref())
        .map(|e| to_ns(&e.estimate, &e.unit))
        .unwrap_or(0.0);

    let throughput_unit = msg
        .throughput
        .as_ref()
        .and_then(|v| v.first())
        .map(|t| t.unit.clone());

    let throughput_per_iter = msg
        .throughput
        .as_ref()
        .and_then(|v| v.first())
        .map(|t| t.per_iteration);

    // id format: "tick/idle/entities/10000"
    let parts: Vec<&str> = id.splitn(2, '/').collect();
    let category = parts.first().copied().unwrap_or("other").to_string();
    let sub_id = parts.get(1).copied().unwrap_or(&id).to_string();
    let param = id.rsplit('/').next().unwrap_or("").to_string();

    Some(BenchResult {
        id,
        category,
        sub_id,
        param,
        median_ns,
        std_dev_ns,
        throughput_unit,
        throughput_per_iter,
    })
}

fn to_ns(value: &f64, unit: &str) -> f64 {
    match unit {
        "ns" => *value,
        "us" | "µs" => value * 1_000.0,
        "ms" => value * 1_000_000.0,
        "s" => value * 1_000_000_000.0,
        _ => *value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CriterionMessage;

    #[test]
    fn parses_benchmark_complete() {
        let json = r#"{
            "reason": "benchmark-complete",
            "id": "tick/idle/entities/10000",
            "typical": { "estimate": 123456.78, "unit": "ns" },
            "median":  { "estimate": 123000.0,  "unit": "ns" },
            "mean":    { "estimate": 123789.0,   "unit": "ns" },
            "std_dev": { "estimate": 500.0,      "unit": "ns" },
            "throughput": [{ "per_iteration": 10000, "unit": "elements" }]
        }"#;
        let msg: CriterionMessage = serde_json::from_str(json).unwrap();
        let result = extract_result(msg).unwrap();
        assert_eq!(result.category, "tick");
        assert_eq!(result.sub_id, "idle/entities/10000");
        assert_eq!(result.param, "10000");
        assert!((result.median_ns - 123000.0).abs() < 1.0);
        assert_eq!(result.throughput_unit.as_deref(), Some("elements"));
        assert_eq!(result.throughput_per_iter, Some(10000));
    }

    #[test]
    fn ignores_non_benchmark_complete() {
        let json = r#"{"reason": "benchmark-start", "id": "tick/idle/entities/100"}"#;
        let msg: CriterionMessage = serde_json::from_str(json).unwrap();
        // reason != "benchmark-complete" so extract_result returns None.
        assert!(msg.reason != "benchmark-complete");
        assert!(extract_result(msg).is_none());
    }
}
