//! Pre-launch hooks.
//!
//! A profile may ship one script inside `$CONFIG/scripts/<profile>/`. The
//! launcher runs it **before** navigating to the profile URL, streams its
//! output to the UI, enforces a timeout and can wait until the port actually
//! accepts connections.
//!
//! Everything here is platform-gated but free of Tauri types, which keeps the
//! behaviour unit-testable on a headless machine.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::{PrelaunchMode, Profile};
use crate::platform;

/// File name candidates, in priority order.
pub const SCRIPT_CANDIDATES: [&str; 4] = [
    "pre-launch.sh",
    "pre-launch.ps1",
    "pre-launch.cmd",
    "pre-launch.bat",
];

/// How a pre-launch invocation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutcomeStatus {
    /// `prelaunch = "skip"`, or no script exists.
    Skipped,
    /// The script exited with code 0.
    Succeeded,
    /// The script exited with a non-zero code.
    Failed,
    /// The timeout elapsed; the process tree was killed.
    TimedOut,
    /// The script could not even be started.
    Error,
    /// The user cancelled the run.
    Cancelled,
}

impl OutcomeStatus {
    #[must_use]
    pub const fn is_success(self) -> bool {
        matches!(self, Self::Skipped | Self::Succeeded)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::TimedOut => "timed-out",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Phase reported while the hook runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrelaunchPhase {
    Resolving,
    Starting,
    Running,
    WaitingForPort,
    Finished,
}

/// A single progress update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrelaunchProgress {
    pub phase: PrelaunchPhase,
    pub elapsed_ms: u64,
    /// Lines appended to the log since the previous update.
    pub lines: Vec<String>,
}

/// Final result of a pre-launch invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrelaunchOutcome {
    pub profile_id: String,
    pub script: Option<String>,
    pub status: OutcomeStatus,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub log_path: Option<String>,
    pub log_tail: Vec<String>,
    pub port_ready: Option<bool>,
    pub message: Option<String>,
}

/// Shared cancellation handle for a running hook.
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Runs the hook for `profile`.
///
/// `on_progress` is called from the calling thread (never concurrently) at a
/// bounded rate, so it is safe to forward the events to a UI.
pub fn run_with_progress<F>(
    profile: &Profile,
    cancel: &Cancellation,
    mut on_progress: F,
) -> PrelaunchOutcome
where
    F: FnMut(PrelaunchProgress),
{
    let started = Instant::now();
    let mut outcome = PrelaunchOutcome {
        profile_id: profile.id.clone(),
        script: None,
        status: OutcomeStatus::Skipped,
        exit_code: None,
        duration_ms: 0,
        log_path: None,
        log_tail: Vec::new(),
        port_ready: None,
        message: None,
    };

    if profile.prelaunch == PrelaunchMode::Skip {
        outcome.message = Some("pre-launch disabled for this profile".to_owned());
        return outcome;
    }

    on_progress(PrelaunchProgress {
        phase: PrelaunchPhase::Resolving,
        elapsed_ms: 0,
        lines: Vec::new(),
    });

    let Some(script) = resolve_script(profile) else {
        outcome.message = Some(format!(
            "no pre-launch script in {}",
            platform::display_relative(&platform::script_dir_for(&profile.id))
        ));
        return outcome;
    };

    outcome.script = Some(platform::display_relative(&script.path));

    let log_path = platform::logs_dir().join(format!(
        "prelaunch-{}.log",
        platform::sanitize_component(&profile.id)
    ));
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = File::create(&log_path);
    outcome.log_path = Some(platform::display_relative(&log_path));

    on_progress(PrelaunchProgress {
        phase: PrelaunchPhase::Starting,
        elapsed_ms: started.elapsed().as_millis() as u64,
        lines: vec![format!("$ {}", script.display_command)],
    });

    let mut command = build_command(profile, &script);

    let log_handle = OpenOptions::new().append(true).open(&log_path).ok();
    if let Some(handle) = log_handle.as_ref() {
        if let Ok(clone) = handle.try_clone() {
            command.stdout(Stdio::from(clone));
        }
        if let Ok(clone) = handle.try_clone() {
            command.stderr(Stdio::from(clone));
        }
    }

    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            outcome.status = OutcomeStatus::Error;
            outcome.message = Some(format!("failed to start the script: {error}"));
            outcome.duration_ms = started.elapsed().as_millis() as u64;
            outcome.log_tail = read_tail(&log_path, 40);
            return outcome;
        }
    };

    register_pid(&profile.id, child.id());
    let mut child = ChildGuard { child: Some(child) };

    let deadline = started + Duration::from_secs(profile.timeout_secs);
    let mut offset = 0_u64;
    let mut last_emit = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                outcome.exit_code = status.code();
                outcome.status = if status.success() {
                    OutcomeStatus::Succeeded
                } else {
                    OutcomeStatus::Failed
                };
                break;
            }
            Ok(None) => {}
            Err(error) => {
                outcome.status = OutcomeStatus::Error;
                outcome.message = Some(format!("could not poll the script: {error}"));
                break;
            }
        }

        if cancel.is_cancelled() {
            child.kill_tree();
            outcome.status = OutcomeStatus::Cancelled;
            outcome.message = Some("cancelled".to_owned());
            break;
        }

        if Instant::now() >= deadline {
            child.kill_tree();
            outcome.status = OutcomeStatus::TimedOut;
            outcome.message = Some(format!(
                "the script did not finish within {}s and was killed",
                profile.timeout_secs
            ));
            break;
        }

        if last_emit.elapsed() >= Duration::from_millis(350) {
            last_emit = Instant::now();
            let lines = read_new_lines(&log_path, &mut offset);
            on_progress(PrelaunchProgress {
                phase: PrelaunchPhase::Running,
                elapsed_ms: started.elapsed().as_millis() as u64,
                lines,
            });
        }

        thread::sleep(Duration::from_millis(60));
    }

    unregister_pid(&profile.id);
    outcome.duration_ms = started.elapsed().as_millis() as u64;

    if outcome.status.is_success() && profile.prelaunch == PrelaunchMode::WaitForPort {
        on_progress(PrelaunchProgress {
            phase: PrelaunchPhase::WaitingForPort,
            elapsed_ms: outcome.duration_ms,
            lines: Vec::new(),
        });
        let ready = wait_for_port(&profile.url, profile.wait_timeout_secs, cancel);
        outcome.port_ready = Some(ready);
        if !ready {
            outcome.status = OutcomeStatus::TimedOut;
            outcome.message = Some(format!(
                "nothing accepted connections on the profile URL within {}s",
                profile.wait_timeout_secs
            ));
        }
    }

    let mut lines = read_new_lines(&log_path, &mut offset);
    if !lines.is_empty() {
        on_progress(PrelaunchProgress {
            phase: PrelaunchPhase::Finished,
            elapsed_ms: outcome.duration_ms,
            lines: std::mem::take(&mut lines),
        });
    }

    outcome.log_tail = read_tail(&log_path, 200);
    on_progress(PrelaunchProgress {
        phase: PrelaunchPhase::Finished,
        elapsed_ms: outcome.duration_ms,
        lines: Vec::new(),
    });
    outcome
}

/// Resolved script: absolute path plus the command line shown to the user.
#[derive(Debug, Clone)]
pub struct ResolvedScript {
    pub path: PathBuf,
    pub display_command: String,
}

/// Finds the script a profile should run.
///
/// `None` means "there is no (safe) script", which is not an error: profiles may
/// legitimately have no hook. A script name that escapes its directory is
/// rejected by returning `None` as well.
#[must_use]
pub fn resolve_script(profile: &Profile) -> Option<ResolvedScript> {
    let dir = platform::script_dir_for(&profile.id);
    let candidate = if let Some(name) = profile.script.as_deref() {
        if !crate::config::is_safe_file_name(name) {
            return None;
        }
        let path = dir.join(name);
        if path.is_file() {
            Some(path)
        } else {
            None
        }
    } else {
        SCRIPT_CANDIDATES
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file())
    };

    candidate.map(|path| {
        let display_command = format!(
            "{} {}",
            platform::display_relative(&path),
            profile.args.join(" ")
        )
        .trim()
        .to_owned();
        ResolvedScript {
            path,
            display_command,
        }
    })
}

fn build_command(profile: &Profile, script: &ResolvedScript) -> Command {
    let mut command = script_command(&script.path);
    command
        .current_dir(script.path.parent().unwrap_or(Path::new(".")))
        .args(&profile.args)
        .stdin(Stdio::null())
        .env_remove("NO_COLOR")
        .env("TAURI_3080_PROFILE", &profile.id)
        .env("TAURI_3080_URL", &profile.url)
        .env("TAURI_3080_CONFIG_DIR", platform::config_dir());

    for (key, value) in &profile.env {
        if key.is_empty() || key.contains('=') || key.contains('\0') {
            continue;
        }
        command.env(key, value);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // New process group so the whole tree can be killed on timeout.
        command.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
}

/// Builds the platform-appropriate command for a script path.
fn script_command(path: &Path) -> Command {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    #[cfg(windows)]
    {
        match extension.as_str() {
            "ps1" => {
                let mut command = Command::new("powershell.exe");
                command
                    .arg("-NoProfile")
                    .arg("-NonInteractive")
                    .arg("-ExecutionPolicy")
                    .arg("Bypass")
                    .arg("-File")
                    .arg(path);
                command
            }
            "cmd" | "bat" => {
                let mut command = Command::new("cmd.exe");
                command.arg("/C").arg(path);
                command
            }
            _ => {
                let mut command = Command::new("sh.exe");
                command.arg(path);
                command
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = extension;
        // Running through `sh` keeps the hook working when the executable bit
        // was lost (zip downloads, synced folders, Windows-made archives).
        let mut command = Command::new("sh");
        command.arg(path);
        command
    }
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        match self.child.as_mut() {
            Some(child) => child.try_wait(),
            None => Ok(None),
        }
    }

    fn kill_tree(&mut self) {
        let Some(pid) = self.child.as_ref().map(Child::id) else {
            return;
        };
        kill_process_tree(pid);
        if let Some(mut child) = self.child.take() {
            let _ = child.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Kills a process and every descendant it spawned.
pub fn kill_process_tree(pid: u32) {
    #[cfg(unix)]
    {
        // Negative pid targets the whole process group created in
        // `build_command`.
        let group = i32::try_from(pid).unwrap_or(i32::MAX);
        // SAFETY: `kill(2)` with a negative pid signals the process group we
        // created; a stale pgid simply fails with ESRCH.
        #[allow(unsafe_code)]
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
        best_effort_kill(pid);
    }

    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(unix)]
fn best_effort_kill(pid: u32) {
    if let Ok(pid) = i32::try_from(pid) {
        // SAFETY: signalling a single pid that we just spawned; failure is
        // ignored because the process may already be gone.
        #[allow(unsafe_code)]
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

/// Remembers the currently running hook per profile so `kill_previous` works.
fn running_table() -> &'static Mutex<BTreeMap<String, u32>> {
    static TABLE: std::sync::OnceLock<Mutex<BTreeMap<String, u32>>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn register_pid(profile_id: &str, pid: u32) {
    if let Ok(mut table) = running_table().lock() {
        table.insert(profile_id.to_owned(), pid);
    }
}

fn unregister_pid(profile_id: &str) {
    if let Ok(mut table) = running_table().lock() {
        table.remove(profile_id);
    }
}

/// Kills the hook that is still running for `profile_id`, if any.
pub fn kill_running(profile_id: &str) -> bool {
    let pid = running_table()
        .lock()
        .ok()
        .and_then(|mut table| table.remove(profile_id));
    match pid {
        Some(pid) => {
            kill_process_tree(pid);
            true
        }
        None => false,
    }
}

/// Waits until `url`'s host and port accept a TCP connection.
#[must_use]
pub fn wait_for_port(url: &str, timeout_secs: u64, cancel: &Cancellation) -> bool {
    let Some((host, port)) = host_and_port(url) else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);

    while Instant::now() < deadline {
        if cancel.is_cancelled() {
            return false;
        }
        if let Ok(addresses) = (host.as_str(), port).to_socket_addrs() {
            for address in addresses {
                if std::net::TcpStream::connect_timeout(&address, Duration::from_millis(700))
                    .is_ok()
                {
                    return true;
                }
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Extracts `(host, port)` from an http(s) URL.
#[must_use]
pub fn host_and_port(url: &str) -> Option<(String, u16)> {
    let lowered = url.trim().to_ascii_lowercase();
    let (scheme, rest) = if let Some(rest) = lowered.strip_prefix("http://") {
        ("http", rest)
    } else {
        ("https", lowered.strip_prefix("https://")?)
    };

    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() {
        return None;
    }

    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        // IPv6 literal, e.g. [::1]:3080
        let close = stripped.find(']')?;
        let host = &stripped[..close];
        let remainder = &stripped[close + 1..];
        let port = remainder
            .strip_prefix(':')
            .and_then(|raw| raw.parse::<u16>().ok());
        (host.to_owned(), port)
    } else if let Some((host, raw_port)) = authority.rsplit_once(':') {
        if raw_port.chars().all(|character| character.is_ascii_digit()) && !raw_port.is_empty() {
            (host.to_owned(), Some(raw_port.parse::<u16>().ok()?))
        } else {
            (authority.to_owned(), None)
        }
    } else {
        (authority.to_owned(), None)
    };

    let default_port = if scheme == "https" { 443 } else { 80 };
    Some((host, port.unwrap_or(default_port)))
}

fn read_new_lines(path: &Path, offset: &mut u64) -> Vec<String> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    if file.seek(SeekFrom::Start(*offset)).is_err() {
        return Vec::new();
    }
    let mut buffer = String::new();
    if file.read_to_string(&mut buffer).is_err() {
        return Vec::new();
    }
    *offset += buffer.len() as u64;

    let mut lines: Vec<String> = buffer
        .lines()
        .map(|line| line.chars().take(500).collect())
        .collect();
    if lines.len() > 200 {
        lines = lines.split_off(lines.len() - 200);
    }
    lines
}

fn read_tail(path: &Path, max_lines: usize) -> Vec<String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = raw
        .lines()
        .map(|line| line.chars().take(500).collect())
        .collect();
    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a profile whose script directory already contains a hook.
    #[cfg(unix)]
    fn profile_with_script(body: &str) -> Profile {
        let profile = Profile {
            id: "hook-test".to_owned(),
            timeout_secs: 5,
            ..Profile::default()
        };
        let script_dir = platform::script_dir_for(&profile.id);
        fs::create_dir_all(&script_dir).unwrap();
        fs::write(script_dir.join("pre-launch.sh"), body).unwrap();
        profile
    }

    #[cfg(unix)]
    #[test]
    fn runs_a_script_and_reports_success() {
        let dir = tempfile::tempdir().unwrap();
        platform::with_config_dir_override(dir.path(), || {
            let profile = profile_with_script("echo hello from hook\n");
            let mut phases = Vec::new();
            let outcome = run_with_progress(&profile, &Cancellation::new(), |progress| {
                phases.push(progress.phase);
            });

            assert_eq!(outcome.status, OutcomeStatus::Succeeded);
            assert_eq!(outcome.exit_code, Some(0));
            assert!(outcome
                .log_tail
                .iter()
                .any(|line| line.contains("hello from hook")));
            assert!(phases.contains(&PrelaunchPhase::Starting));
            assert!(outcome
                .log_path
                .as_deref()
                .is_some_and(|path| path.starts_with("logs/")));
        });
    }

    #[cfg(unix)]
    #[test]
    fn reports_failure_exit_codes() {
        let dir = tempfile::tempdir().unwrap();
        platform::with_config_dir_override(dir.path(), || {
            let profile = profile_with_script("echo boom >&2\nexit 3\n");
            let outcome = run_with_progress(&profile, &Cancellation::new(), |_| {});

            assert_eq!(outcome.status, OutcomeStatus::Failed);
            assert_eq!(outcome.exit_code, Some(3));
            assert!(outcome.log_tail.iter().any(|line| line.contains("boom")));
        });
    }

    #[cfg(unix)]
    #[test]
    fn enforces_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        platform::with_config_dir_override(dir.path(), || {
            let mut profile = profile_with_script("sleep 30\n");
            profile.timeout_secs = 2;
            let outcome = run_with_progress(&profile, &Cancellation::new(), |_| {});

            assert_eq!(outcome.status, OutcomeStatus::TimedOut);
            assert!(outcome.duration_ms >= 1_500);
            assert!(outcome.duration_ms < 10_000);
        });
    }

    #[test]
    fn skips_when_disabled_or_missing() {
        let dir = tempfile::tempdir().unwrap();
        platform::with_config_dir_override(dir.path(), || {
            let profile = Profile {
                id: "nothing".to_owned(),
                prelaunch: PrelaunchMode::Skip,
                ..Profile::default()
            };
            assert_eq!(
                run_with_progress(&profile, &Cancellation::new(), |_| {}).status,
                OutcomeStatus::Skipped
            );

            let profile = Profile {
                prelaunch: PrelaunchMode::Run,
                ..profile
            };
            let outcome = run_with_progress(&profile, &Cancellation::new(), |_| {});
            assert_eq!(outcome.status, OutcomeStatus::Skipped);
            assert!(outcome.script.is_none());
        });
    }

    #[test]
    fn parses_hosts_and_ports() {
        assert_eq!(
            host_and_port("http://127.0.0.1:3080"),
            Some(("127.0.0.1".to_owned(), 3080))
        );
        assert_eq!(
            host_and_port("https://example.com/path?x=1"),
            Some(("example.com".to_owned(), 443))
        );
        assert_eq!(
            host_and_port("http://localhost"),
            Some(("localhost".to_owned(), 80))
        );
        assert_eq!(
            host_and_port("http://[::1]:8080/"),
            Some(("::1".to_owned(), 8080))
        );
        assert_eq!(host_and_port("file:///tmp/x.html"), None);
    }

    #[test]
    fn wait_for_port_finds_a_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");
        assert!(wait_for_port(&url, 3, &Cancellation::new()));
    }

    #[test]
    fn wait_for_port_times_out() {
        // Port 1 is almost always closed for unprivileged test runs.
        let started = Instant::now();
        assert!(!wait_for_port(
            "http://127.0.0.1:1",
            1,
            &Cancellation::new()
        ));
        assert!(started.elapsed() >= Duration::from_millis(800));
    }

    #[test]
    fn cancellation_stops_waiting() {
        let cancel = Cancellation::new();
        cancel.cancel();
        assert!(!wait_for_port("http://127.0.0.1:1", 5, &cancel));
    }

    #[test]
    fn rejects_path_traversal_in_custom_script_names() {
        let dir = tempfile::tempdir().unwrap();
        platform::with_config_dir_override(dir.path(), || {
            let profile = Profile {
                id: "traversal".to_owned(),
                script: Some("../../evil.sh".to_owned()),
                ..Profile::default()
            };
            assert!(resolve_script(&profile).is_none());
        });
    }
}
