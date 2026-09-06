use serde_json::{json, Value as Json};
use crate::error::Result;
use crate::session::Session;
use crate::commands::info::parse_hex_or_dec;

pub fn cmd_dumpso(session: &mut Session, args: &[&str]) -> Result<()> {
    let out_path = args.first().copied().unwrap_or("libg_dump.so");
    let layout = session.rpc("layout", &[])?;
    let base = layout["base"].as_str().unwrap_or("0");
    let size = layout["size"].as_u64().unwrap_or(0) as usize;
    let segments = layout["segments"].as_array().cloned().unwrap_or_default();
    println!("  libg.so base={base} size={size} (0x{size:x})");
    println!("  Dumping {} segment(s) to {out_path}...", segments.len());
    let mut buf = vec![0u8; size];
    let chunk = 2 * 1024 * 1024usize;
    let mut total = 0usize;
    for seg in &segments {
        let off = parse_hex_or_dec(seg["offset"].as_str().unwrap_or("0")).unwrap_or(0) as usize;
        let seglen = parse_hex_or_dec(seg["size"].as_str().unwrap_or("0")).unwrap_or(0) as usize;
        let mut pos = 0usize;
        while pos < seglen {
            let n = chunk.min(seglen - pos);
            let data = session.rpc("readbytes", &[
                Json::String((off + pos).to_string()),
                Json::String(n.to_string()),
            ]);
            match data {
                Ok(Json::Array(arr)) => {
                    let bytes: Vec<u8> = arr.iter().filter_map(|b| b.as_u64().map(|x| x as u8)).collect();
                    let end = (off + pos + bytes.len()).min(size);
                    let start = off + pos;
                    if start < end { buf[start..end].copy_from_slice(&bytes[..end-start]); }
                    pos += bytes.len();
                    total += bytes.len();
                    print!("\r  {:.1} MB dumped", total as f64 / 1024.0 / 1024.0);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
                _ => { println!("\n  [!] read failed at offset 0x{:x}", off + pos); break; }
            }
        }
    }
    println!();
    std::fs::write(out_path, &buf)
        .map_err(|e| crate::error::LoaderError::Io(e))?;
    println!("  Wrote {size} bytes to {out_path}");
    println!("  Ghidra: Raw Binary, language AARCH64:LE:64:v8a, base 0x0");
    Ok(())
}

pub fn cmd_snap(session: &mut Session, args: &[&str]) -> Result<()> {
    let reason = if args.is_empty() {
        "Manual diagnostic snapshot requested by user".to_string()
    } else {
        args.join(" ")
    };
    let snap = crate::diagnostics::SessionSnapshot::from_session(session);
    let dump_dir = crate::diagnostics::capture_crash_dump(&session.adb, &session.device_id, &reason, Some(&snap));
    println!("  Diagnostic snapshot saved to {}", dump_dir.display());
    Ok(())
}
