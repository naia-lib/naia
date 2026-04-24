//! Golden wire-trace record/check commands.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

/// Direction of a captured wire packet (serializable form for golden files).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoldenDirection {
    ClientToServer,
    ServerToClient,
}

/// A single packet entry in a golden trace file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenPacket {
    pub direction: GoldenDirection,
    /// Packet bytes as a hex string (for stable JSON diffs).
    pub hex: String,
}

/// A serializable golden trace file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenTrace {
    /// Scenario key used to reproduce this trace.
    pub scenario_key: String,
    /// Packets captured during the scenario, in order.
    pub packets: Vec<GoldenPacket>,
}

#[derive(Subcommand, Debug)]
pub enum TracesCommand {
    /// Record a golden trace for a named scenario
    Record {
        /// Scenario key (e.g. "contract06_spawn_in_scope")
        key: String,
    },
    /// Check all golden traces pass
    Check,
}

pub fn run(cmd: TracesCommand) -> Result<()> {
    match cmd {
        TracesCommand::Record { key } => record(&key),
        TracesCommand::Check => check(),
    }
}

/// Directory where golden trace JSON files are stored.
fn golden_dir() -> PathBuf {
    // Locate relative to CARGO_MANIFEST_DIR (test/spec_tool/) → test/golden_traces/
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir).join("..").join("golden_traces")
}

fn golden_path(key: &str) -> PathBuf {
    golden_dir().join(format!("{key}.json"))
}

/// Registry of all trace-capturable scenarios.
///
/// Each entry maps a scenario key to a function that returns the `Trace`
/// captured during its execution.
fn scenario_registry() -> HashMap<&'static str, fn() -> naia_test_harness::Trace> {
    use naia_test_harness::scenarios;
    let mut map: HashMap<&'static str, fn() -> naia_test_harness::Trace> = HashMap::new();
    map.insert("contract06_scope_entry", scenarios::contract06_scope_entry);
    map.insert("contract07_component_update", scenarios::contract07_component_update);
    map.insert("contract10_delegation_grant", scenarios::contract10_delegation_grant);
    map
}

fn run_scenario(key: &str) -> Result<naia_test_harness::Trace> {
    let registry = scenario_registry();
    let f = registry
        .get(key)
        .with_context(|| format!("Unknown scenario key '{key}'. Register it in scenario_registry()"))?;
    Ok(f())
}

fn record(key: &str) -> Result<()> {
    eprintln!("Recording trace for scenario '{key}'...");
    let trace = run_scenario(key)?;

    let packets: Vec<GoldenPacket> = trace
        .packets
        .iter()
        .map(|p| GoldenPacket {
            direction: match p.direction {
                naia_test_harness::TraceDirection::ClientToServer => GoldenDirection::ClientToServer,
                naia_test_harness::TraceDirection::ServerToClient => GoldenDirection::ServerToClient,
            },
            hex: hex_encode(&p.bytes),
        })
        .collect();

    let golden = GoldenTrace {
        scenario_key: key.to_string(),
        packets,
    };

    let dir = golden_dir();
    fs::create_dir_all(&dir)
        .with_context(|| format!("Cannot create golden_traces dir at {dir:?}"))?;

    let path = golden_path(key);
    let json = serde_json::to_string_pretty(&golden)?;
    fs::write(&path, &json)
        .with_context(|| format!("Cannot write golden trace to {path:?}"))?;

    eprintln!(
        "Wrote {} packets to {}",
        golden.packets.len(),
        path.display()
    );
    Ok(())
}

fn check() -> Result<()> {
    let dir = golden_dir();
    if !dir.exists() {
        eprintln!("No golden_traces directory found — nothing to check.");
        return Ok(());
    }

    let mut failures = Vec::new();
    let mut checked = 0;

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let json = fs::read_to_string(&path)
            .with_context(|| format!("Cannot read {path:?}"))?;
        let golden: GoldenTrace = serde_json::from_str(&json)
            .with_context(|| format!("Cannot parse {path:?}"))?;
        let key = &golden.scenario_key;

        eprint!("Checking '{key}'... ");
        let trace = match run_scenario(key) {
            Ok(t) => t,
            Err(e) => {
                failures.push(format!("{key}: scenario error — {e}"));
                eprintln!("ERROR");
                continue;
            }
        };

        // Compare packet count and contents.
        let new_packets: Vec<GoldenPacket> = trace
            .packets
            .iter()
            .map(|p| GoldenPacket {
                direction: match p.direction {
                    naia_test_harness::TraceDirection::ClientToServer => {
                        GoldenDirection::ClientToServer
                    }
                    naia_test_harness::TraceDirection::ServerToClient => {
                        GoldenDirection::ServerToClient
                    }
                },
                hex: hex_encode(&p.bytes),
            })
            .collect();

        if new_packets == golden.packets {
            eprintln!("OK ({} packets)", new_packets.len());
        } else {
            let msg = diff_summary(&golden.packets, &new_packets);
            failures.push(format!("{key}: trace mismatch — {msg}"));
            eprintln!("FAIL");
        }
        checked += 1;
    }

    eprintln!("Checked {checked} trace(s). Failures: {}", failures.len());
    if failures.is_empty() {
        Ok(())
    } else {
        for f in &failures {
            eprintln!("  FAIL: {f}");
        }
        bail!("{} golden trace(s) failed", failures.len())
    }
}

fn diff_summary(old: &[GoldenPacket], new: &[GoldenPacket]) -> String {
    if old.len() != new.len() {
        return format!("packet count {} → {}", old.len(), new.len());
    }
    let first_diff = old.iter().zip(new.iter()).position(|(a, b)| a != b);
    match first_diff {
        Some(i) => format!("first diff at packet {i}"),
        None => "identical (bug in comparison logic)".to_string(),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
