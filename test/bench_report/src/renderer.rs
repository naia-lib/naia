use std::fmt::Write as FmtWrite;

use crate::charts::ChartConfig;
use crate::grouper::BenchGroup;
use crate::model::BenchResult;

const CHART_JS: &str = include_str!("../assets/chart.min.js");

pub fn render_html(all_results: &[BenchResult], groups: &[BenchGroup], title: &str) -> String {
    let mut out = String::new();

    let timestamp = "2026-04-24"; // deterministic; replace with runtime date if desired

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

    // Summary table
    out.push_str("<h2>Summary</h2>\n");
    out.push_str("<table class=\"summary-table\">\n");
    out.push_str("<tr><th>Benchmark</th><th>Median</th><th>Std Dev</th></tr>\n");
    for r in all_results {
        let (median_label, unit) = format_ns(r.median_ns);
        let (dev_label, _) = format_ns(r.std_dev_ns);
        let _ = write!(
            out,
            "<tr><td>{}</td><td>{:.3} {}</td><td>± {:.3} {}</td></tr>\n",
            html_escape(&r.id), median_label, unit, dev_label, unit
        );
    }
    out.push_str("</table>\n");

    // Charts grouped by category
    let categories = {
        let mut cats: Vec<&str> = groups.iter().map(|g| g.category.as_str()).collect();
        cats.dedup();
        cats
    };

    for cat in categories {
        let _ = write!(out, "<h2>{}</h2>\n<div class=\"chart-grid\">\n", html_escape(cat));
        for group in groups.iter().filter(|g| g.category == cat) {
            let cfg = crate::charts::build_chart(group);
            emit_chart_card(&mut out, &cfg);
        }
        out.push_str("</div>\n");
    }

    // Inline Chart.js + init scripts
    out.push_str("<script>");
    out.push_str(CHART_JS);
    out.push_str("</script>\n");
    out.push_str("<script>window.__naia_charts__.forEach(function(c){new Chart(document.getElementById(c.id),c.config);});</script>\n");
    out.push_str("</body>\n</html>\n");
    out
}

static CHART_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn emit_chart_card(out: &mut String, cfg: &ChartConfig) {
    let idx = CHART_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let canvas_id = format!("chart_{idx}");
    let _ = write!(
        out,
        "<div class=\"chart-card\"><canvas id=\"{canvas_id}\"></canvas></div>\n"
    );
    // Accumulate config in a global array so Chart.js is only initialised once.
    let _ = write!(
        out,
        "<script>window.__naia_charts__ = window.__naia_charts__ || [];\
         window.__naia_charts__.push({{id:\"{canvas_id}\",config:{config}}});</script>\n",
        canvas_id = canvas_id,
        config = cfg.json,
    );
}

fn format_ns(ns: f64) -> (f64, &'static str) {
    if ns >= 1_000_000.0 { (ns / 1_000_000.0, "ms") }
    else if ns >= 1_000.0 { (ns / 1_000.0, "µs") }
    else { (ns, "ns") }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
