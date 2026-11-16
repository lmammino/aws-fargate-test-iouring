use serde_json::{Map, Value, json};
use std::io;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Passed,
    Failed,
    Skipped,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Passed => "passed",
            Status::Failed => "failed",
            Status::Skipped => "skipped",
        }
    }
}

/// Result of a single probe test. Serialized into the JSON report.
pub struct TestResult {
    pub status: Status,
    fields: Map<String, Value>,
}

impl TestResult {
    pub fn passed() -> Self {
        Self::new(Status::Passed)
    }

    pub fn skipped(reason: &str) -> Self {
        Self::new(Status::Skipped).with("reason", reason)
    }

    pub fn failed(stage: &str, err: &io::Error) -> Self {
        let mut result = Self::new(Status::Failed).with("stage", stage);
        result.fields.extend(error_fields(err));
        result
    }

    pub fn failed_msg(stage: &str, message: &str) -> Self {
        Self::new(Status::Failed)
            .with("stage", stage)
            .with("error", message)
    }

    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.fields.insert(key.to_string(), value.into());
        self
    }

    pub fn is_passed(&self) -> bool {
        self.status == Status::Passed
    }

    pub fn to_json(&self) -> Value {
        let mut obj = Map::new();
        obj.insert("status".into(), self.status.as_str().into());
        obj.extend(self.fields.clone());
        Value::Object(obj)
    }

    /// One-line human summary, e.g. `failed (errno 1 EPERM: Operation not permitted)`.
    pub fn describe(&self) -> String {
        let mut text = self.status.as_str().to_string();

        if let Some(errno) = self.fields.get("errno") {
            let name = self.fields.get("errno_name").and_then(Value::as_str);
            let msg = self.fields.get("error").and_then(Value::as_str);
            text += &format!(
                " (errno {} {}: {})",
                errno,
                name.unwrap_or("?"),
                msg.unwrap_or("?")
            );
            return text;
        }

        let detail = ["error", "reason"]
            .iter()
            .find_map(|k| self.fields.get(*k).and_then(Value::as_str));
        if let Some(detail) = detail {
            text += &format!(" ({detail})");
        }
        text
    }

    fn new(status: Status) -> Self {
        Self {
            status,
            fields: Map::new(),
        }
    }
}

/// Builds `errno`, `errno_name` and `error` fields from an OS error.
pub fn error_fields(err: &io::Error) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("error".into(), err.to_string().into());

    if let Some(code) = err.raw_os_error() {
        map.insert("errno".into(), code.into());
        map.insert("errno_name".into(), errno_name(code).into());
    }
    map
}

/// Converts a negative CQE result into an OS error.
pub fn cqe_error(res: i32) -> io::Error {
    io::Error::from_raw_os_error(-res)
}

pub fn errno_name(code: i32) -> &'static str {
    match code {
        libc::EPERM => "EPERM",
        libc::ENOENT => "ENOENT",
        libc::EINTR => "EINTR",
        libc::EIO => "EIO",
        libc::EBADF => "EBADF",
        libc::EAGAIN => "EAGAIN",
        libc::ENOMEM => "ENOMEM",
        libc::EACCES => "EACCES",
        libc::EFAULT => "EFAULT",
        libc::EBUSY => "EBUSY",
        libc::EINVAL => "EINVAL",
        libc::ENFILE => "ENFILE",
        libc::EMFILE => "EMFILE",
        libc::ENOSYS => "ENOSYS",
        libc::EOPNOTSUPP => "EOPNOTSUPP",
        _ => "OTHER",
    }
}

pub fn summary_json(classification: &str, explanation: &str, exit_code: i32) -> Value {
    json!({
        "classification": classification,
        "explanation": explanation,
        "exit_code": exit_code,
    })
}
