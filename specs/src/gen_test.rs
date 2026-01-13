use std::path::{Path, PathBuf};
use std::fs;
use crate::util::{print_info, print_error, basename};

pub fn run_gen_test(root: &Path, contract_id: &str) -> anyhow::Result<()> {
    if contract_id.is_empty() {
        print_error("Usage: ./spec_tool.sh gen-test <contract-id>");
        println!("Example: ./spec_tool.sh gen-test entity-scopes-07");
        return Ok(());
    }

    let contracts_dir = root.join("specs/contracts");
    let mut spec_file: Option<PathBuf> = None;

    // Find the spec file containing this contract
    if let Ok(entries) = fs::read_dir(&contracts_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |e| e == "md") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if content.contains(&format!("[{}]", contract_id)) && !basename(&path).contains("REGISTRY") {
                            spec_file = Some(path);
                            break;
                        }
                    }
                }
            }
        }
    }

    let spec_path = match spec_file {
        Some(path) => path,
        None => {
            print_error(&format!("Contract [{}] not found in any spec file", contract_id));
            return Ok(());
        }
    };

    let spec_basename = basename(&spec_path);
    print_info(&format!("Found contract in: {}", spec_basename));
    println!("");

    let fn_name = contract_id.replace('-', "_");

    println!("/// Contract: [{}]", contract_id);
    println!("/// Source: {}", spec_basename);
    println!("///");
    println!("/// Guarantee: TODO - Copy from spec");
    println!("///");
    println!("/// Scenario: TODO - Describe Given/When/Then");
    println!("/// Given:");
    println!("///   - TODO: Initial conditions");
    println!("/// When:");
    println!("///   - TODO: Trigger action");
    println!("/// Then:");
    println!("///   - TODO: Expected outcome");
    println!("#[test]");
    println!("fn {}_scenario_1() {{", fn_name);
    println!("    use naia_server::ServerConfig;");
    println!("    use naia_test::{{protocol, Auth, Scenario}};");
    println!("");
    println!("    let mut scenario = Scenario::new();");
    println!("    let test_protocol = protocol();");
    println!("");
    println!("    scenario.server_start(ServerConfig::default(), test_protocol.clone());");
    println!("");
    println!("    let _room_key = scenario.mutate(|ctx| {{");
    println!("        ctx.server(|server| server.make_room().key())");
    println!("    }});");
    println!("");
    println!("    // TODO: Connect clients as needed");
    println!("    // let client_key = client_connect(&mut scenario, &room_key, \"Client\", Auth::new(\"user\", \"pass\"), test_client_config(), test_protocol);");
    println!("");
    println!("    // TODO: Setup preconditions (Given)");
    println!("    scenario.mutate(|_ctx| {{");
    println!("        // Setup");
    println!("    }});");
    println!("");
    println!("    // TODO: Trigger action (When)");
    println!("    scenario.mutate(|_ctx| {{");
    println!("        // Action");
    println!("    }});");
    println!("");
    println!("    // TODO: Verify postconditions (Then)");
    println!("    scenario.expect(|_ctx| {{");
    println!("        // Assertion");
    println!("        todo!(\"Implement assertion for [{}]\")", contract_id);
    println!("    }});");
    println!("}}");

    Ok(())
}
