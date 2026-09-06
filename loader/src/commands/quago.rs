use serde_json::Value as Json;
use crate::error::Result;
use crate::session::Session;
use crate::adb::adb_cmd;

pub fn cmd_nquago(session: &mut Session, args: &[&str]) -> Result<()> {
    let sub = args.first().map(|s| s.to_lowercase()).unwrap_or_else(|| "log".to_string());
    match sub.as_str() {
        "status" => {
            let st = session.probe_rpc("status", &[])?;
            println!("  quago status: {st}");
        }
        "block" => {
            let on = args.get(1).map(|s| matches!(*s, "on"|"1"|"true"|"yes")).unwrap_or(false);
            let r = session.probe_rpc("set_block", &[Json::Bool(on)])?;
            println!("  Quago upload block -> {r}");
        }
        "spoof" => {
            let on = args.get(1).map(|s| matches!(*s, "on"|"1"|"true"|"yes")).unwrap_or(false);
            let r = session.probe_rpc("set_spoof", &[Json::Bool(on)])?;
            println!("  accel emulation -> {r}");
        }
        _ => {
            // Show logcat
            let adb = session.adb.clone();
            let dev = session.device_id.clone();
            if let Ok(out) = adb_cmd(&adb, &dev, &["shell", "logcat -d -s QUAGOPROBE:V"]) {
                let text = String::from_utf8_lossy(&out.stdout);
                let lines: Vec<&str> = text.lines()
                    .filter(|l| l.contains("QUAGOPROBE")).collect();
                if lines.is_empty() {
                    println!("  no QUAGOPROBE logcat. Launch with $env:NX_QUAGO='1'.");
                } else {
                    println!("  --- quago module ({} lines) ---", lines.len());
                    for l in lines.iter().take(150) { println!("   {l}"); }
                }
            }
        }
    }
    Ok(())
}

pub fn cmd_nstate(session: &mut Session, _args: &[&str]) -> Result<()> {
    let st = session.probe_rpc("state", &[])?;
    if let Some(obj) = st.as_object() {
        println!("  screen/segment: {}", obj.get("segment").map(|v| v.to_string()).unwrap_or_else(|| "(none)".into()));
        for (k, v) in obj.iter() {
            if k != "segment" && k != "_updated" {
                println!("    {k} = {v}");
            }
        }
    } else {
        println!("  {st}");
    }
    Ok(())
}
