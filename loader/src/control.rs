// ============================================================================
//  HAY STAR - TCP Control Server (port 31350)
//  Mirrors MStarConsole.start_control_server() in loader.py.
//  Protocol: one request line -> one reply line ("OK ..." / "ERR ...").
//  This is the seam that the existing Windows GUI talks to.
// ============================================================================

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use crate::session::Session;
use crate::config::CONTROL_PORT;

pub fn start_control_server(session_arc: Arc<Mutex<Session>>) {
    let addr = format!("127.0.0.1:{CONTROL_PORT}");
    let listener = match TcpListener::bind(&addr) {
        Ok(l)  => l,
        Err(e) => {
            eprintln!("[!] Control server NOT started on {addr}: {e}");
            return;
        }
    };
    println!("[+] Control server listening on {addr}");

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(conn) => {
                    let arc = Arc::clone(&session_arc);
                    thread::spawn(move || handle_client(conn, arc));
                }
                Err(e) => eprintln!("[!] control accept error: {e}"),
            }
        }
    });
}

fn handle_client(stream: TcpStream, session_arc: Arc<Mutex<Session>>) {
    let _ = stream.set_nodelay(true);
    let mut writer = match stream.try_clone() {
        Ok(w)  => w,
        Err(e) => { eprintln!("  [!] control clone: {e}"); return; }
    };
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        let line = line.trim().to_string();
        if line.is_empty() { continue; }
        let reply = dispatch_control(&session_arc, &line);
        if writer.write_all((reply + "\n").as_bytes()).is_err() { break; }
    }
}

fn dispatch_control(session_arc: &Arc<Mutex<Session>>, line: &str) -> String {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() { return "ERR empty".to_string(); }
    let cmd = parts[0].to_lowercase();
    let args = &parts[1..];

    match cmd.as_str() {
        "status" => {
            let s = session_arc.lock().unwrap();
            let farm = if s.is_farm_running() { "running" } else { "stopped" };
            let attached = if s.is_attached() { "yes" } else { "no" };
            let gate = if s.nat_cave.is_some() { "ready" } else { "down" };
            format!("OK farm={farm} fields={} attached={attached} gate={gate}",
                s.last_field_count)
        }
        "adb" => {
            let s = session_arc.lock().unwrap();
            let out = crate::adb::adb_cmd(&s.adb, &s.device_id, &["get-state"]);
            match out {
                Ok(o) => {
                    let st = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if st.contains("device") {
                        format!("OK adb {}: {}", s.device_id, st)
                    } else {
                        format!("ERR adb {}: {}", s.device_id, if st.is_empty() { "offline".to_string() } else { st })
                    }
                }
                Err(e) => format!("ERR adb: {e}"),
            }
        }
        "info" => {
            let s = session_arc.lock().unwrap();
            match s.rpc("info", &[]) {
                Ok(v) => format!("OK {v}"),
                Err(e) => format!("ERR {e}"),
            }
        }
        "fields" => {
            let mut s = session_arc.lock().unwrap();
            let ids = crate::native_engine::field_ids(&mut s, false);
            if ids.is_empty() { "ERR no fields".to_string() }
            else { format!("OK {}", ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")) }
        }
        "plant" => {
            let crop: u32 = args.first().and_then(|s| s.parse().ok()).unwrap_or(crate::config::WHEAT_ITEM);
            let mut s = session_arc.lock().unwrap();
            let ids = crate::native_engine::field_ids(&mut s, false);
            let n = crate::native_engine::native_cmd(&mut s, crate::native_engine::CMD_PLANT, crop, Some(&ids), 8.0);
            n.map(|n| format!("OK {n}")).unwrap_or_else(|| "ERR plant failed".to_string())
        }
        "harvest" => {
            let mut s = session_arc.lock().unwrap();
            let ids = crate::native_engine::field_ids(&mut s, false);
            let n = crate::native_engine::native_cmd(&mut s, crate::native_engine::CMD_HARVEST, 0, Some(&ids), 8.0);
            n.map(|n| format!("OK {n}")).unwrap_or_else(|| "ERR harvest failed".to_string())
        }
        "farm" => {
            let sub = args.first().copied().unwrap_or("status");
            let s = session_arc.lock().unwrap();
            match sub {
                "start"  => "OK farm background start not yet implemented via TCP".to_string(),
                "stop"   => { s.stop_farm(); "OK farm stopping".to_string() }
                "status" => format!("OK farm={}", if s.is_farm_running() { "running" } else { "stopped" }),
                _ => format!("ERR unknown farm sub: {sub}"),
            }
        }
        "sell" => {
            if args.is_empty() { return "ERR sell <slot> [count] [price] [ad] [item]".to_string(); }
            let slot  = args[0].parse::<u32>().unwrap_or(0);
            let count = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10u32);
            let price = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1u32);
            let ad    = args.get(3).map(|s| !matches!(*s, "0"|"no"|"false")).unwrap_or(false) as u32;
            let item  = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(crate::config::WHEAT_ITEM);
            let ids = vec![slot, item, count, price, ad];
            let mut s = session_arc.lock().unwrap();
            let n = crate::native_engine::native_cmd(&mut s, crate::native_engine::CMD_SELL, 0, Some(&ids), 8.0);
            n.map(|n| format!("OK {n}")).unwrap_or_else(|| "ERR sell failed".to_string())
        }
        "ping" => "OK pong".to_string(),
        _ => format!("ERR unknown command: {cmd}"),
    }
}
