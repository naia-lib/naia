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

    let title = std::env::args()
        .skip_while(|a| a != "--title")
        .nth(1)
        .unwrap_or_else(|| "Naia Benchmark Report".to_string());

    let groups = grouper::group_results(results.clone());
    let html = renderer::render_html(&results, &groups, &title);
    print!("{}", html);
}
