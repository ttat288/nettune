//! Developer tool: run one real diagnostic pass from the terminal and print JSON.
//! Usage: cargo run --example diagnose -- [interface] [samples] [timeout_ms]
//! Read-only: it never changes system configuration.

use nettune_lib::{detect, diagnose, models::DiagnosticConfig, powersave::BackupStore};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ifaces = detect::list_interfaces().expect("interface detection failed");
    let name = args
        .first()
        .cloned()
        .or_else(|| ifaces.first().map(|i| i.name.clone()))
        .expect("no network interfaces found");
    let config = DiagnosticConfig {
        samples: args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20),
        timeout_ms: args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000),
    };
    let report =
        diagnose::run(&name, config, &BackupStore::default_location()).expect("diagnostics failed");
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
