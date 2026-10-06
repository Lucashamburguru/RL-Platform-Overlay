//! Best-effort evidence for unhandled Windows exceptions, including driver faults.
//! The fault callback avoids heap allocation, logging locks, and opening files.
use std::fmt::{self, Write as _};
use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use winapi::um::errhandlingapi::{LPTOP_LEVEL_EXCEPTION_FILTER, SetUnhandledExceptionFilter};
use winapi::um::fileapi::{FlushFileBuffers, WriteFile};
use winapi::um::processthreadsapi::{GetCurrentProcess, GetCurrentThreadId, GetProcessHandleCount};
use winapi::um::psapi::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use winapi::um::sysinfoapi::{GetSystemTimeAsFileTime, GetTickCount64};
use winapi::um::winnt::{EXCEPTION_POINTERS, LONG};

struct NativeLog {
    file: File,
    header: String,
    started_tick: u64,
}

static NATIVE_LOG: OnceLock<NativeLog> = OnceLock::new();
static PREVIOUS_FILTER: OnceLock<LPTOP_LEVEL_EXCEPTION_FILTER> = OnceLock::new();
static REPORTING: AtomicBool = AtomicBool::new(false);

pub(super) fn install(directory: &Path) -> io::Result<()> {
    let path = directory.join("native-crash.log");
    // Keep earlier fault evidence when the user restarts the app.
    if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() > 64 * 1024) {
        let previous = directory.join("native-crash.previous.log");
        match std::fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        std::fs::rename(&path, previous)?;
    }
    let log = NativeLog {
        file: super::open_private(&path, true)?,
        header: format!(
            "\nRL Platform Overlay Windows exception\nversion={}\narch={}\npid={}\nsession_started={}\n",
            crate::app_version(),
            std::env::consts::ARCH,
            std::process::id(),
            chrono::Utc::now().to_rfc3339()
        ),
        // SAFETY: GetTickCount64 has no arguments or preconditions.
        started_tick: unsafe { GetTickCount64() },
    };
    if NATIVE_LOG.set(log).is_ok() {
        // SAFETY: The callback has the Windows ABI and stays valid for the process lifetime.
        let previous = unsafe { SetUnhandledExceptionFilter(Some(exception_filter)) };
        let _ = PREVIOUS_FILTER.set(previous);
    }
    Ok(())
}

struct ReportBuffer {
    bytes: [u8; 2048],
    len: usize,
}

impl fmt::Write for ReportBuffer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        let target = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        target.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

unsafe extern "system" fn exception_filter(info: *mut EXCEPTION_POINTERS) -> LONG {
    if !REPORTING.swap(true, Ordering::SeqCst)
        && let Some(log) = NATIVE_LOG.get()
    {
        let mut report = ReportBuffer {
            bytes: [0; 2048],
            len: 0,
        };
        let _ = report.write_str(&log.header);
        // SAFETY: Windows supplies valid exception pointers during this callback.
        // Null checks also allow a missing exception record to be reported safely.
        unsafe {
            let mut time = std::mem::zeroed();
            GetSystemTimeAsFileTime(&mut time);
            let ticks = (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
            let unix_ms = ticks.saturating_sub(116_444_736_000_000_000) / 10_000;
            let _ = writeln!(
                report,
                "time_unix_ms={unix_ms}\nuptime_ms={}\nthread_id={}",
                GetTickCount64().saturating_sub(log.started_tick),
                GetCurrentThreadId()
            );
            if let Some(pointers) = info.as_ref()
                && let Some(record) = pointers.ExceptionRecord.as_ref()
            {
                let _ = writeln!(
                    report,
                    "exception_code=0x{:08x}\nexception_flags=0x{:08x}\nfault_address={:p}",
                    record.ExceptionCode, record.ExceptionFlags, record.ExceptionAddress
                );
                if matches!(record.ExceptionCode, 0xc0000005 | 0xc0000006)
                    && record.NumberParameters >= 2
                {
                    let _ = writeln!(
                        report,
                        "memory_operation={}\nmemory_address=0x{:x}",
                        record.ExceptionInformation[0], record.ExceptionInformation[1]
                    );
                }
            }
            let mut written = 0;
            WriteFile(
                log.file.as_raw_handle().cast(),
                report.bytes.as_ptr().cast(),
                report.len as u32,
                &mut written,
                std::ptr::null_mut(),
            );
            FlushFileBuffers(log.file.as_raw_handle().cast());
        }
    }
    // Preserve the previous handler and Windows Error Reporting; never resume faulting code.
    if let Some(Some(previous)) = PREVIOUS_FILTER.get() {
        // SAFETY: This is the previous OS-registered filter with the same callback contract.
        return unsafe { previous(info) };
    }
    0 // EXCEPTION_CONTINUE_SEARCH
}

pub(super) fn runtime_resources() -> String {
    // SAFETY: The pseudo-handle is owned by Windows. All output buffers are correctly sized.
    unsafe {
        let process = GetCurrentProcess();
        let mut memory: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        memory.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let memory_ok = K32GetProcessMemoryInfo(
            process,
            (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(),
            memory.cb,
        ) != 0;
        let mut handles = 0;
        let handles_ok = GetProcessHandleCount(process, &mut handles) != 0;
        format!(
            " working_set_bytes={:?} private_bytes={:?} handles={:?}",
            memory_ok.then_some(memory.WorkingSetSize),
            memory_ok.then_some(memory.PrivateUsage),
            handles_ok.then_some(handles)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_buffer_rejects_overflow_without_panicking() {
        let mut report = ReportBuffer {
            bytes: [0; 2048],
            len: 0,
        };
        report.write_str("exception evidence").unwrap();
        assert!(report.write_str(&"x".repeat(2048)).is_err());
        assert_eq!(&report.bytes[..report.len], b"exception evidence");
    }

    #[test]
    fn native_exception_child() {
        if std::env::var_os("RL_TEST_NATIVE_CRASH").is_none() {
            return;
        }
        crate::crash_logging::init(false);
        // Suppress the interactive Windows error dialog in unattended tests.
        // SAFETY: This child intentionally raises a noncontinuable test exception.
        unsafe {
            winapi::um::errhandlingapi::SetErrorMode(0x0002);
            winapi::um::errhandlingapi::RaiseException(0xe0420001, 1, 0, std::ptr::null());
        }
    }

    #[test]
    fn native_report_survives_an_unhandled_exception() {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "crash_logging::windows::tests::native_exception_child",
                "--nocapture",
            ])
            .env("RL_TEST_NATIVE_CRASH", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        assert!(!child.wait().unwrap().success());
        let directory = std::env::temp_dir().join(format!("rl_platform_overlay_test_{pid}"));
        let report = std::fs::read_to_string(directory.join("logs/native-crash.log")).unwrap();
        assert!(report.contains("exception_code=0xe0420001"));
        assert!(report.contains("fault_address="));
        assert!(report.contains("uptime_ms="));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
