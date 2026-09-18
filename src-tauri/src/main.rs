// Prevents an extra console window on Windows in the release build.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() == Some(std::ffi::OsStr::new("--hortcmd-plugin-helper")) {
        let Some(path) = args.next() else {
            std::process::exit(2);
        };
        std::process::exit(hortcmd_lib::plugins::native_worker::run(
            std::path::Path::new(&path),
        ));
    }
    hortcmd_lib::run()
}
