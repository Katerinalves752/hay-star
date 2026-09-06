// ============================================================================
//  HAY STAR - Command dispatch table
// ============================================================================

pub mod farm;
pub mod scan;
pub mod info;
pub mod patch;
pub mod hooks;
pub mod native;
pub mod quago;
pub mod spoof;
pub mod misc;

use crate::error::Result;
use crate::session::Session;

/// All recognized commands and their short help strings.
pub const COMMANDS: &[(&str, &str)] = &[
    // Native engine commands
    ("loadnative",  "Load the native ARM64 engine module into the game"),
    ("nfields",     "List current field IDs (must be on farm screen)"),
    ("nplant",      "nplant [cropId=400001]  � plant crop on all fields"),
    ("nharvest",    "Harvest all ready fields"),
    ("nsell",       "nsell <slot> [count] [price] [ad] [item]  � list item in roadside shop"),
    ("nfarm",       "nfarm [wait=130] [cropId]  � auto-farm loop (Ctrl+C to stop)"),
    ("nfdiag",      "Validate native container field enumeration"),
    ("nediag",      "Enumeration breakdown diagnostic"),
    ("nping",       "Ping the native engine (expect 0xABCD)"),
    ("ndiag",       "Show field-holder objects within 2 hops of gameMode"),
    ("nquago",      "nquago [status|block on|off|spoof on|off]  � Quago probe control"),
    ("nstate",      "Live game state from Quago telemetry"),
    ("nspoof",      "nspoof [scan|on|off]  � device fingerprint spoof (Galaxy S24 Ultra)"),
    // Memory
    ("info",        "Show libg.so base/size/arch/pid"),
    ("read",        "read <int|float|double|long|ptr|str|bytes> <offset> [len]"),
    ("write",       "write <int|float|double|long|bytes> <offset> <value>"),
    ("dump",        "dump <offset> <length>  � hex dump at libg.so offset"),
    ("rabs",        "rabs <abs_addr> <len>  � hex dump absolute address"),
    ("wabs",        "wabs <abs_addr> <hexbytes>  � write bytes to absolute address"),
    ("export",      "export <name>  � look up libg.so export"),
    // Scanner
    ("scan",        "scan <pattern>  � AOB scan in libg.so"),
    ("vscan",       "vscan <type> <value>  � scan writable memory for typed value"),
    ("vnarrow",     "vnarrow <type> <value>  � narrow scan results"),
    ("vlist",       "List current scan results with values"),
    ("vwrite",      "vwrite <value> [index]  � write to scan results"),
    ("vreset",      "Clear scan results"),
    // Patching
    ("nop",         "nop <offset> <count>  � NOP patch at libg.so offset"),
    ("farjump",     "farjump <offset> <target>  � 16-byte ARM64 far branch"),
    ("branch",      "branch <offset> <target> [link]  � encode B/BL"),
    ("cave",        "cave [size=256]  � allocate rwx code cave"),
    ("cavetest",    "Test code cave execution via Houdini"),
    ("flushtest",   "Test cache flush + inline hook viability"),
    ("gothook",     "Test GOT slot redirection"),
    // Hooks
    ("cmdhook",     "Hook tryToExecuteCommand and install ring logger"),
    ("cmdlog",      "Dump command ring buffer"),
    ("arghook",     "arghook <offset>  � hook function and log x0-x7"),
    ("arglog",      "Dump argument ring buffer"),
    ("capture",     "capture [secs=12]  — capture commands during action"),
    // Misc
    ("dumpso",      "dumpso [path]  — dump libg.so to file for Ghidra"),
    ("snap",        "snap [reason]  — capture screenshot + logcat + report to error_logs/"),
    ("help",        "Show this help"),
    ("quit",        "Exit the loader"),
];

/// Dispatch a command line to the appropriate handler.
pub fn dispatch(session: &mut Session, line: &str) -> Result<bool> {
    let parts: Vec<&str> = line.trim().split_whitespace().collect();
    if parts.is_empty() { return Ok(true); }

    let cmd = parts[0].to_lowercase();
    let args = &parts[1..];

    match cmd.as_str() {
        // Farm
        "loadnative" => native::cmd_loadnative(session, args),
        "nfields"    => farm::cmd_nfields(session, args),
        "nplant"     => farm::cmd_nplant(session, args),
        "nharvest"   => farm::cmd_nharvest(session, args),
        "nsell"      => farm::cmd_nsell(session, args),
        "nfarm"      => farm::cmd_nfarm(session, args),
        "nfdiag"     => farm::cmd_nfdiag(session, args),
        "nediag"     => farm::cmd_nediag(session, args),
        "nping"      => native::cmd_nping(session, args),
        "ndiag"      => native::cmd_ndiag(session, args),
        // Anti-cheat
        "nquago"     => quago::cmd_nquago(session, args),
        "nstate"     => quago::cmd_nstate(session, args),
        "nspoof"     => spoof::cmd_nspoof(session, args),
        // Memory
        "info"       => info::cmd_info(session, args),
        "read"       => info::cmd_read(session, args),
        "write"      => info::cmd_write(session, args),
        "dump"       => info::cmd_dump(session, args),
        "rabs"       => info::cmd_rabs(session, args),
        "wabs"       => info::cmd_wabs(session, args),
        "export"     => info::cmd_export(session, args),
        // Scanner
        "scan"       => scan::cmd_scan(session, args),
        "vscan"      => scan::cmd_vscan(session, args),
        "vnarrow"    => scan::cmd_vnarrow(session, args),
        "vlist"      => scan::cmd_vlist(session, args),
        "vwrite"     => scan::cmd_vwrite(session, args),
        "vreset"     => scan::cmd_vreset(session, args),
        // Patching
        "nop"        => patch::cmd_nop(session, args),
        "farjump"    => patch::cmd_farjump(session, args),
        "branch"     => patch::cmd_branch(session, args),
        "cave"       => patch::cmd_cave(session, args),
        "cavetest"   => patch::cmd_cavetest(session, args),
        "flushtest"  => patch::cmd_flushtest(session, args),
        "gothook"    => patch::cmd_gothook(session, args),
        // Hooks
        "cmdhook"    => hooks::cmd_cmdhook(session, args),
        "cmdlog"     => hooks::cmd_cmdlog(session, args),
        "arghook"    => hooks::cmd_arghook(session, args),
        "arglog"     => hooks::cmd_arglog(session, args),
        "capture"    => hooks::cmd_capture(session, args),
        // Misc
        "dumpso"     => misc::cmd_dumpso(session, args),
        "snap" | "dumpcrash" => misc::cmd_snap(session, args),
        "help" | "?" => {
            println!("\n  Hay Star commands:");
            for (name, desc) in COMMANDS {
                println!("    {name:<14} {desc}");
            }
            println!();
            Ok(())
        }
        "quit" | "exit" | "q" => {
            println!("  Goodbye.");
            return Ok(false); // signals loop to stop
        }
        _ => {
            println!("  Unknown command: {cmd}  (type 'help' for list)");
            Ok(())
        }
    }?;

    Ok(true) // continue loop
}
