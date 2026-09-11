//! Architectural encodings shared by the CPU and its host-facing API.
//!
//! Raw integer API fields remain compatible with existing simulator clients.

pub mod privilege {
    pub const USER: u8 = 0;
    pub const SUPERVISOR: u8 = 1;
    pub const MACHINE: u8 = 3;
}

/// Access kinds accepted by `Cpu::translate` and `Cpu::load_virtual`.
pub mod access {
    pub const FETCH: u8 = 0;
    pub const LOAD: u8 = 1;
    pub const STORE: u8 = 2;
}

pub mod exception {
    pub const INSTRUCTION_MISALIGNED: u64 = 0;
    pub const INSTRUCTION_ACCESS: u64 = 1;
    pub const ILLEGAL_INSTRUCTION: u64 = 2;
    pub const BREAKPOINT: u64 = 3;
    pub const LOAD_MISALIGNED: u64 = 4;
    pub const LOAD_ACCESS: u64 = 5;
    pub const STORE_MISALIGNED: u64 = 6;
    pub const STORE_ACCESS: u64 = 7;
    pub const ECALL_USER: u64 = 8;
    pub const INSTRUCTION_PAGE: u64 = 12;
    pub const LOAD_PAGE: u64 = 13;
    pub const STORE_PAGE: u64 = 15;
}
