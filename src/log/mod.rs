//! Provides logging utilities, used by application.

use std::io;

use chrono::Local;
use slog::{
    o, Drain, Duplicate, FnValue, Fuse, Level, Logger, PushFnValue, Record,
};
use slog_async::Async;
use slog_json::Json;

pub mod prelude;

/// Builds JSON [`Logger`] which prints all its log records to `w_out` writer,
/// but WARN level (and higher) to `w_err` writer.
///
/// Created [`Logger`] produces log records with `lvl`, `time` and `msg` fields
/// by default.
pub fn new_dual_logger<W1, W2>(w_out: W1, w_err: W2) -> Logger
where
    W1: io::Write + Send + 'static,
    W2: io::Write + Send + 'static,
{
    let drain_out = Json::new(w_out).build();
    let drain_err = Json::new(w_err).build();
    let drain = Duplicate(
        drain_out.filter(|r| !r.level().is_at_least(Level::Warning)),
        drain_err.filter_level(Level::Warning),
    )
    .map(Fuse);
    let drain = slog_envlogger::new(drain).fuse();
    let drain = Async::new(drain).build().fuse();
    add_default_keys(Logger::root(drain, o!()))
}

/// Builds JSON [`Logger`] which writes all its logs to the specified writer.
///
/// Created [`Logger`] produces log records with `lvl`, `time` and `msg` fields
/// by default.
pub fn new_logger<W>(w: W) -> Logger
where
    W: io::Write + Send + 'static,
{
    let drain = Json::new(w).build().fuse();
    let drain = Async::new(drain).build().fuse();
    add_default_keys(Logger::root(drain, o!()))
}

/// Adds default log record data (key-value pairs) to specified [`Logger`]:
/// - `time`: creation date and time of log record in [RFC 3339] format.
/// - `lvl`: logging level of log record.
/// - `msg`: log record message.
///
/// [RFC 3339]: https://www.ietf.org/rfc/rfc3339.txt
fn add_default_keys(logger: Logger) -> Logger {
    logger.new(o!(
        "msg" => PushFnValue(move |record : &Record, ser| {
            ser.emit(record.msg())
        }),
        "time" => PushFnValue(move |_ : &Record, ser| {
            ser.emit(Local::now().to_rfc3339())
        }),
        "lvl" => FnValue(move |rinfo : &Record| {
            rinfo.level().as_str()
        }),
    ))
}
