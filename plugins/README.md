# Reference plugins

These packages exercise the version-1 interface described in
[Plugin interface](../docs/plugin-interface.md). They are opt-in examples, not
built-in features or automatically activated extensions. All package text is English.

## Path tools (JavaScript)

In Configuration → Settings → Plugins, install the absolute path of the
[path-tools](path-tools/plugin.json) folder. Activate it and review the declared
permissions. The colorful command-bar button opens its own window.

The window shows current path, folder path, file path, pane, selection and the
copy speed limit. Refresh context after changing the active pane or selection.
Copy path writes only the displayed path in response to a button click. If the
WebView refuses clipboard access, the path stays selected for the system Copy
shortcut and a visible message explains the failure.

Enter an absolute destination directory or a supported VFS address and choose
Copy selection to destination. The package requests `op:copy`; the commander
performs the normal confirmation/preflight, queueing and transfer work. The
plugin never enumerates files, alters the speed limit or bypasses confirmation.
Paths are inserted as text, including filenames that look like HTML.

No build is required: the HTML, JavaScript and CSS are bundled locally. The
host injects `window.hortcmd`. Register listeners before sending `view:ready`;
use `request` for broker operations and `onEvent` for lifecycle events. No Tauri
API or CDN is imported by the package.

## Native hello (Rust)

With Rust and Node installed, run from the repository root:

```sh
node plugins/native-hello/build.mjs
```

Install the output directory printed by the script, then activate Native hello
and review the native-privilege warning. Its command button starts the helper
and displays a notification through the permission-checked broker.

The example is a dependency-free `cdylib`. The build script compiles only for the
current host and copies the manifest beside that platform's library. Source code
is in [lib.rs](native-hello/lib.rs). The ABI corresponds to
[hortcmd_plugin.h](../docs/plugins/hortcmd_plugin.h): version and structure size,
initialization, event callback and shutdown. Buffers belong to their callers;
no Rust allocations or panic unwinding cross the ABI. Callbacks run on the
helper's owner thread. The example accepts the documented serialized host reply
for its two fixed requests; a larger plugin should use a proper JSON parser.

The helper isolates crashes, not operating-system permissions. Native code has
full user privileges. Broker grants cannot constrain direct native OS calls.

## Verification

`npm run check` includes package installation/activation/removal tests, tests
executing the actual JavaScript package against a typed host stand-in, and a
subprocess test that compiles this Rust source and calls its real ABI in the
commander helper. UI tests cover context rendering, explicit refresh, clipboard
success/refusal, selection copies, duplicate-submit suppression and denial.
Existing host tests cover permission denial, copy review/limits and termination.

The disposable Tauri smoke page in
[harness/plugin-smoke](../harness/plugin-smoke/index.html) installs and approves
both packages, opens the real JS WebView and verifies its handshake/context,
launches the real native helper, receives its broker notification and uninstalls
both packages. Run it with a **separate application identifier**, the dev URL
http://localhost:1420/harness/plugin-smoke/index.html and query parameters
`source` (absolute repository path) and `result` (unused absolute JSON output path).
Configure the main window URL through `tauri dev --config` and set `create:false`,
matching the normal application setup. The page writes its result using the
normal file command. It is a dev-only harness page, excluded from production
build inputs. Do not run it with an existing user's application identity.

Interactive clipboard behavior also depends on the platform WebView's user-
activation policy. The example reports refusal instead of pretending a write
succeeded. Use the visible selected-path fallback when access is unavailable.

## Storage usage

[Storage usage](storage-usage/README.md) is the first functional JS plugin. Install
`plugins/storage-usage` through plugin management; it is not enabled by default.
It provides a bounded Rust-backed folder treemap, table and explicit rescan flow.
