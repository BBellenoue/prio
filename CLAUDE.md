# Prio - project rules

Windows-only priority list in Rust (egui/eframe, wgpu). One crate, one file:
`src/main.rs`. Read `README.md` for what it does and `CONTRIBUTING.md` for the
checks; this file is what an agent must know before touching anything.

## Build, test, run

```powershell
cargo fmt --check && cargo clippy -- -D warnings && cargo test
cargo build --release
.\install-shortcuts.ps1        # kills the resident, reinstalls shortcuts, restarts it
```

The release binary is locked while the resident runs: stop `prio.exe` before
`cargo build --release`, or run the install script which does it.

## Style

- Standard library first, then a crate already in `Cargo.lock`, then a new one
  with a one-line justification in the commit body.
- No assets: system fonts, icons drawn in code.
- Comments explain a constraint or a non-obvious choice, in French, ASCII only
  (no accents in comments; accents are fine in user-facing strings).
- Deliberate shortcuts carry a `ponytail:` comment naming the ceiling and the
  upgrade path.
- The egui context lock is not reentrant: never read the context (`has_focus`,
  `input`, `memory`) inside a `data_mut` / `memory_mut` closure. It froze the
  window once; compute first, then write.
- Plain hyphen `-` everywhere. Never an em dash or an en dash, in code, prose,
  or commit messages.

## Commits

- One logical change per commit, on a branch, merged through a pull request.
- Subject in the imperative, no period, no prefix, no ticket number, under
  72 characters. Body explains why and what was verified.
- Never commit anything from `%APPDATA%\prio` (tasks, backups, `debug.log`,
  `resident.tid`), nor `.claude/`, nor a real person's name, hostname, path or
  credential. Tests use made-up names and `example.com`.
- Run the three checks above before committing; CI runs them on Windows.

## Verifying the resident without a keyboard

`PRIO_DEBUG=1` traces to `%APPDATA%\prio\debug.log`. `PRIO_TEST_HOTKEY=1|2`
fires the add / list shortcut at startup; `PRIO_TEST_SETTINGS=1` opens the
list on the settings panel. From a script, post
`WM_HOTKEY` (0x0312) with `wParam` 1 or 2 to the thread id in
`%APPDATA%\prio\resident.tid`, and `WM_CLOSE` to a window to close it. Do not
use `SendKeys`: it types into whatever window the user has in front.
