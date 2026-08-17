//! CapStore dans l'invité seL4 : ABI C appelée par le PD `capkd` (glue Microkit).
//!
//! Microkit n'a pas de malloc : allocateur bump + `panic=abort` uniquement
//! pour `aarch64-unknown-none`. L'hôte (`cargo test`) utilise le heap std.

#![cfg_attr(target_os = "none", no_std)]

extern crate alloc;

use alloc::string::String;
use aos_caps::{object_matches, CapId, CapStore, HolderId, Rights};
use core::cell::UnsafeCell;
use core::ffi::{c_char, CStr};

#[cfg(target_os = "none")]
mod bump {
    use core::alloc::{GlobalAlloc, Layout};
    use core::cell::UnsafeCell;
    use core::ptr::null_mut;

    const HEAP: usize = 128 * 1024;

    struct Bump {
        buf: UnsafeCell<[u8; HEAP]>,
        pos: UnsafeCell<usize>,
    }

    unsafe impl Sync for Bump {}

    #[global_allocator]
    static ALLOC: Bump = Bump {
        buf: UnsafeCell::new([0; HEAP]),
        pos: UnsafeCell::new(0),
    };

    unsafe impl GlobalAlloc for Bump {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let pos = &mut *self.pos.get();
            let align = layout.align().max(1);
            let aligned = (*pos + align - 1) & !(align - 1);
            let end = match aligned.checked_add(layout.size()) {
                Some(e) if e <= HEAP => e,
                _ => return null_mut(),
            };
            let ptr = (*self.buf.get()).as_mut_ptr().add(aligned);
            *pos = end;
            ptr
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
    }
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}

struct StoreCell(UnsafeCell<Option<CapStore>>);

unsafe impl Sync for StoreCell {}

static STORE: StoreCell = StoreCell(UnsafeCell::new(None));

fn store() -> &'static mut CapStore {
    unsafe {
        let slot = &mut *STORE.0.get();
        slot.get_or_insert_with(CapStore::new)
    }
}

fn cstr(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) };
    s.to_str().ok().map(String::from)
}

/// Réinitialise le magasin (appelé depuis `init` du PD).
#[no_mangle]
pub extern "C" fn aos_cap_init() {
    unsafe {
        *STORE.0.get() = Some(CapStore::new());
    }
}

/// Mint. Retourne l'id (>0) ou 0 si plein / objet invalide.
#[no_mangle]
pub extern "C" fn aos_cap_mint(holder: u64, object: *const c_char, rights: u32) -> u64 {
    let Some(obj) = cstr(object) else {
        return 0;
    };
    let s = store();
    if s.len() >= aos_sel4_abi::MAX_CAPS {
        return 0;
    }
    let r = Rights::from_bits_truncate(rights) | Rights::REVOKE;
    s.mint(HolderId(holder), obj, r, None, None, 0).0
}

/// Check. 0 = OK, 1 = DENIED (aligné sur `AOS_OK` / `AOS_DENIED`).
#[no_mangle]
pub extern "C" fn aos_cap_check(
    holder: u64,
    cap: u64,
    rights: u32,
    object: *const c_char,
) -> i32 {
    let Some(obj) = cstr(object) else {
        return aos_sel4_abi::DENIED as i32;
    };
    let required = Rights::from_bits_truncate(rights);
    match store().authorize(HolderId(holder), CapId(cap), required) {
        Ok(grant) if object_matches(&grant.object, &obj) => aos_sel4_abi::OK as i32,
        _ => aos_sel4_abi::DENIED as i32,
    }
}

/// Revoke unitaire. 0 = OK, 1 = DENIED.
#[no_mangle]
pub extern "C" fn aos_cap_revoke(holder: u64, cap: u64) -> i32 {
    match store().revoke(HolderId(holder), CapId(cap)) {
        Ok(()) => aos_sel4_abi::OK as i32,
        Err(_) => aos_sel4_abi::DENIED as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::CStr;

    fn obj(s: &str) -> *const c_char {
        CStr::from_bytes_with_nul(s.as_bytes())
            .unwrap()
            .as_ptr()
    }

    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn mint_check_revoke_via_abi_c() {
        let _guard = TEST_LOCK.lock().unwrap();
        aos_cap_init();
        let path = obj("fs:/p4/gate.md\0");
        let id = aos_cap_mint(1, path, aos_sel4_abi::READ | aos_sel4_abi::WRITE);
        assert_ne!(id, 0);
        assert_eq!(aos_cap_check(1, id, aos_sel4_abi::READ, path), 0);
        assert_eq!(aos_cap_check(2, id, aos_sel4_abi::READ, path), 1);
        assert_eq!(aos_cap_revoke(1, id), 0);
        assert_eq!(aos_cap_check(1, id, aos_sel4_abi::READ, path), 1);
    }

    #[test]
    fn mint_refuse_au_plafond() {
        let _guard = TEST_LOCK.lock().unwrap();
        aos_cap_init();
        let path = obj("fs:/x\0");
        for _ in 0..aos_sel4_abi::MAX_CAPS {
            assert_ne!(aos_cap_mint(1, path, aos_sel4_abi::READ), 0);
        }
        assert_eq!(aos_cap_mint(1, path, aos_sel4_abi::READ), 0);
    }
}
