# Local command line (ticket 085)

The command line below the panes is available in the desktop app when the active
folder supports a local terminal. Focus it with Tab or a pointer, type a command,
and press unmodified Enter. Escape clears the input. Commands are never launched
by a global Enter shortcut; dialogs, menus and IME composition take precedence.

Each submission starts a fresh, non-interactive `/bin/sh -c` on macOS/Linux or
`cmd.exe /D /S /C` on Windows, with the active local folder captured as its working
directory. Shell syntax, quoting, pipelines and redirections belong to that shell.
A command such as `cd` does not change the file pane or the next command's directory.
The input is passed unchanged; neither paths nor commands are concatenated into a
second shell expression. Standard input is closed. Use the existing terminal
command for interactive programs, and **Network → Open SSH terminal** for SSH.
Remote locations and archive contents cannot run this local command line.

Only one command runs at a time. The compact output below the input shows its
original folder and command, separate stdout/stderr, and the exit code or cancelled
state. Output is plain text (lossy UTF-8), updated every 250 ms, retained only in
memory and capped at 128 KiB per stream while remaining bytes are still drained.
The two streams do not promise a combined chronological order. Close output clears
the result and returns focus to the input; it does not erase the command text.

Cancel terminates the process: the dedicated process group on Unix, and a
`taskkill /T /F` request plus child termination on Windows. Cancellation is not an
undo of changes a command already made. Detached/background daemons and interactive
sessions are outside this control's scope. On Unix, remaining group members are
terminated when the shell exits. Output pipes are given a bounded drain period so
a detached descendant cannot hold the UI open indefinitely. A truncation notice
also covers that case. No command history or session is persisted.

Tests cover local execution, exact working directory and quoting, stderr, nonzero
exit, closed stdin, bounded output, cancellation, overlapping/stale requests and
keyboard ownership. Native Windows/Linux verification is tracked in ticket 135.
