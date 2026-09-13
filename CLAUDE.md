# Prio - project rules

Priority list in Rust (egui/eframe, wgpu), Windows, macOS and Linux (X11). One crate:
`src/main.rs` for everything that does not depend on the system,
`src/platform/` for the rest (one file per system, same surface). Read `README.md` for what it does
and `CONTRIBUTING.md` for the checks; this file is what an agent must know
before touching anything.

## Build, test, run

```powershell
cargo fmt --check && cargo clippy -- -D warnings && cargo test
cargo build --release
.\install-shortcuts.ps1        # kills the resident, reinstalls shortcuts, restarts it
```

```sh
./install-macos.sh             # kills the resident, rebuilds Prio.app and the login agent
./install-linux.sh             # installs to ~/.local/bin plus an autostart entry
```

The release binary is locked while the resident runs on Windows: stop
`prio.exe` before `cargo build --release`, or run the install script which
does it.

## Style

- Standard library first, then a crate already in `Cargo.lock`, then a new one
  with a one-line justification in the commit body.
- No assets: system fonts, icons drawn in code.
- Code, comments and log strings in English, ASCII only. User-facing strings
  stay in French, accents included: the interface is French.
- Deliberate shortcuts carry a `ponytail:` comment naming the ceiling and the
  upgrade path.
- System-specific code goes in `src/platform/{win,mac,linux}.rs`, behind the
  same functions; no `cfg` in the UI. What macOS and Linux share sits in
  `unix.rs`. The three backends must keep working, CI builds them.
- Linux means X11: the global shortcuts are X11 key grabs, and Wayland does
  not hand those to an application.
- The egui context lock is not reentrant: never read the context (`has_focus`,
  `input`, `memory`) inside a `data_mut` / `memory_mut` closure. It froze the
  window once; compute first, then write.
- Plain hyphen `-` everywhere. Never an em dash or an en dash, in code, prose,
  or commit messages.

## Commits

- One logical change per commit, on a branch, merged through a pull request.
- Subject in the imperative, no period, no prefix, no ticket number, under
  72 characters. Body explains why and what was verified.
- Never commit anything from the data folder (`%APPDATA%\prio`,
  `~/Library/Application Support/prio`, `~/.local/share/prio`: tasks, backups,
  `debug.log`,
  `resident.tid`), nor `.claude/`, nor a real person's name, hostname, path or
  credential. Tests use made-up names and `example.com`.
- Run the three checks above before committing; CI runs them on Windows, macOS
  and Linux.

## Verifying the resident without a keyboard

`PRIO_DEBUG=1` traces to `debug.log`, next to `tasks.json`.
`PRIO_TEST_HOTKEY=1|2` fires the add / list shortcut at startup;
`PRIO_TEST_SETTINGS=1` opens the list on the settings panel. From a script,
Windows posts `WM_HOTKEY` (0x0312) with `wParam` 1 or 2 to the thread id in
`%APPDATA%\prio\resident.tid`, and `WM_CLOSE` to a window to close it; macOS
writes `list`, `reload` or `suspend` to the socket
(`echo list | nc -U ~/Library/Application\ Support/prio/prio.sock`). Do not use
`SendKeys` or `System Events` keystrokes: they type into whatever window the
user has in front.
