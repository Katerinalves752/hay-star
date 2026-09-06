// ============================================================================
//  HAY STAR - Session State
//  Central shared state for the loader session.
// ============================================================================

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use serde_json::Value as Json;
use crate::error::{LoaderError, Result};

/// All mutable state for one loader session.
pub struct Session {
    // --- ADB ---
    pub adb: String,
    pub device_id: String,
    pub app_uid: u32,
    pub app_context: String,

    // --- Process tracking ---
    pub pid: Option<u32>,
    pub pid_starttime: Option<String>,

    // --- Frida server ---
    pub server_exe: String,
    pub server_pids: HashSet<u32>,
    pub server_start_attempted: bool,
    pub helper_residues_before: HashSet<String>,

    // --- Port forwards ---
    pub forwarded_ports: HashSet<u16>,

    // --- Frida state (opaque handles managed by frida_mgr) ---
    pub frida_attached: bool,
    pub resumed: bool,
    pub resume_barrier_complete: bool,
    pub spawn_owned: bool,
    pub startup_complete: bool,
    pub closing: bool,
    pub detached_reason: Option<String>,

    // --- RPC function (set by frida_mgr after attach) ---
    pub rpc_fn: Option<Arc<dyn Fn(&str, &[Json]) -> Result<Json> + Send + Sync>>,
    pub probe_rpc: Option<Arc<dyn Fn(&str, &[Json]) -> Result<Json> + Send + Sync>>,

    // --- Native engine ---
    pub nat_base: u64,
    pub nat_mbox: u64,
    pub nat_cave: Option<String>,
    pub nat_f: u32,
    pub nat_stolen: Option<Vec<u8>>,
    pub plant_base: u64,
    pub orig_got: Option<u64>,
    pub flush_cave: Option<String>,

    // --- Legacy cave hooks ---
    pub plant_mbox: Option<String>,
    pub plant_cave: Option<String>,
    pub sell_mbox: Option<String>,
    pub sell_cave: Option<String>,
    pub engine_base: u64,
    pub cmdlog_mbox: Option<String>,

    // --- Value scanner ---
    pub scan_results: Vec<String>,
    pub scan_type: Option<String>,

    // --- Field tracking ---
    pub current_fields: Vec<u32>,
    pub last_field_count: usize,

    // --- Farm control ---
    pub farm_stop: Arc<AtomicBool>,
    pub farm_running: Arc<AtomicBool>,

    // --- Auto restart & Diagnostics ---
    pub last_cmd: Option<String>,
    pub error_logged: Arc<AtomicBool>,
    pub should_restart: Arc<AtomicBool>,

    // --- Command serialization lock (shared with TCP server thread) ---
    // Using a flag rather than Mutex here since the actual Mutex wraps
    // the entire Session in the Arc<Mutex<Session>>.
}

impl Session {
    pub fn new(adb: String, device_id: String) -> Self {
        Session {
            adb,
            device_id,
            app_uid: 0,
            app_context: String::new(),
            pid: None,
            pid_starttime: None,
            server_exe: crate::config::FRIDA_BIN.to_string(),
            server_pids: HashSet::new(),
            server_start_attempted: false,
            helper_residues_before: HashSet::new(),
            forwarded_ports: HashSet::new(),
            frida_attached: false,
            resumed: false,
            resume_barrier_complete: false,
            spawn_owned: false,
            startup_complete: false,
            closing: false,
            detached_reason: None,
            rpc_fn: None,
            probe_rpc: None,
            nat_base: 0,
            nat_mbox: 0,
            nat_cave: None,
            nat_f: 0,
            nat_stolen: None,
            plant_base: 0,
            orig_got: None,
            flush_cave: None,
            plant_mbox: None,
            plant_cave: None,
            sell_mbox: None,
            sell_cave: None,
            engine_base: 0,
            cmdlog_mbox: None,
            scan_results: Vec::new(),
            scan_type: None,
            current_fields: Vec::new(),
            last_field_count: 0,
            farm_stop: Arc::new(AtomicBool::new(false)),
            farm_running: Arc::new(AtomicBool::new(false)),
            last_cmd: None,
            error_logged: Arc::new(AtomicBool::new(false)),
            should_restart: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Invoke an RPC method on the main hook.js script.
    pub fn rpc(&self, method: &str, args: &[Json]) -> Result<Json> {
        match &self.rpc_fn {
            Some(f) => f(method, args),
            None => Err(LoaderError::Rpc("session is not attached".into())),
        }
    }

    /// Invoke an RPC method on the quago probe script.
    pub fn probe_rpc(&self, method: &str, args: &[Json]) -> Result<Json> {
        match &self.probe_rpc {
            Some(f) => f(method, args),
            None => Err(LoaderError::Rpc("quago probe not loaded".into())),
        }
    }

    pub fn is_attached(&self) -> bool {
        self.frida_attached && !self.closing
    }

    pub fn is_farm_running(&self) -> bool {
        self.farm_running.load(Ordering::Relaxed)
    }

    pub fn stop_farm(&self) {
        self.farm_stop.store(true, Ordering::Relaxed);
    }

    /// Read a u32 from the game process via Frida RPC readabs.
    pub fn read_u32(&self, addr: u64) -> u32 {
        let b = self.read_bytes_abs(addr, 4);
        if b.len() >= 4 {
            u32::from_le_bytes(b[..4].try_into().unwrap_or([0; 4]))
        } else {
            0
        }
    }

    /// Read a u64 from the game process via Frida RPC readabs.
    pub fn read_u64(&self, addr: u64) -> u64 {
        let addr_str = format!("0x{addr:x}");
        let raw = self.rpc("readabs", &[Json::String(addr_str), Json::Number(8.into())])
            .ok()
            .and_then(|v| v.as_array().map(|a| a.iter()
                .filter_map(|b| b.as_u64().map(|x| x as u8))
                .collect::<Vec<u8>>()))
            .unwrap_or_default();
        if raw.len() < 8 { return 0; }
        u64::from_le_bytes(raw[..8].try_into().unwrap_or([0;8]))
    }

    /// Read N bytes from an absolute address via Frida RPC.
    pub fn read_bytes_abs(&self, addr: u64, len: usize) -> Vec<u8> {
        let addr_str = format!("0x{addr:x}");
        self.rpc("readabs", &[Json::String(addr_str), Json::Number((len as u64).into())])
            .ok()
            .and_then(|v| v.as_array().map(|a| a.iter()
                .filter_map(|b| b.as_u64().map(|x| x as u8))
                .collect()))
            .unwrap_or_default()
    }

    /// Write bytes to an absolute address via Frida RPC.
    pub fn write_bytes_abs(&self, addr: u64, bytes: &[u8]) -> bool {
        let addr_str = format!("0x{addr:x}");
        let arr: Vec<Json> = bytes.iter().map(|&b| Json::Number((b as u64).into())).collect();
        self.rpc("writeabs", &[Json::String(addr_str), Json::Array(arr)])
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}
