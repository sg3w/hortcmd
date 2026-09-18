//! Stable C layout shared by the helper and native authors. No Rust-owned
//! allocation, string, trait object or unwinding crosses this boundary.
pub const ABI_VERSION: u32 = 1;
pub const MESSAGE_LIMIT: usize = 1024 * 1024;

#[repr(C)]
pub struct HostApi {
    pub abi_version: u32,
    pub struct_size: usize,
    pub request: unsafe extern "C" fn(*const u8, usize, *mut u8, usize) -> isize,
}
#[repr(C)]
pub struct PluginApi {
    pub abi_version: u32,
    pub struct_size: usize,
    pub initialize: unsafe extern "C" fn(*const HostApi) -> i32,
    pub event: unsafe extern "C" fn(*const u8, usize) -> i32,
    pub shutdown: unsafe extern "C" fn(),
}
