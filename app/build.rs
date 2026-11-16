//! Embeds the rustc and io-uring crate versions into the binary for the report.

use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=PROBE_RUSTC_VERSION={version}");

    let lock = std::fs::read_to_string("Cargo.lock").unwrap_or_default();
    println!(
        "cargo:rustc-env=PROBE_IO_URING_VERSION={}",
        locked_version(&lock, "io-uring")
    );
    println!("cargo:rerun-if-changed=Cargo.lock");
}

/// Finds `version = "x"` right after `name = "<crate>"` in Cargo.lock.
fn locked_version(lock: &str, krate: &str) -> String {
    let needle = format!("name = \"{krate}\"");
    let mut lines = lock.lines().skip_while(|l| *l != needle);
    lines
        .nth(1)
        .and_then(|l| l.strip_prefix("version = \""))
        .map(|v| v.trim_end_matches('"').to_string())
        .unwrap_or_else(|| "unknown".into())
}
