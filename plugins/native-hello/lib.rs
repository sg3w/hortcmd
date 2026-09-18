//! Dependency-free example. No Rust allocation or unwinding crosses the C ABI.
use std::sync::atomic::{AtomicPtr, Ordering};
#[repr(C)]
pub struct HostApi {
    abi_version: u32,
    struct_size: usize,
    request: unsafe extern "C" fn(*const u8, usize, *mut u8, usize) -> isize,
}
#[repr(C)]
pub struct PluginApi {
    abi_version: u32,
    struct_size: usize,
    initialize: unsafe extern "C" fn(*const HostApi) -> i32,
    event: unsafe extern "C" fn(*const u8, usize) -> i32,
    shutdown: unsafe extern "C" fn(),
}
static HOST: AtomicPtr<HostApi> = AtomicPtr::new(std::ptr::null_mut());
fn request(json: &[u8], expected: &str) -> i32 {
    let host = HOST.load(Ordering::Acquire);
    if host.is_null() {
        return 1;
    }
    let mut output = [0_u8; 4096];
    // The helper keeps the validated host table alive until shutdown returns.
    let length =
        unsafe { ((*host).request)(json.as_ptr(), json.len(), output.as_mut_ptr(), output.len()) };
    if length < 0 || length as usize > output.len() {
        return 1;
    }
    match std::str::from_utf8(&output[..length as usize]) {
        Ok(reply) if reply == expected => 0,
        _ => 1,
    }
}
unsafe extern "C" fn initialize(host: *const HostApi) -> i32 {
    if host.is_null() {
        return 1;
    }
    let api = unsafe { &*host };
    if api.abi_version != 1 || api.struct_size < std::mem::size_of::<HostApi>() {
        return 1;
    }
    HOST.store(host.cast_mut(), Ordering::Release);
    request(br#"{"type":"ping"}"#, r#"{"value":"pong"}"#)
}
unsafe extern "C" fn event(bytes: *const u8, length: usize) -> i32 {
    if bytes.is_null() || length > 1024 * 1024 {
        return 1;
    }
    let input = unsafe { std::slice::from_raw_parts(bytes, length) };
    if let Ok(text) = std::str::from_utf8(input) {
        if text.contains(r#""type":"command""#) && text.contains(r#""command":"hello""#) {
            return request(
                br#"{"type":"notify","message":"Hello from the native Rust helper."}"#,
                r#"{"value":null}"#,
            );
        }
    }
    0
}
unsafe extern "C" fn shutdown() {
    HOST.store(std::ptr::null_mut(), Ordering::Release);
}
static API: PluginApi = PluginApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<PluginApi>(),
    initialize,
    event,
    shutdown,
};
#[no_mangle]
pub extern "C" fn hortcmd_plugin_v1() -> *const PluginApi {
    &API
}
