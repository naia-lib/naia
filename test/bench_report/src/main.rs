mod adapters;
mod core;
mod ports;

use adapters::capacity_report::CapacityReportSink;
use adapters::criterion::CriterionSource;
use adapters::html::HtmlSink;
use adapters::wins_sink::WinsSink;
use ports::sink::{AssertionSink, CapacitySink, ReportSink};
use ports::source::BenchResultSource;

fn main() {
    let results = CriterionSource.load();

    if results.is_empty() {
        eprintln!("naia-bench-report: no benchmark-complete messages on stdin.");
        eprintln!(
            "Usage: cargo criterion --message-format=json 2>/dev/null \
             | cargo run -p naia-bench-report [--assert-wins] [--capacity-report]"
        );
        std::process::exit(1);
    }

    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--assert-wins") {
        let outcome = core::assertions::run(&results);
        if !WinsSink.emit(&outcome) {
            std::process::exit(1);
        }
        return;
    }

    if args.iter().any(|a| a == "--capacity-report") {
        let profile  = core::capacity::profile_from_results(&results);
        let estimate = core::capacity::estimate(&profile);
        CapacityReportSink.emit(&estimate);
        return;
    }

    // Default: HTML report to stdout.
    let title = extract_title(&args);
    let groups = core::grouper::group_results(results.clone());
    HtmlSink { title }.emit(&results, &groups);
}

fn extract_title(args: &[String]) -> String {
    args.iter()
        .position(|a| a == "--title")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "Naia Benchmark Report".to_string())
}
