// ============================================================================
//  HAY STAR - Rust Loader
//  Stack: Rust (loader) + C++ (ARM64 .so) + TypeScript (Frida hooks)
// ============================================================================

mod error;
mod config;
mod adb;
mod assets;
mod frida_mgr;
mod session;
mod arm64;
mod elf;
mod native_engine;
mod console;
mod control;
mod commands;
mod diagnostics;

use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::path::PathBuf;
use colored::*;
use crate::adb::*;
use crate::assets::*;
use crate::config::*;
use crate::diagnostics::{SessionSnapshot, capture_crash_dump, wake_console_input};
use crate::error::{LoaderError, Result};
use crate::session::Session;

fn print_banner() {
    let banner = r#"
  _   _             ____  _             
 | | | | __ _ _   _/ ___|| |_ __ _ _ __ 
 | |_| |/ _` | | | \___ \| __/ _` | '__|
 |  _  | (_| | |_| |___) | || (_| | |   
 |_| |_|\__,_|\__, |____/ \__\__,_|_|   
              |___/                     
"#;
    println!("{}", banner.bright_yellow());
    println!("  {} Hay Day Bot Loader  (Rust Edition)", "HAY STAR".bright_cyan().bold());
    println!("  {} Rust + C++ ARM64 engine + TypeScript/Frida hooks", "Stack:".dimmed());
    println!("  {} com.supercell.hayday on LDPlayer 9 (x86_64/Houdini)", "Target:".dimmed());
    println!();
}

struct LoaderRunner {
    project_dir: PathBuf,
}

impl LoaderRunner {
    fn new() -> Self {
        LoaderRunner {
            project_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    fn run(&self) -> i32 {
        // 1. Find ADB
        println!("[*] Looking for ADB...");
        let adb = match find_adb() {
            Some(p) => { println!("[+] ADB: {p}"); p }
            None => {
                eprintln!("[!] ADB not found. Checked:");
                for p in ADB_SEARCH_PATHS { eprintln!("    {p}"); }
                eprintln!("[!] Install LDPlayer 9 or add adb.exe to PATH.");
                return 1;
            }
        };

        // 2. Find emulator
        println!("[*] Looking for Android emulator...");
        let device_id = match find_emulator(&adb) {
            Some(d) => { println!("[+] Device: {d}"); d }
            None => {
                eprintln!("[!] No Android device found. Start LDPlayer 9.");
                return 1;
            }
        };

        // 3. Resolve package UID / app context
        println!("[*] Resolving app context ({PACKAGE_NAME})...");
        let app_uid = match resolve_package_uid(&adb, &device_id, PACKAGE_NAME) {
            Ok(uid) => { println!("[+] App UID: {uid}"); uid }
            Err(e) => {
                eprintln!("[!] Could not resolve app UID: {e}");
                eprintln!("[!] Is {PACKAGE_NAME} installed?");
                return 1;
            }
        };
        let app_context = crate::adb::remote_context(&adb, &device_id,
            &format!("/data/user/0/{PACKAGE_NAME}"))
            .unwrap_or_else(|| format!("u:object_r:app_data_file:s0:c{app_uid},c512,c768,c1023"));
        println!("[+] App context: {app_context}");

        // 4. Build session
        let mut session = Session::new(adb.clone(), device_id.clone());
        session.app_uid = app_uid;
        session.app_context = app_context.clone();

        // 5. Prepare assets (verify remote SHA-256, gadget config, hook.js)
        println!();
        let gadget_cfg_path = self.project_dir.join("gadget.config.json");
        let mut server_exe = FRIDA_BIN.to_string();
        match prepare_assets(&adb, &device_id, &self.project_dir, &gadget_cfg_path, &mut server_exe) {
            Ok(()) => println!("[+] Assets verified"),
            Err(e) => { eprintln!("[!] Asset error: {e}"); return 1; }
        }
        session.server_exe = server_exe;

        // 6. Kill any existing Frida server
        println!("[*] Checking for stale injector servers...");
        let existing = matching_server_pids(&adb, &device_id, &session.server_exe)
            .unwrap_or_default();
        if !existing.is_empty() {
            println!("[*] Stopping {} stale server PID(s)...", existing.len());
            stop_server_pids(&adb, &device_id, &session.server_exe, &existing, true);
        }

        // 7. Stage gadget
        println!("[*] Staging runtime gadget...");
        match stage_runtime_gadget(&adb, &device_id, &gadget_cfg_path, app_uid, &app_context) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("[!] Gadget staging failed: {e}");
                cleanup_session(&mut session);
                return 1;
            }
        }

        // 8. Inject Frida (8-step sequence)
        match frida_mgr::run_injection(&mut session, &self.project_dir) {
            Ok(()) => { session.startup_complete = true; }
            Err(e) => {
                eprintln!("[!] Injection failed: {e}");
                cleanup_session(&mut session);
                return 1;
            }
        }

        println!();
        println!("  {}", "Type 'help' for all commands. 'loadnative' to load the ARM64 engine.".dimmed());
        println!();

        // 9. Start TCP control server (GUI interface)
        let session_arc = Arc::new(Mutex::new(session));
        control::start_control_server(Arc::clone(&session_arc));

        // 10. Background health watcher (detects crash / ANR / unexpected exit)
        let health_session = Arc::clone(&session_arc);
        let _health_thread = std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1500));
                let (adb, device_id, pid, startup_complete, closing, should_restart, snap) = {
                    let s = match health_session.lock() {
                        Ok(guard) => guard,
                        Err(_) => break,
                    };
                    (
                        s.adb.clone(),
                        s.device_id.clone(),
                        s.pid,
                        s.startup_complete,
                        s.closing,
                        s.should_restart.clone(),
                        SessionSnapshot::from_session(&s),
                    )
                };

                if closing { break; }
                if !startup_complete { continue; }
                if should_restart.load(Ordering::Relaxed) { break; }

                if let Some(p) = pid {
                    // Check if PID is alive via kill -0
                    let check = crate::adb::su_command(
                        &adb,
                        &device_id,
                        &format!("kill -0 {p} 2>/dev/null && echo ALIVE || echo DEAD"),
                        false,
                    );
                    let is_dead = match check {
                        Ok(out) => String::from_utf8_lossy(&out.stdout).contains("DEAD"),
                        Err(_) => false,
                    };

                    if is_dead {
                        let already = health_session.lock().unwrap().error_logged.swap(true, Ordering::SeqCst);
                        if !already {
                            should_restart.store(true, Ordering::SeqCst);
                            println!("\n\x1b[1;31m[!] Game process (PID {p}) terminated unexpectedly!\x1b[0m");
                            capture_crash_dump(
                                &adb,
                                &device_id,
                                &format!("Game process (PID {p}) disappeared from /proc"),
                                Some(&snap),
                            );
                            wake_console_input();
                        }
                        break;
                    }
                }
            }
        });

        // 11. Console REPL (blocks until quit/error)
        let exit_code = console::console_loop(Arc::clone(&session_arc));
        let mut s = session_arc.lock().unwrap();
        s.closing = true;
        s.stop_farm();

        // Cleanup
        cleanup_session(&mut s);
        exit_code
    }
}

fn cleanup_session(session: &mut Session) {
    println!("[*] Cleaning up...");
    // Always force-stop any lingering game instance
    let _ = adb_cmd(&session.adb, &session.device_id, &["shell", &format!("am force-stop {PACKAGE_NAME}")]);
    remove_forward(&session.adb, &session.device_id, GADGET_PORT);
    session.forwarded_ports.remove(&GADGET_PORT);
    remove_forward(&session.adb, &session.device_id, FRIDA_PORT);
    session.forwarded_ports.remove(&FRIDA_PORT);
    if !session.server_exe.is_empty() {
        let pids = matching_server_pids(&session.adb, &session.device_id, &session.server_exe)
            .unwrap_or_default();
        if !pids.is_empty() {
            stop_server_pids(&session.adb, &session.device_id, &session.server_exe, &pids, true);
        }
    }
    cleanup_frida_residues(&session.adb, &session.device_id, &session.helper_residues_before);
    println!("[*] Done.");
}

fn main() {
    print_banner();
    let runner = LoaderRunner::new();
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        if attempt > 1 {
            println!("\n[*] ===== Auto-retry attempt {attempt} (Ctrl+C to stop) =====");
        }
        let code = runner.run();
        match code {
            0 | 130 => std::process::exit(code),
            _ => {
                println!("[*] Session ended (code {code}). Auto-restarting in 3s... (Ctrl+C to stop)");
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        }
    }
}