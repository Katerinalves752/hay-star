use crate::native_engine::{native_cmd, CMD_SPOOF_SCAN, CMD_SPOOF_ON, CMD_SPOOF_OFF};
use crate::adb::adb_cmd;
use crate::error::Result;
use crate::session::Session;
use std::thread;
use std::time::Duration;

pub fn cmd_nspoof(session: &mut Session, args: &[&str]) -> Result<()> {
    let sub = args.first().map(|s| s.to_lowercase()).unwrap_or_else(|| "log".to_string());
    if sub == "log" || sub == "dump" {
        dump_spoof_logcat(session);
        return Ok(());
    }
    let cmd_id = match sub.as_str() {
        "scan" => CMD_SPOOF_SCAN,
        "on"   => CMD_SPOOF_ON,
        "off"  => CMD_SPOOF_OFF,
        _ => { println!("  usage: nspoof scan | on | off  (or bare 'nspoof' to dump hits)"); return Ok(()); }
    };
    let adb = session.adb.clone();
    let dev = session.device_id.clone();
    let _ = crate::adb::su_command(&adb, &dev, "logcat -c", false);
    let n = native_cmd(session, cmd_id, 0, None, 8.0);
    if n.is_none() { println!("  [!] native gate not live (open the farm and retry)."); return Ok(()); }
    let n = n.unwrap();
    match sub.as_str() {
        "on"  => println!("  nspoof on -> {n} (1=installed, 2=already, 0=open import not found)"),
        "off" => println!("  nspoof off -> {n}"),
        _     => {}
    }
    thread::sleep(Duration::from_millis(400));
    dump_spoof_logcat(session);
    Ok(())
}

fn dump_spoof_logcat(session: &Session) {
    let adb = &session.adb;
    let dev = &session.device_id;
    if let Ok(out) = adb_cmd(adb, dev, &["shell", "logcat -d -s MSTAR:V NXRTH:V"]) {
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines()
            .filter(|l| l.contains("spoof")).collect();
        if lines.is_empty() {
            println!("  no 'spoof' logcat yet (run 'nspoof scan'/'nspoof on' first).");
        } else {
            println!("  --- nspoof logcat ({} lines) ---", lines.len());
            for l in lines.iter().take(60) { println!("   {l}"); }
        }
    }
}
