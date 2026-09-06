// ============================================================================
//  HAY STAR - Minimal ELF64 .dynsym parser
//  Finds nx_mailbox / nx_on_tick RVAs without an external tool.
//  Mirrors MStarConsole._elf_dynsyms() in loader.py.
// ============================================================================

use std::collections::HashMap;

fn u16_le(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(data[off..off+2].try_into().unwrap_or([0;2]))
}
fn u32_le(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(data[off..off+4].try_into().unwrap_or([0;4]))
}
fn u64_le(data: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(data[off..off+8].try_into().unwrap_or([0;8]))
}

/// Parse the .dynsym section of an ELF64 binary.
/// Returns a map of symbol name -> value (RVA).
pub fn parse_dynsyms(data: &[u8]) -> HashMap<String, u64> {
    let mut result = HashMap::new();

    // Verify ELF magic
    if data.len() < 0x40 || &data[..4] != b"\x7fELF" {
        return result;
    }

    // ELF64 header fields
    let e_shoff     = u64_le(data, 0x28) as usize;
    let e_shentsize = u16_le(data, 0x3a) as usize;
    let e_shnum     = u16_le(data, 0x3c) as usize;

    if e_shentsize == 0 || e_shoff == 0 {
        return result;
    }

    // Collect all section headers
    struct Shdr {
        sh_type:    u32,
        sh_offset:  usize,
        sh_size:    usize,
        sh_link:    u32,
        sh_entsize: usize,
    }

    let mut shdrs: Vec<Shdr> = Vec::with_capacity(e_shnum);
    for i in 0..e_shnum {
        let o = e_shoff + i * e_shentsize;
        if o + e_shentsize > data.len() { break; }
        shdrs.push(Shdr {
            sh_type:    u32_le(data, o + 4),
            sh_offset:  u64_le(data, o + 0x18) as usize,
            sh_size:    u64_le(data, o + 0x20) as usize,
            sh_link:    u32_le(data, o + 0x28),
            sh_entsize: u64_le(data, o + 0x38) as usize,
        });
    }

    // Find SHT_DYNSYM (type = 11)
    let dynsym = shdrs.iter().find(|s| s.sh_type == 11);
    let dynsym = match dynsym {
        Some(s) => s,
        None    => return result,
    };

    let link = dynsym.sh_link as usize;
    if link >= shdrs.len() { return result; }
    let dynstr = &shdrs[link];

    let sym_off  = dynsym.sh_offset;
    let sym_size = dynsym.sh_size;
    let sym_ent  = if dynsym.sh_entsize > 0 { dynsym.sh_entsize } else { 24 };
    let str_off  = dynstr.sh_offset;
    let str_end  = str_off + dynstr.sh_size;

    if str_end > data.len() || sym_off + sym_size > data.len() {
        return result;
    }

    // ELF64 Sym: st_name(4), st_info(1), st_other(1), st_shndx(2), st_value(8), st_size(8)
    let count = sym_size / sym_ent;
    for i in 0..count {
        let e = sym_off + i * sym_ent;
        if e + sym_ent > data.len() { break; }
        let st_name  = u32_le(data, e) as usize;
        let st_value = u64_le(data, e + 8);
        // Read null-terminated name from dynstr
        let name_off = str_off + st_name;
        if name_off >= data.len() { continue; }
        let end = data[name_off..].iter().position(|&b| b == 0)
            .map(|p| name_off + p)
            .unwrap_or(data.len());
        let name = String::from_utf8_lossy(&data[name_off..end]).to_string();
        if !name.is_empty() {
            result.insert(name, st_value);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_data_returns_empty() {
        assert!(parse_dynsyms(&[]).is_empty());
    }

    #[test]
    fn non_elf_returns_empty() {
        assert!(parse_dynsyms(b"NOT_AN_ELF_FILE").is_empty());
    }
}
