use crate::core::capacity::{Bottleneck, CapacityEstimate};
use crate::ports::sink::CapacitySink;

/// Prints a human-readable capacity estimate table to stdout.
pub struct CapacityReportSink;

impl CapacitySink for CapacityReportSink {
    fn emit(&self, e: &CapacityEstimate) {
        let load_ms = e.level_load_ms;
        let client_str = if e.client_can_keep_up { "✓ keeps up" } else { "✗ OVERLOADED" };
        let bottleneck_str = match e.bottleneck {
            Bottleneck::Server => "CPU (server tick cost)",
            Bottleneck::Wire   => "Wire (network bandwidth)",
            Bottleneck::Client => "CPU (client receive cost)",
        };

        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║   Cyberlith halo_btb_16v16 — Capacity Estimate @ 25 Hz      ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Level load (10K tiles + 32 units → 16 clients):            ║");
        println!("║    {:<58} ║", format!("{:.1} ms", load_ms));
        println!("║                                                              ║");
        println!("║  Server capacity (CPU):                                      ║");
        println!("║    idle  (0 mutations/tick):   {:<30} ║",
            format!("{} concurrent games", cap_str(e.server_capacity_idle)));
        println!("║    active (32 mutations/tick): {:<30} ║",
            format!("{} concurrent games", cap_str(e.server_capacity_active)));
        println!("║                                                              ║");
        println!("║  Wire capacity (1 Gbps outbound):                            ║");
        println!("║    idle:                       {:<30} ║",
            format!("{} concurrent games", cap_str(e.wire_capacity_idle)));
        println!("║    active:                     {:<30} ║",
            format!("{} concurrent games", cap_str(e.wire_capacity_active)));
        println!("║                                                              ║");
        println!("║  Client (one player, active tick): {:<26} ║", client_str);
        println!("║                                                              ║");
        println!("║  Bottleneck: {:<48} ║", bottleneck_str);
        println!("╚══════════════════════════════════════════════════════════════╝");

        if e.server_wire_bytes_not_measured() {
            println!("  Note: wire capacity shown as ∞ — run the full bench suite");
            println!("  (including wire/bandwidth_realistic_quantized) for wire estimates.");
        }
    }
}

fn cap_str(n: u32) -> String {
    if n == u32::MAX { "∞".to_string() } else { n.to_string() }
}

// Helper: CapacityEstimate doesn't store whether wire was measured, but
// wire_capacity == u32::MAX iff server_wire_bytes was 0 (see estimate()).
// We use this to print the note below the table.
trait WireUnmeasured {
    fn server_wire_bytes_not_measured(&self) -> bool;
}
impl WireUnmeasured for CapacityEstimate {
    fn server_wire_bytes_not_measured(&self) -> bool {
        self.wire_capacity_idle == u32::MAX && self.wire_capacity_active == u32::MAX
    }
}
