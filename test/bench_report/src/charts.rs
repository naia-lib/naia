use crate::grouper::BenchGroup;

/// A Chart.js chart configuration as a JSON string.
#[allow(dead_code)]
pub struct ChartConfig {
    pub title: String,
    pub chart_type: &'static str,
    pub json: String,
}

/// Colour palette — same 8 colours recycled across charts.
const COLORS: &[&str] = &[
    "#4e79a7", "#f28e2b", "#e15759", "#76b7b2",
    "#59a14f", "#edc948", "#b07aa1", "#ff9da7",
];

pub fn build_chart(group: &BenchGroup) -> ChartConfig {
    let title = format!("{}/{}", group.category, group.sub_name);
    let chart_type = pick_chart_type(&group.category, &group.sub_name);

    let json = match chart_type {
        "line" => build_line_chart(group, &title),
        _      => build_bar_chart(group, &title),
    };

    ChartConfig { title, chart_type, json }
}

fn pick_chart_type(category: &str, sub_name: &str) -> &'static str {
    match (category, sub_name) {
        ("tick",      _) => "line",
        ("spawn",     "burst/entities") => "line",
        ("update",    "bulk/mutations") => "line",
        ("authority", "contention/users") => "line",
        _ => "bar",
    }
}

fn labels(group: &BenchGroup) -> Vec<String> {
    group.results.iter().map(|r| r.param.clone()).collect()
}

fn median_values_ns(group: &BenchGroup) -> Vec<f64> {
    group.results.iter().map(|r| r.median_ns).collect()
}

fn ns_to_display(ns: f64, unit_label: &str) -> f64 {
    match unit_label {
        "µs" => ns / 1_000.0,
        "ms" => ns / 1_000_000.0,
        _    => ns,
    }
}

fn pick_display_unit(max_ns: f64) -> &'static str {
    if max_ns >= 1_000_000.0 { "ms" }
    else if max_ns >= 1_000.0 { "µs" }
    else { "ns" }
}

fn build_line_chart(group: &BenchGroup, title: &str) -> String {
    let labels = labels(group);
    let raw = median_values_ns(group);
    let max = raw.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let unit = pick_display_unit(max);
    let data: Vec<f64> = raw.iter().map(|v| ns_to_display(*v, unit)).collect();

    let labels_json = serde_json::to_string(&labels).unwrap();
    let data_json   = serde_json::to_string(&data).unwrap();
    let color = COLORS[0];

    format!(
        r#"{{
  "type": "line",
  "data": {{
    "labels": {labels_json},
    "datasets": [{{
      "label": "{title}",
      "data": {data_json},
      "borderColor": "{color}",
      "backgroundColor": "{color}22",
      "tension": 0.1,
      "pointRadius": 4
    }}]
  }},
  "options": {{
    "responsive": true,
    "plugins": {{ "title": {{ "display": true, "text": "{title}" }} }},
    "scales": {{
      "x": {{ "title": {{ "display": true, "text": "{x_label}" }} }},
      "y": {{ "title": {{ "display": true, "text": "time ({unit})" }}, "beginAtZero": true }}
    }}
  }}
}}"#,
        labels_json = labels_json,
        data_json   = data_json,
        title       = escape_js(title),
        color       = color,
        x_label     = x_label_for(group),
        unit        = unit,
    )
}

fn build_bar_chart(group: &BenchGroup, title: &str) -> String {
    let labels = labels(group);
    let raw = median_values_ns(group);
    let max = raw.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let unit = pick_display_unit(max);
    let data: Vec<f64> = raw.iter().map(|v| ns_to_display(*v, unit)).collect();

    let labels_json = serde_json::to_string(&labels).unwrap();
    let data_json   = serde_json::to_string(&data).unwrap();
    let bg_colors: Vec<String> = (0..data.len())
        .map(|i| COLORS[i % COLORS.len()].to_string())
        .collect();
    let bg_json = serde_json::to_string(&bg_colors).unwrap();

    format!(
        r#"{{
  "type": "bar",
  "data": {{
    "labels": {labels_json},
    "datasets": [{{
      "label": "{title}",
      "data": {data_json},
      "backgroundColor": {bg_json}
    }}]
  }},
  "options": {{
    "responsive": true,
    "plugins": {{ "title": {{ "display": true, "text": "{title}" }} }},
    "scales": {{
      "x": {{ "title": {{ "display": true, "text": "{x_label}" }} }},
      "y": {{ "title": {{ "display": true, "text": "time ({unit})" }}, "beginAtZero": true }}
    }}
  }}
}}"#,
        labels_json = labels_json,
        data_json   = data_json,
        title       = escape_js(title),
        bg_json     = bg_json,
        x_label     = x_label_for(group),
        unit        = unit,
    )
}

fn x_label_for(group: &BenchGroup) -> &'static str {
    let sub = group.sub_name.as_str();
    if sub.contains("entities") { "entity count" }
    else if sub.contains("mutations") { "mutation count" }
    else if sub.contains("users") { "user count" }
    else if sub.contains("components") { "component count" }
    else { "variant" }
}

fn escape_js(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
