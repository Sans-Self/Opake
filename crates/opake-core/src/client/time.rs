// Platform-agnostic Unix timestamp.
//
// Native: std::time::SystemTime. WASM: js_sys::Date.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before UNIX epoch")
        .as_secs() as i64
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn unix_now() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}
