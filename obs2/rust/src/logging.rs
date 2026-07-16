//! Tracing setup for the core.
//!
//! Events are logged to OBS's own log via `blog` (see [`ffi::ge_obs_blog`])
//! so the core's logs land in the OBS log file on every platform.

use std::ffi::CString;
use std::fmt;
use std::io::{self, Write};
use std::sync::Once;

use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::{FmtContext, MakeWriter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

use crate::ffi::{self, GeLogLevel};

static LOGGING_INIT: Once = Once::new();

/// Installs the tracing subscriber. Idempotent: only the first call takes
/// effect, so repeated core loads within one process are safe.
pub fn init() {
    LOGGING_INIT.call_once(|| {
        let subscriber = tracing_subscriber::registry()
            .with(crate::config::logging_filter(env!("CARGO_CRATE_NAME")))
            // OBS log sink: plain text routed through `blog`.
            .with(
                tracing_subscriber::fmt::layer()
                    .event_format(GoldenEyeFormat)
                    .with_ansi(false)
                    .with_writer(ObsMakeWriter),
            );

        let _ = subscriber.try_init();
    });
}

/// Prefixes every line with the plugin tag, then defers to the stock formatter
/// without a timestamp.
struct GoldenEyeFormat;

impl<S, N> FormatEvent<S, N> for GoldenEyeFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(&self, ctx: &FmtContext<'_, S, N>, mut writer: Writer<'_>, event: &Event<'_>) -> fmt::Result {
        write!(writer, "[the_golden_eye] ")?;
        tracing_subscriber::fmt::format().without_time().format_event(ctx, writer, event)
    }
}

/// Maps a tracing [`Level`] to the bridge's [`GeLogLevel`]; the C bridge turns
/// that into the matching OBS `LOG_*` constant.
fn obs_log_level(level: &Level) -> GeLogLevel {
    match *level {
        Level::ERROR => GeLogLevel::Error,
        Level::WARN => GeLogLevel::Warning,
        Level::INFO => GeLogLevel::Info,
        Level::DEBUG | Level::TRACE => GeLogLevel::Debug,
    }
}

/// [`MakeWriter`] that forwards formatted events to OBS's `blog`, tagging each
/// with the OBS level derived from the event's metadata.
struct ObsMakeWriter;

impl<'a> MakeWriter<'a> for ObsMakeWriter {
    type Writer = ObsWriter;

    fn make_writer(&'a self) -> Self::Writer {
        ObsWriter { level: GeLogLevel::Info, buf: Vec::new() }
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        ObsWriter { level: obs_log_level(meta.level()), buf: Vec::new() }
    }
}

/// Buffers one formatted event, then on drop emits each non-empty line as its
/// own `blog` call (`blog` appends the newline). One writer is created per
/// event, so `level` applies to every line it holds.
struct ObsWriter {
    level: GeLogLevel,
    buf: Vec<u8>,
}

impl Write for ObsWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for ObsWriter {
    fn drop(&mut self) {
        for line in self.buf.split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }
            // Skip any line with an interior NUL rather than truncate silently;
            // log lines never legitimately contain one.
            if let Ok(msg) = CString::new(line) {
                // SAFETY: `msg` is a valid NUL-terminated C string for this call.
                unsafe { ffi::ge_obs_blog(self.level, msg.as_ptr()) };
            }
        }
    }
}
