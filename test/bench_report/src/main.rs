mod assert_wins;
mod charts;
mod grouper;
mod model;
mod parser;
mod renderer;

fn main() {
    let results = parser::parse_stdin();

    if results.is_empty() {
        eprintln!("naia_bench_report: no benchmark-complete messages received on stdin.");
        eprintln!("Usage: cargo criterion --message-format=json 2>/dev/null | cargo run -p naia-bench-report");
        std::process::exit(1);
    }

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--assert-wins") {
        let outcome = assert_wins::run(&results);
        if outcome.failed() {
            std::process::exit(1);
        }
        return;
    }

    let title = args
        .iter()
        .position(|a| a == "--title")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "Naia Benchmark Report".to_string());

    let groups = grouper::group_results(results.clone());
    let html = renderer::render_html(&results, &groups, &title);
    print!("{}", html);
}
