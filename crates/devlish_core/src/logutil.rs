//! Leveled logging for the Devlish CLI and daemon (DEVL-109).
//!
//! Levels: `error` < `info` < `debug`.
//! - error: failures only
//! - info: phase progress (compile/run start/finish, HTTP requests)
//! - debug: VM instruction events and detailed host traces
//!
//! Set via `--log-level`, `DEVLISH_LOG`, or `--quiet` (= error).

use serde_json::{json, Value};
use std::env;
use std::sync::atomic::{AtomicU8, Ordering};

static LEVEL: AtomicU8 = AtomicU8::new(1); // default info

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LogLevel {
    Error = 0,
    Info = 1,
    Debug = 2,
}

impl LogLevel {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "error" | "err" => Ok(Self::Error),
            "info" => Ok(Self::Info),
            "debug" | "dbg" | "trace" => Ok(Self::Debug),
            other => Err(format!(
                "unknown log level '{other}' (expected error, info, or debug)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Error,
            2 => Self::Debug,
            _ => Self::Info,
        }
    }
}

pub fn init_from_env_and_args(args: &[String]) -> Result<(), String> {
    let mut level = env::var("DEVLISH_LOG")
        .ok()
        .as_deref()
        .map(LogLevel::parse)
        .transpose()?
        .unwrap_or(LogLevel::Info);

    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--quiet" || arg == "-q" {
            level = LogLevel::Error;
        } else if arg == "--log-level" {
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| "--log-level requires error|info|debug".to_string())?;
            level = LogLevel::parse(value)?;
        } else if let Some(rest) = arg.strip_prefix("--log-level=") {
            level = LogLevel::parse(rest)?;
        }
        i += 1;
    }
    set_level(level);
    Ok(())
}

pub fn set_level(level: LogLevel) {
    LEVEL.store(level as u8, Ordering::Relaxed);
}

pub fn current() -> LogLevel {
    LogLevel::from_u8(LEVEL.load(Ordering::Relaxed))
}

pub fn enabled(level: LogLevel) -> bool {
    (level as u8) <= current() as u8
}

/// Human-readable leveled line on stderr.
pub fn log_line(level: LogLevel, message: &str) {
    if !enabled(level) {
        return;
    }
    eprintln!("[{}] {message}", level.as_str());
}

pub fn error(message: impl AsRef<str>) {
    log_line(LogLevel::Error, message.as_ref());
}

pub fn info(message: impl AsRef<str>) {
    log_line(LogLevel::Info, message.as_ref());
}

pub fn debug(message: impl AsRef<str>) {
    log_line(LogLevel::Debug, message.as_ref());
}

/// Emit a structured VM/host event, tagged with level for tooling filters.
/// Debug: all events. Info: run_started / run_finished / checkpoint / effect_*.
/// Error: never from this path (failures use `error()`).
pub fn emit_event_json(event: &Value) {
    let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
    let level = match event_type {
        "run_started" | "run_finished" | "checkpoint" | "error_recovered" => LogLevel::Info,
        "effect_requested" | "effect_completed" => LogLevel::Info,
        "instruction_started" | "instruction_finished" | "variable_assigned" => LogLevel::Debug,
        _ => LogLevel::Debug,
    };
    if !enabled(level) {
        return;
    }
    let mut tagged = event.clone();
    if let Some(obj) = tagged.as_object_mut() {
        obj.insert("level".to_string(), json!(level.as_str()));
    }
    if let Ok(line) = serde_json::to_string(&tagged) {
        eprintln!("{line}");
    }
}
