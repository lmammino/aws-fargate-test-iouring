//! Raw io_uring syscall probes, independent of the `io-uring` crate.
//!
//! Reachability trick: each syscall is called with arguments the kernel
//! rejects cheaply (bad fd, zero entries). If the kernel handles the call we
//! get EBADF/EINVAL. A seccomp filter rejects it before argument checks, so
//! the errno is the filter's action (EPERM in Docker/containerd profiles), and
//! a kernel without io_uring returns ENOSYS.

use crate::report::{TestResult, error_fields};
use serde_json::{Value, json};
use std::io;

const SETUP_ENTRIES: u32 = 8;
const INVALID_FD: libc::c_int = -1;

/// Mirrors `struct io_uring_params` (120 bytes) from include/uapi/linux/io_uring.h.
#[repr(C)]
#[derive(Default)]
struct IoUringParams {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off: [u32; 10],
    cq_off: [u32; 10],
}

const _: () = assert!(std::mem::size_of::<IoUringParams>() == 120);

fn setup(entries: u32, params: &mut IoUringParams) -> io::Result<libc::c_int> {
    let ret = unsafe { libc::syscall(libc::SYS_io_uring_setup, entries, params as *mut _) };
    match ret {
        -1 => Err(io::Error::last_os_error()),
        fd => Ok(fd as libc::c_int),
    }
}

/// Test 2: plain `io_uring_setup(8, {0})` via syscall(2).
pub fn direct_setup() -> TestResult {
    let mut params = IoUringParams::default();
    let request =
        json!({ "entries": SETUP_ENTRIES, "flags": 0, "syscall_nr": libc::SYS_io_uring_setup });

    let fd = match setup(SETUP_ENTRIES, &mut params) {
        Ok(fd) => fd,
        Err(err) => return TestResult::failed("io_uring_setup", &err).with("request", request),
    };

    unsafe { libc::close(fd) };

    TestResult::passed().with("request", request).with(
        "returned_params",
        json!({
            "sq_entries": params.sq_entries,
            "cq_entries": params.cq_entries,
            "features": format!("{:#x}", params.features),
        }),
    )
}

/// Calls each io_uring syscall with invalid arguments and records the errno.
pub fn reachability() -> Value {
    let mut params = IoUringParams::default();
    let setup_zero = setup(0, &mut params).map(|fd| unsafe { libc::close(fd) });

    let enter = unsafe {
        libc::syscall(
            libc::SYS_io_uring_enter,
            INVALID_FD,
            0u32,
            0u32,
            0u32,
            std::ptr::null::<libc::c_void>(),
            0usize,
        )
    };
    let register = unsafe {
        libc::syscall(
            libc::SYS_io_uring_register,
            INVALID_FD,
            0u32,
            std::ptr::null::<libc::c_void>(),
            0u32,
        )
    };

    json!({
        "io_uring_setup(entries=0)": outcome(setup_zero.map(|_| 0)),
        "io_uring_enter(fd=-1)": outcome(syscall_result(enter)),
        "io_uring_register(fd=-1)": outcome(syscall_result(register)),
        "interpretation": "EBADF/EINVAL: syscall reached the kernel. EPERM: blocked before the kernel handler (seccomp, or io_uring_disabled for setup only). ENOSYS: not implemented or filtered with ENOSYS.",
    })
}

fn syscall_result(ret: libc::c_long) -> io::Result<libc::c_long> {
    match ret {
        -1 => Err(io::Error::last_os_error()),
        v => Ok(v),
    }
}

fn outcome(result: io::Result<libc::c_long>) -> Value {
    match result {
        Ok(v) => json!({ "returned": v }),
        Err(err) => Value::Object(error_fields(&err)),
    }
}
