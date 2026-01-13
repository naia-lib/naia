use clap::{Parser, Subcommand};
use naia_spec_tool::{index::Index, packet, coverage, adequacy, stats, lint, check_orphans, check_refs, validate};

fn find_workspace_root() -> std::path::PathBuf {
    let mut root = std::env::current_dir().expect("Failed to get current directory");
    if !root.join("specs/contracts").exists() {
         if let Some(parent) = root.parent() {
             if parent.join("specs/contracts").exists() {
                 root = parent.to_path_buf();
             }
         }
    }
    root
}

#[derive(Parser)]
#[command(name = "spec_tool")]
#[command(bin_name = "spec_tool")]
#[command(disable_help_flag = true)] // We handle help manually
#[command(disable_help_subcommand = true)]
struct Cli {
    #[arg(long, global = true)]
    deterministic: bool,

    #[command(subcommand)]
    command: Option<Commands>, // Option to handle no-args case
}

#[derive(Subcommand)]
enum Commands {
    /// Generate NAIA_SPECS.md bundle
    Bundle {
        /// Output file (optional)
        #[arg(index = 1)]
        output: Option<String>,

        /// Exclude template
        #[arg(long)]
        no_template: bool,
    },
    /// Check all specs for consistency issues
    Lint,
    /// Run all validation checks (lint + check-refs + check-orphans)
    Validate,
    /// Extract all contract IDs to registry file
    Registry {
        /// Output file (optional)
        #[arg(index = 1)]
        output: Option<String>,
    },
    /// Find MUST/MUST NOT statements without contract IDs
    CheckOrphans,
    /// Verify all cross-reference links
    CheckRefs,
    /// Show statistics about specs
    Stats,
    /// Analyze contract test coverage
    Coverage,
    /// Check obligation-to-label mapping adequacy (no cargo)
    Adequacy {
        /// Fail if issues found
        #[arg(long)]
        strict: bool,
    },
    /// Generate test skeleton for a contract
    GenTest {
        /// Contract ID
        #[arg(index = 1)]
        contract_id: String,
    },
    /// Generate contract-to-test matrix
    Traceability {
        #[arg(index = 1)]
        output: Option<String>,
    },
    /// Generate contract review packet
    Packet {
        /// Contract ID
        #[arg(index = 1)]
        contract_id: String,

        /// Output path
        #[arg(long)]
        out: Option<String>,

        /// Generate packet with full test code
        #[arg(long)]
        full_tests: bool,
    },
    /// CI-grade verification: validate + lint + tests + coverage
    Verify {
        /// Test only one contract
        #[arg(long)]
        contract: Option<String>,

        /// Fail if orphan MUSTs exist
        #[arg(long)]
        strict_orphans: bool,

        /// Fail if any contracts uncovered
        #[arg(long)]
        strict_coverage: bool,

        /// Include full reports with --contract
        #[arg(long)]
        full_report: bool,

        /// Write summary to file
        #[arg(long)]
        write_report: Option<String>,
    },
    /// Show help message
    Help,
}

const HELP_TEXT: &str = r#"spec_tool - Comprehensive CLI for Naia specifications management

USAGE:
    cargo run -p naia_spec_tool -- <command> [options]

COMMANDS:
    bundle [output]     Generate NAIA_SPECS.md bundle
                        Options: --no-template (exclude template)

    lint                Check all specs for consistency issues
                        - Title format (Spec: prefix)
                        - Contract ID format
                        - Test obligation format
                        - Terminology consistency

    validate            Run all validation checks (lint + check-refs + check-orphans)

    registry [output]   Extract all contract IDs to registry file
                        Default output: CONTRACT_REGISTRY.md

    check-orphans       Find MUST/MUST NOT statements without contract IDs

    check-refs          Verify all cross-reference links resolve

    stats               Show statistics about specifications

    coverage            Analyze contract test coverage
                        Shows which contracts have test annotations

    adequacy [options]  Check obligation-to-label mapping adequacy (no cargo)
                        Options:
                          --strict      Fail if any contracts don't meet adequacy
                        Checks:
                          - Contracts have test functions
                          - Tests have labeled assertions (spec_expect)
                          - Obligation IDs map to labeled assertions

    gen-test <id>       Generate test skeleton for a contract
                        Example: cargo run -p naia_spec_tool -- gen-test entity-scopes-07

    traceability [out]  Generate contract-to-test traceability matrix
                        Default output: TRACEABILITY.md

    packet <id> [opts]  Generate contract review packet (spec + tests)
                        Options:
                          --out <path>      Output path (default: packets/<id>.md)
                          --full-tests      Include full test bodies (default: assertions only)
                        Example: cargo run -p naia_spec_tool -- packet connection-01

    verify [options]    CI-grade verification: validate + lint + tests + coverage
                        Options:
                          --contract <id>       Run tests only for specific contract
                          --strict-orphans      Fail if orphan MUSTs exist
                          --strict-coverage     Fail if any contracts uncovered
                          --full-report         Include full reports with --contract
                          --write-report <path> Write summary to file

    help                Show this help message

EXAMPLES:
    cargo run -p naia_spec_tool -- bundle                    # Generate NAIA_SPECS.md
    cargo run -p naia_spec_tool -- lint                      # Check for issues
    cargo run -p naia_spec_tool -- validate                  # Full validation
    cargo run -p naia_spec_tool -- registry                  # Generate contract registry
    cargo run -p naia_spec_tool -- stats                     # Show spec statistics"#;

fn main() {
    // Force colored output to match legacy bash script
    colored::control::set_override(true);

    let args: Vec<String> = std::env::args().collect();
    
    // Handle manual help flags or empty args
    if args.len() == 1 || 
       (args.len() > 1 && (args[1] == "--help" || args[1] == "-h")) {
        println!("{}\n", HELP_TEXT);
        return;
    }

    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(_e) => {
             // Try to find the unknown command name
             // e.kind() == clap::error::ErrorKind::InvalidSubcommand
             // Extract context if possible, or just parse args manually for message
             let unknown_cmd = &args[1];
             // Manual ANSI codes to match legacy script behavior exactly
             let red_x = "\x1b[0;31m✗\x1b[0m"; 
             println!("{} Unknown command: {}", red_x, unknown_cmd);
             println!();
             println!("{}\n", HELP_TEXT);
             std::process::exit(1);
        }
    };

    // If we get here with None, it means no subcommand but we handled empty args above?
    // Actually Cli::try_parse might succeed with None if command is Option.
    // If we have random flags but no subcommand, it might fail or return None.
    
    match &cli.command {
        Some(Commands::Help) => {
            println!("{}\n", HELP_TEXT);
        }
        Some(Commands::Stats) => {
             let root = find_workspace_root();
             let index = Index::build(root).expect("Failed to build index");
             stats::run_stats(&index).expect("Failed to run stats");
        }
        Some(Commands::Packet { contract_id, full_tests, out }) => {
             let root = find_workspace_root();
             let index = Index::build(root).expect("Failed to build index");
             packet::generate_packet(&index, contract_id, *full_tests, out.clone(), cli.deterministic).expect("Failed to generate packet");
        }
        Some(Commands::Registry { output }) => {
             let root = find_workspace_root();
             naia_spec_tool::registry::run_registry(&root, output.clone(), cli.deterministic).expect("Failed to run registry");
        }
        Some(Commands::Coverage) => {
             let root = find_workspace_root();
             let index = Index::build(root).expect("Failed to build index");
             coverage::run_coverage(&index).expect("Failed to run coverage");
        }
        Some(Commands::Adequacy { strict }) => {
             let root = find_workspace_root();
             let index = Index::build(root).expect("Failed to build index");
             if let Err(_) = adequacy::run_adequacy(&index, *strict) {
                 std::process::exit(1);
             }
        }
        Some(Commands::Lint) => {
             let root = find_workspace_root();
             // Lint scans the directory directly, doesn't need Index parsing
             match lint::run_lint(&root) {
                 Ok(issues) if issues > 0 => std::process::exit(issues as i32),
                 Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); },
                 _ => {}
             }
        }
        Some(Commands::Validate) => {
             let root = find_workspace_root();
             match validate::run_validate(&root) {
                 Ok(errors) if errors > 0 => std::process::exit(errors as i32),
                 Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); },
                 _ => {}
             }
        }
        Some(Commands::CheckOrphans) => {
             let root = find_workspace_root();
             match check_orphans::run_check_orphans(&root) {
                 Ok(count) if count > 0 => std::process::exit(count as i32),
                 Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); },
                 _ => {}
             }
        }
        Some(Commands::CheckRefs) => {
             let root = find_workspace_root();
             match check_refs::run_check_refs(&root) {
                 Ok(errors) if errors > 0 => std::process::exit(errors as i32),
                 Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); },
                 _ => {}
             }
        }
        Some(Commands::Traceability { output }) => {
             let root = find_workspace_root();
             naia_spec_tool::traceability::run_traceability(&root, output.clone(), false, cli.deterministic).expect("Failed to run traceability");
        }
        Some(Commands::Verify { contract, strict_orphans, strict_coverage, full_report, write_report }) => {
             let root = find_workspace_root();
             match naia_spec_tool::verify::run_verify(&root, contract.clone(), *strict_orphans, *strict_coverage, *full_report, write_report.clone(), cli.deterministic) {
                 Ok(errors) if errors > 0 => std::process::exit(errors as i32),
                 Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); },
                 _ => {}
             }
        }
        Some(Commands::Bundle { output, .. }) => {
             let root = find_workspace_root();
             naia_spec_tool::bundle::run_bundle(&root, output.clone(), cli.deterministic).expect("Failed to run bundle");
        }
        Some(Commands::GenTest { contract_id }) => {
             let root = find_workspace_root();
             naia_spec_tool::gen_test::run_gen_test(&root, contract_id).expect("Failed to run gen-test");
        }
        _ => {
            // Should be handled by error checking above, or explicit variants
            println!("Command not implemented yet");
        }
    }
}
