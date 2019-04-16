//! Allocation wrappers.
use std::alloc::{alloc, dealloc, Layout};

#[no_mangle]
pub fn secp256k1_alloc(size: usize) -> *mut u8 {
    unsafe { alloc(Layout::from_size_align_unchecked(size, 1)) }
}

#[no_mangle]
pub fn secp256k1_dealloc(ptr: *mut u8, size: usize) {
    unsafe { dealloc(ptr, Layout::from_size_align_unchecked(size, 1)) }
}
