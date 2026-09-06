// ============================================================================
//  HAY STAR - ARM64 instruction encoding helpers
//  Mirrors all the inline arm64 assembly in loader.py
// ============================================================================

/// Emit MOVZ Xd, #imm, LSL #shift
pub fn movz(rd: u32, imm: u64, shift: u32) -> u32 {
    0xD2800000 | ((shift / 16) << 21) | (((imm & 0xFFFF) as u32) << 5) | rd
}

/// Emit MOVK Xd, #imm, LSL #shift
pub fn movk(rd: u32, imm: u64, shift: u32) -> u32 {
    0xF2800000 | ((shift / 16) << 21) | (((imm & 0xFFFF) as u32) << 5) | rd
}

/// Load a 64-bit immediate into register rd using MOVZ + 3�MOVK.
/// Returns 4 ARM64 instructions (16 bytes) as little-endian bytes.
pub fn load_imm64(rd: u32, value: u64) -> Vec<u8> {
    let words = [
        movz(rd, value,        0),
        movk(rd, value >> 16, 16),
        movk(rd, value >> 32, 32),
        movk(rd, value >> 48, 48),
    ];
    let mut out = Vec::with_capacity(16);
    for w in &words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

/// Write a u32 instruction as little-endian bytes.
pub fn insn(x: u32) -> [u8; 4] {
    x.to_le_bytes()
}

/// Build a 16-byte ARM64 far branch:
///   LDR X17, #8   ; load target from 8 bytes ahead
///   BR  X17
///   .quad target
pub fn far_branch_bytes(target: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&insn(0x58000051)); // ldr x17, #8
    out.extend_from_slice(&insn(0xD61F0220)); // br  x17
    out.extend_from_slice(&target.to_le_bytes());
    out
}

/// Encode B (or BL) from `from_abs` to `to_abs`.
/// Returns None if the offset is out of the �128 MB B/BL range.
pub fn encode_branch(from_abs: u64, to_abs: u64, link: bool) -> Option<u32> {
    let diff = (to_abs as i64) - (from_abs as i64);
    if diff % 4 != 0 { return None; }
    let imm26 = diff / 4;
    // ARM64 B/BL imm26: signed 26-bit = �128 MB range
    if imm26 < -(1 << 25) || imm26 >= (1 << 25) { return None; }
    let base: u32 = if link { 0x94000000 } else { 0x14000000 };
    Some(base | ((imm26 as u32) & 0x03FF_FFFF))
}

/// ARM64 NOP instruction
pub const NOP: u32 = 0xD503201F;

/// Build a sequence of NOP instructions (length must be divisible by 4).
pub fn nop_patch(byte_count: usize) -> Vec<u8> {
    assert_eq!(byte_count % 4, 0, "NOP length must be divisible by 4");
    let mut out = Vec::with_capacity(byte_count);
    for _ in 0..(byte_count / 4) {
        out.extend_from_slice(&NOP.to_le_bytes());
    }
    out
}

// ---------------------------------------------------------------------------
//  Code cave builders - each mirrors the Python _build_*_cave methods.
// ---------------------------------------------------------------------------

/// Build the bridge cave that calls nx_on_tick(gameMode) on every game tick.
/// Layout:
///   sub sp, sp, #0x20
///   stp x0, x30, [sp]
///   <load_imm64 x16, on_tick_abs>
///   blr x16
///   ldp x0, x30, [sp]
///   add sp, sp, #0x20
///   <4 stolen tick prologue instructions>
///   ldr x17, #8
///   br  x17
///   .quad  fabs+16        ; return to tick+16
pub fn build_bridge_cave(on_tick_abs: u64, stolen: &[u8; 16], fabs: u64) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&insn(0xD1000000 | (0x20 << 10) | (31 << 5) | 31)); // sub sp,sp,#0x20
    out.extend_from_slice(&insn(0xA9000000 | (30 << 10) | (31 << 5) | 0));     // stp x0,x30,[sp]
    out.extend_from_slice(&load_imm64(16, on_tick_abs));                         // x16 = on_tick
    out.extend_from_slice(&insn(0xD63F0000 | (16 << 5)));                       // blr x16
    out.extend_from_slice(&insn(0xA9400000 | (30 << 10) | (31 << 5) | 0));     // ldp x0,x30,[sp]
    out.extend_from_slice(&insn(0x91000000 | (0x20 << 10) | (31 << 5) | 31));  // add sp,sp,#0x20
    out.extend_from_slice(stolen);                                                // stolen tick prologue
    out.extend_from_slice(&insn(0x58000051));                                    // ldr x17,#8
    out.extend_from_slice(&insn(0xD61F0220));                                    // br  x17
    out.extend_from_slice(&(fabs + 16).to_le_bytes());                          // .quad tick+16
    out
}

/// Build a counter cave for gothook / cavetest.
/// Increments a 64-bit counter at mbox_addr, then runs stolen instructions
/// and returns to ret_abs.
pub fn build_counter_cave(mbox_addr: u64, stolen: &[u8], ret_abs: u64) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&load_imm64(16, mbox_addr));
    out.extend_from_slice(&insn(0xF9400000 | (16 << 5) | 17)); // ldr x17,[x16]
    out.extend_from_slice(&insn(0x91000400 | (17 << 5) | 17)); // add x17,x17,#1
    out.extend_from_slice(&insn(0xF9000000 | (16 << 5) | 17)); // str x17,[x16]
    out.extend_from_slice(stolen);
    out.extend_from_slice(&insn(0x58000051));                   // ldr x17,#8
    out.extend_from_slice(&insn(0xD61F0220));                   // br  x17
    out.extend_from_slice(&ret_abs.to_le_bytes());
    out
}

/// Build a cache flush cave: DC CVAU + DSB ISH + IC IVAU + DSB ISH + ISB
/// on addr_abs, then BR to orig_target.
pub fn build_flush_cave(addr_abs: u64, orig_target: u64) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&load_imm64(16, addr_abs));
    out.extend_from_slice(&insn(0xD50B7B20 | 16)); // dc cvau, x16
    out.extend_from_slice(&insn(0xD5033B9F));       // dsb ish
    out.extend_from_slice(&insn(0xD50B7520 | 16)); // ic ivau, x16
    out.extend_from_slice(&insn(0xD5033B9F));       // dsb ish
    out.extend_from_slice(&insn(0xD5033FDF));       // isb
    out.extend_from_slice(&insn(0x58000051));       // ldr x17,#8
    out.extend_from_slice(&insn(0xD61F0220));       // br  x17
    out.extend_from_slice(&orig_target.to_le_bytes());
    out
}

/// Build the command ring logger cave (cmdlog / cmdhook).
/// Logs this/cmd/vtable/[cmd+8..+32] into a ring at mbox_addr.
pub fn build_cmdlog_cave(mbox_addr: u64, stolen: &[u8; 16], ret_abs: u64) -> Vec<u8> {
    const N: u32 = 24;
    let mut out = Vec::new();
    out.extend_from_slice(&load_imm64(9, mbox_addr));              // x9 = mbox
    // cbz x1, skip (offset computed below - use 5 instructions * 4 = 20 bytes for branch)
    // We pre-encode with estimated offset and then fix up - or use a fixed large-enough stub
    // For simplicity, encode as in Python (fixed layout):
    out.extend_from_slice(&insn(0xB4000000 | ((84 / 4) << 5) | 1)); // cbz x1, +84
    out.extend_from_slice(&insn(0xB9400000 | (9 << 5) | 10));       // ldr w10,[x9]
    out.extend_from_slice(&insn(0x71000000 | (N << 10) | (10 << 5) | 31)); // cmp w10,#N
    out.extend_from_slice(&insn(0x54000000 | ((72 / 4) << 5) | 2)); // b.hs +72
    out.extend_from_slice(&insn(0xD3400000 | (58 << 16) | (57 << 10) | (10 << 5) | 11)); // lsl x11,x10,#6
    out.extend_from_slice(&insn(0x8B000000 | (11 << 16) | (9 << 5) | 11)); // add x11,x9,x11
    out.extend_from_slice(&insn(0x91000000 | (8 << 10) | (11 << 5) | 11)); // add x11,x11,#8
    out.extend_from_slice(&insn(0xF9000000 | (11 << 5) | 0));       // str x0,[x11]
    out.extend_from_slice(&insn(0xF9000000 | (1 << 10) | (11 << 5) | 1)); // str x1,[x11,#8]
    out.extend_from_slice(&insn(0xF9400000 | (1 << 5) | 12));       // ldr x12,[x1]
    out.extend_from_slice(&insn(0xF9000000 | (2 << 10) | (11 << 5) | 12)); // str x12,[x11,#16]
    out.extend_from_slice(&insn(0xF9400000 | (1 << 10) | (1 << 5) | 12)); // ldr x12,[x1,#8]
    out.extend_from_slice(&insn(0xF9000000 | (3 << 10) | (11 << 5) | 12)); // str x12,[x11,#24]
    out.extend_from_slice(&insn(0xF9400000 | (2 << 10) | (1 << 5) | 12)); // ldr x12,[x1,#16]
    out.extend_from_slice(&insn(0xF9000000 | (4 << 10) | (11 << 5) | 12)); // str x12,[x11,#32]
    out.extend_from_slice(&insn(0xF9400000 | (3 << 10) | (1 << 5) | 12)); // ldr x12,[x1,#24]
    out.extend_from_slice(&insn(0xF9000000 | (5 << 10) | (11 << 5) | 12)); // str x12,[x11,#40]
    out.extend_from_slice(&insn(0xF9400000 | (4 << 10) | (1 << 5) | 12)); // ldr x12,[x1,#32]
    out.extend_from_slice(&insn(0xF9000000 | (6 << 10) | (11 << 5) | 12)); // str x12,[x11,#48]
    out.extend_from_slice(&insn(0x11000400 | (10 << 5) | 10)); // add w10,w10,#1
    out.extend_from_slice(&insn(0xB9000000 | (9 << 5) | 10));  // str w10,[x9]
    out.extend_from_slice(stolen);                               // stolen (16 bytes)
    out.extend_from_slice(&insn(0x58000051));                   // ldr x17,#8
    out.extend_from_slice(&insn(0xD61F0220));                   // br  x17
    out.extend_from_slice(&ret_abs.to_le_bytes());
    out
}

/// Build the x0-x7 register logger cave (arghook).
pub fn build_arghook_cave(mbox_addr: u64, stolen: &[u8; 16], ret_abs: u64) -> Vec<u8> {
    const N: u32 = 24;
    let mut out = Vec::new();
    out.extend_from_slice(&load_imm64(9, mbox_addr));
    out.extend_from_slice(&insn(0xB9400000 | (9 << 5) | 10));       // ldr w10,[x9]
    out.extend_from_slice(&insn(0x71000000 | (N << 10) | (10 << 5) | 31)); // cmp w10,#N
    out.extend_from_slice(&insn(0x54000000 | ((40 / 4) << 5) | 2)); // b.hs +40
    out.extend_from_slice(&insn(0xD3400000 | (58 << 16) | (57 << 10) | (10 << 5) | 11)); // lsl x11,x10,#6
    out.extend_from_slice(&insn(0x8B000000 | (11 << 16) | (9 << 5) | 11)); // add x11,x9,x11
    out.extend_from_slice(&insn(0x91000000 | (8 << 10) | (11 << 5) | 11)); // add x11,x11,#8
    out.extend_from_slice(&insn(0xA9000000 | (1 << 10) | (11 << 5) | 0)); // stp x0,x1,[x11]
    out.extend_from_slice(&insn(0xA9000000 | (2 << 15) | (3 << 10) | (11 << 5) | 2)); // stp x2,x3,[x11,#16]
    out.extend_from_slice(&insn(0xA9000000 | (4 << 15) | (5 << 10) | (11 << 5) | 4)); // stp x4,x5,[x11,#32]
    out.extend_from_slice(&insn(0xA9000000 | (6 << 15) | (7 << 10) | (11 << 5) | 6)); // stp x6,x7,[x11,#48]
    out.extend_from_slice(&insn(0x11000400 | (10 << 5) | 10)); // add w10,w10,#1
    out.extend_from_slice(&insn(0xB9000000 | (9 << 5) | 10));  // str w10,[x9]
    out.extend_from_slice(stolen);
    out.extend_from_slice(&insn(0x58000051));
    out.extend_from_slice(&insn(0xD61F0220));
    out.extend_from_slice(&ret_abs.to_le_bytes());
    out
}

/// Check whether 16 bytes of code are position-independent (safe to relocate).
/// Used to verify the stolen tick prologue is safe to copy to a cave.
/// Mirrors MStarConsole._stolen_is_pi() logic.
pub fn stolen_is_pi(stolen: &[u8]) -> bool {
    if stolen.len() < 16 { return false; }
    for i in (0..16).step_by(4) {
        let w = u32::from_le_bytes(stolen[i..i+4].try_into().unwrap());
        // Reject PC-relative loads/branches that embed offsets
        let op = (w >> 24) & 0xFF;
        if matches!(op, 0x10 | 0x30 | 0x14 | 0x17 | 0x94 | 0x97) { return false; }
        if (w & 0x9F00_0000) == 0x1000_0000 { return false; } // ADR/ADRP
        if (w & 0xFC00_0000) == 0x14000000  { return false; } // B
        if (w & 0xFC00_0000) == 0x94000000  { return false; } // BL
        if (w & 0xFF00_0010) == 0x54000000  { return false; } // B.cond
        if (w & 0x7E00_0000) == 0x34000000  { return false; } // CBZ/CBNZ
        if (w & 0x7E00_0000) == 0x36000000  { return false; } // TBZ/TBNZ
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nop_patch_8_bytes() {
        let p = nop_patch(8);
        assert_eq!(p.len(), 8);
        assert_eq!(&p[..4], &NOP.to_le_bytes());
        assert_eq!(&p[4..], &NOP.to_le_bytes());
    }

    #[test]
    fn load_imm64_length() {
        let out = load_imm64(0, 0x1234_5678_9ABC_DEF0);
        assert_eq!(out.len(), 16);
    }

    #[test]
    fn far_branch_length() {
        let b = far_branch_bytes(0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(b.len(), 16);
    }
}
