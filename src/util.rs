//! Tiny helpers: cheap sysfs/proc reads and a timeout-guarded subprocess runner.
//!
//! Every helper returns `Option`/`Result` and **never panics** on I/O failure —
//! the bar must stay alive when a file disappears (battery unplugged, iface gone).

use std::ffi::CString;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

/// Read a sysfs/proc file and return its trimmed first line as `String`.
pub fn read_first_line(path: &str) -> Option<String> {
    let mut buf = String::new();
    fs::File::open(path).ok()?.read_to_string(&mut buf).ok()?;
    Some(buf.lines().next()?.trim().to_owned())
}

/// Read a whole small file as a trimmed string.
pub fn read_to_string(path: &str) -> Option<String> {
    let mut buf = String::new();
    fs::File::open(path).ok()?.read_to_string(&mut buf).ok()?;
    Some(buf.trim().to_owned())
}

/// Read a sysfs integer (e.g. `temp`, `capacity`).
pub fn read_int(path: &str) -> Option<i64> {
    read_first_line(path)?.trim().parse::<i64>().ok()
}

/// Read a `/sys/.../value` that holds "milli-units" (e.g. temp in millicelsius)
/// and convert to the base unit.
pub fn read_milli(path: &str) -> Option<i64> {
    Some(read_int(path)? / 1000)
}

/// List the names of entries directly under a directory (non-recursive).
pub fn list_dir(path: &str) -> Vec<String> {
    fs::read_dir(path)
        .map(|rd| {
            rd.filter_map(std::result::Result::ok)
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Glob `/sys/class/<cls>/<glob>` → absolute paths, sorted for stable ordering.
pub fn glob_class(class: &str, pat: &str) -> Vec<String> {
    let base = format!("/sys/class/{class}");
    let mut out: Vec<String> = list_dir(&base)
        .into_iter()
        .filter(|n| glob_match(pat, n))
        .map(|n| format!("{base}/{n}"))
        .collect();
    out.sort();
    out
}

/// Tiny `?`/`*` glob matcher (single segment only — good enough for sysfs names).
pub fn glob_match(pat: &str, name: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_rec(&p, 0, &n, 0)
}

fn glob_rec(p: &[char], pi: usize, n: &[char], ni: usize) -> bool {
    match p.get(pi) {
        None => ni == n.len(),
        Some(&'*') => (ni..=n.len()).any(|skip| glob_rec(p, pi + 1, n, skip)),
        Some(&c) => ni < n.len() && n[ni] == c && glob_rec(p, pi + 1, n, ni + 1),
    }
}

/// Run a subprocess, capture stdout, and hard-kill it after `timeout_ms` so a
/// hanging helper (`pamixer`, `iw`, `bluetoothctl`) can never freeze a bar tick.
/// Returns the trimmed stdout on a clean exit within the deadline.
pub fn run_with_timeout(cmd: &str, args: &[&str], timeout_ms: u64) -> Option<String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .ok()?;

    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut o) = stdout {
            let _ = o.read_to_string(&mut s);
        }
        s
    });

    match child.wait_timeout(Duration::from_millis(timeout_ms)) {
        Ok(Some(_status)) => {
            let s = reader.join().unwrap_or_default();
            Some(s.trim().to_owned())
        }
        Ok(None) => {
            // Timed out — kill, reap, and drop. The reader thread unblocks once
            // the killed child's stdout pipe closes.
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            None
        }
        Err(_) => {
            let _ = child.kill();
            let _ = reader.join();
            None
        }
    }
}

/// `statvfs(2)` for a mount point → `(total_bytes, free_bytes)`.
///
/// One `unsafe` call into libc for filesystem stats — there is no pure-std way.
/// Missing/odd mounts return `None` and the disk block silently skips them.
pub fn statvfs(mount: &str) -> Option<(u64, u64)> {
    let c_path = CString::new(mount).ok()?;
    let mut buf: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `statvfs` reads the path and writes into our zeroed struct; the
    // pointer is a valid NUL-terminated CString living on this stack frame.
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &raw mut buf) };
    if rc != 0 {
        return None;
    }
    let bsize = buf.f_bsize;
    let total = bsize.checked_mul(buf.f_blocks)?;
    let avail = bsize.checked_mul(buf.f_bavail)?;
    Some((total, avail))
}

/// True when `path` exists and is a directory (used to detect e.g. a wireless iface).
pub fn is_dir(path: &str) -> bool {
    Path::new(path).is_dir()
}

/// Humanize a byte count → e.g. `4.2G`, `812M`, `0B`.
#[allow(clippy::cast_precision_loss)]
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
    let mut val = bytes as f64;
    let mut unit = 0;
    while val >= 1024.0 && unit + 1 < UNITS.len() {
        val /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else {
        format!("{val:.1}{}", UNITS[unit])
    }
}

/// Humanize a per-second rate → e.g. `1.2M/s`, `0B/s`.
pub fn human_rate(bytes_per_sec: u64) -> String {
    format!("{}/s", human_bytes(bytes_per_sec))
}
