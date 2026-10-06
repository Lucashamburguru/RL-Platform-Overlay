//! Persistent diagnostics, installed before the runtime and UI are created.
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Once;

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
static INIT: Once = Once::new();

#[cfg(windows)]
#[path = "crash_logging/windows.rs"]
mod windows;

pub fn log_directory() -> PathBuf {
    crate::state::config_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("rl-platform-overlay"))
        .join("logs")
}

fn open_private(path: &Path, append: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

struct RuntimeLog {
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
    limit: u64,
}

impl RuntimeLog {
    fn new(path: PathBuf, limit: u64) -> io::Result<Self> {
        let file = open_private(&path, true)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            path,
            file: Some(file),
            bytes,
            limit,
        })
    }

    fn write_file(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.bytes.saturating_add(bytes.len() as u64) > self.limit {
            // Close first so rotation works on Windows too.
            self.file.take();
            let previous = self.path.with_extension("previous.log");
            match std::fs::remove_file(&previous) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            std::fs::rename(&self.path, previous)?;
            self.bytes = 0;
        }
        if self.file.is_none() {
            self.file = Some(open_private(&self.path, true)?);
        }
        if let Some(file) = &mut self.file {
            // Bound even a single unusually large log message.
            let bytes = &bytes[..bytes.len().min(self.limit as usize)];
            file.write_all(bytes)?;
            file.flush()?;
            self.bytes = self.bytes.saturating_add(bytes.len() as u64);
        }
        Ok(())
    }
}

impl Write for RuntimeLog {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(bytes);
        if let Err(error) = self.write_file(bytes) {
            let _ = writeln!(io::stderr(), "Could not write runtime log: {error}");
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = &mut self.file {
            file.flush()?;
        }
        Ok(())
    }
}

/// The panic hook also runs with the release build's `panic = "abort"`.
/// It writes directly, avoiding the logger's lock during a panic.
pub fn init(debug_enabled: bool) {
    INIT.call_once(|| {
        let directory = log_directory();
        if let Err(error) = std::fs::create_dir_all(&directory) {
            eprintln!("Could not create diagnostics directory: {error}");
        }
        #[cfg(windows)]
        if let Err(error) = windows::install(&directory) {
            eprintln!("Could not install Windows exception logging: {error}");
        }
        let crash_path = directory.join("crash.log");
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            let report = format!(
                "RL Platform Overlay panic report\ntime={}\nversion={}\nos={}\narch={}\npid={}\nthread={}\n{info}\nbacktrace:\n{}\n",
                chrono::Utc::now().to_rfc3339(), crate::app_version(),
                std::env::consts::OS, std::env::consts::ARCH, std::process::id(),
                thread.name().unwrap_or("unnamed"), std::backtrace::Backtrace::force_capture(),
            );
            // Preserve the last panic across restarts; cap oversized payloads.
            let end = report.floor_char_boundary(report.len().min(128 * 1024));
            match open_private(&crash_path, false).and_then(|mut file| {
                file.write_all(&report.as_bytes()[..end])?;
                file.sync_all()
            }) {
                Ok(()) => {},
                Err(error) => eprintln!("Could not save panic report: {error}"),
            }
            previous_hook(info);
        }));

        let mut builder = env_logger::Builder::new();
        builder.filter_level(log::LevelFilter::Warn);
        builder.filter_module("rl_platform_overlay", if debug_enabled {
            log::LevelFilter::Debug
        } else { log::LevelFilter::Info });
        builder.format_timestamp_millis();
        builder.write_style(env_logger::WriteStyle::Never);
        match RuntimeLog::new(directory.join("app.log"), MAX_LOG_BYTES) {
            Ok(writer) => { builder.target(env_logger::Target::Pipe(Box::new(writer))); },
            Err(error) => eprintln!("Could not open runtime log: {error}"),
        }
        let _ = builder.try_init();
        log::info!("Session started: version={} os={} arch={} pid={} debug={debug_enabled}",
            crate::app_version(), std::env::consts::OS, std::env::consts::ARCH, std::process::id());
    });
}

/// Called by the UI every 30 seconds, so the log also shows that frames are progressing.
pub(crate) fn log_runtime_health(state: &crate::state::AppState) {
    use std::sync::atomic::Ordering;
    let config = state.system.config.load();
    log::info!(
        "Runtime health: launched={} dashboard={} connected={} players={} replay_upload={}{}",
        state.flags.is_launched.load(Ordering::SeqCst),
        config.dashboard_enabled,
        state.flags.is_connected.load(Ordering::SeqCst),
        state.game.players.load().len(),
        state.replays.upload_progress.load().running,
        runtime_resources(),
    );
}

#[cfg(windows)]
fn runtime_resources() -> String {
    windows::runtime_resources()
}

#[cfg(not(windows))]
fn runtime_resources() -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_log_rotates_and_bounds_large_messages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.log");
        let mut writer = RuntimeLog::new(path.clone(), 16).unwrap();
        writer.write_file(b"before crash\n").unwrap();
        writer.write_file(b"after restart\n").unwrap();
        assert_eq!(
            std::fs::read(path.with_extension("previous.log")).unwrap(),
            b"before crash\n"
        );
        writer.write_file(&[b'x'; 32]).unwrap();
        assert_eq!(std::fs::metadata(path).unwrap().len(), 16);
    }

    #[test]
    fn panic_hook_child() {
        if std::env::var_os("RL_TEST_CRASH_HOOK").is_none() {
            return;
        }
        init(false);
        std::thread::Builder::new()
            .name("crash-test-worker".into())
            .spawn(|| {
                panic!("test panic evidence");
            })
            .unwrap()
            .join()
            .unwrap_err();
        // Emulate abrupt termination after the hook without running destructors.
        std::process::exit(73);
    }

    #[test]
    fn panic_report_survives_process_exit() {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "crash_logging::tests::panic_hook_child",
                "--nocapture",
            ])
            .env("RL_TEST_CRASH_HOOK", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(73),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        // Test config directories are isolated by the child's actual process ID.
        let dir = std::env::temp_dir().join(format!("rl_platform_overlay_test_{pid}/logs"));
        let report = std::fs::read_to_string(dir.join("crash.log")).unwrap();
        assert!(report.contains("test panic evidence"));
        assert!(report.contains("thread=crash-test-worker"));
        assert!(report.contains(file!()));
        assert!(report.contains("backtrace:"));
        assert!(report.contains(&format!("version={}", crate::app_version())));
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }
}
