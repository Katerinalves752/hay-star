use serde_json::{json, Value as Json};
use crate::error::Result;
use crate::session::Session;

const TYPE_SIZES: &[(&str, usize, &str)] = &[
    ("int",    4, "<i"),
    ("float",  4, "<f"),
    ("double", 8, "<d"),
    ("short",  2, "<h"),
    ("long",   8, "<q"),
];

fn value_to_bytes(dtype: &str, value_str: &str) -> Option<Vec<u8>> {
    match dtype {
        "int"    => value_str.parse::<i32>().ok().map(|v| v.to_le_bytes().to_vec()),
        "short"  => value_str.parse::<i16>().ok().map(|v| v.to_le_bytes().to_vec()),
        "long"   => value_str.parse::<i64>().ok().map(|v| v.to_le_bytes().to_vec()),
        "float"  => value_str.parse::<f32>().ok().map(|v| v.to_le_bytes().to_vec()),
        "double" => value_str.parse::<f64>().ok().map(|v| v.to_le_bytes().to_vec()),
        _ => None,
    }
}

fn bytes_to_hex_pattern(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

pub fn cmd_scan(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.is_empty() { println!("  Usage: scan <pattern>"); return Ok(()); }
    let pattern = args.join(" ");
    let results = session.rpc("scan", &[Json::String(pattern)]);
    match results {
        Ok(Json::Array(arr)) if !arr.is_empty() => {
            for r in &arr {
                let off = r.get("offset").and_then(|v| v.as_str()).unwrap_or("?");
                let addr = r.get("address").and_then(|v| v.as_str()).unwrap_or("?");
                println!("  {off}  ({addr})");
            }
        }
        _ => println!("  No matches"),
    }
    Ok(())
}

pub fn cmd_vscan(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 2 { println!("  Usage: vscan <int|float|double|short|long> <value>"); return Ok(()); }
    let dtype = args[0];
    let bytes = value_to_bytes(dtype, args[1]);
    let bytes = match bytes { Some(b) => b, None => { println!("  Error: invalid type or value"); return Ok(()); } };
    let pattern = bytes_to_hex_pattern(&bytes);
    println!("  Scanning writable memory for {dtype} {} [{pattern}]...", args[1]);
    let results = session.rpc("scanmem", &[Json::String(pattern)]);
    match results {
        Ok(Json::Array(arr)) => {
            let count = arr.len();
            session.scan_results = arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
            session.scan_type = Some(dtype.to_string());
            println!("  Found {count} matches{}", if count >= 4096 { " (capped at 4096)" } else { "" });
            if count <= 20 {
                for addr in &session.scan_results { println!("    {addr}"); }
            }
        }
        _ => println!("  No matches or error"),
    }
    Ok(())
}

pub fn cmd_vnarrow(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 2 { println!("  Usage: vnarrow <type> <value>"); return Ok(()); }
    if session.scan_results.is_empty() { println!("  No previous scan. Use vscan first."); return Ok(()); }
    let dtype = args[0];
    let bytes = match value_to_bytes(dtype, args[1]) {
        Some(b) => b, None => { println!("  Error: invalid type or value"); return Ok(()); }
    };
    let pattern = bytes_to_hex_pattern(&bytes);
    let prev = session.scan_results.len();
    let addrs: Vec<Json> = session.scan_results.iter().map(|s| Json::String(s.clone())).collect();
    println!("  Narrowing {prev} results for {dtype} {}...", args[1]);
    let results = session.rpc("narrowmem", &[Json::Array(addrs), Json::String(pattern)]);
    match results {
        Ok(Json::Array(arr)) => {
            let count = arr.len();
            session.scan_results = arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
            session.scan_type = Some(dtype.to_string());
            println!("  {count} matches remain (eliminated {})", prev - count);
        }
        _ => { session.scan_results.clear(); println!("  0 matches remain"); }
    }
    Ok(())
}

pub fn cmd_vlist(session: &mut Session, _args: &[&str]) -> Result<()> {
    if session.scan_results.is_empty() { println!("  No scan results"); return Ok(()); }
    let dtype = session.scan_type.as_deref().unwrap_or("int");
    let size = TYPE_SIZES.iter().find(|(t,_,_)| *t == dtype).map(|(_,s,_)| *s).unwrap_or(4);
    let count = session.scan_results.len();
    let show = count.min(50);
    println!("  {count} results ({dtype}):");
    for i in 0..show {
        let addr = session.scan_results[i].clone();
        let raw = session.rpc("readabs", &[Json::String(addr.clone()), Json::Number((size as u64).into())]);
        let val = format_value(raw.ok().as_ref(), dtype, size);
        println!("    [{i}] {addr} = {val}");
    }
    if count > show { println!("    ... and {} more", count - show); }
    Ok(())
}

pub fn cmd_vwrite(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.is_empty() { println!("  Usage: vwrite <value> [index]"); return Ok(()); }
    if session.scan_results.is_empty() { println!("  No scan results"); return Ok(()); }
    let dtype = session.scan_type.clone().unwrap_or_else(|| "int".to_string());
    let bytes = match value_to_bytes(&dtype, args[0]) {
        Some(b) => b, None => { println!("  Error: invalid value"); return Ok(()); }
    };
    let arr: Vec<Json> = bytes.iter().map(|&b| Json::Number((b as u64).into())).collect();
    if let Some(idx_str) = args.get(1) {
        let idx: usize = idx_str.parse().unwrap_or(usize::MAX);
        if idx < session.scan_results.len() {
            let addr = session.scan_results[idx].clone();
            let ok = session.rpc("writeabs", &[Json::String(addr.clone()), Json::Array(arr)])
                .ok().and_then(|v| v.as_bool()).unwrap_or(false);
            println!("  {}: {addr}", if ok { "OK" } else { "FAIL" });
        } else {
            println!("  Index out of range (0-{})", session.scan_results.len() - 1);
        }
    } else {
        let mut ok = 0;
        for addr in session.scan_results.clone() {
            if session.rpc("writeabs", &[Json::String(addr), Json::Array(arr.clone())])
                .ok().and_then(|v| v.as_bool()).unwrap_or(false) { ok += 1; }
        }
        println!("  Wrote to {}/{}", ok, session.scan_results.len());
    }
    Ok(())
}

pub fn cmd_vreset(session: &mut Session, _args: &[&str]) -> Result<()> {
    session.scan_results.clear();
    session.scan_type = None;
    println!("  Scan results cleared");
    Ok(())
}

fn format_value(raw: Option<&Json>, dtype: &str, size: usize) -> String {
    let bytes: Vec<u8> = raw.and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|b| b.as_u64().map(|x| x as u8)).collect())
        .unwrap_or_default();
    if bytes.len() < size { return "?".to_string(); }
    match dtype {
        "int"    => i32::from_le_bytes(bytes[..4].try_into().unwrap_or([0;4])).to_string(),
        "short"  => i16::from_le_bytes(bytes[..2].try_into().unwrap_or([0;2])).to_string(),
        "long"   => i64::from_le_bytes(bytes[..8].try_into().unwrap_or([0;8])).to_string(),
        "float"  => format!("{:.4}", f32::from_le_bytes(bytes[..4].try_into().unwrap_or([0;4]))),
        "double" => format!("{:.6}", f64::from_le_bytes(bytes[..8].try_into().unwrap_or([0;8]))),
        _ => "?".to_string(),
    }
}
