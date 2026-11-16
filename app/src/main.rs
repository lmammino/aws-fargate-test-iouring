//! io_uring availability probe.
//!
//! Runs independent tests, logs human-readable lines to stderr and prints a
//! single-line JSON report to stdout, prefixed with `RESULT_PREFIX`.
//!
//! Exit codes (see README):
//!   0  SUPPORTED     ring setup, NOP, read and write all passed
//!   10 PARTIAL       io_uring is reachable but at least one core test failed
//!   11 UNAVAILABLE   ring creation failed (blocked or not implemented)
//!   12 INCONCLUSIVE  the conventional I/O control test failed
//!   2  PROBE_ERROR   the probe itself could not run (e.g. no temp dir)
//!   3  TIMEOUT       the watchdog fired before the tests finished
//!   101              Rust panic (application bug)

mod environment;
mod report;
mod ring_tests;
mod syscalls;

use report::{TestResult, summary_json};
use serde_json::{Map, Value, json};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

const RESULT_PREFIX: &str = "IOURING_PROBE_RESULT";
const SCHEMA_VERSION: u32 = 1;
const WATCHDOG: Duration = Duration::from_secs(60);
const PRETTY_FLAG: &str = "--pretty";

const EXIT_SUPPORTED: u8 = 0;
const EXIT_PROBE_ERROR: u8 = 2;
const EXIT_TIMEOUT: i32 = 3;
const EXIT_PARTIAL: u8 = 10;
const EXIT_UNAVAILABLE: u8 = 11;
const EXIT_INCONCLUSIVE: u8 = 12;

const CORE_TESTS: [&str; 4] = ["ring_initialization", "nop", "file_write", "file_read"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Output {
    Compact,
    Pretty,
}

/// Ordered collection of named test results.
struct Tests(Vec<(&'static str, TestResult)>);

impl Tests {
    fn add(&mut self, name: &'static str, result: TestResult) {
        log(&format!("test {name}: {}", result.describe()));
        self.0.push((name, result));
    }

    fn passed(&self, name: &str) -> bool {
        self.0.iter().any(|(n, r)| *n == name && r.is_passed())
    }

    fn to_json(&self) -> Value {
        let map: Map<String, Value> = self
            .0
            .iter()
            .map(|(name, result)| (name.to_string(), result.to_json()))
            .collect();
        Value::Object(map)
    }
}

fn main() -> ExitCode {
    let output = match std::env::args().any(|a| a == PRETTY_FLAG) {
        true => Output::Pretty,
        false => Output::Compact,
    };

    start_watchdog();
    log(&format!(
        "starting io_uring probe v{}",
        env!("CARGO_PKG_VERSION")
    ));

    let work_dir = std::env::temp_dir().join(format!("iouring-probe-{}", std::process::id()));
    if let Err(err) = std::fs::create_dir_all(&work_dir) {
        log(&format!(
            "cannot create work dir {}: {err}",
            work_dir.display()
        ));
        return ExitCode::from(EXIT_PROBE_ERROR);
    }

    let environment = environment::collect();
    log(&format!("environment: {environment}"));

    let reachability = syscalls::reachability();
    log(&format!("syscall reachability: {reachability}"));

    let tests = run_tests(&work_dir);
    let _ = std::fs::remove_dir_all(&work_dir);

    let (classification, explanation, exit_code) = classify(&tests);
    log(&format!("classification: {classification} ({explanation})"));

    let report = json!({
        "schema_version": SCHEMA_VERSION,
        "probe": {
            "version": env!("CARGO_PKG_VERSION"),
            "rustc": env!("PROBE_RUSTC_VERSION"),
            "io_uring_crate": env!("PROBE_IO_URING_VERSION"),
        },
        "environment": environment,
        "syscall_reachability": reachability,
        "tests": tests.to_json(),
        "summary": summary_json(classification, explanation, exit_code.into()),
    });
    print_report(&report, output);

    ExitCode::from(exit_code)
}

fn run_tests(dir: &Path) -> Tests {
    let mut tests = Tests(Vec::new());
    tests.add("direct_syscall", syscalls::direct_setup());

    let mut ring = match ring_tests::init_ring() {
        Ok(ring) => ring,
        Err(fail) => {
            tests.add("ring_initialization", fail);
            let reason = "ring initialization failed";
            for name in [
                "nop",
                "file_write",
                "file_read",
                "opcode_probe",
                "registered_buffers",
                "registered_files",
                "sqpoll",
            ] {
                tests.add(name, TestResult::skipped(reason));
            }
            tests.add("conventional_io", ring_tests::conventional_io(dir));
            return tests;
        }
    };

    tests.add("ring_initialization", ring_tests::describe_ring(&ring));
    tests.add("nop", ring_tests::nop(&mut ring));
    tests.add("file_write", ring_tests::file_write(&mut ring, dir));
    tests.add("file_read", ring_tests::file_read(&mut ring, dir));

    // Optional features: reported, but never affect the classification.
    tests.add("opcode_probe", ring_tests::opcode_probe(&ring));
    tests.add(
        "registered_buffers",
        ring_tests::registered_buffers(&mut ring, dir),
    );
    tests.add(
        "registered_files",
        ring_tests::registered_files(&mut ring, dir),
    );
    tests.add("sqpoll", ring_tests::sqpoll());

    tests.add("conventional_io", ring_tests::conventional_io(dir));
    tests
}

fn classify(tests: &Tests) -> (&'static str, &'static str, u8) {
    if !tests.passed("conventional_io") {
        return (
            "inconclusive",
            "conventional file I/O failed, so io_uring results cannot be trusted",
            EXIT_INCONCLUSIVE,
        );
    }

    if !tests.passed("direct_syscall") && !tests.passed("ring_initialization") {
        return (
            "unavailable",
            "io_uring_setup failed both via raw syscall and via the io-uring crate",
            EXIT_UNAVAILABLE,
        );
    }

    if CORE_TESTS.iter().all(|name| tests.passed(name)) {
        return (
            "supported",
            "ring setup, NOP, file write and file read all succeeded",
            EXIT_SUPPORTED,
        );
    }

    (
        "partial",
        "io_uring is reachable but at least one core test failed",
        EXIT_PARTIAL,
    )
}

fn print_report(report: &Value, output: Output) {
    if output == Output::Pretty {
        println!(
            "{}",
            serde_json::to_string_pretty(report).unwrap_or_default()
        );
        return;
    }
    // One line, so CloudWatch keeps it as a single log event.
    println!("{RESULT_PREFIX} {report}");
}

fn log(message: &str) {
    eprintln!("[iouring-probe] {message}");
}

/// Guarantees the task terminates even if a submission never completes.
fn start_watchdog() {
    std::thread::spawn(|| {
        std::thread::sleep(WATCHDOG);
        log(&format!(
            "watchdog: tests did not finish within {WATCHDOG:?}"
        ));
        std::process::exit(EXIT_TIMEOUT);
    });
}
