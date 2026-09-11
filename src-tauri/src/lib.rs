// ============================================================
// App setup & registration of the Tauri commands.
// ============================================================

mod commands;
mod menu;
mod net;
mod vfs;

/// Origins the main window may hold (TICKET-030). In a packaged build
/// that is only the app document itself; a dev build additionally serves
/// it from the Vite origin named in `tauri.conf.json`.
///
/// Everything else — `http(s)` pages, `file:`, `javascript:`, `data:`,
/// foreign custom protocols, malformed URLs — is refused, so no rendered
/// content can replace the trusted document that holds the IPC surface.
/// Links out of a preview take `open_external` to the system browser
/// instead.
fn is_trusted_origin(url: &tauri::Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    match url.scheme() {
        // The packaged app document (macOS/iOS serve it as `tauri:`,
        // Windows/Linux as `http://tauri.localhost`).
        "tauri" => true,
        "http" | "https" if host == "tauri.localhost" => true,
        // The Vite dev server, development builds only.
        "http" if cfg!(debug_assertions) && matches!(host, "localhost" | "127.0.0.1") => {
            url.port() == Some(1420)
        }
        _ => false,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // The window carries the full Commander menu; macOS additionally
        // gets a reduced native menu (TICKET-040).
        .setup(|app| {
            // The main window is declared in `tauri.conf.json` with
            // `"create": false` and built here instead, because the
            // navigation policy (TICKET-030) can only be attached while
            // the WebView is being created.
            let config = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .ok_or("Kein Fenster in tauri.conf.json konfiguriert")?;
            tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
                // Navigation boundary of the WebView. Only the app
                // document loads here; a blocked attempt is noted in
                // development with the URL alone — never with page or
                // file contents.
                .on_navigation(|url| {
                    let allowed = is_trusted_origin(url);
                    if !allowed && cfg!(debug_assertions) {
                        eprintln!("[hortcmd] Navigation blockiert: {}", url);
                    }
                    allowed
                })
                .build()?;
            menu::install(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::fs::dir::list_dir,
            commands::fs::dir::list_drives,
            commands::fs::dir::home_dir,
            commands::fs::size::dir_size,
            commands::fs::size::cancel_dir_size,
            commands::fs::volume::volume_info,
            commands::fs::file::make_dir,
            commands::fs::file::write_text_file,
            commands::fs::file::rename_entry,
            commands::fs::file::rename_batch,
            commands::fs::file::copy_entries,
            commands::fs::file::move_entries,
            commands::fs::file::cancel_transfer,
            commands::fs::file::pause_transfer,
            commands::fs::file::resolve_collision,
            commands::fs::file::delete_entries,
            commands::fs::file::trash_entries,
            commands::fs::preview::read_preview,
            commands::fs::preview::open_path,
            commands::fs::preview::open_external,
            commands::fs::preview::quick_look,
            commands::fs::preview::open_with,
            commands::fs::preview::open_terminal,
            commands::fs::preview::open_system_properties,
            commands::fs::archive::list_archive,
            commands::fs::archive::extract_entries,
            commands::fs::archive::create_archive,
            commands::fs::watch::set_watched,
            commands::fs::git::git_status_watch,
            commands::fs::compare::compare_dirs,
            commands::fs::compare::sync_copy,
            commands::fs::compare::compare_levels,
            commands::fs::selection::save_selection,
            commands::fs::selection::load_selection,
            commands::fs::filecompare::compare_files,
            commands::fs::search::search,
            commands::fs::search::cancel_search,
            commands::fs::props::file_props,
            commands::fs::props::set_permissions,
            commands::fs::props::set_owner,
            commands::fs::props::file_checksums,
            commands::fs::tags::get_tags,
            commands::fs::tags::set_tags,
            net::session::net_list_profiles,
            net::session::net_save_profile,
            net::session::net_delete_profile,
            net::session::net_connect,
            net::session::net_trust_host_key,
            net::session::net_forget_host_key,
            net::session::net_disconnect,
            net::session::net_list_connections,
            net::credentials::net_cred_status,
            net::credentials::net_cred_set_backend,
            net::credentials::net_cred_unlock,
            net::credentials::net_cred_lock,
            net::terminal::ssh_open_shell,
            net::terminal::ssh_send,
            net::terminal::ssh_resize,
            net::terminal::ssh_close,
            menu::set_menu_language,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Start der Tauri-Anwendung");
}
