# TICKET-040: Native Application Menus (Win/Linux/macOS)


## Goal
Implement a native system menu structure for the application that adapts to the target operating system, ensuring a "native feel" on Windows and Linux while adhering to the specific global menu bar API requirements of macOS.


## Background
To ensure the app feels like a first-class citizen on every platform, we need to move away from (or supplement) custom HTML menus towards native OS menus. While Windows and Linux typically attach the menu bar directly to the application window, macOS handles menus via a global system bar. This requires a different implementation approach in Tauri, as the "App Menu" must be handled separately from the "Window Menu" via the Tauri API rather than being defined inline within the window context.


## Requirements
- **Unified Menu Definition**: Create a centralized menu structure (e.g., File, Edit, View, Help) that can be shared across platforms to maintain consistency.
- **Windows/Linux Implementation**: Implement the menu as an inline bar attached directly to the main application window.
- **macOS Special Case**: 
    - Use the Tauri API to implement the global system menu bar (outside the window).
    - Properly separate the Application Menu (the one with the App Name) from the functional Window Menus.
    - Ensure that macOS-specific items (like "Quit" or "Preferences") are placed in the correct native locations according to Apple's Human Interface Guidelines.
- **Command Integration**: Link menu items to corresponding Tauri commands (`invoke`) to trigger backend logic.
- **Keyboard Shortcuts**: Implement platform-specific accelerators (e.g., `Ctrl+S` on Win/Lin, `Cmd+S` on Mac).
- **Location**: Menu setup logic should be integrated into `src-tauri/src/main.rs` (or a dedicated `menu.rs` module) and communicated to the frontend via events if UI updates are needed.


## Acceptance criteria
- [ ] Native menu bar is visible and correctly placed on Windows and Linux (top of window).
- [ ] Native menu bar is visible in the macOS system menu bar (not inline in the window).
- [ ] All defined menu items trigger their respective backend functions correctly.
- [ ] Keyboard shortcuts work according to the OS standard (`Cmd` vs `Ctrl`).
- [ ] The app behaves correctly when "Quit" is triggered from the native menu on all platforms.


## Tests
- **Windows**: Verify inline menu placement and shortcut functionality.
- **Linux**: Verify inline menu placement and shortcut functionality.
- **macOS**: Verify that the menu appears in the system bar, the App Menu contains the app name, and shortcuts use `Cmd`.


## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass on all three target platforms (Windows, Linux, macOS)
- Existing functionality unchanged
- Documentation updated regarding menu command triggers
- Rename the ticket to *.done.md
