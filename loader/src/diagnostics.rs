// ============================================================================
//  HAY STAR - Diagnostics & Crash Dump Subsystem
//  Automatically captures emulator screenshot, logcat, ANR traces, and
//  tombstones into a timestamped error_logs/ folder for manual/AI debugging.
// ============================================================================

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use serde_json::json;

use crate::adb::su_command;
use crate::session::Session;

/// Snapshot of session information to record in problem reports.
#[derive(Clone, Default)]
pub struct SessionSnapshot {
    pub pid: Option<u32>,
    pub app_uid: u32,
    pub app_context: String,
    pub plant_base: u64,
    pub nat_base: u64,
    pub nat_mbox: u64,
    pub nat_f: u32,
    pub orig_got: Option<u64>,
    pub last_cmd: Option<String>,
}

impl SessionSnapshot {
    pub fn from_session(session: &Session) -> Self {
        SessionSnapshot {
            pid: session.pid,
            app_uid: session.app_uid,
            app_context: session.app_context.clone(),
            plant_base: session.plant_base,
            nat_base: session.nat_base,
            nat_mbox: session.nat_mbox,
            nat_f: session.nat_f,
            orig_got: session.orig_got,
            last_cmd: session.last_cmd.clone(),
        }
    }
}

/// Capture a full crash dump into error_logs/crash_<timestamp>/
pub fn capture_crash_dump(
    adb: &str,
    device_id: &str,
    reason: &str,
    session: Option<&SessionSnapshot>,
) -> PathBuf {
    let now = SystemTime::now();
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();

    // Format folder name: crash_YYYYMMDD_HHMMSS or crash_<timestamp>
    let timestamp_str = format_timestamp(secs);
    let dump_dir = PathBuf::from("error_logs").join(format!("crash_{timestamp_str}"));
    let _ = fs::create_dir_all(&dump_dir);

    println!("\n\x1b[1;31m[!] ========================================================\x1b[0m");
    println!("\x1b[1;31m[!] CRASH / ERROR DETECTED: {reason}\x1b[0m");
    println!("\x1b[1;33m[*] Creating diagnostic dump in: {}\x1b[0m", dump_dir.display());

    // 1. Screenshot of the emulator screen via adb exec-out screencap -p
    let screenshot_path = dump_dir.join("screenshot.png");
    match Command::new(adb)
        .args(["-s", device_id, "exec-out", "screencap", "-p"])
        .output()
    {
        Ok(out) if out.status.success() && !out.stdout.is_empty() => {
            let _ = fs::write(&screenshot_path, &out.stdout);
            println!("  \x1b[32m[+] Screenshot captured:\x1b[0m {}", screenshot_path.display());
        }
        _ => {
            println!("  \x1b[33m[!] Could not capture screenshot via screencap\x1b[0m");
        }
    }

    // 2. Full recent logcat
    let logcat_path = dump_dir.join("logcat.txt");
    match Command::new(adb)
        .args(["-s", device_id, "logcat", "-d"])
        .output()
    {
        Ok(out) => {
            let _ = fs::write(&logcat_path, &out.stdout);
            println!("  \x1b[32m[+] Logcat saved:\x1b[0m {}", logcat_path.display());
        }
        _ => {}
    }

    // 3. ANR traces (if any in /data/anr/traces.txt or traces*.txt)
    let anr_path = dump_dir.join("anr_traces.txt");
    let anr_out = su_command(
        adb,
        device_id,
        "cat /data/anr/traces.txt 2>/dev/null || cat /data/anr/traces*.txt 2>/dev/null",
        false,
    );
    if let Ok(out) = anr_out {
        if !out.stdout.is_empty() {
            let _ = fs::write(&anr_path, &out.stdout);
            println!("  \x1b[32m[+] ANR traces saved:\x1b[0m {}", anr_path.display());
        }
    }

    // 4. Tombstones (if any in /data/tombstones/)
    let tombstones_path = dump_dir.join("tombstones.txt");
    let tomb_out = su_command(
        adb,
        device_id,
        "for f in /data/tombstones/*; do echo \"===== $f =====\"; cat \"$f\"; done 2>/dev/null",
        false,
    );
    if let Ok(out) = tomb_out {
        if !out.stdout.is_empty() {
            let _ = fs::write(&tombstones_path, &out.stdout);
            println!("  \x1b[32m[+] Tombstones saved:\x1b[0m {}", tombstones_path.display());
        }
    }

    // 5. Extract critical logcat lines relating to Hay Day or errors
    let logcat_content = fs::read_to_string(&logcat_path).unwrap_or_default();
    let relevant_lines: Vec<&str> = logcat_content
        .lines()
        .filter(|l| {
            l.contains("hayday")
                || l.contains("ActivityManager")
                || l.contains("FATAL")
                || l.contains("SIGSEGV")
                || l.contains("ANR in")
                || l.contains("MSTAR")
                || l.contains("NXRTH")
        })
        .collect();
    let tail_relevant: Vec<&str> = relevant_lines
        .iter()
        .rev()
        .take(50)
        .rev()
        .cloned()
        .collect();

    // 6. Generate detailed problem_report.md
    let report_path = dump_dir.join("problem_report.md");
    let snap = session.cloned().unwrap_or_default();
    let report_content = format!(
        r#"# Hay Star Crash / Diagnostics Report

- **Timestamp**: {timestamp_str} ({secs})
- **Trigger Reason**: `{reason}`
- **Device ID**: `{device_id}`
- **ADB Path**: `{adb}`

## Process & Memory State
- **PID**: `{pid:?}`
- **App UID**: `{uid}`
- **App Context**: `{ctx}`
- **Last Executed Command**: `{last_cmd:?}`
- **libg.so Base**: `0x{plant_base:x}`
- **libmstar.so Base**: `0x{nat_base:x}`
- **Native Mailbox**: `0x{nat_mbox:x}`
- **Tick Function Offset**: `0x{nat_f:x}`
- **Original GOT Pointer**: `0x{orig_got:x}`

## Captured Artifacts in this Folder
1. [`screenshot.png`](screenshot.png) - Emulator screen state at time of crash.
2. [`logcat.txt`](logcat.txt) - Full system logcat output.
3. [`anr_traces.txt`](anr_traces.txt) - Android ANR traces (if generated).
4. [`tombstones.txt`](tombstones.txt) - Native crash dump / backtrace (if generated).

## Key Relevant Logcat Snippets (Tail 50)
```text
{logcat_tail}
```

## AI / Manual Debugging Guide
1. Check `screenshot.png` to see if the game is showing ANR ("Hay Day isn't responding"), a black screen, or Supercell logo.
2. If ANR occurred, look for lock contentions or modified code pages detected by Promon SHIELD in `anr_traces.txt`.
3. Verify that `TICK_FUNC_OFF` (0xae2430) was properly disarmed and that `TICK_GOT_OFF` (0x15057f0) holds the original pointer.
4. If `SIGSEGV` or crash occurred, check `tombstones.txt` for fault address (pc/lr/sp).
"#,
        timestamp_str = timestamp_str,
        secs = secs,
        reason = reason,
        device_id = device_id,
        adb = adb,
        pid = snap.pid,
        uid = snap.app_uid,
        ctx = snap.app_context,
        last_cmd = snap.last_cmd,
        plant_base = snap.plant_base,
        nat_base = snap.nat_base,
        nat_mbox = snap.nat_mbox,
        nat_f = snap.nat_f,
        orig_got = snap.orig_got.unwrap_or(0),
        logcat_tail = tail_relevant.join("\n")
    );
    let _ = fs::write(&report_path, report_content);

    // 7. Structured JSON metadata for AI inspection
    let json_path = dump_dir.join("state.json");
    let state_json = json!({
        "timestamp": timestamp_str,
        "epoch_secs": secs,
        "reason": reason,
        "device_id": device_id,
        "pid": snap.pid,
        "app_uid": snap.app_uid,
        "app_context": snap.app_context,
        "plant_base": format!("0x{:x}", snap.plant_base),
        "nat_base": format!("0x{:x}", snap.nat_base),
        "nat_mbox": format!("0x{:x}", snap.nat_mbox),
        "last_cmd": snap.last_cmd,
        "orig_got": snap.orig_got.map(|v| format!("0x{:x}", v)),
        "screenshot": screenshot_path.exists(),
        "anr_traces": anr_path.exists(),
        "tombstones": tombstones_path.exists(),
    });
    let _ = fs::write(&json_path, serde_json::to_string_pretty(&state_json).unwrap_or_default());

    println!("  \x1b[32m[+] Report written:\x1b[0m {}", report_path.display());
    println!("\x1b[1;31m[!] ========================================================\x1b[0m\n");

    dump_dir
}

fn format_timestamp(epoch_secs: u64) -> String {
    // Simple breakdown for folder naming without chrono crate
    let days = epoch_secs / 86400;
    let rem = epoch_secs % 86400;
    let hours = rem / 3600;
    let rem2 = rem % 3600;
    let mins = rem2 / 60;
    let secs = rem2 % 60;

    // Approximate year/month/day
    let mut y = 1970u64;
    let mut d = days;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if d < days_in_year { break; }
        d -= days_in_year;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 1u64;
    for &md in &month_days {
        if d < md { break; }
        d -= md;
        m += 1;
    }
    let day = d + 1;

    format!("{y:04}{m:02}{day:02}_{hours:02}{mins:02}{secs:02}")
}

#[cfg(windows)]
pub fn wake_console_input() {
    use std::mem::zeroed;
    #[repr(C)]
    struct CHAR_INFO_UNION {
        unicode_char: u16,
    }
    #[repr(C)]
    struct KEY_EVENT_RECORD {
        b_key_down: i32,
        w_repeat_count: u16,
        w_virtual_key_code: u16,
        w_virtual_scan_code: u16,
        u_char: CHAR_INFO_UNION,
        dw_control_key_state: u32,
    }
    #[repr(C)]
    struct INPUT_RECORD {
        event_type: u16,
        _pad: u16,
        event: KEY_EVENT_RECORD,
    }
    extern "system" {
        fn GetStdHandle(n_std_handle: u32) -> isize;
        fn WriteConsoleInputW(
            h_console_input: isize,
            lp_buffer: *const INPUT_RECORD,
            n_length: u32,
            lp_number_of_events_written: *mut u32,
        ) -> i32;
    }
    unsafe {
        let h = GetStdHandle(0xFFFFFFF6); // STD_INPUT_HANDLE = -10
        let mut rec: INPUT_RECORD = zeroed();
        rec.event_type = 0x0001; // KEY_EVENT
        rec.event.b_key_down = 1;
        rec.event.w_repeat_count = 1;
        rec.event.w_virtual_key_code = 0x0D; // VK_RETURN
        rec.event.u_char.unicode_char = 0x0D;
        let mut written = 0u32;
        WriteConsoleInputW(h, &rec, 1, &mut written);
        rec.event.b_key_down = 0;
        WriteConsoleInputW(h, &rec, 1, &mut written);
    }
}

#[cfg(not(windows))]
pub fn wake_console_input() {}
