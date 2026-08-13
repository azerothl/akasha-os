//! Contrat IPC seL4 (phase PV) — miroir de `vm/sel4/abi.h`.
//!
//! `no_std` : le même ABI sera lié dans les PDs Rust. Les opcodes doivent
//! rester alignés sur le header C tant que capkd/bus sont en C.

#![no_std]

pub const OP_PING: u64 = 1;
pub const OP_MINT: u64 = 2;
pub const OP_CHECK: u64 = 3;
pub const OP_REVOKE: u64 = 4;
pub const OP_LOOKUP: u64 = 5;

pub const OK: u64 = 0;
pub const DENIED: u64 = 1;
pub const BAD_OP: u64 = 2;
pub const FULL: u64 = 3;

pub const READ: u32 = 1;
pub const WRITE: u32 = 2;
pub const EXECUTE: u32 = 4;
pub const GRANT: u32 = 8;
pub const REVOKE: u32 = 16;

pub const OBJ_WORDS: usize = 4;
pub const OBJ_BYTES: usize = OBJ_WORDS * 8;
pub const MAX_CAPS: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligne_sur_abi_h() {
        let h = include_str!("../../../vm/sel4/abi.h");
        assert!(h.contains("AOS_OP_PING = 1"));
        assert!(h.contains("AOS_OP_MINT = 2"));
        assert!(h.contains("AOS_OP_CHECK = 3"));
        assert!(h.contains("AOS_OP_REVOKE = 4"));
        assert!(h.contains("AOS_OP_LOOKUP = 5"));
        assert!(h.contains("AOS_READ = 1"));
        assert!(h.contains("AOS_REVOKE = 16"));
        assert_eq!(READ, aos_caps::Rights::READ.bits());
        assert_eq!(WRITE, aos_caps::Rights::WRITE.bits());
        assert_eq!(EXECUTE, aos_caps::Rights::EXECUTE.bits());
        assert_eq!(GRANT, aos_caps::Rights::GRANT.bits());
        assert_eq!(REVOKE, aos_caps::Rights::REVOKE.bits());
    }
}
