use std::time::{Duration, Instant};
use std::thread;
use rand::Rng;
use serde_json::Value as Json;
use crate::native_engine::*;
use crate::error::Result;
use crate::session::Session;

pub fn cmd_nfields(session: &mut Session, _args: &[&str]) -> Result<()> {
    let ids = field_ids(session, true);
    println!("  nfields -> {} field(s): {:?}", ids.len(), ids);
    Ok(())
}

pub fn cmd_nplant(session: &mut Session, args: &[&str]) -> Result<()> {
    let crop: u32 = args.first().and_then(|s| s.parse().ok()).unwrap_or(crate::config::WHEAT_ITEM);
    let ids = field_ids(session, false);
    if ids.is_empty() {
        println!("  nplant -> no fields (are you in the farm?)");
        return Ok(());
    }
    let c = native_cmd(session, CMD_PLANT, crop, Some(&ids), 8.0);
    if let Some(n) = c { println!("  nplant -> {n} field(s) (crop {crop})"); }
    Ok(())
}

pub fn cmd_nharvest(session: &mut Session, _args: &[&str]) -> Result<()> {
    let ids = field_ids(session, false);
    if ids.is_empty() {
        println!("  nharvest -> no fields (are you in the farm?)");
        return Ok(());
    }
    let c = native_cmd(session, CMD_HARVEST, 0, Some(&ids), 8.0);
    if let Some(n) = c { println!("  nharvest -> {n} field(s)"); }
    Ok(())
}

pub fn cmd_nsell(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.is_empty() {
        println!("  Usage: nsell <slot> [count=10] [price=1] [ad=0] [item=400001]");
        println!("         open the roadside shop first; slot is the crate index");
        return Ok(());
    }
    let slot:  u32 = args[0].parse().unwrap_or(0);
    let count: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let price: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
    let ad:    u32 = args.get(3).map(|s| !matches!(*s, "0"|"no"|"false"|"n")).unwrap_or(false) as u32;
    let item:  u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(crate::config::WHEAT_ITEM);

    let sell_ids = vec![slot, item, count, price, ad];
    let c = native_cmd(session, CMD_SELL, 0, Some(&sell_ids), 8.0);
    if c.is_some() {
        println!("  nsell -> item {item} x{count} @ {price} coin, slot {slot}, ad={ad}");
    }
    Ok(())
}

pub fn cmd_nfarm(session: &mut Session, args: &[&str]) -> Result<()> {
    let wait: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(130);
    let crop: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(crate::config::WHEAT_ITEM);
    println!("  Auto-farm (native): harvest -> plant every ~{wait}s. Ctrl+C to stop.");

    let mut rng = rand::thread_rng();
    let mut cycle = 0u64;

    loop {
        cycle += 1;
        let ids = field_ids(session, false);
        if ids.is_empty() {
            println!("  [{cycle}] no fields; retrying shortly");
        } else {
            let h = native_cmd(session, CMD_HARVEST, 0, Some(&ids), 8.0).unwrap_or(0);
            thread::sleep(Duration::from_secs_f64(rng.gen_range(0.8..2.2)));
            let ids2 = field_ids(session, false);
            let ids2 = if ids2.is_empty() { ids.clone() } else { ids2 };
            let p = native_cmd(session, CMD_PLANT, crop, Some(&ids2), 8.0).unwrap_or(0);
            println!("  [{cycle}] harvested {h}, planted {p} ({} fields)", ids2.len());
        }
        // Jittered growth wait
        let mut delay = wait as f64 * rng.gen_range(1.02..1.18);
        if cycle % rng.gen_range(4u64..8) == 0 {
            delay += rng.gen_range(20.0..90.0);
        }
        let mut left = delay as u64;
        loop {
            print!("\r  growing... {:4}s ", left);
            use std::io::Write;
            let _ = std::io::stdout().flush();
            let step = left.min(2);
            // Check for Ctrl+C via the farm_stop flag
            if session.farm_stop.load(std::sync::atomic::Ordering::Relaxed) {
                println!("\n  Auto-farm stopped.");
                session.farm_stop.store(false, std::sync::atomic::Ordering::Relaxed);
                return Ok(());
            }
            thread::sleep(Duration::from_secs(step));
            if left <= step { break; }
            left -= step;
        }
        print!("\r                          \r");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
}

pub fn cmd_nfdiag(session: &mut Session, _args: &[&str]) -> Result<()> {
    let adb = session.adb.clone();
    let device_id = session.device_id.clone();
    let _ = crate::adb::su_command(&adb, &device_id, "logcat -c", false);
    native_refresh_ranges(session, 2.5);
    let n = native_cmd(session, CMD_FIELDS_DIAG, 0, None, 8.0);
    if n.is_none() {
        println!("  [!] native gate not live (open the farm and retry).");
        return Ok(());
    }
    let n = n.unwrap();
    let ids: Vec<u32> = native_read_ids(session, n as usize);
    let ids: Vec<u32> = { let mut v = ids; v.sort(); v.dedup(); v };
    println!("  nfdiag: type-4 field sub-manager -> {n} field(s)");
    println!("    ids: {:?}", ids);
    thread::sleep(Duration::from_millis(400));
    // Show MSTAR logcat
    if let Ok(out) = crate::adb::adb_cmd(&adb, &device_id, &["shell", "logcat -d -s MSTAR:V NXRTH:V"]) {
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text
            .lines()
            .filter(|l| l.contains("fdiag"))
            .collect();
        if !lines.is_empty() {
            println!("  --- module logcat (fdiag) ---");
            for l in lines.iter().take(80) { println!("   {l}"); }
        }
    }
    Ok(())
}

pub fn cmd_nediag(session: &mut Session, _args: &[&str]) -> Result<()> {
    println!("  nediag: running native container enumeration breakdown...");
    // Install gate if needed
    if session.nat_cave.is_none() {
        crate::native_engine::install_native_gate(session)?;
    }
    let m = session.nat_mbox;
    // Arm hook
    let cave = session.nat_cave.clone().unwrap_or_default();
    let f = session.nat_f;
    crate::native_engine::arm_hook(session, f, &cave, m);
    let gm = session.read_u64(m + 0x20);
    println!("  gm=0x{gm:x}");
    crate::native_engine::disarm_native_gate(session);
    Ok(())
}
