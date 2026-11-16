//! Collects runtime and security context. None of these values alone proves
//! whether io_uring works; they help explain the test results.

use serde_json::{Map, Value, json};
use std::ffi::CStr;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const PROC_STATUS: &str = "/proc/self/status";
const IO_URING_DISABLED: &str = "/proc/sys/kernel/io_uring_disabled";
const IO_URING_GROUP: &str = "/proc/sys/kernel/io_uring_group";
const KALLSYMS: &str = "/proc/kallsyms";
const ECS_METADATA_ENV: &str = "ECS_CONTAINER_METADATA_URI_V4";
const HTTP_PORT: u16 = 80;
const METADATA_TIMEOUT: Duration = Duration::from_secs(2);

/// Kernel symbol that exists only when the kernel is built with CONFIG_IO_URING.
const SETUP_SYMBOL: &str = "io_uring_setup";

/// Fields of /proc/self/status worth reporting.
const STATUS_KEYS: [&str; 9] = [
    "Uid",
    "Gid",
    "CapInh",
    "CapPrm",
    "CapEff",
    "CapBnd",
    "CapAmb",
    "NoNewPrivs",
    "Seccomp_filters",
];

/// Capability names indexed by bit number (include/uapi/linux/capability.h).
const CAP_NAMES: [&str; 41] = [
    "CAP_CHOWN",
    "CAP_DAC_OVERRIDE",
    "CAP_DAC_READ_SEARCH",
    "CAP_FOWNER",
    "CAP_FSETID",
    "CAP_KILL",
    "CAP_SETGID",
    "CAP_SETUID",
    "CAP_SETPCAP",
    "CAP_LINUX_IMMUTABLE",
    "CAP_NET_BIND_SERVICE",
    "CAP_NET_BROADCAST",
    "CAP_NET_ADMIN",
    "CAP_NET_RAW",
    "CAP_IPC_LOCK",
    "CAP_IPC_OWNER",
    "CAP_SYS_MODULE",
    "CAP_SYS_RAWIO",
    "CAP_SYS_CHROOT",
    "CAP_SYS_PTRACE",
    "CAP_SYS_PACCT",
    "CAP_SYS_ADMIN",
    "CAP_SYS_BOOT",
    "CAP_SYS_NICE",
    "CAP_SYS_RESOURCE",
    "CAP_SYS_TIME",
    "CAP_SYS_TTY_CONFIG",
    "CAP_MKNOD",
    "CAP_LEASE",
    "CAP_AUDIT_WRITE",
    "CAP_AUDIT_CONTROL",
    "CAP_SETFCAP",
    "CAP_MAC_OVERRIDE",
    "CAP_MAC_ADMIN",
    "CAP_SYSLOG",
    "CAP_WAKE_ALARM",
    "CAP_BLOCK_SUSPEND",
    "CAP_AUDIT_READ",
    "CAP_PERFMON",
    "CAP_BPF",
    "CAP_CHECKPOINT_RESTORE",
];

pub fn collect() -> Value {
    let status = fs::read_to_string(PROC_STATUS).unwrap_or_default();
    let field = |key: &str| status_field(&status, key);
    let cap_eff = field("CapEff");

    let mut proc_status = Map::new();
    for key in STATUS_KEYS {
        proc_status.insert(key.into(), field(key).map_or(Value::Null, Value::from));
    }

    json!({
        "kernel": uname(),
        "kernel_proc_version": read_trimmed("/proc/version"),
        "architecture": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "os_release": os_release(),
        "cpu_model": cpu_model(),
        "cpu_count": std::thread::available_parallelism().map(|n| n.get()).ok(),
        "uid": unsafe { libc::geteuid() },
        "gid": unsafe { libc::getegid() },
        "seccomp_mode": field("Seccomp").and_then(|v| v.parse::<u32>().ok()),
        "seccomp_mode_meaning": field("Seccomp").map(|v| seccomp_meaning(&v)),
        "effective_capabilities": cap_eff.as_deref().map(decode_caps),
        "proc_status": proc_status,
        "io_uring_disabled": read_trimmed(IO_URING_DISABLED),
        "io_uring_disabled_meaning": read_trimmed(IO_URING_DISABLED).as_deref().map(disabled_meaning),
        "io_uring_group": read_trimmed(IO_URING_GROUP),
        "kallsyms_io_uring_setup": kallsyms_has(SETUP_SYMBOL),
        "rlimit_memlock": rlimit_memlock(),
        "ecs_task_metadata": ecs_task_metadata(),
    })
}

fn uname() -> Value {
    let mut uts: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut uts) } != 0 {
        return Value::Null;
    }

    let text = |raw: &[libc::c_char]| {
        unsafe { CStr::from_ptr(raw.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    };

    json!({
        "sysname": text(&uts.sysname),
        "release": text(&uts.release),
        "version": text(&uts.version),
        "machine": text(&uts.machine),
    })
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn status_field(status: &str, key: &str) -> Option<String> {
    status
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
        .map(|v| v.trim().to_string())
}

fn os_release() -> Option<String> {
    let content = fs::read_to_string("/etc/os-release").ok()?;
    let line = content.lines().find(|l| l.starts_with("PRETTY_NAME="))?;
    Some(line["PRETTY_NAME=".len()..].trim_matches('"').to_string())
}

fn cpu_model() -> Option<String> {
    let content = fs::read_to_string("/proc/cpuinfo").ok()?;
    let line = content.lines().find(|l| l.starts_with("model name"))?;
    Some(line.split_once(':')?.1.trim().to_string())
}

fn seccomp_meaning(mode: &str) -> &'static str {
    match mode {
        "0" => "disabled",
        "1" => "strict",
        "2" => "filter",
        _ => "unknown",
    }
}

fn disabled_meaning(value: &str) -> &'static str {
    match value {
        "0" => "all processes may create rings",
        "1" => "only CAP_SYS_ADMIN or io_uring_group members may create rings",
        "2" => "ring creation disabled for everyone",
        _ => "unknown",
    }
}

fn decode_caps(hex: &str) -> Value {
    let Ok(mask) = u64::from_str_radix(hex, 16) else {
        return Value::Null;
    };

    let names: Vec<String> = (0..64)
        .filter(|bit| mask & (1u64 << bit) != 0)
        .map(|bit| match CAP_NAMES.get(bit) {
            Some(name) => (*name).to_string(),
            None => format!("CAP_{bit}"),
        })
        .collect();

    json!({ "hex": hex, "names": names })
}

/// `Some(true)` means the kernel has io_uring compiled in, regardless of
/// whether seccomp or sysctl blocks it. `None` means kallsyms is unreadable.
fn kallsyms_has(symbol: &str) -> Option<bool> {
    let content = fs::read_to_string(KALLSYMS).ok()?;
    if content.is_empty() {
        return None;
    }

    let found = content
        .lines()
        .filter_map(|line| line.split_whitespace().nth(2))
        .any(|name| name == symbol || name.ends_with(&format!("sys_{symbol}")));
    Some(found)
}

fn rlimit_memlock() -> Value {
    let mut lim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut lim) } != 0 {
        return Value::Null;
    }

    let fmt = |v: libc::rlim_t| match v {
        libc::RLIM_INFINITY => Value::from("unlimited"),
        n => Value::from(n),
    };
    json!({ "soft": fmt(lim.rlim_cur), "hard": fmt(lim.rlim_max) })
}

/// Fetches selected fields of the ECS task metadata endpoint v4, if present.
fn ecs_task_metadata() -> Value {
    let Ok(base) = std::env::var(ECS_METADATA_ENV) else {
        return Value::Null;
    };

    let body = match http_get(&format!("{base}/task")) {
        Ok(body) => body,
        Err(err) => return json!({ "error": err }),
    };

    let Ok(task) = serde_json::from_str::<Value>(&body) else {
        return json!({ "error": "invalid JSON from metadata endpoint" });
    };

    let keys = [
        "Cluster",
        "TaskARN",
        "Family",
        "Revision",
        "LaunchType",
        "AvailabilityZone",
        "Limits",
    ];
    let mut out = Map::new();
    for key in keys {
        out.insert(key.into(), task.get(key).cloned().unwrap_or(Value::Null));
    }
    Value::Object(out)
}

/// Minimal HTTP/1.0 GET for the link-local metadata endpoint (plain HTTP).
fn http_get(url: &str) -> Result<String, String> {
    let rest = url.strip_prefix("http://").ok_or("not an http URL")?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));

    let with_port = match host.contains(':') {
        true => host.to_string(),
        false => format!("{host}:{HTTP_PORT}"),
    };
    let addr = with_port
        .parse()
        .map_err(|e| format!("bad metadata address {host}: {e}"))?;
    let mut stream =
        TcpStream::connect_timeout(&addr, METADATA_TIMEOUT).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(METADATA_TIMEOUT))
        .map_err(|e| e.to_string())?;

    let request = format!("GET /{path} HTTP/1.0\r\nHost: {host}\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| e.to_string())?;

    let (_, body) = response
        .split_once("\r\n\r\n")
        .ok_or("malformed HTTP response")?;
    Ok(body.to_string())
}
