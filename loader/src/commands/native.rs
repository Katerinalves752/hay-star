use serde_json::Value as Json;
use std::thread;
use std::time::Duration;
use crate::native_engine::*;
use crate::adb::{adb_cmd, su_command};
use crate::assets::stage_native_module;
use crate::error::Result;
use crate::session::Session;

pub fn cmd_loadnative(session: &mut Session, args: &[&str]) -> Result<()> {
    let force = args.iter().any(|a| a.to_lowercase() == "force");
    if !force {
        let base = native_module_base(session);
        if base != 0 && ensure_native_mbox(session) {
            println!("  native module already loaded at 0x{base:x} (mailbox 0x{:x}).", session.nat_mbox);
            println!("  Restart game + loader to pick up a NEW build ('loadnative force' to stage anyway).");
            return Ok(());
        }
    }

    let adb = session.adb.clone();
    let device_id = session.device_id.clone();

    // Print SELinux mode
    if let Ok(out) = su_command(&adb, &device_id, "getenforce", false) {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() { println!("  SELinux: {s}"); }
    }

    // Print bridge info
    match session.rpc("bridgeinfo", &[]) {
        Ok(info) => {
            println!("  bridge: mods={} ext={} capturedNs={}",
                info["modules"], info["hasExt"], info["capturedNs"]);
            if let Some(from) = info["capturedFrom"].as_str() {
                println!("  captured ns from: {from}");
            }
        }
        Err(e) => println!("  (bridgeinfo unavailable: {e})"),
    }

    // Stage libmstar.so
    let project_dir = std::env::current_dir().unwrap_or_default();
    let module_local = project_dir.join("native").join("build").join("libmstar.so");
    stage_native_module(&adb, &device_id, &module_local, session.app_uid, &session.app_context)?;

    // Clear logcat and load module via RPC
    let _ = su_command(&adb, &device_id, "logcat -c", false);
    let mr = crate::config::module_remote();
    let res = session.rpc("loadmodule", &[Json::String(mr)])?;
    println!("  loadmodule -> {res}");

    thread::sleep(Duration::from_millis(700));

    // Check logcat for MODULE LIVE
    if let Ok(out) = adb_cmd(&adb, &device_id, &["shell", "logcat -d -s MSTAR:V NXRTH:V"]) {
        let lines: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines().filter(|l| l.contains("MSTAR") || l.contains("NXRTH")).map(str::to_string).collect();
        if !lines.is_empty() {
            println!("  --- module logcat (MSTAR) ---");
            for l in lines.iter().take(12) { println!("   {l}"); }
            if lines.iter().any(|l| l.contains("nx_init: OK")) {
                println!("  >>> MODULE LIVE: native engine resident in-process.");
            }
        } else {
            println!("  (no MSTAR logcat - load may have failed)");
        }
    }
    Ok(())
}

pub fn cmd_nping(session: &mut Session, _args: &[&str]) -> Result<()> {
    let c = native_cmd(session, CMD_PING, 0, None, 5.0);
    if let Some(n) = c {
        let ok = if n == 0xABCD { "OK" } else { "MISMATCH" };
        println!("  nping -> count=0x{n:x} (expect 0xabcd) {ok}");
    }
    Ok(())
}

pub fn cmd_ndiag(session: &mut Session, _args: &[&str]) -> Result<()> {
    let n = native_cmd(session, CMD_DIAG, 0, None, 8.0);
    let n = match n { Some(n) => n, None => return Ok(()) };
    let ids = native_read_ids(session, (n * 4).min(128) as usize);
    println!("  ndiag: {n} field-holder / manager object(s) within 2 hops of gm/level");
    for k in 0..n.min(30) as usize {
        if 4 * k + 3 >= ids.len() { break; }
        let (key, o2, vt, fc) = (ids[4*k], ids[4*k+1], ids[4*k+2], ids[4*k+3]);
        let src = if (key >> 28) == 0 { "gm" } else { "level" };
        let o1 = key & 0x0FFF_FFFF;
        let path = if o2 == 0xFFFF_FFFF { format!("{src}+0x{o1:x}") }
                   else { format!("{src}+0x{o1:x}+0x{o2:x}") };
        let tag = if vt == 0x14ba5b0 { " [MGR]" }
                  else if vt == crate::config::FIELD_VTABLE_OFF as u32 { " [FIELD]" }
                  else { "" };
        let fc_tag = if fc > 0 { format!("  <<< HOLDS {fc} FIELDS") } else { String::new() };
        println!("   {path:<22} -> vt+0x{vt:07x} fc={fc}{tag}{fc_tag}");
    }
    Ok(())
}
