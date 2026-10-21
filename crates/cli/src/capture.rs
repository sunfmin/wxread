//! Capturing WeChat's database keys with Frida
//! (docs/adr/0005-keys-from-the-crypto-call.md). This is the one module that
//! builds the capture command line. It writes the hook and the driver to a
//! temporary directory, runs the driver through `uv` (which supplies a pinned
//! Frida), and reads the keys the driver prints; it keeps nothing. The pure
//! crate [`wxread_crack`] proves each captured key against a database.

#[cfg(target_os = "macos")]
use std::io::{BufRead, BufReader};
#[cfg(target_os = "macos")]
use std::path::PathBuf;

use wxread_crack::{Failure, KEY_LEN};

/// The hook Frida runs inside WeChat, and the driver that loads it.
const HOOK: &str = include_str!("capture/hook.js");
const DRIVER: &str = include_str!("capture/driver.py");

/// How uv is installed, named when it is missing.
const UV_INSTALL: &str = "brew install uv";
/// The Frida uv supplies to run the driver: pinned below 17, the versions on
/// which this CommonCrypto hook fires (docs/adr/0005-keys-from-the-crypto-call.md).
const FRIDA_SPEC: &str = "frida<17";
/// How long the capture waits for a new database's key with none arriving
/// before it decides the burst is over and stops early. Logging in decrypts
/// every database within a second or two, so once nothing new has come for
/// this long the keys are in; `--seconds` is the cap, not the wait.
#[cfg(target_os = "macos")]
const IDLE_STOP: std::time::Duration = std::time::Duration::from_secs(10);

/// What `doctor` says of the capture tool: whether uv is found, what needs
/// it, and the line that installs it when it is not.
pub fn readiness() -> serde_json::Value {
    #[cfg(target_os = "macos")]
    let found = which("uv").is_some();
    #[cfg(not(target_os = "macos"))]
    let found = false;
    serde_json::json!({
        "uv": {
            "found": found,
            "needed_for": "`sudo wxread keys`, which runs Frida through uv to capture the keys",
            "fix": (!found).then(|| format!("install uv: `{UV_INSTALL}`")),
        }
    })
}

/// Captures the distinct 32-byte keys WeChat hands CommonCrypto while
/// `seconds` pass, handing each one to `on_key` the moment it is seen (so the
/// caller can prove and report it live), as candidates for
/// [`wxread_crack::verify`]; or the one line saying why the capture could not
/// run. `on_key` is called once per distinct key, never twice for the same.
#[cfg(target_os = "macos")]
pub fn probe(seconds: u64, on_key: impl FnMut(&[u8; KEY_LEN])) -> Result<(), Failure> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(Failure(wxread_crack::NOT_ROOT.to_string()));
    }
    let Some(pid) = crate::memory::find_wechat() else {
        return Err(Failure(wxread_crack::NOT_RUNNING.to_string()));
    };
    let Some(uv) = which("uv") else {
        return Err(wxread_crack::tool_missing("uv", UV_INSTALL));
    };
    run(&uv, pid, seconds, on_key)
}

/// Off macOS there is no WeChat for Mac to capture from
/// (docs/adr/0001-macos-only-and-apache-2.md).
#[cfg(not(target_os = "macos"))]
pub fn probe(_seconds: u64, _on_key: impl FnMut(&[u8; KEY_LEN])) -> Result<(), Failure> {
    Err(wxread_crack::capture_failed(
        "wxread keys captures from WeChat for Mac, and this is not a macOS build",
    ))
}

/// Writes the hook and driver out, runs the driver through uv against `pid`
/// for `seconds`, and hands each distinct captured key to `on_key` as it
/// arrives.
#[cfg(target_os = "macos")]
fn run(
    uv: &std::path::Path,
    pid: i32,
    seconds: u64,
    mut on_key: impl FnMut(&[u8; KEY_LEN]),
) -> Result<(), Failure> {
    use std::process::{Command, Stdio};

    let dir = std::env::temp_dir().join(format!("wxread-capture-{}", std::process::id()));
    let hook = dir.join("hook.js");
    let driver = dir.join("driver.py");
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&hook, HOOK)?;
        std::fs::write(&driver, DRIVER)
    };
    if let Err(e) = write() {
        return Err(wxread_crack::capture_failed(&format!(
            "the hook could not be written to {}: {e}",
            dir.display()
        )));
    }

    // Tell the person what to do while the capture runs: a key appears only
    // while WeChat decrypts a page, so they browse. This is the progress of
    // the run, so it goes to stderr; stdout is the one JSON object.
    eprintln!(
        "wxread keys: capturing (up to {seconds}s, stops once the keys stop coming) — log in, or open chats, use search and open files, so WeChat decrypts its databases"
    );

    let mut child = match Command::new(uv)
        .args(["run", "--no-project", "--with", FRIDA_SPEC, "python"])
        .arg(&driver)
        .arg(&hook)
        .arg(pid.to_string())
        .arg(seconds.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(wxread_crack::capture_failed(&format!("uv could not be run: {e}")));
        }
    };

    // Drain stderr in another thread so a full pipe never blocks the driver.
    let stderr = child.stderr.take();
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stderr) = stderr {
            use std::io::Read;
            let _ = BufReader::new(stderr).read_to_string(&mut text);
        }
        text
    });

    // Read the keys off stdout on another thread, over a channel, so the loop
    // below can wake on a timer: keys stream in while WeChat decrypts, and when
    // they stop coming the burst is over. Holding the child's stdin keeps the
    // driver running; dropping it asks the driver to stop (it watches stdin).
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let (tx, rx) = std::sync::mpsc::channel::<[u8; KEY_LEN]>();
    let reader = std::thread::spawn(move || {
        if let Some(stdout) = stdout {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(key) = key_of(&line)
                    && tx.send(key).is_err()
                {
                    break;
                }
            }
        }
    });

    let mut seen: Vec<[u8; KEY_LEN]> = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    loop {
        let now = std::time::Instant::now();
        let Some(left) = deadline.checked_duration_since(now) else {
            break; // the whole window has passed
        };
        // Wait for the next key, but no longer than the idle window once at
        // least one key is in: a quiet stretch that long means the burst is
        // over, so stop early instead of running out the window.
        let wait = if seen.is_empty() { left } else { left.min(IDLE_STOP) };
        match rx.recv_timeout(wait) {
            Ok(key) => {
                if !seen.contains(&key) {
                    seen.push(key);
                    on_key(&key);
                }
            }
            // Nothing for the whole wait: the burst is over (we have keys), or
            // the window ran out. Either way, stop.
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !seen.is_empty() {
                    break;
                }
            }
            // The driver closed stdout: it has exited, so we are done.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    // Ask the driver to stop (it watches stdin for this) and reap it, but do
    // not wait out its window if it ignores us: kill it after a short grace.
    drop(stdin);
    let status = wait_or_kill(&mut child);
    let _ = reader.join();
    let errors = errors.join().unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);

    // Frida could not attach — almost always a WeChat without get-task-allow.
    if let Some(cause) = attach_error(&errors) {
        return Err(wxread_crack::attach_refused(&cause));
    }
    // uv or the driver failed, and nothing was captured to make up for it.
    if seen.is_empty()
        && let Ok(status) = &status
        && !status.success()
    {
        return Err(wxread_crack::capture_failed(&failure_cause(&errors)));
    }
    Ok(())
}

/// Reaps the driver after its stdin was closed to stop it, without waiting out
/// its whole window: if it has not exited within a short grace, kill it.
#[cfg(target_os = "macos")]
fn wait_or_kill(child: &mut std::process::Child) -> std::io::Result<std::process::ExitStatus> {
    let grace = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if std::time::Instant::now() >= grace {
                    let _ = child.kill();
                    return child.wait();
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(e),
        }
    }
}

/// The 32-byte key a driver stdout line carries, if it is a `{"wxkey": …}`
/// line with 64 hex digits.
#[cfg(target_os = "macos")]
fn key_of(line: &str) -> Option<[u8; KEY_LEN]> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let hex = value.get("wxkey")?.as_str()?;
    wxread_keys::decode_key(hex.as_bytes())
}

/// The cause the driver reported for a refused attach, if that is why it
/// stopped (its `attach_error` line).
#[cfg(target_os = "macos")]
fn attach_error(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim())
            && let Some(cause) = value.get("attach_error").and_then(|c| c.as_str())
        {
            return Some(cause.to_string());
        }
    }
    None
}

/// A short cause for a failed capture: the last non-empty stderr line, or that
/// there was none.
#[cfg(target_os = "macos")]
fn failure_cause(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("uv exited without a reason")
        .to_string()
}

/// The path to command `name`, from `PATH` and the places uv is usually
/// installed that a root `PATH` often drops (Homebrew, the user's own bin).
#[cfg(target_os = "macos")]
fn which(name: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let mut extra = vec![PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")];
    if let Some(user) = std::env::var_os("SUDO_USER")
        && let Some(home) = crate::world::home_of_user(user.to_string_lossy().as_ref())
    {
        extra.push(home.join(".local/bin"));
        extra.push(home.join(".cargo/bin"));
    }
    extra.into_iter().map(|dir| dir.join(name)).find(|c| c.is_file())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn a_wxkey_line_is_read_only_as_a_full_32_byte_key() {
        let hex = "e93dee73c8bf038e5384a4e3a77f4247d780da78a097d11f8b2f2891f95f6f0a";
        assert_eq!(
            key_of(&format!("{{\"wxkey\":\"{hex}\"}}")),
            wxread_keys::decode_key(hex.as_bytes())
        );
        // Not a key line, not JSON, a short run: nothing.
        assert_eq!(key_of("{\"ready\":true}"), None);
        assert_eq!(key_of("objc[123]: some noise"), None);
        assert_eq!(key_of("{\"wxkey\":\"dead\"}"), None);
    }

    #[test]
    fn a_refused_attach_is_read_from_the_drivers_stderr() {
        let err = "objc noise\n{\"attach_error\": \"unable to access process with pid 42\"}\n";
        assert_eq!(attach_error(err).as_deref(), Some("unable to access process with pid 42"));
        assert_eq!(attach_error("{\"frida_error\": \"x\"}\nnoise"), None);
    }

    #[test]
    fn a_failure_cause_is_the_last_thing_said() {
        assert_eq!(failure_cause("first\n\nlast line\n\n"), "last line");
        assert_eq!(failure_cause("   \n\n"), "uv exited without a reason");
    }
}
