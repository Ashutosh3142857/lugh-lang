//! Time and output that work both natively and in the browser (WebAssembly).
//!
//! On wasm32 the host page provides `lugh_now_ms` (Date.now()), because
//! std::time is not available in the browser.

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    // Supplied by the host page as `env.lugh_now_ms` (Date.now()).
    fn lugh_now_ms() -> f64;
}

/// Seconds since the Unix epoch.
pub fn now() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        // SAFETY: a plain function import with no arguments, supplied by the host page.
        unsafe { lugh_now_ms() / 1000.0 }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
}
