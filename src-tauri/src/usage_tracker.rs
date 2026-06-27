//! Background foreground-app usage sampler.
//!
//! Desktop OSes (unlike Android/iOS) expose no aggregated "screen time" API, so
//! we approximate it by polling the foreground window every `TICK_INTERVAL_SECS`
//! and accumulating that interval's seconds onto the owning app for the current
//! local day. The accumulated rows live in `storage::usage` (`app_usage` table)
//! and feed both the Usage view and the `app_usage_stats` builtin tool.
//!
//! Platform coverage:
//! - **Windows** — `GetForegroundWindow` + `QueryFullProcessImageNameW`, idle via
//!   `GetLastInputInfo`. Fully implemented.
//! - **macOS** — `NSWorkspace.sharedWorkspace.frontmostApplication` (AppKit via
//!   the `objc` runtime), idle via `CGEventSourceSecondsSinceLastEventType`.
//! - **Linux (X11)** — shells out to `xprop` for `_NET_ACTIVE_WINDOW` /
//!   `_NET_WM_PID`, resolves the name from `/proc/<pid>/comm`. Wayland has no
//!   standard active-window protocol, so it degrades to unsupported (returns
//!   `None`); idle detection is X11-only and currently skipped on Linux.
//!
//! Every platform impl returns `None` on any failure — the sampler then records
//! nothing that tick, so a missing `xprop` or a permission gap never crashes.

use crate::storage::Storage;
use chrono::Local;
use std::sync::Arc;
use tokio::time::{interval, Duration};

/// Seconds attributed per tick to whatever window is foreground at tick time.
/// Granularity is a trade-off: shorter = more accurate attribution across rapid
/// app switches, longer = less DB churn and lower wakeups. 15s keeps a day's
/// rows to a handful hundred at most while staying responsive.
const TICK_INTERVAL_SECS: u64 = 15;

/// A foreground app identified by a stable, low-cardinality key (exe stem) and
/// a human-friendly name for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundApp {
    pub key: String,
    pub name: String,
}

/// Detect the foreground window's owning app. `None` means "unsupported on this
/// OS" or "no foreground" — the sampler treats both as "record nothing this
/// tick". Kept free of `Storage`/`tokio` deps so it's trivially unit-testable.
pub fn current_foreground_app() -> Option<ForegroundApp> {
    #[cfg(target_os = "windows")]
    {
        win::foreground_app()
    }
    #[cfg(target_os = "macos")]
    {
        mac::foreground_app()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        x11::foreground_app()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        None
    }
}

/// Seconds since the last user input (mouse/keyboard), if detectable. Used to
/// avoid crediting an app for idle "away" time when a window is focused but the
/// user stepped away. `None` => unsupported; the sampler then records blindly.
pub fn current_idle_seconds() -> Option<u64> {
    #[cfg(target_os = "windows")]
    {
        win::idle_seconds()
    }
    #[cfg(target_os = "macos")]
    {
        mac::idle_seconds()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        None
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        None
    }
}

// --- Windows implementation ------------------------------------------------

#[cfg(target_os = "windows")]
mod win {
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    /// Quote an absolute exe path down to a display key: the file stem (no
    /// extension), e.g. `C:\...\Code.exe` → `Code`. Lower-cased for stable
    /// grouping regardless of install path casing.
    fn exe_stem(exe_path: &str) -> Option<(String, String)> {
        let stem = exe_path.rsplit(['/', '\\']).next()?;
        let stem = stem.strip_suffix(".exe").unwrap_or(stem);
        if stem.is_empty() {
            return None;
        }
        Some((stem.to_lowercase(), stem.to_string()))
    }

    pub(super) fn foreground_app() -> Option<super::ForegroundApp> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
            if handle.is_null() {
                return None;
            }
            // Stack buffer is plenty for a Win32 path (MAX_PATH-ish);
            // QueryFullProcessImageNameW wants a chars count incl. null terminator.
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(handle);
            if ok == FALSE {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let (key, name) = exe_stem(&path)?;
            Some(super::ForegroundApp { key, name })
        }
    }

    pub(super) fn idle_seconds() -> Option<u64> {
        unsafe {
            let mut info = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if GetLastInputInfo(&mut info) == FALSE {
                return None;
            }
            // Both GetLastInputInfo.dwTime and GetTickCount are millisecond
            // tick counts since boot; their difference is the idle duration.
            // GetTickCount wraps ~49 days, but for idle thresholds of minutes
            // the wraparound window is negligible.
            let now = GetTickCount();
            let idle_ms = now.wrapping_sub(info.dwTime);
            Some((idle_ms / 1000) as u64)
        }
    }
}

// --- macOS implementation --------------------------------------------------

#[cfg(target_os = "macos")]
mod mac {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::Object;
    use std::ffi::CStr;
    use std::os::raw::c_char;

    // Force-link the frameworks whose symbols we touch. AppKit provides
    // NSWorkspace (resolved at runtime via objc_getClass); CoreGraphics provides
    // the idle FFI below. Tauri already pulls these in transitively, but the
    // explicit link keeps this module self-contained.
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        #[allow(dead_code)]
        fn _ipet_appkit_link_placeholder();
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(stateID: u32, eventType: u32) -> f64;
    }

    // kCGEventSourceStateHIDSystemState = 1, kCGAnyInputEventType = ~0.
    const kCGEventSourceStateHIDSystemState: u32 = 1;
    const kCGAnyInputEventType: u32 = 0xFFFF_FFFF;

    pub(super) fn foreground_app() -> Option<super::ForegroundApp> {
        unsafe {
            // NSWorkspace *ws = [NSWorkspace sharedWorkspace];
            let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
            if workspace.is_null() {
                return None;
            }
            // NSRunningApplication *app = [ws frontmostApplication];
            let app: *mut Object = msg_send![workspace, frontmostApplication];
            if app.is_null() {
                return None;
            }
            let name = nsstring(msg_send![app, localizedName])?;
            // bundleIdentifier is the stable grouping key (e.g. com.apple.Safari);
            // fall back to the localized name when it's missing (some helper
            // processes have no bundle id).
            let key = nsstring(msg_send![app, bundleIdentifier]).unwrap_or_else(|| name.clone());
            Some(super::ForegroundApp {
                key: key.to_lowercase(),
                name,
            })
        }
    }

    pub(super) fn idle_seconds() -> Option<u64> {
        unsafe {
            let secs = CGEventSourceSecondsSinceLastEventType(
                kCGEventSourceStateHIDSystemState,
                kCGAnyInputEventType,
            );
            if !secs.is_finite() || secs < 0.0 {
                return None;
            }
            Some(secs as u64)
        }
    }

    /// Read an NSString's UTF-8 bytes into a Rust String. The pointer is
    /// autoreleased; without an enclosing autorelease pool it stays valid for
    /// the call (and leaks harmlessly), so a get-rule read is safe here.
    unsafe fn nsstring(ptr: *mut Object) -> Option<String> {
        if ptr.is_null() {
            return None;
        }
        let utf8: *const c_char = msg_send![ptr, UTF8String];
        if utf8.is_null() {
            return None;
        }
        Some(CStr::from_ptr(utf8).to_string_lossy().into_owned())
    }
}

// --- Linux (X11) implementation -------------------------------------------
//
// Uses `xprop` rather than an X11 Rust binding so there's no new native dep to
// build, and because the sampler already runs on the blocking pool — two tiny
// subprocesses every 15s is negligible (mirrors the clipboard/screenshot tools
// which shell out the same way). Returns None on any parse failure or when
// `xprop` is absent (e.g. pure Wayland), so the feature degrades cleanly.

#[cfg(all(unix, not(target_os = "macos")))]
mod x11 {
    use std::process::Command;

    pub(super) fn foreground_app() -> Option<super::ForegroundApp> {
        // xprop -root _NET_ACTIVE_WINDOW →
        //   _NET_ACTIVE_WINDOW(WINDOW): window id # 0x0380000a
        let root_out = run_xprop(&["-root", "_NET_ACTIVE_WINDOW"])?;
        let window = extract_hex(&root_out)?;
        if window == 0 {
            return None;
        }
        // xprop -id <win> _NET_WM_PID WM_CLASS →
        //   _NET_WM_PID(CARDINAL) = 12345
        //   WM_CLASS(STRING) = "code", "Code"
        let win_out = run_xprop(&["-id", &format!("0x{window:x}"), "_NET_WM_PID", "WM_CLASS"])?;
        let pid = extract_pid(&win_out);

        // Prefer the second WM_CLASS field (the instance→class name, e.g. "Code");
        // fall back to /proc/<pid>/comm for the bare process name.
        let name = extract_wm_class(&win_out)
            .or_else(|| {
                pid.and_then(|p| {
                    std::fs::read_to_string(format!("/proc/{p}/comm"))
                        .ok()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                })
            })
            .filter(|s| !s.is_empty())?;
        Some(super::ForegroundApp {
            key: name.to_lowercase(),
            name,
        })
    }

    fn run_xprop(args: &[&str]) -> Option<String> {
        let out = Command::new("xprop").args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Pull the first `# 0x…` window id from `xprop -root _NET_ACTIVE_WINDOW`.
    pub(super) fn extract_hex(text: &str) -> Option<u32> {
        let line = text.lines().find(|l| l.contains("window id"))?;
        let after = line.split('#').nth(1)?;
        let tok = after.split_whitespace().next()?;
        let hex = tok.strip_prefix("0x").unwrap_or(tok);
        u32::from_str_radix(hex, 16).ok()
    }

    pub(super) fn extract_pid(text: &str) -> Option<u32> {
        let line = text.lines().find(|l| l.contains("_NET_WM_PID"))?;
        let after = line.split('=').nth(1)?;
        after.split_whitespace().next()?.parse::<u32>().ok()
    }

    /// Take the second quoted field of `WM_CLASS(STRING) = "inst", "Class"`,
    /// which is the human-friendly class name.
    pub(super) fn extract_wm_class(text: &str) -> Option<String> {
        let line = text.lines().find(|l| l.contains("WM_CLASS"))?;
        let quoted: Vec<&str> = line.split('"').collect();
        // ["... = ", "inst", ", ", "Class", ""] → index 3 is the class.
        quoted.get(3).map(|s| s.to_string()).filter(|s| !s.is_empty())
    }
}

/// The sampler loop. Spawned once from `lib::run()`; runs for the app's
/// lifetime. Each tick: re-read the `track_app_usage` toggle (so the System
/// view's switch takes effect without an event bus), skip if the user is idle
/// past the configured threshold, otherwise credit `TICK_INTERVAL_SECS` to the
/// foreground app for today's local date.
pub struct UsageSampler {
    storage: Arc<Storage>,
}

impl UsageSampler {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    /// Run forever, ticking every `TICK_INTERVAL_SECS`. Errors are non-fatal —
    /// a transient DB lock failure just skips that tick; the loop continues.
    pub async fn run(self) {
        let mut ticker = interval(Duration::from_secs(TICK_INTERVAL_SECS));
        // First tick fires immediately; skip it so we don't credit 0s of
        // "foreground" before we've actually observed a full interval.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(err) = self.tick().await {
                tracing::warn!(error = %err, "app-usage sampler tick failed");
            }
        }
    }

    async fn tick(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let storage = self.storage.clone();
        // Storage I/O is blocking (SQLite under a std Mutex); run it on the
        // blocking pool so we never stall a Tokio worker. The foreground/idle
        // probes are tiny syscalls, fine inline.
        tokio::task::spawn_blocking(move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let settings = storage.load_llm_settings()?;
            if !settings.track_app_usage {
                return Ok(());
            }
            // Idle threshold: 0 means "never skip on idle". Otherwise skip the
            // tick if the user has been quiet longer than the threshold.
            if let Some(idle) = current_idle_seconds() {
                if settings.app_usage_idle_minutes > 0
                    && idle >= settings.app_usage_idle_minutes * 60
                {
                    return Ok(());
                }
            }
            let Some(app) = current_foreground_app() else {
                // Unsupported OS or no foreground window this tick — nothing
                // to record. Not an error.
                return Ok(());
            };
            let day = Local::now().format("%Y-%m-%d").to_string();
            let last_seen = chrono::Utc::now().to_rfc3339();
            storage.record_app_usage(&day, &app.key, &app.name, TICK_INTERVAL_SECS as i64, &last_seen)?;
            Ok(())
        })
        .await??;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn foreground_app_is_callable_without_panicking() {
        // No foreground guarantee under CI/headless, but the probe must not
        // panic regardless of platform. On a real desktop session with a
        // focused window it returns Some; on an unsupported/empty desktop it
        // returns None.
        let _ = current_foreground_app();
    }

    #[test]
    fn idle_seconds_is_callable_without_panicking() {
        let _ = current_idle_seconds();
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn x11_parsers_window_id_and_wm_class() {
        // Exercises the xprop output parsers without needing a live X server.
        let root = "_NET_ACTIVE_WINDOW(WINDOW): window id # 0x0380000a\n";
        assert_eq!(super::x11::extract_hex(root), Some(0x0380_000a));

        let win = "_NET_WM_PID(CARDINAL) = 12345\nWM_CLASS(STRING) = \"code\", \"Code\"\n";
        assert_eq!(super::x11::extract_pid(win), Some(12345));
        assert_eq!(super::x11::extract_wm_class(win).as_deref(), Some("Code"));
    }

    /// The sampler must respect the `track_app_usage = false` toggle: with it
    /// off, ticking writes no rows even though a foreground app exists.
    #[tokio::test]
    async fn sampler_skips_when_tracking_disabled() {
        let dir = TempDir::new("sampler-disabled");
        let storage = Arc::new(
            Storage::open(dir.path().join("ipet-sampler-test.sqlite3")).unwrap(),
        );
        let mut settings = storage.load_llm_settings().unwrap();
        settings.track_app_usage = false;
        storage.save_llm_settings(&settings).unwrap();

        // Tick once via the private path. We can't easily drive `run()` (it
        // loops forever), so call tick() directly — it's the unit of work.
        let sampler = UsageSampler::new(storage.clone());
        sampler.tick().await.unwrap();

        let stats = storage.app_usage_stats("30d", 50).unwrap();
        assert_eq!(stats.total_seconds, 0, "disabled sampler must not record");
    }
}
