// ============================================================
// App setup & registration of the Tauri commands.
// ============================================================

mod commands;
mod net;
mod vfs;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
            net::session::net_disconnect,
            net::session::net_list_connections,
            net::terminal::ssh_open_shell,
            net::terminal::ssh_send,
            net::terminal::ssh_resize,
            net::terminal::ssh_close,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Start der Tauri-Anwendung");
}
