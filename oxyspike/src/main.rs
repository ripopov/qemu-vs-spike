use oxyspike::csr;
use oxyspike::{
    Cpu, elf,
    memory::{Memory, RAM_BASE},
};
use std::{
    env, fs,
    io::{self, Read, Write},
    process::ExitCode,
    time::Instant,
};
const RAM_SIZE: usize = 256 * 1024 * 1024;
const DTB_ADDRESS: u64 = RAM_BASE + 0x0fe0_0000;
// Tick on instruction attempts, including traps, before executing the next step.
const ATTEMPTS_PER_TICK: u32 = 100;
const HTIF_CONSOLE_READ: u64 = 0x100;
const HTIF_CONSOLE_WRITE: u64 = 0x101;
const HTIF_INPUT_RESPONSE: u64 = 0x0100_0000_0000_0100;

fn run() -> Result<u8, String> {
    let mut args = env::args_os().skip(1);
    let mut path = None;
    let mut limit = u64::MAX;
    let mut dtb = None;
    let mut trace = false;
    let mut dump = None;
    let mut options = true;
    while let Some(arg) = args.next() {
        if !options {
            if path.replace(arg).is_some() {
                return Err("Only one ELF is supported".into());
            }
            continue;
        }
        match arg.to_str() {
            Some("--dtb") => dtb = Some(args.next().ok_or("Missing DTB path")?),
            Some("--dump-memory") => dump = Some(args.next().ok_or("Missing dump path")?),
            Some("--trace-traps") => trace = true,
            Some("--max-instructions") => {
                limit = args
                    .next()
                    .ok_or("Missing instruction limit")?
                    .to_str()
                    .ok_or("Invalid instruction limit")?
                    .parse()
                    .map_err(|_| "Invalid instruction limit")?
            }
            Some("--") => options = false,
            Some("--help" | "-h") => {
                println!(
                    "OxySpike: oxyspike [--dtb FILE] [--max-instructions N] [--trace-traps] [--dump-memory FILE] [--] program.elf"
                );
                return Ok(0);
            }
            _ if arg.as_encoded_bytes().starts_with(b"-") => {
                return Err(format!("Unknown option {}", arg.to_string_lossy()));
            }
            _ => {
                if path.replace(arg).is_some() {
                    return Err("Only one ELF is supported".into());
                }
            }
        }
    }
    let path = path.ok_or("Usage: oxyspike [--max-instructions N] program.elf")?;
    let mut mem = Memory::new(RAM_SIZE);
    let elf = elf::load(&fs::read(&path).map_err(|e| e.to_string())?, &mut mem)?;
    mem.tohost = elf.tohost.unwrap_or(u64::MAX);
    mem.htif_pending = elf.tohost.is_some();
    let mut cpu = Cpu::new(mem, elf.entry);
    cpu.x[2] = RAM_BASE + RAM_SIZE as u64 - 16;
    if let Some(path) = dtb {
        let addr = DTB_ADDRESS;
        cpu.memory
            .copy_in(addr, &fs::read(path).map_err(|e| e.to_string())?)?;
        cpu.x[11] = addr;
    }
    let (input_tx, input_rx) = std::sync::mpsc::sync_channel(1024);
    std::thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut byte = [0];
        while stdin.read(&mut byte).ok() == Some(1) {
            if input_tx.send(byte[0]).is_err() {
                break;
            }
        }
    });
    let mut pending_reads = 0usize;
    let start = Instant::now();
    let mut attempts = 0u64;
    let mut until_tick = ATTEMPTS_PER_TICK;
    while attempts < limit {
        attempts += 1;
        until_tick -= 1;
        let tick = until_tick == 0;
        if tick {
            until_tick = ATTEMPTS_PER_TICK;
            cpu.memory.advance_time(1);
        }
        if !cpu.poll_interrupt()
            && let Err(e) = cpu.step()
        {
            if trace {
                eprintln!(
                    "trap pc={:#x} priv={} cause={} value={:#x}",
                    cpu.pc, cpu.privilege, e.cause, e.value
                );
            }
            if cpu.csrs[csr::MTVEC] == 0 {
                return Err(format!("Trap at PC {:#x}: {e:?}", cpu.pc));
            }
            cpu.take_trap(e);
        }
        if pending_reads != 0
            && tick
            && let Some(from) = elf.fromhost
            && cpu.memory.load(from, 8).map_err(|e| format!("{e:?}"))? == 0
            && let Ok(byte) = input_rx.try_recv()
        {
            cpu.memory
                .store(from, 8, HTIF_INPUT_RESPONSE | byte as u64)
                .map_err(|e| format!("{e:?}"))?;
            pending_reads -= 1;
        }
        if cpu.memory.htif_pending
            && let Some(addr) = elf.tohost
        {
            let req = cpu.memory.load(addr, 8).map_err(|e| format!("{e:?}"))?;
            cpu.memory.htif_pending = false;
            if req != 0 {
                cpu.memory.store(addr, 8, 0).map_err(|e| format!("{e:?}"))?;
                cpu.memory.htif_pending = false;
                if req >> 48 == HTIF_CONSOLE_WRITE {
                    io::stdout()
                        .write_all(&[req as u8])
                        .map_err(|e| e.to_string())?;
                    io::stdout().flush().map_err(|e| e.to_string())?;
                } else if req >> 48 == HTIF_CONSOLE_READ {
                    pending_reads = pending_reads
                        .checked_add(1)
                        .ok_or("Too many HTIF input requests")?;
                } else if req >> 48 == 0 && req & 1 != 0 {
                    eprintln!(
                        "OxySpike: instret={} in {:.6}s",
                        cpu.retired,
                        start.elapsed().as_secs_f64()
                    );
                    #[cfg(feature = "dispatch-profile")]
                    eprintln!("OXY_DISPATCH_PROFILE {}", cpu.dispatch_profile_json());
                    return Ok(((req >> 1) & 255) as u8);
                } else {
                    return Err(format!("Unsupported HTIF request {req:#x}"));
                }
            }
        }
    }

    #[cfg(feature = "dispatch-profile")]
    eprintln!("OXY_DISPATCH_PROFILE {}", cpu.dispatch_profile_json());
    if let Some(path) = dump {
        fs::write(path, &cpu.memory.ram).map_err(|e| e.to_string())?;
    }
    Err(format!(
        "Instruction limit reached at PC {:#x} after {} instructions (ra={:#x}, sp={:#x}, a0={:#x}, time={})",
        cpu.pc, cpu.retired, cpu.x[1], cpu.x[2], cpu.x[10], cpu.memory.mtime
    ))
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("oxyspike: {e}");
            ExitCode::FAILURE
        }
    }
}
