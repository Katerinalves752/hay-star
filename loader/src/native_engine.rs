// ============================================================================
//  HAY STAR - Native engine management (libmstar.so)
//  Mirrors _native_module_base, _install_native_gate, _arm_hook,
//  _disarm_native_gate, _native_cmd, _field_ids, etc. from loader.py
// ============================================================================

use std::time::{Duration, Instant};
use std::thread;
use serde_json::{json, Value as Json};

use crate::adb::{adb_cmd, su_command, shell_quote};
use crate::arm64::*;
use crate::config::*;
use crate::elf::parse_dynsyms;
use crate::error::{LoaderError, Result};
use crate::session::Session;

// Native command IDs (must match native/src/main.cpp CMD_* enums)
pub const CMD_PING:        u32 = 1;
pub const CMD_DIAG:        u32 = 2;
pub const CMD_FIELDS:      u32 = 3;
pub const CMD_PLANT:       u32 = 4;
pub const CMD_HARVEST:     u32 = 5;
pub const CMD_SELL:        u32 = 6;
pub const CMD_FIELDS_DIAG: u32 = 7;
pub const CMD_SPOOF_SCAN:  u32 = 8;
pub const CMD_SPOOF_ON:    u32 = 9;
pub const CMD_SPOOF_OFF:   u32 = 10;

/// Find the base address of libmstar.so in /proc/<pid>/maps.
pub fn native_module_base(session: &Session) -> u64 {
    let pid = match session.pid { Some(p) => p, None => return 0 };
    let adb = &session.adb;
    let device_id = &session.device_id;
    let out = su_command(adb, device_id, &format!("cat /proc/{pid}/maps"), false)
        .unwrap_or_else(|_| std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: vec![],
            stderr: vec![],
        });
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if line.contains("libmstar.so") || line.contains("libmstar.so") {
            if let Some(range) = line.split('-').next() {
                return u64::from_str_radix(range.trim(), 16).unwrap_or(0);
            }
        }
    }
    0
}

/// Resolve the mailbox address from the loaded module (no gate needed).
pub fn ensure_native_mbox(session: &mut Session) -> bool {
    if session.nat_mbox != 0 { return true; }
    let base = native_module_base(session);
    if base == 0 {
        println!("  [!] native module not loaded (run 'loadnative' first).");
        return false;
    }
    let project_dir = std::env::current_dir().unwrap_or_default();
    let module_local = project_dir.join("native").join("build").join("libmstar.so");
    let data = match std::fs::read(&module_local) {
        Ok(d) => d,
        Err(_) => { println!("  [!] cannot read {}", module_local.display()); return false; }
    };
    let syms = parse_dynsyms(&data);
    match syms.get("mstar_mailbox").or_else(|| syms.get("nx_mailbox")) {
        Some(&rva) => {
            session.nat_base = base;
            session.nat_mbox = base + rva;
            true
        }
        None => {
            println!("  [!] module missing mstar_mailbox/nx_mailbox export.");
            false
        }
    }
}

/// Install the ARM64 bridge cave and farjump on the game tick function.
pub fn install_native_gate(session: &mut Session) -> Result<()> {
    let base = native_module_base(session);
    if base == 0 {
        return Err(LoaderError::NativeEngine("native module not loaded (run 'loadnative' first)".into()));
    }

    let project_dir = std::env::current_dir().unwrap_or_default();
    let module_local = project_dir.join("native").join("build").join("libmstar.so");
    let data = std::fs::read(&module_local)
        .map_err(|e| LoaderError::NativeEngine(format!("cannot read module: {e}")))?;
    let syms = parse_dynsyms(&data);

    let mbox_rva = syms.get("mstar_mailbox").or_else(|| syms.get("nx_mailbox"))
        .copied()
        .ok_or_else(|| LoaderError::NativeEngine("module missing mstar_mailbox/nx_mailbox export".into()))?;
    let on_tick_rva = syms.get("mstar_on_tick").or_else(|| syms.get("nx_on_tick"))
        .copied()
        .ok_or_else(|| LoaderError::NativeEngine("module missing mstar_on_tick/nx_on_tick export".into()))?;

    let libg_base = {
        let info = session.rpc("info", &[])?;
        let b = info.get("base").and_then(|v| v.as_str()).unwrap_or("0");
        u64::from_str_radix(b.trim_start_matches("0x"), 16)
            .map_err(|_| LoaderError::NativeEngine(format!("invalid libg.so base: {b}")))?
    };

    let fabs = libg_base + TICK_FUNC_OFF as u64;
    let on_tick_abs = base + on_tick_rva;
    let mbox_abs = base + mbox_rva;

    session.plant_base = libg_base;
    session.nat_base   = base;
    session.nat_mbox   = mbox_abs;
    session.nat_f      = TICK_FUNC_OFF;

    // Prefer the clean prologue captured by the module at load time
    let stolen_raw = session.read_bytes_abs(mbox_abs + 0x538, 16);
    let stolen: [u8; 16] = if stolen_raw.len() >= 16 && stolen_is_pi(&stolen_raw) {
        stolen_raw[..16].try_into().unwrap()
    } else {
        read_stolen(session, TICK_FUNC_OFF)?
    };

    // Allocate cave and write bridge code
    let cave = session.rpc("alloccave", &[Json::Number(256.into())])?;
    let cave_str = cave.as_str()
        .ok_or_else(|| LoaderError::NativeEngine("alloccave returned non-string".into()))?
        .to_string();

    let cave_bytes = build_bridge_cave(on_tick_abs, &stolen, fabs);
    let cave_arr: Vec<Json> = cave_bytes.iter().map(|&b| Json::Number((b as u64).into())).collect();
    session.rpc("writeabs", &[Json::String(cave_str.clone()), Json::Array(cave_arr)])?;

    session.nat_cave   = Some(cave_str.clone());
    session.nat_stolen = Some(stolen.to_vec());

    println!("  native gate: module@0x{base:x} mailbox@0x{mbox_abs:x} on_tick@0x{on_tick_abs:x}");
    Ok(())
}

/// Read the stolen 16-byte prologue from the tick function directly.
fn read_stolen(session: &Session, func_off: u32) -> Result<[u8; 16]> {
    // Wait for Promon's revert if needed
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let raw = session.read_bytes_abs(
            session.plant_base + func_off as u64, 16
        );
        if raw.len() >= 16 && stolen_is_pi(&raw) {
            return Ok(raw[..16].try_into().unwrap());
        }
        if Instant::now() > deadline {
            return Err(LoaderError::NativeEngine("tick prologue not relocatable".into()));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Arm the hook: install farjump from func_off -> cave_addr, flush cache via GOT.
/// Restores original GOT entry as soon as heartbeat increment confirms hook is live.
pub fn arm_hook(session: &mut Session, func_off: u32, cave_addr: &str, mbox_addr: u64) -> bool {
    let base = session.plant_base;
    let fabs = base + func_off as u64;
    let got_abs = base + TICK_GOT_OFF as u64;

    // Read and securely preserve original GOT value once
    if session.orig_got.is_none() {
        let orig_bytes = session.read_bytes_abs(got_abs, 8);
        if orig_bytes.len() >= 8 {
            let val = u64::from_le_bytes(orig_bytes[..8].try_into().unwrap());
            if val != 0 {
                session.orig_got = Some(val);
            }
        }
    }

    let orig_i = match session.orig_got {
        Some(v) => v,
        None => return false,
    };

    let hb_addr = mbox_addr + 0x18;
    let hb0 = session.read_u32(hb_addr);

    // Ensure flush cave is allocated and written
    let fcave_str = match &session.flush_cave {
        Some(cs) => cs.clone(),
        None => {
            let flush_bytes = build_flush_cave(fabs, orig_i);
            match session.rpc("alloccave", &[Json::Number(64.into())]) {
                Ok(v) => {
                    let s = v.as_str().unwrap_or("").to_string();
                    if !s.is_empty() {
                        let farr: Vec<Json> = flush_bytes.iter().map(|&b| Json::Number((b as u64).into())).collect();
                        let _ = session.rpc("writeabs", &[Json::String(s.clone()), Json::Array(farr)]);
                        session.flush_cave = Some(s.clone());
                        s
                    } else {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        }
    };

    let fcave_i = u64::from_str_radix(fcave_str.trim_start_matches("0x"), 16).unwrap_or(0);
    if fcave_i == 0 { return false; }

    // Write farjump at func_off -> cave
    let _ = session.rpc("farjump", &[
        Json::String(func_off.to_string()),
        Json::String(cave_addr.to_string()),
    ]);

    // Redirect GOT -> flush cave
    session.write_bytes_abs(got_abs, &fcave_i.to_le_bytes());

    // Wait for heartbeat bump to confirm hook is running
    let mut live = false;
    for _ in 0..20 {
        thread::sleep(Duration::from_millis(100));
        let hb = session.read_u32(hb_addr);
        if hb > hb0 {
            live = true;
            break;
        }
    }

    // CRITICAL: Always restore original GOT pointer immediately!
    session.write_bytes_abs(got_abs, &orig_i.to_le_bytes());

    live
}

/// Disarm: restore tick prologue + flush GOT slot back to original.
pub fn disarm_native_gate(session: &mut Session) {
    let stolen = match &session.nat_stolen { Some(s) => s.clone(), None => return };
    if session.nat_f == 0 || session.plant_base == 0 { return; }
    let base = session.plant_base;
    let fabs = base + session.nat_f as u64;
    let got_abs = base + TICK_GOT_OFF as u64;

    let orig_i = match session.orig_got {
        Some(v) => v,
        None => {
            let b = session.read_bytes_abs(got_abs, 8);
            if b.len() >= 8 { u64::from_le_bytes(b[..8].try_into().unwrap()) } else { return; }
        }
    };

    // 1. Restore clean 16-byte prologue on the tick function
    let _ = session.rpc("writeabs", &[
        Json::String(format!("0x{fabs:x}")),
        Json::Array(stolen.iter().map(|&b| Json::Number((b as u64).into())).collect()),
    ]);

    // 2. Ensure flush cave is available
    let fcave_str = match &session.flush_cave {
        Some(cs) => cs.clone(),
        None => {
            let flush_bytes = build_flush_cave(fabs, orig_i);
            match session.rpc("alloccave", &[Json::Number(64.into())]) {
                Ok(v) => {
                    let s = v.as_str().unwrap_or("").to_string();
                    if !s.is_empty() {
                        let farr: Vec<Json> = flush_bytes.iter().map(|&b| Json::Number((b as u64).into())).collect();
                        let _ = session.rpc("writeabs", &[Json::String(s.clone()), Json::Array(farr)]);
                        session.flush_cave = Some(s.clone());
                        s
                    } else {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    };

    let fcave_i = u64::from_str_radix(fcave_str.trim_start_matches("0x"), 16).unwrap_or(0);
    if fcave_i != 0 {
        // Redirect GOT -> flush cave to invalidate instruction cache on fabs
        session.write_bytes_abs(got_abs, &fcave_i.to_le_bytes());
        // Allow the hooked function to flush fabs once
        thread::sleep(Duration::from_millis(200));
        // Restore original GOT pointer
        session.write_bytes_abs(got_abs, &orig_i.to_le_bytes());
    }
}

/// Send a command to the native engine via the mailbox.
/// Arms gate, writes mailbox, polls until cleared, disarms.
pub fn native_cmd(
    session: &mut Session,
    cmd_id: u32,
    arg0: u32,
    ids: Option<&[u32]>,
    timeout_secs: f64,
) -> Option<u32> {
    if session.nat_cave.is_none() {
        if let Err(e) = install_native_gate(session) {
            println!("  [!] gate install failed: {e}");
            return None;
        }
    }

    let m = session.nat_mbox;
    let cave = session.nat_cave.clone()?;

    // Write field IDs into mailbox.ids[] and arg1 (count)
    if let Some(ids) = ids {
        let ids = &ids[..ids.len().min(128)];
        let id_bytes: Vec<u8> = ids.iter()
            .flat_map(|&id| id.to_le_bytes())
            .collect();
        session.write_bytes_abs(m + 0x28, &id_bytes);
        session.write_bytes_abs(m + 0x0c, &(ids.len() as u32).to_le_bytes());
    }

    // Write arg0 and command
    session.write_bytes_abs(m + 0x08, &arg0.to_le_bytes());
    session.write_bytes_abs(m + 0x04, &cmd_id.to_le_bytes());

    // Arm gate, poll for completion, disarm
    let f = session.nat_f;
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_secs);
    let mut result: Option<u32> = None;

    while result.is_none() && Instant::now() < deadline {
        if !arm_hook(session, f, &cave, m) {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        for _ in 0..8 {
            thread::sleep(Duration::from_millis(100));
            let c = session.read_u32(m + 0x04);
            if c == 0 {
                result = Some(session.read_u32(m + 0x14));
                break;
            }
        }
    }

    if result.is_none() {
        // Clear stale command
        session.write_bytes_abs(m + 0x04, &0u32.to_le_bytes());
        println!("  native cmd timed out.");
    }

    disarm_native_gate(session);
    result
}

/// Read field IDs from the mailbox after a CMD_FIELDS call.
pub fn native_read_ids(session: &Session, count: usize) -> Vec<u32> {
    let count = count.min(128);
    let raw = session.read_bytes_abs(session.nat_mbox + 0x28, count * 4);
    raw.chunks(4)
        .filter_map(|c| c.try_into().ok().map(u32::from_le_bytes))
        .collect()
}

/// Request a range refresh from the native module.
pub fn native_refresh_ranges(session: &mut Session, timeout_secs: f64) -> bool {
    if !ensure_native_mbox(session) { return false; }
    let m = session.nat_mbox;
    let rd = |o: u64| {
        let b = session.read_bytes_abs(m + o, 4);
        if b.len() >= 4 { u32::from_le_bytes(b[..4].try_into().unwrap()) } else { 0 }
    };
    let gen0 = rd(0x22c);
    let new_req = rd(0x548).wrapping_add(1);
    session.write_bytes_abs(m + 0x548, &new_req.to_le_bytes());
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_secs);
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
        if rd(0x22c) != gen0 { return true; }
    }
    false
}

/// Full field enumeration with sanity check and fallback.
pub fn field_ids(session: &mut Session, verbose: bool) -> Vec<u32> {
    native_refresh_ranges(session, 2.5);
    let n = match native_cmd(session, CMD_FIELDS, 0, None, 8.0) {
        Some(n) => n,
        None => {
            if verbose { println!("  [!] native gate not live (open the farm and retry)."); }
            return vec![];
        }
    };
    let ids: Vec<u32> = {
        let mut v = native_read_ids(session, n as usize);
        v.sort();
        v.dedup();
        v
    };

    // Sanity gate: field count should not jump drastically
    let prev = session.last_field_count;
    if prev > 0 && !ids.is_empty() {
        let lo = (prev as f64 * 0.5) as usize;
        let hi = (prev as f64 * 1.5) as usize;
        if ids.len() < lo || ids.len() > hi {
            println!("  [!] enumeration returned {} fields but farm had {}; ignoring.", ids.len(), prev);
            return vec![];
        }
    }
    if !ids.is_empty() { session.last_field_count = ids.len(); }
    if verbose {
        if ids.is_empty() {
            println!("  (none found - in the farm?)");
        } else {
            println!("  {} field(s) [{:?}..{:?}]", ids.len(), ids.first(), ids.last());
        }
    }
    ids
}
