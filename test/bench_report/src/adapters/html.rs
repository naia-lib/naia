use std::fmt::Write as FmtWrite;

use crate::core::grouper::BenchGroup;
use crate::core::model::BenchResult;
use crate::ports::sink::ReportSink;

const CHART_JS: &str = include_str!("../../assets/chart.min.js");

/// Renders a self-contained Chart.js HTML report to stdout.
pub struct HtmlSink {
    pub title: String,
}

impl ReportSink for HtmlSink {
    fn emit(&self, results: &[BenchResult], groups: &[BenchGroup]) {
        print!("{}", render_html(results, groups, &self.title));
    }
}

pub fn render_html(all_results: &[BenchResult], groups: &[BenchGroup], title: &str) -> String {
    let mut out = String::new();
    let timestamp = "2026-04-24";

    write!(out, r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
  body {{ font-family: system-ui, sans-serif; margin: 2rem; background: #f8f9fa; color: #212529; }}
  h1 {{ color: #1a1a2e; }}
  h2 {{ color: #16213e; border-bottom: 2px solid #4e79a7; padding-bottom: 4px; margin-top: 2.5rem; }}
  .summary-table {{ width: 100%; border-collapse: collapse; margin-bottom: 2rem; }}
  .summary-table th, .summary-table td {{ border: 1px solid #dee2e6; padding: 6px 10px; text-align: left; }}
  .summary-table th {{ background: #4e79a7; color: white; }}
  .summary-table tr:nth-child(even) {{ background: #e9ecef; }}
  .chart-grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(480px, 1fr)); gap: 1.5rem; }}
  .chart-card {{ background: white; border-radius: 8px; box-shadow: 0 2px 8px rgba(0,0,0,.1); padding: 1rem; }}
  canvas {{ max-height: 320px; }}
  .meta {{ color: #6c757d; font-size: .85rem; margin-bottom: 1.5rem; }}
</style>
</head>
<body>
<h1>{title}</h1>
<p class="meta">Generated: {timestamp} &nbsp;|&nbsp; {n} benchmarks</p>
"#, title = html_escape(title), timestamp = timestamp, n = all_results.len()).unwrap();

    out.push_str("<h2>Summary</h2>\n<table class=\"summary-table\">\n");
    out.push_str("<tr><th>Benchmark</th><th>Median</th><th>Std Dev</th></tr>\n");
    for r in all_results {
        let (median_label, unit) = format_ns(r.median_ns);
        let (dev_label, _)       = format_ns(r.std_dev_ns);
        let _ = write!(
            out,
            "<tr><td>{}</td><td>{:.3} {}</td><td>± {:.3} {}</td></tr>\n",
            html_escape(&r.id), median_label, unit, dev_label, unit
        );
    }
    out.push_str("</table>\n");

    let categories = {
        let mut cats: Vec<&str> = groups.iter().map(|g| g.category.as_str()).collect();
        cats.dedup();
        cats
    };
    for cat in categories {
        let _ = write!(out, "<h2>{}</h2>\n<div class=\"chart-grid\">\n", html_escape(cat));
        for group in groups.iter().filter(|g| g.category == cat) {
            let cfg = build_chart(group);
            emit_chart_card(&mut out, &cfg);
        }
        out.push_str("</div>\n");
    }

    out.push_str("<script>");
    out.push_str(CHART_JS);
    out.push_str("</script>\n");
    out.push_str("<script>window.__naia_charts__.forEach(function(c){new Chart(document.getElementById(c.id),c.config);});</script>\n");
    out.push_str("</body>\n</html>\n");
    out
}

// ─── Chart building (merged from charts.rs) ───────────────────────────────────

struct ChartConfig {
    json: String,
}

const COLORS: &[&str] = &[
    "#4e79a7", "#f28e2b", "#e15759", "#76b7b2",
    "#59a14f", "#edc948", "#b07aa1", "#ff9da7",
];

fn build_chart(group: &BenchGroup) -> ChartConfig {
    let title = format!("{}/{}", group.category, group.sub_name);
    let chart_type = pick_chart_type(&group.category, &group.sub_name);
    let json = match chart_type {
        "line" => build_line_chart(group, &title),
        _      => build_bar_chart(group, &title),
    };
    ChartConfig { json }
}

fn pick_chart_type(category: &str, sub_name: &str) -> &'static str {
    match (category, sub_name) {
        ("tick",      _)                       => "line",
        ("spawn",     "burst/entities")        => "line",
        ("update",    "bulk/mutations")        => "line",
        ("authority", "contention/users")      => "line",
        _ => "bar",
    }
}

fn build_line_chart(group: &BenchGroup, title: &str) -> String {
    let labels = labels(group);
    let raw    = medians_ns(group);
    let unit   = pick_unit(raw.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let data: Vec<f64> = raw.iter().map(|v| to_display(*v, unit)).collect();
    let color = COLORS[0];
    format!(
        r#"{{"type":"line","data":{{"labels":{lj},"datasets":[{{"label":"{t}","data":{dj},"borderColor":"{c}","backgroundColor":"{c}22","tension":0.1,"pointRadius":4}}]}},"options":{{"responsive":true,"plugins":{{"title":{{"display":true,"text":"{t}"}}}},"scales":{{"x":{{"title":{{"display":true,"text":"{xl}"}}}},"y":{{"title":{{"display":true,"text":"time ({u})"}},"beginAtZero":true}}}}}}}}"#,
        lj = serde_json::to_string(&labels).unwrap(),
        dj = serde_json::to_string(&data).unwrap(),
        t  = escape_js(title), c = color,
        xl = x_label(group), u = unit,
    )
}

fn build_bar_chart(group: &BenchGroup, title: &str) -> String {
    let labels = labels(group);
    let raw    = medians_ns(group);
    let unit   = pick_unit(raw.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
    let data: Vec<f64> = raw.iter().map(|v| to_display(*v, unit)).collect();
    let bg: Vec<String> = (0..data.len()).map(|i| COLORS[i % COLORS.len()].to_string()).collect();
    format!(
        r#"{{"type":"bar","data":{{"labels":{lj},"datasets":[{{"label":"{t}","data":{dj},"backgroundColor":{bj}}}]}},"options":{{"responsive":true,"plugins":{{"title":{{"display":true,"text":"{t}"}}}},"scales":{{"x":{{"title":{{"display":true,"text":"{xl}"}}}},"y":{{"title":{{"display":true,"text":"time ({u})"}},"beginAtZero":true}}}}}}}}"#,
        lj = serde_json::to_string(&labels).unwrap(),
        dj = serde_json::to_string(&data).unwrap(),
        bj = serde_json::to_string(&bg).unwrap(),
        t  = escape_js(title),
        xl = x_label(group), u = unit,
    )
}

fn labels(group: &BenchGroup) -> Vec<String> {
    group.results.iter().map(|r| r.param.clone()).collect()
}
fn medians_ns(group: &BenchGroup) -> Vec<f64> {
    group.results.iter().map(|r| r.median_ns).collect()
}
fn pick_unit(max_ns: f64) -> &'static str {
    if max_ns >= 1_000_000.0 { "ms" } else if max_ns >= 1_000.0 { "µs" } else { "ns" }
}
fn to_display(ns: f64, unit: &str) -> f64 {
    match unit { "µs" => ns / 1_000.0, "ms" => ns / 1_000_000.0, _ => ns }
}
fn x_label(group: &BenchGroup) -> &'static str {
    let s = group.sub_name.as_str();
    if s.contains("entities") { "entity count" }
    else if s.contains("mutations") { "mutation count" }
    else if s.contains("users") { "user count" }
    else if s.contains("components") { "component count" }
    else { "variant" }
}
fn escape_js(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
fn format_ns(ns: f64) -> (f64, &'static str) {
    if ns >= 1_000_000.0 { (ns / 1_000_000.0, "ms") }
    else if ns >= 1_000.0 { (ns / 1_000.0, "µs") }
    else { (ns, "ns") }
}

static CHART_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn emit_chart_card(out: &mut String, cfg: &ChartConfig) {
    let idx       = CHART_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let canvas_id = format!("chart_{idx}");
    let _ = write!(out, "<div class=\"chart-card\"><canvas id=\"{canvas_id}\"></canvas></div>\n");
    let _ = write!(
        out,
        "<script>window.__naia_charts__ = window.__naia_charts__ || [];\
         window.__naia_charts__.push({{id:\"{canvas_id}\",config:{config}}});</script>\n",
        canvas_id = canvas_id, config = cfg.json,
    );
}
