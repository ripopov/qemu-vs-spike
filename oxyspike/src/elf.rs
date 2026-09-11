//! Load little-endian ELF64 segments and locate the HTIF symbols.

use crate::memory::Memory;
const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const SECTION_HEADER_SIZE: usize = 64;
const SYMBOL_SIZE: usize = 24;
const EM_RISCV: u64 = 243;
const PT_LOAD: u64 = 1;
const SHT_SYMTAB: u64 = 2;

pub struct Elf {
    pub entry: u64,
    pub tohost: Option<u64>,
    pub fromhost: Option<u64>,
}

fn read_le(bytes: &[u8], offset: usize, width: usize) -> Result<u64, String> {
    let field = bytes
        .get(offset..offset.checked_add(width).ok_or("ELF offset overflow")?)
        .ok_or("Truncated ELF")?;
    let mut value = [0; 8];
    value[..width].copy_from_slice(field);
    Ok(u64::from_le_bytes(value))
}

fn host_size(value: u64) -> Result<usize, String> {
    usize::try_from(value).map_err(|_| "ELF size exceeds host address space".into())
}

fn range(bytes: &[u8], offset: u64, size: u64) -> Result<&[u8], String> {
    let offset = host_size(offset)?;
    let size = host_size(size)?;
    bytes
        .get(offset..offset.checked_add(size).ok_or("ELF offset overflow")?)
        .ok_or_else(|| "Truncated ELF range".into())
}

fn table(
    bytes: &[u8],
    offset: u64,
    stride: usize,
    count: usize,
    minimum: usize,
) -> Result<&[u8], String> {
    if count == 0 {
        return Ok(&[]);
    }
    if stride < minimum {
        return Err("Invalid ELF table entry size".into());
    }
    let size = stride.checked_mul(count).ok_or("ELF table size overflow")?;
    range(bytes, offset, size as u64)
}

pub fn load(bytes: &[u8], memory: &mut Memory) -> Result<Elf, String> {
    let header = bytes.get(..ELF_HEADER_SIZE).ok_or("Truncated ELF header")?;
    if header.get(..6) != Some(b"\x7fELF\x02\x01") || read_le(header, 18, 2)? != EM_RISCV {
        return Err("Expected little-endian ELF64 RISC-V executable".into());
    }
    let mut elf = Elf {
        entry: read_le(header, 24, 8)?,
        tohost: None,
        fromhost: None,
    };
    let program_stride = read_le(header, 54, 2)? as usize;
    let program_count = read_le(header, 56, 2)? as usize;
    let programs = table(
        bytes,
        read_le(header, 32, 8)?,
        program_stride,
        program_count,
        PROGRAM_HEADER_SIZE,
    )?;
    for program in programs.chunks(program_stride.max(1)) {
        if read_le(program, 0, 4)? != PT_LOAD {
            continue;
        }
        let addr = read_le(program, 24, 8)?;
        let size = read_le(program, 32, 8)?;
        let memory_size = read_le(program, 40, 8)?;
        if memory_size < size {
            return Err("ELF filesz exceeds memsz".into());
        }
        let segment = range(bytes, read_le(program, 8, 8)?, size)?;
        memory.copy_in(addr, segment)?;
        // Explicitly clear overlapping BSS, even when RAM is already zeroed.
        if memory_size > size {
            let start = addr.checked_add(size).ok_or("ELF address overflow")?;
            memory.zero_range(start, host_size(memory_size - size)?)?;
        }
    }
    let section_stride = read_le(header, 58, 2)? as usize;
    let section_count = read_le(header, 60, 2)? as usize;
    let sections = table(
        bytes,
        read_le(header, 40, 8)?,
        section_stride,
        section_count,
        SECTION_HEADER_SIZE,
    )?;
    for section in sections.chunks(section_stride.max(1)) {
        if read_le(section, 4, 4)? != SHT_SYMTAB {
            continue;
        }
        let symbols = range(bytes, read_le(section, 24, 8)?, read_le(section, 32, 8)?)?;
        let symbol_stride = host_size(read_le(section, 56, 8)?)?;
        if symbol_stride < SYMBOL_SIZE || symbols.len() % symbol_stride != 0 {
            return Err("Invalid symbol entry size".into());
        }
        let link = read_le(section, 40, 4)? as usize;
        if link >= section_count {
            return Err("Invalid string table index".into());
        }
        let string_header = &sections[link * section_stride..(link + 1) * section_stride];
        let strings = range(
            bytes,
            read_le(string_header, 24, 8)?,
            read_le(string_header, 32, 8)?,
        )?;
        for symbol in symbols.chunks(symbol_stride) {
            let name_offset = read_le(symbol, 0, 4)? as usize;
            let name = strings.get(name_offset..).ok_or("Bad symbol name")?;
            let name = &name[..name
                .iter()
                .position(|&c| c == 0)
                .ok_or("Unterminated symbol")?];
            match name {
                b"tohost" => elf.tohost = Some(read_le(symbol, 8, 8)?),
                b"fromhost" => elf.fromhost = Some(read_le(symbol, 8, 8)?),
                _ => {}
            }
        }
    }
    Ok(elf)
}
