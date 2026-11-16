//! io_uring tests built on the `io-uring` crate.

use crate::report::{TestResult, cqe_error};
use io_uring::{IoUring, Probe, opcode, squeue, types};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

pub const QUEUE_DEPTH: u32 = 8;
const SQPOLL_IDLE_MS: u32 = 100;
const PAYLOAD: &[u8] = b"io_uring probe payload: 0123456789abcdef";
const USER_DATA: u64 = 0x42;

pub fn init_ring() -> Result<IoUring, TestResult> {
    IoUring::new(QUEUE_DEPTH)
        .map_err(|err| TestResult::failed("IoUring::new", &err).with("queue_depth", QUEUE_DEPTH))
}

pub fn describe_ring(ring: &IoUring) -> TestResult {
    let p = ring.params();
    TestResult::passed()
        .with("queue_depth", QUEUE_DEPTH)
        .with("sq_entries", p.sq_entries())
        .with("cq_entries", p.cq_entries())
        .with(
            "features",
            json!({
                "single_mmap": p.is_feature_single_mmap(),
                "nodrop": p.is_feature_nodrop(),
                "submit_stable": p.is_feature_submit_stable(),
                "rw_cur_pos": p.is_feature_rw_cur_pos(),
                "cur_personality": p.is_feature_cur_personality(),
                "fast_poll": p.is_feature_fast_poll(),
                "poll_32bits": p.is_feature_poll_32bits(),
                "sqpoll_nonfixed": p.is_feature_sqpoll_nonfixed(),
                "ext_arg": p.is_feature_ext_arg(),
                "native_workers": p.is_feature_native_workers(),
                "resource_tagging": p.is_feature_resource_tagging(),
                "skip_cqe_on_success": p.is_feature_skip_cqe_on_success(),
                "linked_file": p.is_feature_linked_file(),
                "recvsend_bundle": p.is_feature_recvsend_bundle(),
                "min_timeout": p.is_feature_min_timeout(),
                "no_iowait": p.is_feature_no_iowait(),
            }),
        )
}

/// Pushes one SQE, waits for its CQE and returns the CQE result.
/// The caller must keep any buffer referenced by `entry` alive until return.
fn submit_one(ring: &mut IoUring, entry: squeue::Entry) -> Result<i32, TestResult> {
    let entry = entry.user_data(USER_DATA);

    unsafe { ring.submission().push(&entry) }
        .map_err(|_| TestResult::failed_msg("sq_push", "submission queue full"))?;

    ring.submit_and_wait(1)
        .map_err(|err| TestResult::failed("io_uring_enter", &err))?;

    let cqe = ring
        .completion()
        .next()
        .ok_or_else(|| TestResult::failed_msg("cq_pop", "no completion entry"))?;

    if cqe.user_data() != USER_DATA {
        return Err(TestResult::failed_msg("cq_pop", "unexpected user_data"));
    }

    match cqe.result() {
        res if res < 0 => Err(TestResult::failed("cqe", &cqe_error(res))),
        res => Ok(res),
    }
}

pub fn nop(ring: &mut IoUring) -> TestResult {
    match submit_one(ring, opcode::Nop::new().build()) {
        Ok(res) => TestResult::passed().with("cqe_result", res),
        Err(fail) => fail,
    }
}

/// Writes PAYLOAD with IORING_OP_WRITE, then verifies it with std::fs.
pub fn file_write(ring: &mut IoUring, dir: &Path) -> TestResult {
    let path = dir.join("uring-write.bin");
    let file = match File::create(&path) {
        Ok(f) => f,
        Err(err) => return TestResult::failed("create_temp_file", &err),
    };

    let fd = types::Fd(file.as_raw_fd());
    let entry = opcode::Write::new(fd, PAYLOAD.as_ptr(), PAYLOAD.len() as u32).build();
    let written = match submit_one(ring, entry) {
        Ok(n) => n,
        Err(fail) => return fail,
    };
    drop(file);

    let on_disk = fs::read(&path).unwrap_or_default();
    let _ = fs::remove_file(&path);
    verify("bytes_written", written, &on_disk)
}

/// Writes PAYLOAD with std::fs, then reads it back with IORING_OP_READ.
pub fn file_read(ring: &mut IoUring, dir: &Path) -> TestResult {
    let path = dir.join("uring-read.bin");
    if let Err(err) = fs::write(&path, PAYLOAD) {
        return TestResult::failed("create_temp_file", &err);
    }

    let file = match File::open(&path) {
        Ok(f) => f,
        Err(err) => return TestResult::failed("open_temp_file", &err),
    };

    let mut buf = vec![0u8; PAYLOAD.len()];
    let fd = types::Fd(file.as_raw_fd());
    let entry = opcode::Read::new(fd, buf.as_mut_ptr(), buf.len() as u32).build();
    let result = submit_one(ring, entry);

    drop(file);
    let _ = fs::remove_file(&path);

    match result {
        Ok(n) => verify("bytes_read", n, &buf[..n as usize]),
        Err(fail) => fail,
    }
}

fn verify(key: &str, count: i32, data: &[u8]) -> TestResult {
    if data != PAYLOAD {
        return TestResult::failed_msg("verify", "data mismatch")
            .with(key, count)
            .with("expected_len", PAYLOAD.len());
    }
    TestResult::passed()
        .with(key, count)
        .with("data_verified", true)
}

/// Test 5a: SQ polling thread. Unprivileged since Linux 5.11.
pub fn sqpoll() -> TestResult {
    let mut ring = match IoUring::builder()
        .setup_sqpoll(SQPOLL_IDLE_MS)
        .build(QUEUE_DEPTH)
    {
        Ok(r) => r,
        Err(err) => return TestResult::failed("setup_sqpoll", &err),
    };

    if !ring.params().is_setup_sqpoll() {
        return TestResult::failed_msg("setup_sqpoll", "kernel ignored IORING_SETUP_SQPOLL");
    }

    match submit_one(&mut ring, opcode::Nop::new().build()) {
        Ok(res) => TestResult::passed().with("nop_cqe_result", res),
        Err(fail) => fail,
    }
}

/// Test 5b: register one buffer, then use IORING_OP_READ_FIXED into it.
pub fn registered_buffers(ring: &mut IoUring, dir: &Path) -> TestResult {
    let path = dir.join("uring-fixed.bin");
    if let Err(err) = fs::write(&path, PAYLOAD) {
        return TestResult::failed("create_temp_file", &err);
    }
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(err) => return TestResult::failed("open_temp_file", &err),
    };

    let mut buf = vec![0u8; PAYLOAD.len()];
    let iov = libc::iovec {
        iov_base: buf.as_mut_ptr().cast(),
        iov_len: buf.len(),
    };
    if let Err(err) = unsafe { ring.submitter().register_buffers(&[iov]) } {
        return TestResult::failed("register_buffers", &err);
    }

    let fd = types::Fd(file.as_raw_fd());
    let entry = opcode::ReadFixed::new(fd, buf.as_mut_ptr(), buf.len() as u32, 0).build();
    let result = submit_one(ring, entry);

    let _ = ring.submitter().unregister_buffers();
    drop(file);
    let _ = fs::remove_file(&path);

    match result {
        Ok(n) => verify("bytes_read", n, &buf[..n as usize]),
        Err(fail) => fail,
    }
}

/// Test 5c: register a file, then read through its fixed index.
pub fn registered_files(ring: &mut IoUring, dir: &Path) -> TestResult {
    let path = dir.join("uring-regfile.bin");
    if let Err(err) = fs::write(&path, PAYLOAD) {
        return TestResult::failed("create_temp_file", &err);
    }
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(err) => return TestResult::failed("open_temp_file", &err),
    };

    if let Err(err) = ring.submitter().register_files(&[file.as_raw_fd()]) {
        return TestResult::failed("register_files", &err);
    }

    let mut buf = vec![0u8; PAYLOAD.len()];
    let entry = opcode::Read::new(types::Fixed(0), buf.as_mut_ptr(), buf.len() as u32).build();
    let result = submit_one(ring, entry);

    let _ = ring.submitter().unregister_files();
    drop(file);
    let _ = fs::remove_file(&path);

    match result {
        Ok(n) => verify("bytes_read", n, &buf[..n as usize]),
        Err(fail) => fail,
    }
}

/// Test 5d: IORING_REGISTER_PROBE, reports supported opcodes by number.
pub fn opcode_probe(ring: &IoUring) -> TestResult {
    let mut probe = Probe::new();
    if let Err(err) = ring.submitter().register_probe(&mut probe) {
        return TestResult::failed("register_probe", &err);
    }

    let supported: Vec<u8> = (0..=u8::MAX).filter(|op| probe.is_supported(*op)).collect();
    let named = [
        ("NOP", opcode::Nop::CODE),
        ("READ", opcode::Read::CODE),
        ("WRITE", opcode::Write::CODE),
        ("READ_FIXED", opcode::ReadFixed::CODE),
        ("ACCEPT", opcode::Accept::CODE),
        ("RECV", opcode::Recv::CODE),
        ("SEND", opcode::Send::CODE),
        ("OPENAT", opcode::OpenAt::CODE),
        ("STATX", opcode::Statx::CODE),
        ("SPLICE", opcode::Splice::CODE),
    ];
    let named: serde_json::Map<String, Value> = named
        .iter()
        .map(|(name, code)| (name.to_string(), probe.is_supported(*code).into()))
        .collect();

    TestResult::passed()
        .with("supported_opcode_count", supported.len())
        .with("supported_opcodes", supported)
        .with("common_opcodes", Value::Object(named))
}

/// Test 6: ordinary std::fs round trip, rules out filesystem problems.
pub fn conventional_io(dir: &Path) -> TestResult {
    let path = dir.join("conventional.bin");
    let result: io::Result<Vec<u8>> = fs::write(&path, PAYLOAD).and_then(|_| fs::read(&path));
    let _ = fs::remove_file(&path);

    match result {
        Ok(data) => verify("bytes_read", data.len() as i32, &data),
        Err(err) => TestResult::failed("std_fs_roundtrip", &err),
    }
}
