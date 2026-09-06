use serde_json::{json, Value as Json};
use std::thread;
use std::time::Duration;
use crate::arm64::*;
use crate::error::Result;
use crate::session::Session;
use crate::commands::info::parse_hex_or_dec;

pub fn cmd_nop(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 2 { println!("  Usage: nop <offset> <byte_count>"); return Ok(()); }
    let ok = session.rpc("nop", &[Json::String(args[0].to_string()), Json::String(args[1].to_string())])
        .ok().and_then(|v| v.as_bool()).unwrap_or(false);
    println!("  {}", if ok { "Patched" } else { "FAIL" });
    Ok(())
}

pub fn cmd_farjump(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 2 { println!("  Usage: farjump <libg_offset> <target_abs_addr>"); return Ok(()); }
    let original = session.rpc("farjump", &[Json::String(args[0].to_string()), Json::String(args[1].to_string())])?;
    if let Some(arr) = original.as_array() {
        let orig_hex: String = arr.iter().filter_map(|b| b.as_u64()).map(|b| format!("{b:02x}")).collect();
        println!("  Redirected {} -> {}", args[0], args[1]);
        println!("  Original 16 bytes: {orig_hex}");
    } else {
        println!("  FAIL");
    }
    Ok(())
}

pub fn cmd_branch(session: &mut Session, args: &[&str]) -> Result<()> {
    if args.len() < 2 { println!("  Usage: branch <libg_offset> <target_abs_addr> [link]"); return Ok(()); }
    let link = args.get(2).map(|s| matches!(*s, "link"|"bl"|"1"|"true")).unwrap_or(false);
    let encoded = session.rpc("makebranch", &[
        Json::String(args[0].to_string()),
        Json::String(args[1].to_string()),
        Json::Bool(link),
    ])?;
    if let Some(arr) = encoded.as_array() {
        let hexstr: String = arr.iter().filter_map(|b| b.as_u64()).map(|b| format!("{b:02x}")).collect();
        let ok = session.rpc("writebytes", &[Json::String(args[0].to_string()), Json::String(hexstr.clone())])
            .ok().and_then(|v| v.as_bool()).unwrap_or(false);
        let kind = if link { "BL" } else { "B" };
        println!("  {}: {kind} {} -> {} [{hexstr}]", if ok { "OK" } else { "FAIL" }, args[0], args[1]);
    } else {
        println!("  FAIL to encode");
    }
    Ok(())
}

pub fn cmd_cave(session: &mut Session, args: &[&str]) -> Result<()> {
    let size: u64 = args.first().and_then(|s| parse_hex_or_dec(s)).unwrap_or(256);
    let addr = session.rpc("alloccave", &[Json::Number(size.into())])?;
    println!("  Cave allocated: {addr} ({size} bytes, rwx)");
    println!("  Fill it with:  wabs {addr} <hexbytes>");
    Ok(())
}

pub fn cmd_cavetest(session: &mut Session, args: &[&str]) -> Result<()> {
    let target: u64 = args.first().and_then(|s| parse_hex_or_dec(s)).unwrap_or(0x011116e4);
    println!("  Testing code cave execution at 0x{target:08x}...");
    let info = session.rpc("info", &[])?;
    let base = parse_hex_or_dec(info["base"].as_str().unwrap_or("0")).unwrap_or(0);
    let cave = session.rpc("alloccave", &[Json::Number(128.into())])?;
    let mbox = session.rpc("alloccave", &[Json::Number(16.into())])?;
    let mbox_i = parse_hex_or_dec(mbox.as_str().unwrap_or("0")).unwrap_or(0);
    session.write_bytes_abs(mbox_i, &[0u8; 8]);

    let stolen_raw = session.read_bytes_abs(base + target, 16);
    if stolen_raw.len() < 16 { println!("  FAIL: could not read 16 stolen bytes"); return Ok(()); }
    let stolen: [u8; 16] = stolen_raw[..16].try_into().unwrap();
    let ret_abs = base + target + 16;

    let sc = build_counter_cave(mbox_i, &stolen, ret_abs);
    let arr: Vec<Json> = sc.iter().map(|&b| Json::Number((b as u64).into())).collect();
    if !session.rpc("writeabs", &[cave.clone(), Json::Array(arr)])
        .ok().and_then(|v| v.as_bool()).unwrap_or(false) {
        println!("  FAIL: could not write cave shellcode"); return Ok(());
    }
    session.rpc("farjump", &[Json::String(target.to_string()), cave.clone()])?;
    println!("  Hooked 0x{target:08x} -> cave {cave} (mailbox {mbox})");
    println!("  Watching counter for 6s...");
    let mut last = 0u64;
    for i in 0..12 {
        thread::sleep(Duration::from_millis(500));
        let v = session.read_bytes_abs(mbox_i, 8);
        let cnt = if v.len() >= 8 { u64::from_le_bytes(v[..8].try_into().unwrap()) } else { 0 };
        println!("    t={:.1}s  counter={cnt}", i as f64 * 0.5);
        last = cnt;
    }
    if last > 0 { println!("  >>> SUCCESS: Houdini executes our cave code."); }
    else { println!("  >>> Counter stayed 0: patch not picked up (translation cache)."); }
    Ok(())
}

pub fn cmd_flushtest(session: &mut Session, args: &[&str]) -> Result<()> {
    let f: u64 = args.first().and_then(|s| parse_hex_or_dec(s)).unwrap_or(0x011116e4);
    let got_off: u64 = 0x015057f0;
    println!("  Testing cache flush + inline hook at 0x{f:08x}...");
    let info = session.rpc("info", &[])?;
    let base = parse_hex_or_dec(info["base"].as_str().unwrap_or("0")).unwrap_or(0);
    let fabs = base + f;
    let got_abs = base + got_off;

    let cmbox = session.rpc("alloccave", &[Json::Number(16.into())])?;
    let cmbox_i = parse_hex_or_dec(cmbox.as_str().unwrap_or("0")).unwrap_or(0);
    session.write_bytes_abs(cmbox_i, &[0u8; 8]);

    let stolen = session.read_bytes_abs(base + f, 16);
    if stolen.len() < 16 { println!("  FAIL: stolen bytes"); return Ok(()); }
    let stolen16: [u8; 16] = stolen[..16].try_into().unwrap();

    // Counter cave
    let ccave = session.rpc("alloccave", &[Json::Number(128.into())])?;
    let sc = build_counter_cave(cmbox_i, &stolen, fabs + 16);
    let arr: Vec<Json> = sc.iter().map(|&b| Json::Number((b as u64).into())).collect();
    session.rpc("writeabs", &[ccave.clone(), Json::Array(arr)])?;
    session.rpc("farjump", &[Json::String(f.to_string()), ccave.clone()])?;
    println!("  Inline-patched 0x{f:08x} -> counter cave {ccave}");

    // Flush cave on GOT
    let orig_bytes = session.read_bytes_abs(got_abs, 8);
    if orig_bytes.len() < 8 { println!("  FAIL: read GOT"); return Ok(()); }
    let orig_i = u64::from_le_bytes(orig_bytes[..8].try_into().unwrap());
    let fcave = session.rpc("alloccave", &[Json::Number(128.into())])?;
    let fc = build_flush_cave(fabs, orig_i);
    let farr: Vec<Json> = fc.iter().map(|&b| Json::Number((b as u64).into())).collect();
    session.rpc("writeabs", &[fcave.clone(), Json::Array(farr)])?;
    let fcave_i = parse_hex_or_dec(fcave.as_str().unwrap_or("0")).unwrap_or(0);
    session.write_bytes_abs(got_abs, &fcave_i.to_le_bytes());
    println!("  Flush cave on GOT 0x{got_off:08x} -> {fcave}. Watching 6s...");

    let mut last = 0u64;
    let result = std::panic::catch_unwind(|| {
        for i in 0..12 {
            thread::sleep(Duration::from_millis(500));
        }
    });
    // Restore GOT
    session.write_bytes_abs(got_abs, &orig_i.to_le_bytes());
    println!("  GOT slot restored");
    for i in 0..12 {
        let v = session.read_bytes_abs(cmbox_i, 8);
        last = if v.len() >= 8 { u64::from_le_bytes(v[..8].try_into().unwrap()) } else { 0 };
    }
    if last > 0 { println!("  >>> SUCCESS: cache flush works."); }
    else { println!("  >>> Counter 0: Houdini ignores guest IC/DC. Use data hooks only."); }
    Ok(())
}

pub fn cmd_gothook(session: &mut Session, args: &[&str]) -> Result<()> {
    let got_off: u64 = args.first().and_then(|s| parse_hex_or_dec(s)).unwrap_or(0x015057f0);
    println!("  Testing GOT hook at 0x{got_off:08x}...");
    let info = session.rpc("info", &[])?;
    let base = parse_hex_or_dec(info["base"].as_str().unwrap_or("0")).unwrap_or(0);
    let got_abs = base + got_off;
    let orig_bytes = session.read_bytes_abs(got_abs, 8);
    if orig_bytes.len() < 8 { println!("  FAIL: could not read GOT slot"); return Ok(()); }
    let orig_i = u64::from_le_bytes(orig_bytes[..8].try_into().unwrap());
    println!("  GOT slot 0x{got_off:08x} -> orig 0x{orig_i:x}");

    let cave = session.rpc("alloccave", &[Json::Number(128.into())])?;
    let mbox = session.rpc("alloccave", &[Json::Number(16.into())])?;
    let mbox_i = parse_hex_or_dec(mbox.as_str().unwrap_or("0")).unwrap_or(0);
    session.write_bytes_abs(mbox_i, &[0u8; 8]);

    // Build counter cave that tail-calls orig
    let stolen = [0u8; 0]; // GOT hook: no stolen bytes, direct tail-call
    let mut sc = load_imm64(16, mbox_i);
    sc.extend_from_slice(&0xF9400000u32.wrapping_add((16 << 5) | 17).to_le_bytes()); // ldr x17,[x16]
    sc.extend_from_slice(&0x91000400u32.wrapping_add((17 << 5) | 17).to_le_bytes()); // add x17,x17,#1
    sc.extend_from_slice(&0xF9000000u32.wrapping_add((16 << 5) | 17).to_le_bytes()); // str x17,[x16]
    sc.extend_from_slice(&insn(0x58000051)); // ldr x17,#8
    sc.extend_from_slice(&insn(0xD61F0220)); // br  x17
    sc.extend_from_slice(&orig_i.to_le_bytes());

    let arr: Vec<Json> = sc.iter().map(|&b| Json::Number((b as u64).into())).collect();
    if !session.rpc("writeabs", &[cave.clone(), Json::Array(arr)])
        .ok().and_then(|v| v.as_bool()).unwrap_or(false) {
        println!("  FAIL: could not write cave"); return Ok(());
    }
    let cave_i = parse_hex_or_dec(cave.as_str().unwrap_or("0")).unwrap_or(0);
    session.write_bytes_abs(got_abs, &cave_i.to_le_bytes());
    println!("  GOT redirected -> cave {cave}. Watching 6s...");

    let mut last = 0u64;
    for i in 0..12 {
        thread::sleep(Duration::from_millis(500));
        let v = session.read_bytes_abs(mbox_i, 8);
        let cnt = if v.len() >= 8 { u64::from_le_bytes(v[..8].try_into().unwrap()) } else { 0 };
        println!("    t={:.1}s  counter={cnt}", i as f64 * 0.5);
        last = cnt;
    }
    session.write_bytes_abs(got_abs, &orig_i.to_le_bytes());
    println!("  GOT slot restored");
    if last > 0 { println!("  >>> SUCCESS: indirect/data hook works."); }
    else { println!("  >>> Counter 0: slot not called or write ineffective."); }
    Ok(())
}
