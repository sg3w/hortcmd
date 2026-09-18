//! This module executes only after the dedicated helper flag, before Tauri starts.
use super::native_abi::{HostApi, PluginApi, ABI_VERSION, MESSAGE_LIMIT};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    sync::{Mutex, OnceLock},
};

struct Wire {
    input: BufReader<std::io::Stdin>,
    output: std::io::Stdout,
}
static OWNER: OnceLock<std::thread::ThreadId> = OnceLock::new();
static WIRE: OnceLock<Mutex<Wire>> = OnceLock::new();
fn wire() -> &'static Mutex<Wire> {
    WIRE.get_or_init(|| {
        Mutex::new(Wire {
            input: BufReader::new(std::io::stdin()),
            output: std::io::stdout(),
        })
    })
}
impl Wire {
    fn send(&mut self, value: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        if bytes.len() > MESSAGE_LIMIT {
            return Err("Native message exceeds limit".into());
        }
        self.output
            .write_all(&bytes)
            .and_then(|_| self.output.write_all(b"\n"))
            .and_then(|_| self.output.flush())
            .map_err(|e| e.to_string())
    }
    fn read(&mut self) -> Result<Value, String> {
        let mut bytes = Vec::new();
        (&mut self.input)
            .take((MESSAGE_LIMIT + 1) as u64)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > MESSAGE_LIMIT || bytes.last() != Some(&b'\n') {
            return Err("Native pipe closed or message exceeds limit".into());
        }
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    }
}
unsafe extern "C" fn request(
    input: *const u8,
    len: usize,
    output: *mut u8,
    capacity: usize,
) -> isize {
    // A faulty pointer still terminates only this helper. The C contract requires
    // valid caller-owned buffers; catch_unwind handles Rust errors, not bad memory.
    if OWNER.get() != Some(&std::thread::current().id()) {
        return -1;
    }
    std::panic::catch_unwind(|| {
        if input.is_null() || output.is_null() || len > MESSAGE_LIMIT || capacity > MESSAGE_LIMIT {
            return -1;
        }
        // SAFETY: callers promise a readable input buffer of len bytes.
        let bytes = unsafe { std::slice::from_raw_parts(input, len) };
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            return -1;
        };
        let Ok(mut wire) = wire().lock() else {
            return -1;
        };
        if wire
            .send(&json!({"type":"request", "request":value}))
            .is_err()
        {
            return -1;
        }
        let Ok(response) = wire.read() else {
            return -1;
        };
        let Ok(bytes) = serde_json::to_vec(&response) else {
            return -1;
        };
        if bytes.len() > capacity {
            return -2;
        }
        // SAFETY: callers promise a writable output buffer of capacity bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
        }
        bytes.len() as isize
    })
    .unwrap_or(-1)
}
fn execute(path: &Path) -> Result<(), String> {
    let _ = OWNER.set(std::thread::current().id());
    // SAFETY: native packages are explicitly trusted. The library and every
    // function pointer remain confined to this process, and the library outlives
    // initialize/event/shutdown. No Rust ABI values cross the boundary.
    unsafe {
        let library = libloading::Library::new(path).map_err(|e| e.to_string())?;
        let entry: libloading::Symbol<unsafe extern "C" fn() -> *const PluginApi> = library
            .get(b"hortcmd_plugin_v1\0")
            .map_err(|e| e.to_string())?;
        let pointer = entry();
        if pointer.is_null() {
            return Err("Null native API".into());
        }
        let api = &*pointer;
        if api.abi_version != ABI_VERSION || api.struct_size != std::mem::size_of::<PluginApi>() {
            return Err("Incompatible native ABI".into());
        }
        let host = HostApi {
            abi_version: ABI_VERSION,
            struct_size: std::mem::size_of::<HostApi>(),
            request,
        };
        if (api.initialize)(&host) != 0 {
            return Err("Native initialization failed".into());
        }
        wire()
            .lock()
            .map_err(|_| "Native pipe unavailable")?
            .send(&json!({"type":"ready", "abiVersion":ABI_VERSION}))?;
        let result = loop {
            let message = match wire().lock().map_err(|_| "Native pipe unavailable")?.read() {
                Ok(value) => value,
                Err(error) => break Err(error),
            };
            match message["type"].as_str() {
                Some("close") => break Ok(()),
                Some("event") => {
                    let bytes = serde_json::to_vec(&message["event"]).map_err(|e| e.to_string())?;
                    if (api.event)(bytes.as_ptr(), bytes.len()) != 0 {
                        break Err("Native event handler failed".into());
                    }
                    wire()
                        .lock()
                        .map_err(|_| "Native pipe unavailable")?
                        .send(&json!({"type":"idle"}))?;
                }
                _ => break Err("Invalid native host message".into()),
            }
        };
        (api.shutdown)();
        result
    }
}
/// Returns the helper exit code. The main application does not call this during
/// ordinary startup and never opens a library itself.
pub fn run(path: &Path) -> i32 {
    match execute(path) {
        Ok(()) => 0,
        Err(error) => {
            if let Ok(mut wire) = wire().lock() {
                let _ = wire.send(&json!({"type":"error", "error":error}));
            }
            1
        }
    }
}
