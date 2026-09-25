// ============================================================
// Reduced native macOS menu (TICKET-040).
//
// The complete Commander menu (Dateien · Markieren · Befehle · Netz ·
// Ansicht · Konfiguration · Hilfe) lives in the window, in React, on
// every platform. The system menu bar of macOS cannot be left empty —
// AppKit always shows one — so it carries exactly the parts that have to
// be native:
//
// * the app menu (About, Settings, Services, Hide, Quit),
// * an Edit menu, whose predefined items are what gives the WebView's
//   text fields working copy/paste/undo key equivalents on macOS, and
// * a Window menu for minimizing, zooming and full screen.
//
// Deliberately absent is the "Close Window" item of Tauri's default
// menu: its ⌘W would swallow the keystroke that closes a panel tab.
// Quit is not the predefined item either — that one terminates the
// process behind the back of the running-transfer guard. It closes the
// window instead, which runs through the same `onCloseRequested` flow as
// the window button (TICKET-004).
//
// About and Settings only emit an event; the frontend answers with the
// very dialogs the in-window menu opens, so each exists exactly once.
//
// Windows and Linux get no native menu: Tauri installs its default menu
// on macOS only, and a second, native bar would duplicate the one in the
// window.
// ============================================================

use crate::errors::{AppError, AppResult};

/// Event that carries the id of a native menu item to the frontend.
#[cfg(target_os = "macos")]
pub const MENU_EVENT: &str = "native-menu";

#[cfg(target_os = "macos")]
mod imp {
    use tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
        AppHandle, Emitter, Runtime,
    };

    pub const ID_ABOUT: &str = "app.about";
    pub const ID_SETTINGS: &str = "app.settings";
    pub const ID_QUIT: &str = "app.quit";

    /// Labels of the native menu. The app is bilingual, so the system
    /// menu follows the language chosen in the settings.
    struct Labels {
        about: &'static str,
        settings: &'static str,
        services: &'static str,
        hide: &'static str,
        hide_others: &'static str,
        show_all: &'static str,
        quit: &'static str,
        edit: &'static str,
        undo: &'static str,
        redo: &'static str,
        cut: &'static str,
        copy: &'static str,
        paste: &'static str,
        select_all: &'static str,
        window: &'static str,
        minimize: &'static str,
        zoom: &'static str,
        fullscreen: &'static str,
    }

    const DE: Labels = Labels {
        about: "Über",
        settings: "Einstellungen …",
        services: "Dienste",
        hide: "ausblenden",
        hide_others: "Andere ausblenden",
        show_all: "Alle einblenden",
        quit: "beenden",
        edit: "Bearbeiten",
        undo: "Rückgängig",
        redo: "Wiederholen",
        cut: "Ausschneiden",
        copy: "Kopieren",
        paste: "Einfügen",
        select_all: "Alles auswählen",
        window: "Fenster",
        minimize: "Minimieren",
        zoom: "Zoomen",
        fullscreen: "Vollbild",
    };

    const EN: Labels = Labels {
        about: "About",
        settings: "Settings …",
        services: "Services",
        hide: "Hide",
        hide_others: "Hide Others",
        show_all: "Show All",
        quit: "Quit",
        edit: "Edit",
        undo: "Undo",
        redo: "Redo",
        cut: "Cut",
        copy: "Copy",
        paste: "Paste",
        select_all: "Select All",
        window: "Window",
        minimize: "Minimize",
        zoom: "Zoom",
        fullscreen: "Toggle Full Screen",
    };

    fn labels_for(lang: &str) -> &'static Labels {
        if lang == "en" {
            &EN
        } else {
            &DE
        }
    }

    /// German puts the app name first ("hortcmd beenden"), English last
    /// ("Quit hortcmd").
    fn with_name(lang: &str, verb: &str, name: &str) -> String {
        if lang == "en" {
            format!("{verb} {name}")
        } else {
            format!("{name} {verb}")
        }
    }

    fn build<R: Runtime>(app: &AppHandle<R>, lang: &str) -> tauri::Result<Menu<R>> {
        let l = labels_for(lang);
        let name = app.package_info().name.clone();

        // The three app-level entries are custom items: they must reuse
        // the frontend's dialogs and its window-close flow.
        let about = MenuItem::with_id(
            app,
            ID_ABOUT,
            // "Über hortcmd" / "About hortcmd" — the verb leads in both.
            format!("{} {name}", l.about),
            true,
            None::<&str>,
        )?;
        let settings = MenuItem::with_id(app, ID_SETTINGS, l.settings, true, Some("CmdOrCtrl+,"))?;
        let quit = MenuItem::with_id(
            app,
            ID_QUIT,
            with_name(lang, l.quit, &name),
            true,
            Some("CmdOrCtrl+Q"),
        )?;

        let app_menu = Submenu::with_items(
            app,
            name.clone(),
            true,
            &[
                &about,
                &PredefinedMenuItem::separator(app)?,
                &settings,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::services(app, Some(l.services))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, Some(&with_name(lang, l.hide, &name)))?,
                &PredefinedMenuItem::hide_others(app, Some(l.hide_others))?,
                &PredefinedMenuItem::show_all(app, Some(l.show_all))?,
                &PredefinedMenuItem::separator(app)?,
                &quit,
            ],
        )?;

        // Text editing inside the WebView. Without these items macOS has
        // no key equivalents for the standard editing selectors and the
        // input fields lose copy/paste/undo. This is *text* undo — file
        // operations get their own undo in TICKET-048.
        let edit_menu = Submenu::with_items(
            app,
            l.edit,
            true,
            &[
                &PredefinedMenuItem::undo(app, Some(l.undo))?,
                &PredefinedMenuItem::redo(app, Some(l.redo))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::cut(app, Some(l.cut))?,
                &PredefinedMenuItem::copy(app, Some(l.copy))?,
                &PredefinedMenuItem::paste(app, Some(l.paste))?,
                &PredefinedMenuItem::select_all(app, Some(l.select_all))?,
            ],
        )?;

        // No "Close Window" here: ⌘W belongs to the panel tab.
        let window_menu = Submenu::with_items(
            app,
            l.window,
            true,
            &[
                &PredefinedMenuItem::minimize(app, Some(l.minimize))?,
                &PredefinedMenuItem::maximize(app, Some(l.zoom))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::fullscreen(app, Some(l.fullscreen))?,
            ],
        )?;

        Menu::with_items(app, &[&app_menu, &edit_menu, &window_menu])
    }

    /// Builds the menu and makes it the application menu.
    pub fn apply<R: Runtime>(app: &AppHandle<R>, lang: &str) -> tauri::Result<()> {
        app.set_menu(build(app, lang)?)?;
        Ok(())
    }

    /// Forwards the three app-level items to the frontend.
    pub fn wire_events<R: Runtime>(app: &AppHandle<R>) {
        app.on_menu_event(|app, event| {
            let id = event.id().0.as_str();
            if matches!(id, ID_ABOUT | ID_SETTINGS | ID_QUIT) {
                let _ = app.emit(super::MENU_EVENT, id);
            }
        });
    }
}

/// Installs the reduced native menu. No-op outside macOS.
pub fn install<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        imp::wire_events(app);
        imp::apply(app, "de")?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows/Linux: the whole menu lives in the window (React).
        let _ = app;
    }
    Ok(())
}

/// Rebuilds the native menu in the given language. The frontend calls
/// this on startup and whenever the language setting changes, because
/// only it knows the persisted choice.
#[tauri::command]
pub fn set_menu_language(app: tauri::AppHandle, lang: String) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        // muda touches AppKit, which is main-thread only.
        let handle = app.clone();
        app.run_on_main_thread(move || {
            if let Err(_error) = imp::apply(&handle, &lang) {
                #[cfg(debug_assertions)]
                eprintln!("Failed to apply native menu language: {_error}");
            }
        })
        .map_err(AppError::from)?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, lang);
    }
    Ok(())
}
