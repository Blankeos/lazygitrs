use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

struct TraceState {
    file: std::fs::File,
    started: Instant,
}

static TRACE: OnceLock<Option<Mutex<TraceState>>> = OnceLock::new();
static TRACE_LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn target_from_env(value: Option<&str>) -> Option<PathBuf> {
    match value {
        None | Some("") | Some("0") => None,
        Some(v) if v == "1" || v.eq_ignore_ascii_case("true") => {
            Some(std::env::temp_dir().join("lazygitrs-input-trace.log"))
        }
        Some(path) => Some(PathBuf::from(path)),
    }
}

fn write_locked(state: &mut TraceState, category: &str, message: std::fmt::Arguments<'_>) {
    let elapsed = state.started.elapsed();
    let secs = elapsed.as_secs();
    let millis = elapsed.subsec_millis();
    let _ = writeln!(state.file, "[+{secs}.{millis:03}] {category}: {message}");
}

fn init_trace() -> Option<Mutex<TraceState>> {
    let path = target_from_env(std::env::var("LAZYGITRS_INPUT_TRACE").ok().as_deref())?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    let _ = TRACE_LOG_PATH.set(path);
    let started = Instant::now();
    let state = Mutex::new(TraceState { file, started });
    if let Ok(mut guard) = state.lock() {
        write_locked(
            &mut guard,
            "env",
            format_args!("version={}", env!("CARGO_PKG_VERSION")),
        );
        for var in [
            "TERM",
            "TERM_PROGRAM",
            "TERM_PROGRAM_VERSION",
            "COLORTERM",
            "LANG",
        ] {
            let value = std::env::var(var).unwrap_or_else(|_| "<unset>".to_string());
            write_locked(&mut guard, "env", format_args!("{var}={value}"));
        }
    }
    Some(state)
}

pub(crate) fn enabled() -> bool {
    match TRACE.get() {
        Some(opt) => opt.is_some(),
        None => TRACE.get_or_init(init_trace).is_some(),
    }
}

pub(crate) fn write(category: &str, message: std::fmt::Arguments<'_>) {
    let Some(mutex) = TRACE.get().and_then(|opt| opt.as_ref()) else {
        return;
    };
    if let Ok(mut state) = mutex.lock() {
        write_locked(&mut state, category, message);
    }
}

pub(crate) fn path() -> Option<&'static Path> {
    TRACE_LOG_PATH.get().map(PathBuf::as_path)
}

#[macro_export]
macro_rules! input_trace {
    ($category:expr, $($arg:tt)*) => {
        if $crate::gui::input_trace::enabled() {
            $crate::gui::input_trace::write($category, format_args!($($arg)*));
        }
    };
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::target_from_env;

    #[test]
    fn target_from_env_disabled() {
        assert_eq!(target_from_env(None), None);
        assert_eq!(target_from_env(Some("")), None);
        assert_eq!(target_from_env(Some("0")), None);
    }

    #[test]
    fn target_from_env_default_path() {
        for value in ["1", "true", "TRUE", "True"] {
            let path = target_from_env(Some(value)).expect(value);
            assert_eq!(
                path.file_name().and_then(|name| name.to_str()),
                Some("lazygitrs-input-trace.log")
            );
        }
    }

    #[test]
    fn target_from_env_custom_path() {
        assert_eq!(
            target_from_env(Some("/tmp/my-trace.log")),
            Some(PathBuf::from("/tmp/my-trace.log"))
        );
    }
}
