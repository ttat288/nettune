//! Safe external command execution.
//!
//! Commands are always spawned directly with an argument vector (never through a
//! shell), with a fixed `C` locale, a closed stdin and a hard timeout after which
//! the child is killed and reaped.

use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOutput {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdError {
    /// The program is not installed / not on PATH.
    NotFound(String),
    /// The program exceeded its time budget and was killed.
    Timeout(String),
    /// Any other spawn/wait failure.
    Io(String),
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CmdError::NotFound(p) => write!(f, "`{p}` is not installed or not on PATH"),
            CmdError::Timeout(p) => write!(f, "`{p}` did not finish in time and was stopped"),
            CmdError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// Abstraction over command execution so configuration logic can be tested
/// without touching the real system.
pub trait Runner: Send + Sync {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> Result<CmdOutput, CmdError>;
}

pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> Result<CmdOutput, CmdError> {
        run(program, args, timeout)
    }
}

pub fn run(program: &str, args: &[&str], timeout: Duration) -> Result<CmdOutput, CmdError> {
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => CmdError::NotFound(program.to_string()),
            _ => CmdError::Io(format!("failed to start `{program}`: {e}")),
        })?;

    // Drain pipes on helper threads so a chatty child can never block on a full pipe.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_t = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let err_t = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_t.join();
                let _ = err_t.join();
                return Err(CmdError::Timeout(program.to_string()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(15)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CmdError::Io(format!("waiting for `{program}` failed: {e}")));
            }
        }
    };

    let stdout = String::from_utf8_lossy(&out_t.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_t.join().unwrap_or_default()).into_owned();
    Ok(CmdOutput {
        status: status.code(),
        stdout,
        stderr,
    })
}

/// Split one line of `nmcli -t` output on unescaped `:` and unescape `\:` / `\\`.
pub fn split_nmcli_terse(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ':' => fields.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

/// Unescape a single `nmcli -g` value (same escaping as `-t`, but no splitting).
pub fn unescape_nmcli(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_split_handles_escapes() {
        let f = split_nmcli_terse(r"*:90:my\:ssid:4E\:45\:7C:2437 MHz");
        assert_eq!(f, vec!["*", "90", "my:ssid", "4E:45:7C", "2437 MHz"]);
        assert_eq!(split_nmcli_terse(r"a\\:b"), vec![r"a\", "b"]);
        assert_eq!(split_nmcli_terse("x::y"), vec!["x", "", "y"]);
    }

    #[test]
    fn unescape_value() {
        assert_eq!(unescape_nmcli(r"fe80\:\:1"), "fe80::1");
    }

    #[test]
    fn missing_program_is_reported() {
        let r = run(
            "nettune-definitely-missing-binary",
            &[],
            Duration::from_secs(1),
        );
        assert!(matches!(r, Err(CmdError::NotFound(_))));
    }

    #[test]
    fn timeout_kills_child() {
        let t0 = Instant::now();
        let r = run("sleep", &["5"], Duration::from_millis(200));
        assert!(matches!(r, Err(CmdError::Timeout(_))));
        assert!(t0.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn captures_output() {
        let r = run("echo", &["hello; rm -rf /"], Duration::from_secs(2)).unwrap();
        // The argument is passed literally — no shell interpretation.
        assert_eq!(r.stdout.trim(), "hello; rm -rf /");
        assert!(r.success());
    }
}
