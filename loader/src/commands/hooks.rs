use serde_json::{json, Value as Json};
use std::thread;
use std::time::{Duration, Instant};
use crate::arm64::*;
use crate::error::Result;
use crate::session::Session;
use crate::commands::info::parse_hex_or_dec;

const TRY_EXEC: u64 = 0x00ae3bc4;

pub fn cmd_cmdhook(session: &mut Session, _args: &[&str]) -> Result<()> {
    let info = session.rpc("info", &[])?;
    let base = parse_hex_or_dec(info["base"].as_str().unwrap_or("0")).unwrap_or(0);
    session.engine_base = base;
    let fabs = base + TRY_EXEC;
    let mbox = session.rpc("alloccave", &[Json::Number(2048.into())])?;
    let mbox_i = parse_hex_or_dec(mbox.as_str().unwrap_or("0")).unwrap_or(0);
    session.write_bytes_abs(mbox_i, &[0u8; 8]);
    let stolen_raw = session.read_bytes_abs(base + TRY_EXEC, 16);
    if stolen_raw.len() < 16 { println!("  FAIL: stolen bytes"); return Ok(()); }
    let stolen: [u8; 16] = stolen_raw[..16].try_into().unwrap();
    let cave = session.rpc("alloccave", &[Json::Number(256.into())])?;
    let sc = build_cmdlog_cave(mbox_i, &stolen, fabs + 16);
    let arr: Vec<Json> = sc.iter().map(|&b| Json::Number((b as u64).into())).collect();
    session.rpc("writeabs", &[cave.clone(), Json::Array(arr)])?;
    session.rpc("farjump", &[Json::String(TRY_EXEC.to_string()), cave.clone()])?;
    session.cmdlog_mbox = Some(mbox.as_str().unwrap_or("").to_string());
    println!("  Hooked tryToExecuteCommand @ 0x{TRY_EXEC:08x}");
    println!("  Command ring @ {mbox}");
    println!("  >>> Now PLANT WHEAT in-game, then run: cmdlog");
    Ok(())
}

pub fn cmd_cmdlog(session: &mut Session, _args: &[&str]) -> Result<()> {
    let mbox_str = match &session.cmdlog_mbox { Some(m) => m.clone(), None => { println!("  Run cmdhook first."); return Ok(()); } };
    let mbox_i = parse_hex_or_dec(&mbox_str).unwrap_or(0);
    let base = session.engine_base;
    let head_bytes = session.read_bytes_abs(mbox_i, 8);
    let idx = if head_bytes.len() >= 4 { u32::from_le_bytes(head_bytes[..4].try_into().unwrap()) } else { 0 };
    println!("  {idx} command(s) captured:");
    for i in 0..idx.min(24) as u64 {
        let eab = mbox_i + 8 + i * 64;
        let e = session.read_bytes_abs(eab, 56);
        if e.len() < 56 { continue; }
        let vals: [u64; 7] = {
            let mut v = [0u64; 7];
            for j in 0..7 { v[j] = u64::from_le_bytes(e[j*8..j*8+8].try_into().unwrap()); }
            v
        };
        let (_, cmd, vt, m8, m16, m24, m32) = (vals[0], vals[1], vals[2], vals[3], vals[4], vals[5], vals[6]);
        let size = 0x1654c00u64;
        let vtxt = if base > 0 && base <= vt && vt < base + size { format!("vtbl+0x{:08x}", vt - base) }
                   else { format!("vtbl=0x{vt:x}") };
        println!("    [{i:2}] {vtxt}  params=[0x{m8:x} 0x{m16:x} 0x{m24:x} 0x{m32:x}]");
    }
    Ok(())
}

pub fn cmd_arghook(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.is_empty() { println!("  Usage: arghook <func_off>"); return Ok(()); }
    let f = parse_hex_or_dec(args[0]).unwrap_or(0);
    let info = session.rpc("info", &[])?;
    let base = parse_hex_or_dec(info["base"].as_str().unwrap_or("0")).unwrap_or(0);
    session.engine_base = base;
    let fabs = base + f;
    let mbox = session.rpc("alloccave", &[Json::Number(2048.into())])?;
    let mbox_i = parse_hex_or_dec(mbox.as_str().unwrap_or("0")).unwrap_or(0);
    session.write_bytes_abs(mbox_i, &[0u8; 8]);
    let stolen_raw = session.read_bytes_abs(base + f, 16);
    if stolen_raw.len() < 16 { println!("  FAIL: stolen bytes"); return Ok(()); }
    let stolen: [u8; 16] = stolen_raw[..16].try_into().unwrap();
    let cave = session.rpc("alloccave", &[Json::Number(256.into())])?;
    let sc = build_arghook_cave(mbox_i, &stolen, fabs + 16);
    let arr: Vec<Json> = sc.iter().map(|&b| Json::Number((b as u64).into())).collect();
    session.rpc("writeabs", &[cave.clone(), Json::Array(arr)])?;
    session.rpc("farjump", &[Json::String(f.to_string()), cave.clone()])?;
    session.cmdlog_mbox = Some(mbox.as_str().unwrap_or("").to_string());
    println!("  Arg-hooked 0x{f:08x}; ring @ {mbox}");
    println!("  Trigger the action in-game, then run: arglog");
    Ok(())
}

pub fn cmd_arglog(session: &mut Session, _args: &[&str]) -> Result<()> {
    let mbox_str = match &session.cmdlog_mbox { Some(m) => m.clone(), None => { println!("  Run arghook first."); return Ok(()); } };
    let mbox_i = parse_hex_or_dec(&mbox_str).unwrap_or(0);
    let base = session.engine_base;
    let head_bytes = session.read_bytes_abs(mbox_i, 8);
    let idx = if head_bytes.len() >= 4 { u32::from_le_bytes(head_bytes[..4].try_into().unwrap()) } else { 0 };
    let size = 0x1654c00u64;
    println!("  {idx} call(s) captured:");
    for i in 0..idx.min(24) as u64 {
        let eab = mbox_i + 8 + i * 64;
        let raw = session.read_bytes_abs(eab, 64);
        if raw.len() < 64 { continue; }
        let regs: [u64; 8] = {
            let mut v = [0u64; 8];
            for j in 0..8 { v[j] = u64::from_le_bytes(raw[j*8..j*8+8].try_into().unwrap()); }
            v
        };
        let parts: Vec<String> = regs.iter().enumerate().map(|(r, &v)| {
            if base > 0 && base <= v && v < base + size { format!("x{r}=lib+0x{:x}", v - base) }
            else if v < 0x1_0000_0000 { format!("x{r}={v}") }
            else { format!("x{r}=0x{v:x}") }
        }).collect();
        println!("    [{i:2}] {}", parts.join("  "));
    }
    Ok(())
}

pub fn cmd_capture(session: &mut Session, args: &[&str]) -> Result<()> {
    let secs: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(12);
    let f: u64 = TRY_EXEC;
    let info = session.rpc("info", &[])?;
    let base = parse_hex_or_dec(info["base"].as_str().unwrap_or("0")).unwrap_or(0);
    session.engine_base = base;
    let fabs = base + f;
    let mbox = session.rpc("alloccave", &[Json::Number(2048.into())])?;
    let mbox_i = parse_hex_or_dec(mbox.as_str().unwrap_or("0")).unwrap_or(0);
    session.write_bytes_abs(mbox_i, &[0u8; 8]);
    let stolen_raw = session.read_bytes_abs(base + f, 16);
    if stolen_raw.len() < 16 { println!("  FAIL: prologue not relocatable"); return Ok(()); }
    let stolen: [u8; 16] = stolen_raw[..16].try_into().unwrap();
    let cave = session.rpc("alloccave", &[Json::Number(256.into())])?;
    let sc = build_cmdlog_cave(mbox_i, &stolen, fabs + 16);
    let arr: Vec<Json> = sc.iter().map(|&b| Json::Number((b as u64).into())).collect();
    session.rpc("writeabs", &[cave.clone(), Json::Array(arr)])?;
    println!("  Capturing tryToExecuteCommand for {secs}s -- DO THE ACTION NOW...");
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        session.rpc("farjump", &[Json::String(f.to_string()), cave.clone()])?;
        thread::sleep(Duration::from_millis(400));
    }
    let head = session.read_bytes_abs(mbox_i, 8);
    let idx = if head.len() >= 4 { u32::from_le_bytes(head[..4].try_into().unwrap()) } else { 0 };
    let size = 0x1654c00u64;
    println!("  {idx} command(s) captured:");
    let mut seen = std::collections::HashSet::new();
    for i in 0..idx.min(24) as u64 {
        let eab = mbox_i + 8 + i * 0x40;
        let e = session.read_bytes_abs(eab, 0x40);
        if e.len() < 0x40 { continue; }
        let qs: [u64; 8] = {
            let mut v = [0u64; 8];
            for j in 0..8 { v[j] = u64::from_le_bytes(e[j*8..j*8+8].try_into().unwrap()); }
            v
        };
        let vt = qs[0];
        let vtxt = if base <= vt && vt < base + size { format!("vtbl+0x{:08x}", vt - base) }
                   else { format!("vtbl=0x{vt:x}") };
        let dup = if seen.contains(&vt) { "  (dup)" } else { "" };
        seen.insert(vt);
        let p24 = (qs[4] >> 32) & 0xFFFF_FFFF;
        let p28 = qs[5] & 0xFFFF_FFFF;
        println!("    [{i:2}] {vtxt}  +24={p24} +28={p28}{dup}");
    }
    Ok(())
}
