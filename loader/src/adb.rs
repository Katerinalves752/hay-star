// ============================================================================
//  HAY STAR - ADB helpers (mirrors loader.py ADB functions)
// ============================================================================

use std::process::{Command, Output};
use std::time::Duration;
use regex::Regex;
use crate::error::{LoaderError, Result};
use crate::config::ADB_SEARCH_PATHS;

fn run_with_timeout(cmd: &mut Command, timeout_secs: u64) -> std::io::Result<Output> {
    // On Windows we cannot easily set a per-process timeout via std::process.
    // We run the command synchronously; the timeout is advisory.
    // For true timeout, a thread + channel pattern can be used if needed.
    let _ = timeout_secs;
    cmd.output()
}

pub fn find_adb() -> Option<String> {
    for path in ADB_SEARCH_PATHS {
        let result = Command::new(path)
            .args(["version"])
            .output();
        if let Ok(out) = result {
            if out.status.success() {
                return Some(path.to_string());
            }
        }
    }
    None
}

pub fn find_emulator(adb: &str) -> Option<String> {
    let result = Command::new(adb).args(["devices"]).output().ok()?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    let mut devices: Vec<String> = Vec::new();
    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() || line.contains("offline") { continue; }
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() >= 2 && parts[1].trim() == "device" {
            devices.push(parts[0].to_string());
        }
    }
    // Prefer 127.0.0.1 (LDPlayer), then emulator-*, then first
    for d in &devices { if d.contains("127.0.0.1") { return Some(d.clone()); } }
    for d in &devices { if d.contains("emulator")  { return Some(d.clone()); } }
    devices.into_iter().next()
}

/// Run `adb -s <device> <args>` and return the Output.
pub fn adb_cmd(adb: &str, device_id: &str, args: &[&str]) -> Result<Output> {
    let mut cmd = Command::new(adb);
    cmd.arg("-s").arg(device_id);
    for a in args { cmd.arg(a); }
    cmd.output().map_err(|e| LoaderError::Adb(format!("adb exec error: {e}")))
}

/// Run adb, return Ok(Output) or Err if non-zero exit.
pub fn adb_checked(adb: &str, device_id: &str, args: &[&str]) -> Result<Output> {
    let out = adb_cmd(adb, device_id, args)?;
    if !out.status.success() {
        let detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let detail = if detail.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else { detail };
        return Err(LoaderError::Adb(format!(
            "adb {} failed: {}",
            args.join(" "),
            if detail.is_empty() { "unknown error".into() } else { detail }
        )));
    }
    Ok(out)
}

/// Run `su -c <cmd>` on device.
pub fn su_command(adb: &str, device_id: &str, command: &str, check: bool) -> Result<Output> {
    let shell_cmd = format!("su -c {}", shell_quote(command));
    let out = adb_cmd(adb, device_id, &["shell", &shell_cmd])?;
    if check && !out.status.success() {
        let detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(LoaderError::Adb(format!("su command failed: {detail}")));
    }
    Ok(out)
}

/// Shell-quote a string for Android sh (single-quote wrapping).
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Query the game's PIDs via `pidof <package>`.
pub fn find_game_pids(adb: &str, device_id: &str, package: &str) -> Result<Vec<u32>> {
    let out = adb_cmd(adb, device_id, &["shell", &format!("pidof {package}")])?;
    // pidof returns exit code 1 when process is absent � that's OK
    let stdout = String::from_utf8_lossy(&out.stdout);
    let pids = stdout.split_whitespace()
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    Ok(pids)
}

/// Get the executable path of a PID from /proc.
pub fn process_exe(adb: &str, device_id: &str, pid: u32) -> Option<String> {
    let cmd = format!("readlink /proc/{pid}/exe 2>/dev/null");
    let out = su_command(adb, device_id, &cmd, false).ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim()
        .trim_end_matches(" (deleted)").to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Check if a PID directory exists in /proc.
pub fn pid_exists(adb: &str, device_id: &str, pid: u32) -> Result<bool> {
    let out = su_command(adb, device_id, &format!("test -d /proc/{pid}"), false)?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(LoaderError::Adb(format!("could not check PID {pid}"))),
    }
}

/// Read starttime field from /proc/<pid>/stat for PID identity tracking.
pub fn pid_starttime(adb: &str, device_id: &str, pid: u32) -> Result<Option<String>> {
    if !pid_exists(adb, device_id, pid)? { return Ok(None); }
    let out = su_command(adb, device_id, &format!("cat /proc/{pid}/stat"), false)?;
    if out.status.code() != Some(0) {
        if !pid_exists(adb, device_id, pid)? { return Ok(None); }
        return Err(LoaderError::Adb(format!("could not stat PID {pid}")));
    }
    let stat = String::from_utf8_lossy(&out.stdout);
    let stat = stat.trim();
    // Format: pid (comm) state ... starttime is field 22 (index 21 after split past ')')
    let comm_end = stat.rfind(')').unwrap_or(0);
    let fields: Vec<&str> = stat[comm_end + 2..].split_whitespace().collect();
    if fields.len() < 20 {
        return Err(LoaderError::Adb(format!("invalid /proc stat for PID {pid}")));
    }
    Ok(Some(fields[19].to_string()))
}

/// Resolve the UID of a package from its app data dir stat or pm.
pub fn resolve_package_uid(adb: &str, device_id: &str, package: &str) -> Result<u32> {
    let app_data = format!("/data/user/0/{package}");
    let out = su_command(adb, device_id,
        &format!("stat -c %u {}", shell_quote(&app_data)), false)?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(uid) = s.parse::<u32>() {
            if uid > 0 { return Ok(uid); }
        }
    }
    // Fallback: pm list packages -U
    let out = adb_cmd(adb, device_id,
        &["shell", &format!("pm list packages -U {}", shell_quote(package))])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let re = Regex::new(r"uid:(\d+)").unwrap();
    if let Some(cap) = re.captures(&stdout) {
        if let Ok(uid) = cap[1].parse::<u32>() {
            if uid > 0 { return Ok(uid); }
        }
    }
    Err(LoaderError::Adb(format!("could not resolve package UID for {package}")))
}

/// Get SELinux context of a path.
pub fn remote_context(adb: &str, device_id: &str, path: &str) -> Option<String> {
    let re = Regex::new(r"u:[A-Za-z0-9_]+:[A-Za-z0-9_]+:[A-Za-z0-9_:,.-]+").unwrap();
    for cmd in &[
        format!("stat -c %C {}", shell_quote(path)),
        format!("ls -Zd {}", shell_quote(path)),
    ] {
        if let Ok(out) = su_command(adb, device_id, cmd, false) {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(cap) = re.find(&s) {
                return Some(cap.as_str().to_string());
            }
        }
    }
    None
}

/// Compute SHA-256 of a remote file via `sha256sum`.
pub fn remote_sha256(adb: &str, device_id: &str, path: &str) -> Result<String> {
    let out = su_command(adb, device_id,
        &format!("sha256sum {}", shell_quote(path)), false)?;
    if !out.status.success() {
        return Err(LoaderError::Asset(format!("could not hash remote asset: {path}")));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let digest = stdout.split_whitespace().next().unwrap_or("").to_lowercase();
    let re = Regex::new(r"^[0-9a-f]{64}$").unwrap();
    if !re.is_match(&digest) {
        return Err(LoaderError::Asset(format!("invalid SHA-256 output for: {path}")));
    }
    Ok(digest)
}

/// Resolve symlink target of a remote path.
pub fn remote_realpath(adb: &str, device_id: &str, path: &str) -> String {
    if let Ok(out) = su_command(adb, device_id,
        &format!("readlink -f {}", shell_quote(path)), false) {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() { return s; }
    }
    path.to_string()
}

/// Add an ADB port forward: tcp:<port> -> tcp:<port>
pub fn add_forward(adb: &str, device_id: &str, port: u16) -> Result<()> {
    let local = format!("tcp:{port}");
    // Remove existing forward first (ignore error)
    let _ = adb_cmd(adb, device_id, &["forward", "--remove", &local]);
    adb_checked(adb, device_id, &["forward", &local, &local])?;
    Ok(())
}

/// Remove an ADB port forward.
pub fn remove_forward(adb: &str, device_id: &str, port: u16) {
    let _ = adb_cmd(adb, device_id, &["forward", "--remove", &format!("tcp:{port}")]);
}

/// Find PIDs of a process by name that match the expected exe path.
pub fn matching_server_pids(adb: &str, device_id: &str, server_exe: &str) -> Result<Vec<u32>> {
    let name = server_exe.rsplit('/').next().unwrap_or(server_exe);
    let out = adb_cmd(adb, device_id, &["shell", &format!("pidof {name}")])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let real_exe = remote_realpath(adb, device_id, server_exe);
    let pids: Vec<u32> = stdout.split_whitespace()
        .filter_map(|s| s.parse::<u32>().ok())
        .filter(|&pid| {
            if let Some(exe) = process_exe(adb, device_id, pid) {
                exe == server_exe || exe == real_exe
            } else {
                false
            }
        })
        .collect();
    Ok(pids)
}

/// Kill a set of server PIDs gracefully, then forcefully if needed.
pub fn stop_server_pids(adb: &str, device_id: &str, server_exe: &str, pids: &[u32], force: bool) -> Vec<u32> {
    let real_exe = remote_realpath(adb, device_id, server_exe);
    for &pid in pids {
        if let Some(exe) = process_exe(adb, device_id, pid) {
            if exe == server_exe || exe == real_exe {
                let sig = if force { "kill -9" } else { "kill" };
                let _ = su_command(adb, device_id, &format!("{sig} {pid}"), false);
            }
        }
    }
    // Wait up to 2s for them to die
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut survivors: Vec<u32> = pids.to_vec();
    while !survivors.is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        survivors.retain(|&pid| {
            process_exe(adb, device_id, pid).map(|exe| exe == server_exe || exe == real_exe).unwrap_or(false)
        });
    }
    // Force kill survivors with kill -9
    for &pid in &survivors {
        let _ = su_command(adb, device_id, &format!("kill -9 {pid}"), false);
    }
    std::thread::sleep(Duration::from_millis(300));
    survivors.retain(|&pid| {
        process_exe(adb, device_id, pid).map(|exe| exe == server_exe || exe == real_exe).unwrap_or(false)
    });
    survivors
}

/// Clean up Frida helper residue files in /data/local/tmp
pub fn cleanup_frida_residues(adb: &str, device_id: &str, before: &std::collections::HashSet<String>) {
    let out = su_command(adb, device_id,
        "find /data/local/tmp -mindepth 1 -maxdepth 1 -print 2>/dev/null", false);
    if let Ok(out) = out {
        let re = Regex::new(
            r"^/data/local/tmp/(?:frida-[0-9a-fA-F]{32}|frida-helper-[A-Za-z0-9][A-Za-z0-9._-]*|\.frida-[A-Za-z0-9][A-Za-z0-9._-]*)$"
        ).unwrap();
        let current: std::collections::HashSet<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().trim_end_matches('/').to_string())
            .filter(|p| re.is_match(p))
            .collect();
        for path in current.difference(before) {
            let _ = su_command(adb, device_id, &format!("rm -rf {}", shell_quote(path)), false);
        }
    }
    let _ = su_command(adb, device_id,
        &format!("rm -rf {}", shell_quote(crate::config::INJECTOR_OAT_DIR)), false);
}

/// List current Frida helper residues in /data/local/tmp
pub fn list_frida_residues(adb: &str, device_id: &str) -> std::collections::HashSet<String> {
    let re = Regex::new(
        r"^/data/local/tmp/(?:frida-[0-9a-fA-F]{32}|frida-helper-[A-Za-z0-9][A-Za-z0-9._-]*|\.frida-[A-Za-z0-9][A-Za-z0-9._-]*)$"
    ).unwrap();
    if let Ok(out) = su_command(adb, device_id,
        "find /data/local/tmp -mindepth 1 -maxdepth 1 -print 2>/dev/null", false) {
        return String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().trim_end_matches('/').to_string())
            .filter(|p| re.is_match(p))
            .collect();
    }
    std::collections::HashSet::new()
}
