//! WebAssembly entry points for the browser playground. No wasm-bindgen: the page
//! writes the program into memory from `lugh_alloc`, calls `lugh_run`, and reads
//! the output back with `lugh_output_ptr` / `lugh_output_len`.

use std::cell::RefCell;

thread_local! {
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn lugh_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// # Safety
/// `ptr` and `len` must come from a single `lugh_alloc` call.
#[no_mangle]
pub unsafe extern "C" fn lugh_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len));
}

/// mode: 0 = run, 1 = test, 2 = check. Returns the output length in bytes.
///
/// # Safety
/// `ptr` must point to `len` initialised bytes of UTF-8.
#[no_mangle]
pub unsafe extern "C" fn lugh_run(ptr: *const u8, len: usize, mode: u32) -> usize {
    let src = String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned();
    let mode = match mode {
        1 => "test",
        2 => "check",
        _ => "run",
    };
    let out = crate::playground::run(&src, mode).into_bytes();
    let n = out.len();
    OUTPUT.with(|o| *o.borrow_mut() = out);
    n
}

#[no_mangle]
pub extern "C" fn lugh_output_ptr() -> *const u8 {
    OUTPUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn lugh_version_len() -> usize {
    env!("CARGO_PKG_VERSION").len()
}
