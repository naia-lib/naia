mod wins;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let input_path = args
        .iter()
        .position(|a| a == "--input")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let path = match input_path {
        Some(p) => p,
        None => {
            eprintln!("naia-bench: --input <results.json> is required");
            eprintln!("Usage: naia-bench --assert-wins --input <results.json>");
            std::process::exit(1);
        }
    };

    let body = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("naia-bench: cannot read {}: {}", path, e);
        std::process::exit(1);
    });

    let results: Vec<bench_core::BenchResult> =
        serde_json::from_str(&body).unwrap_or_else(|e| {
            eprintln!("naia-bench: cannot parse {}: {}", path, e);
            std::process::exit(1);
        });

    if results.is_empty() {
        eprintln!("naia-bench: no results in input file");
        std::process::exit(1);
    }

    if args.iter().any(|a| a == "--assert-wins") {
        let outcome = wins::run(&results);
        if outcome.failed() {
            std::process::exit(1);
        }
        return;
    }

    eprintln!("naia-bench: no mode specified — use --assert-wins");
    std::process::exit(1);
}
