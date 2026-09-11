//! Emit the actual compressed decoder's expansion for every 16-bit encoding.
#[path = "../src/compressed.rs"]
mod compressed;
fn sext(v: u64, bits: u32) -> u64 {
    ((v << (64 - bits)) as i64 >> (64 - bits)) as u64
}
fn main() {
    for c in 0..=u16::MAX {
        match compressed::expand(c) {
            Some(i) => println!("{c:04x} {i:08x}"),
            None => println!("{c:04x} -"),
        }
    }
}
