// ============================================================================
//  HAY STAR - Interactive REPL console (mstar> prompt)
// ============================================================================

use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use rustyline::{Editor, DefaultEditor};
use rustyline::error::ReadlineError;
use crate::commands;
use crate::diagnostics::{self, SessionSnapshot};
use crate::error::Result;
use crate::session::Session;

const HISTORY_FILE: &str = ".mstar_history";
const PROMPT: &str = "\x1b[1;36mmstar\x1b[0m\x1b[1;37m> \x1b[0m";

pub fn console_loop(session_arc: Arc<Mutex<Session>>) -> i32 {
    let mut rl = DefaultEditor::new().expect("readline init");
    let _ = rl.load_history(HISTORY_FILE);

    loop {
        // Check if a background crash monitor requested restart
        {
            let session = session_arc.lock().unwrap();
            if session.should_restart.load(Ordering::Relaxed) {
                let _ = rl.save_history(HISTORY_FILE);
                return 1;
            }
        }

        let readline = rl.readline(PROMPT);
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    // Check if should_restart was set while waiting for input
                    let session = session_arc.lock().unwrap();
                    if session.should_restart.load(Ordering::Relaxed) {
                        let _ = rl.save_history(HISTORY_FILE);
                        return 1;
                    }
                    continue;
                }
                let _ = rl.add_history_entry(trimmed);

                let (disp_res, is_fatal, adb, device_id, snap) = {
                    let mut session = session_arc.lock().unwrap();
                    session.last_cmd = Some(trimmed.to_string());
                    let res = commands::dispatch(&mut session, trimmed);
                    let mut fatal = false;
                    if let Err(ref e) = res {
                        let s = e.to_string().to_lowercase();
                        fatal = s.contains("script is destroyed")
                            || s.contains("session is not attached")
                            || s.contains("empty response from frida bridge")
                            || s.contains("connection closed")
                            || s.contains("broken pipe")
                            || s.contains("process terminated")
                            || s.contains("timed out");
                    }
                    let snap = SessionSnapshot::from_session(&session);
                    let adb = session.adb.clone();
                    let dev = session.device_id.clone();
                    if fatal {
                        session.should_restart.store(true, Ordering::SeqCst);
                    }
                    (res, fatal, adb, dev, snap)
                };

                match disp_res {
                    Ok(true)  => {}
                    Ok(false) => { let _ = rl.save_history(HISTORY_FILE); return 0; }
                    Err(e) => {
                        eprintln!("  \x1b[31mError: {e}\x1b[0m");
                        if is_fatal {
                            diagnostics::capture_crash_dump(
                                &adb,
                                &device_id,
                                &format!("Fatal error during '{trimmed}': {e}"),
                                Some(&snap),
                            );
                            let _ = rl.save_history(HISTORY_FILE);
                            return 1; // Trigger auto-restart
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                let mut session = session_arc.lock().unwrap();
                if session.is_farm_running() {
                    println!("\n  [!] Ctrl+C - stopping farm...");
                    session.stop_farm();
                } else {
                    println!("\n  [!] Ctrl+C");
                    let _ = rl.save_history(HISTORY_FILE);
                    return 130;
                }
            }
            Err(ReadlineError::Eof) => {
                println!("\n  [!] EOF");
                let _ = rl.save_history(HISTORY_FILE);
                return 0;
            }
            Err(e) => {
                eprintln!("  [!] readline error: {e}");
                let _ = rl.save_history(HISTORY_FILE);
                return 1;
            }
        }
    }
}