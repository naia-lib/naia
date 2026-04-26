use std::collections::HashMap;

use crate::core::model::BenchResult;

/// A named group of bench results that map to one chart.
#[derive(Debug)]
pub struct BenchGroup {
    pub category: String,
    pub sub_name: String,
    pub results: Vec<BenchResult>,
}

/// Partition results by (category, sub-benchmark prefix).
/// E.g. "tick/idle/entities/100" and ".../10000" end up in the same group.
pub fn group_results(results: Vec<BenchResult>) -> Vec<BenchGroup> {
    let mut map: HashMap<(String, String), Vec<BenchResult>> = HashMap::new();

    for result in results {
        let sub_prefix = sub_prefix(&result.sub_id);
        let key = (result.category.clone(), sub_prefix);
        map.entry(key).or_default().push(result);
    }

    let mut groups: Vec<BenchGroup> = map
        .into_iter()
        .map(|((category, sub_name), mut results)| {
            results.sort_by(|a, b| {
                let an = a.param.parse::<f64>().unwrap_or(0.0);
                let bn = b.param.parse::<f64>().unwrap_or(0.0);
                an.partial_cmp(&bn).unwrap_or(std::cmp::Ordering::Equal)
            });
            BenchGroup { category, sub_name, results }
        })
        .collect();

    groups.sort_by(|a, b| {
        a.category.cmp(&b.category).then(a.sub_name.cmp(&b.sub_name))
    });
    groups
}

fn sub_prefix(sub_id: &str) -> String {
    let parts: Vec<&str> = sub_id.rsplitn(2, '/').collect();
    if parts.len() == 2 {
        let last = parts[0];
        if last.parse::<f64>().is_ok() || looks_like_variant(last) {
            return parts[1].to_string();
        }
    }
    sub_id.to_string()
}

fn looks_like_variant(s: &str) -> bool {
    matches!(s, "mutable" | "immutable" | "legacy" | "coalesced")
}
