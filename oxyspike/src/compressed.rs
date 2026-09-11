// Immediate layouts follow riscv/decode.h in the C++ Spike reference.
fn it(op: u32, rd: u32, f: u32, rs: u32, imm: u32) -> u32 {
    ((imm & 4095) << 20) | (rs << 15) | (f << 12) | (rd << 7) | op
}

fn rt(rd: u32, f: u32, a: u32, b: u32, hi: u32, word: bool) -> u32 {
    (hi << 25) | (b << 20) | (a << 15) | (f << 12) | (rd << 7) | if word { 0x3b } else { 0x33 }
}

fn st(f: u32, a: u32, b: u32, imm: u32) -> u32 {
    ((imm >> 5) << 25) | (b << 20) | (a << 15) | (f << 12) | ((imm & 31) << 7) | 0x23
}

pub fn expand(c: u16) -> Option<u32> {
    let c = c as u32;
    let x = |s: u32, n: u32| (c >> s) & ((1 << n) - 1);
    let rd = x(7, 5);
    let rs2 = x(2, 5);
    let rdp = 8 + x(2, 3);
    let rsp = 8 + x(7, 3);
    let zimm = x(2, 5) | (x(12, 1) << 5);
    let imm = super::sext(zimm as u64, 6) as u32;
    Some(match (c & 3, c >> 13) {
        (0, 0) => {
            let n = (x(6, 1) << 2) | (x(5, 1) << 3) | (x(11, 2) << 4) | (x(7, 4) << 6);
            if n == 0 {
                return None;
            }
            it(0x13, rdp, 0, 2, n)
        }
        (0, 2) => it(
            3,
            rdp,
            2,
            rsp,
            (x(6, 1) << 2) | (x(10, 3) << 3) | (x(5, 1) << 6),
        ),
        (0, 1) => it(7, rdp, 3, rsp, (x(10, 3) << 3) | (x(5, 2) << 6)),
        (0, 5) => (st(3, rsp, rdp, (x(10, 3) << 3) | (x(5, 2) << 6)) & !127) | 0x27,
        (0, 3) => it(3, rdp, 3, rsp, (x(10, 3) << 3) | (x(5, 2) << 6)),
        (0, 6) => st(
            2,
            rsp,
            rdp,
            (x(6, 1) << 2) | (x(10, 3) << 3) | (x(5, 1) << 6),
        ),
        (0, 7) => st(3, rsp, rdp, (x(10, 3) << 3) | (x(5, 2) << 6)),
        (1, 0) => it(0x13, rd, 0, rd, imm),
        (1, 1) if rd != 0 => it(0x1b, rd, 0, rd, imm),
        (1, 2) => it(0x13, rd, 0, 0, imm),
        (1, 3) if rd == 2 => {
            let n =
                (x(6, 1) << 4) | (x(2, 1) << 5) | (x(5, 1) << 6) | (x(3, 2) << 7) | (x(12, 1) << 9);
            if n == 0 {
                return None;
            }
            it(0x13, 2, 0, 2, super::sext(n as u64, 10) as u32)
        }
        (1, 3) if zimm != 0 => (imm << 12) | (rd << 7) | 0x37,
        (1, 4) => match x(10, 2) {
            0 => it(0x13, rsp, 5, rsp, zimm),
            1 => it(0x13, rsp, 5, rsp, zimm | 0x400),
            2 => it(0x13, rsp, 7, rsp, imm),
            _ => match (x(12, 1), x(5, 2)) {
                (0, 0) => rt(rsp, 0, rsp, rdp, 32, false),
                (0, 1) => rt(rsp, 4, rsp, rdp, 0, false),
                (0, 2) => rt(rsp, 6, rsp, rdp, 0, false),
                (0, 3) => rt(rsp, 7, rsp, rdp, 0, false),
                (1, 0) => rt(rsp, 0, rsp, rdp, 32, true),
                (1, 1) => rt(rsp, 0, rsp, rdp, 0, true),
                _ => return None,
            },
        },
        (1, 5) => {
            let n = (x(3, 3) << 1)
                | (x(11, 1) << 4)
                | (x(2, 1) << 5)
                | (x(7, 1) << 6)
                | (x(6, 1) << 7)
                | (x(9, 2) << 8)
                | (x(8, 1) << 10)
                | (x(12, 1) << 11);
            let n = super::sext(n as u64, 12) as u32;
            ((n & 0x100000) << 11) | ((n & 0x7fe) << 20) | ((n & 0x800) << 9) | (n & 0xff000) | 0x6f
        }
        (1, 6) | (1, 7) => {
            let n = (x(3, 2) << 1)
                | (x(10, 2) << 3)
                | (x(2, 1) << 5)
                | (x(5, 2) << 6)
                | (x(12, 1) << 8);
            let n = super::sext(n as u64, 9) as u32;
            ((n & 0x1000) << 19)
                | ((n & 0x7e0) << 20)
                | (rsp << 15)
                | (x(13, 1) << 12)
                | ((n & 30) << 7)
                | ((n & 0x800) >> 4)
                | 0x63
        }
        (2, 0) => it(0x13, rd, 1, rd, zimm),
        (2, 2) if rd != 0 => it(
            3,
            rd,
            2,
            2,
            (x(4, 3) << 2) | (x(12, 1) << 5) | (x(2, 2) << 6),
        ),
        (2, 1) => it(
            7,
            rd,
            3,
            2,
            (x(5, 2) << 3) | (x(12, 1) << 5) | (x(2, 3) << 6),
        ),
        (2, 5) => (st(3, 2, rs2, (x(10, 3) << 3) | (x(7, 3) << 6)) & !127) | 0x27,
        (2, 3) if rd != 0 => it(
            3,
            rd,
            3,
            2,
            (x(5, 2) << 3) | (x(12, 1) << 5) | (x(2, 3) << 6),
        ),
        (2, 4) => match (x(12, 1), rs2, rd) {
            (0, 0, 0) => return None,
            (0, 0, _) => it(0x67, 0, 0, rd, 0),
            (0, _, _) => rt(rd, 0, 0, rs2, 0, false),
            (1, 0, 0) => 0x00100073,
            (1, 0, _) => it(0x67, 1, 0, rd, 0),
            _ => rt(rd, 0, rd, rs2, 0, false),
        },
        (2, 6) => st(2, 2, rs2, (x(9, 4) << 2) | (x(7, 2) << 6)),
        (2, 7) => st(3, 2, rs2, (x(10, 3) << 3) | (x(7, 3) << 6)),
        _ => return None,
    })
}
