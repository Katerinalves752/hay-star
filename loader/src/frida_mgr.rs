// ============================================================================
//  HAY STAR - Frida session management (Python bridge powered)
// ============================================================================

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::thread;
use serde_json::{json, Value as Json};

use crate::adb::*;
use crate::config::*;
use crate::error::{LoaderError, Result};
use crate::session::Session;

struct BridgeRpcChannel {
    reader: Mutex<BufReader<std::process::ChildStdout>>,
    writer: Mutex<std::process::ChildStdin>,
    call_id: std::sync::atomic::AtomicU64,
}

impl BridgeRpcChannel {
    fn call(&self, method: &str, args: &[Json]) -> Result<Json> {
        let id = self.call_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let req = json!({
            "id": id,
            "method": method,
            "args": args,
        });
        let mut line = req.to_string();
        line.push('\n');

        {
            let mut w = self.writer.lock().map_err(|_| LoaderError::Rpc("writer lock poisoned".into()))?;
            w.write_all(line.as_bytes()).map_err(|e| LoaderError::Rpc(format!("send to bridge failed: {e}")))?;
            w.flush().map_err(|e| LoaderError::Rpc(format!("flush bridge failed: {e}")))?;
        }

        let mut reader = self.reader.lock().map_err(|_| LoaderError::Rpc("reader lock poisoned".into()))?;
        let mut reply_line = String::new();
        reader.read_line(&mut reply_line).map_err(|e| LoaderError::Rpc(format!("recv from bridge failed: {e}")))?;

        if reply_line.trim().is_empty() {
            return Err(LoaderError::Rpc("empty response from frida bridge".into()));
        }

        let reply: Json = serde_json::from_str(reply_line.trim())
            .map_err(|e| LoaderError::Rpc(format!("bad JSON from bridge: {e}, line: {reply_line}")))?;

        if let Some(err) = reply.get("error") {
            return Err(LoaderError::Rpc(err.as_str().unwrap_or(&err.to_string()).to_string()));
        }
        Ok(reply.get("result").cloned().unwrap_or(Json::Null))
    }
}

pub fn run_injection(
    session: &mut Session,
    project_dir: &Path,
) -> Result<()> {
    let adb = session.adb.clone();
    let device_id = session.device_id.clone();

    println!("\n[*] === SETUP ===");
    println!("[1/3] Starting Frida server (port {FRIDA_PORT})...");
    session.helper_residues_before = list_frida_residues(&adb, &device_id);

    // Start frida-server on device using the verified server_exe path
    let server_cmd = format!(
        "nohup {} -D -l 127.0.0.1:{} >{} 2>&1 &",
        shell_quote(&session.server_exe), FRIDA_PORT, shell_quote(SERVER_LOG)
    );
    session.server_start_attempted = true;
    su_command(&adb, &device_id, &server_cmd, false)?;

    // Wait for server PID
    let deadline = Instant::now() + Duration::from_secs_f64(CONNECT_TIMEOUT);
    loop {
        let pids = matching_server_pids(&adb, &device_id, &session.server_exe)?;
        if !pids.is_empty() {
            session.server_pids = pids.into_iter().collect();
            break;
        }
        if Instant::now() > deadline {
            return Err(LoaderError::Frida(format!(
                "injector server failed to start at {}", session.server_exe
            )));
        }
        thread::sleep(Duration::from_millis(200));
    }
    let pid_list: Vec<String> = session.server_pids.iter().map(|p| p.to_string()).collect();
    println!("[+] Frida server PID(s): {}", pid_list.join(", "));

    println!("[2/3] Forwarding ports...");
    add_forward(&adb, &device_id, FRIDA_PORT)?;
    session.forwarded_ports.insert(FRIDA_PORT);
    add_forward(&adb, &device_id, GADGET_PORT)?;
    session.forwarded_ports.insert(GADGET_PORT);

    println!("[3/3] Verifying Frida server is alive...");
    verify_frida_server(&adb, &device_id)?;

    println!("\n[*] === FILE-BACKED GADGET INJECTION ===");
    // Force stop game first
    let _ = adb_cmd(&adb, &device_id, &["shell", &format!("am force-stop {PACKAGE_NAME}")]);
    wait_for_process_gone(&adb, &device_id, PACKAGE_NAME, 5.0)?;

    // Launch python bridge
    println!("[1/8] Starting Frida bridge and spawning game...");
    let bridge_path = project_dir.join("frida_bridge.py");
    let mut child = Command::new("python")
        .args([
            bridge_path.to_str().unwrap_or("frida_bridge.py"),
            "--frida-port", &FRIDA_PORT.to_string(),
            "--gadget-port", &GADGET_PORT.to_string(),
            "--project-dir", project_dir.to_str().unwrap_or("."),
            "--package-name", PACKAGE_NAME,
            "--gadget-bin", &gadget_bin(),
            "--adb", &session.adb,
            "--device-id", &session.device_id,
            "--server-exe", &session.server_exe,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| LoaderError::Frida(format!("cannot launch python frida_bridge: {e}")))?;

    let child_stdout = child.stdout.take()
        .ok_or_else(|| LoaderError::Frida("failed to capture bridge stdout".into()))?;
    let child_stdin = child.stdin.take()
        .ok_or_else(|| LoaderError::Frida("failed to capture bridge stdin".into()))?;

    let mut reader = BufReader::new(child_stdout);
    let mut init_line = String::new();
    reader.read_line(&mut init_line)
        .map_err(|e| LoaderError::Frida(format!("failed to read readiness from bridge: {e}")))?;

    let ready_val: Json = serde_json::from_str(init_line.trim())
        .map_err(|e| LoaderError::Frida(format!("invalid bridge init JSON: {e}, received: {init_line}")))?;

    if ready_val.get("status").and_then(|v| v.as_str()) != Some("ready") {
        let err_msg = ready_val.get("error").and_then(|v| v.as_str()).unwrap_or(&init_line);
        return Err(LoaderError::Frida(format!("bridge initialization failed: {err_msg}")));
    }

    let game_pid = ready_val.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let engine_base = ready_val.get("base").and_then(|v| v.as_str()).unwrap_or("").to_string();

    session.pid = Some(game_pid);
    session.spawn_owned = true;
    session.frida_attached = true;
    session.resumed = true;
    session.resume_barrier_complete = true;
    session.server_pids.clear();
    session.server_start_attempted = false;
    session.forwarded_ports.remove(&FRIDA_PORT);

    println!("[+] Game spawned with PID {game_pid}");
    println!("[+] libg.so detected at {engine_base}");

    // Setup RPC channel
    let bridge_channel = Arc::new(BridgeRpcChannel {
        reader: Mutex::new(reader),
        writer: Mutex::new(child_stdin),
        call_id: std::sync::atomic::AtomicU64::new(1),
    });

    let chan_clone = Arc::clone(&bridge_channel);
    session.rpc_fn = Some(Arc::new(move |method: &str, args: &[Json]| {
        chan_clone.call(method, args)
    }));

    let chan_probe_clone = Arc::clone(&bridge_channel);
    session.probe_rpc = Some(Arc::new(move |method: &str, args: &[Json]| {
        chan_probe_clone.call(&format!("probe_{method}"), args)
    }));

    println!("[+] === INJECTION COMPLETE ===");
    Ok(())
}

fn verify_frida_server(_adb: &str, _device_id: &str) -> Result<()> {
    let addr = format!("127.0.0.1:{FRIDA_PORT}");
    let deadline = Instant::now() + Duration::from_secs_f64(CONNECT_TIMEOUT);
    let mut last_err = String::new();
    while Instant::now() < deadline {
        match TcpStream::connect(&addr) {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_err = e.to_string();
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
    Err(LoaderError::Frida(format!("Frida server not responding on {addr}: {last_err}")))
}

fn wait_for_process_gone(adb: &str, device_id: &str, package: &str, timeout: f64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs_f64(timeout);
    loop {
        let pids = find_game_pids(adb, device_id, package)?;
        if pids.is_empty() { return Ok(()); }
        if Instant::now() > deadline {
            return Err(LoaderError::Frida(format!("process {package} did not stop: {:?}", pids)));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn release_injector(session: &mut Session) -> Result<()> {
    let adb = session.adb.clone();
    let device_id = session.device_id.clone();
    let server_exe = session.server_exe.clone();
    let owned: Vec<u32> = session.server_pids.iter().cloned().collect();
    if !owned.is_empty() {
        stop_server_pids(&adb, &device_id, &server_exe, &owned, false);
    }
    session.server_start_attempted = false;
    remove_forward(&adb, &device_id, FRIDA_PORT);
    session.forwarded_ports.remove(&FRIDA_PORT);
    let _ = su_command(&adb, &device_id, &format!("rm -f {}", shell_quote(SERVER_LOG)), false);
    cleanup_frida_residues(&adb, &device_id, &session.helper_residues_before);
    println!("[+] Injector server released");
    Ok(())
}
