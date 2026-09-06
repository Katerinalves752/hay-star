use serde_json::{json, Value as Json};
use crate::error::Result;
use crate::session::Session;

pub fn cmd_info(session: &mut Session, _args: &[&str]) -> Result<()> {
    let info = session.rpc("info", &[])?;
    println!("  Base:     {}", info["base"]);
    println!("  Size:     {} (0x{:x})", info["size"], info["size"].as_u64().unwrap_or(0));
    println!("  Arch:     {}", info["arch"]);
    println!("  Platform: {}", info["platform"]);
    println!("  Houdini:  {}", info["houdini"]);
    println!("  PID:      {}", info["pid"]);
    Ok(())
}

pub fn cmd_read(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 2 { println!("  Usage: read <type> <offset> [length]"); return Ok(()); }
    let (dtype, offset) = (args[0], args[1]);
    let result = match dtype {
        "int"    => session.rpc("readint",     &[Json::String(offset.to_string())]),
        "float"  => session.rpc("readfloat",   &[Json::String(offset.to_string())]),
        "double" => session.rpc("readdouble",  &[Json::String(offset.to_string())]),
        "long"   => session.rpc("readlong",    &[Json::String(offset.to_string())]),
        "ptr"    => session.rpc("readpointer", &[Json::String(offset.to_string())]),
        "str"    => {
            let len = args.get(2).copied().unwrap_or("256");
            session.rpc("readstring", &[Json::String(offset.to_string()), Json::String(len.to_string())])
        }
        "bytes"  => {
            let len = args.get(2).copied().unwrap_or("64");
            session.rpc("readbytes", &[Json::String(offset.to_string()), Json::String(len.to_string())])
        }
        _ => { println!("  Unknown type: {dtype}"); return Ok(()); }
    };
    match result {
        Ok(v) if dtype == "bytes" => {
            if let Some(arr) = v.as_array() {
                let hex: String = arr.iter()
                    .filter_map(|b| b.as_u64())
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>().join(" ");
                println!("  [{offset}] = {hex}");
            }
        }
        Ok(v)  => println!("  [{offset}] = {v}"),
        Err(e) => println!("  Error: {e}"),
    }
    Ok(())
}

pub fn cmd_write(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 3 { println!("  Usage: write <type> <offset> <value>"); return Ok(()); }
    let (dtype, offset, value) = (args[0], args[1], args[2]);
    let ok = match dtype {
        "int"    => session.rpc("writeint",    &[Json::String(offset.to_string()), Json::String(value.to_string())]),
        "float"  => session.rpc("writefloat",  &[Json::String(offset.to_string()), Json::String(value.to_string())]),
        "double" => session.rpc("writedouble", &[Json::String(offset.to_string()), Json::String(value.to_string())]),
        "long"   => session.rpc("writelong",   &[Json::String(offset.to_string()), Json::String(value.to_string())]),
        "bytes"  => session.rpc("writebytes",  &[Json::String(offset.to_string()), Json::String(value.to_string())]),
        _ => { println!("  Unknown type: {dtype}"); return Ok(()); }
    };
    println!("  {}", if ok.ok().and_then(|v| v.as_bool()).unwrap_or(false) { "OK" } else { "FAIL" });
    Ok(())
}

pub fn cmd_dump(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 2 { println!("  Usage: dump <offset> <length>"); return Ok(()); }
    let data = session.rpc("dump", &[Json::String(args[0].to_string()), Json::String(args[1].to_string())])?;
    let bytes: Vec<u8> = data.as_array().map(|a| a.iter()
        .filter_map(|b| b.as_u64().map(|x| x as u8)).collect()).unwrap_or_default();
    if bytes.is_empty() { println!("  FAIL"); return Ok(()); }
    let off = parse_hex_or_dec(args[0]).unwrap_or(0) as usize;
    hexdump(&bytes, off);
    Ok(())
}

pub fn cmd_rabs(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 2 { println!("  Usage: rabs <abs_addr> <len>"); return Ok(()); }
    let len: usize = parse_hex_or_dec(args[1]).unwrap_or(0) as usize;
    let data = session.rpc("readabs", &[Json::String(args[0].to_string()), Json::Number((len as u64).into())])?;
    let bytes: Vec<u8> = data.as_array().map(|a| a.iter()
        .filter_map(|b| b.as_u64().map(|x| x as u8)).collect()).unwrap_or_default();
    if bytes.is_empty() { println!("  FAIL (unmapped?)"); return Ok(()); }
    let base = parse_hex_or_dec(args[0]).unwrap_or(0) as usize;
    hexdump(&bytes, base);
    Ok(())
}

pub fn cmd_wabs(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 2 { println!("  Usage: wabs <abs_addr> <hexbytes>"); return Ok(()); }
    let hex_str = args[1].replace(' ', "");
    let bytes: Option<Vec<u8>> = (0..hex_str.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_str[i..i+2], 16).ok())
        .collect();
    let bytes = match bytes { Some(b) => b, None => { println!("  Invalid hex"); return Ok(()); } };
    let arr: Vec<Json> = bytes.iter().map(|&b| Json::Number((b as u64).into())).collect();
    let ok = session.rpc("writeabs", &[Json::String(args[0].to_string()), Json::Array(arr)])
        .ok().and_then(|v| v.as_bool()).unwrap_or(false);
    println!("  {}: wrote {} bytes to {}", if ok { "OK" } else { "FAIL" }, bytes.len(), args[0]);
    Ok(())
}

pub fn cmd_export(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.is_empty() { println!("  Usage: export <function_name>"); return Ok(()); }
    let result = session.rpc("getexport", &[Json::String(args[0].to_string())])?;
    if result.is_null() { println!("  Not found"); }
    else { println!("  {} -> {}", args[0], result); }
    Ok(())
}

fn hexdump(data: &[u8], base_off: usize) {
    for i in (0..data.len()).step_by(16) {
        let chunk = &data[i..data.len().min(i + 16)];
        let hex: String = chunk.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
        let ascii: String = chunk.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' }).collect();
        println!("  {:08x}  {hex:<48}  {ascii}", base_off + i);
    }
}

pub fn parse_hex_or_dec(s: &str) -> Option<u64> {
    if s.starts_with("0x") || s.starts_with("0X") {
        u64::from_str_radix(&s[2..], 16).ok()
    } else {
        s.parse().ok()
    }
}
