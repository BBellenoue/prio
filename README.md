<h1 align="center">Prio</h1>

<p align="center">
  <a href="https://github.com/BBellenoue/prio/actions/workflows/ci.yml"><img src="https://github.com/BBellenoue/prio/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/BBellenoue/prio/actions/workflows/security.yml"><img src="https://github.com/BBellenoue/prio/actions/workflows/security.yml/badge.svg" alt="Security"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-MIT-blue.svg" alt="Licence: MIT"></a>
</p>

A small Windows app for keeping one ordered list of priorities. Two global
shortcuts, drag and drop to reorder, and nothing else in the way.

It exists because a pile of Notepad files is not a system: nothing dates
anything, nothing ages, nothing tells you who is waiting on what, and you never
go back to them. Prio keeps the capture as cheap (one shortcut, type, Enter) and
makes the list honest about what is rotting.

## Features

- **Ctrl+Alt+A**: capture a priority. Title, who asked, an optional deadline
  (typed, or picked in a calendar with *Tomorrow / Friday / +1 week / +1 month*),
  free tags and notes. Names and tags already used come back as clickable chips.
- **Ctrl+Alt+P**: the list. Rank, title, requester avatar, deadline pill
  (red when overdue or due today, amber inside three days), tag pills. Drag the
  rank handle to reorder; the order is the priority.
- **Aging**: a task with no deadline that has sat for 14 days greys out and says
  so, so it gets done, dated or archived instead of forgotten.
- **Waiting on someone**: mark a task as waiting on a person and it sinks to a
  dimmed section at the bottom until *Unblocked*.
- **Notes and links**: each card unfolds into a notes area; URLs in it become
  clickable links.
- **Tags**: free text, comma separated. Every tag in use becomes a filter chip
  under the list title.
- **Archive**: the check mark archives with the date; *Copy this week* puts a
  Markdown checklist of what you finished in the last seven days on the
  clipboard, for a weekly report. Anything can be restored.
- **Resident**: one process at logon, a tray icon (left click opens the list,
  right click for a menu), windows created on the shortcut and destroyed on
  close. About 20 ms from the key press to the window.

Dark, frameless windows drawn with the system's Segoe UI; no assets shipped.

## Data

Everything lives in `%APPDATA%\prio\`:

| File | Content |
|---|---|
| `tasks.json` | `{ "active": [...], "done": [...] }`, one object per task, readable and editable |
| `backup\tasks.YYYY-MM-DD.json` | a copy taken before the first write of each day, 30 days kept |
| `resident.tid` | the id of the resident's hotkey thread, so a second launch can wake it |

Nothing leaves the machine. To sync between computers, point the folder at
a synced location with a junction (both instances must not be open at once,
the last write wins):

```powershell
Move-Item "$env:APPDATA\prio" "$env:OneDrive\prio"
cmd /c mklink /J "$env:APPDATA\prio" "$env:OneDrive\prio"
```

## Install

Requires Windows 10 or 11 and a [Rust](https://rustup.rs) toolchain.

```powershell
cargo build --release
.\install-shortcuts.ps1
```

The script puts a shortcut in the Startup folder (so Prio starts with your
session, resident, no window), another in the Start menu, and launches it. To
stop it: right click the tray icon, *Quitter*, or the *Quitter* link at the
bottom of the list.

`prio.exe add` and `prio.exe list` open a single window and exit when it
closes, for use from any other launcher.

## Layout

One crate, one file. `src/main.rs` reads top to bottom:

- `Date`: calendar arithmetic on the standard library plus `GetLocalTime`,
  no date crate
- `Task`, `Store`, `load`, `save`, `backup`: the JSON file and its daily copy
- `main`, `Resident`, `hotkey_loop`: the resident process, `RegisterHotKey`,
  the tray icon, one child viewport per window
- `Add`, `List`, `card`, `calendar`: the UI, built on
  [egui](https://github.com/emilk/egui) through eframe with the wgpu backend

## Development

```powershell
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

CI runs those on Windows. A second workflow runs Semgrep, Trivy, cargo-deny
and Gitleaks on every push and every Monday, with findings in the repository's
Security tab; `cargo deny check` runs the same advisory and licence checks
locally.

`PRIO_DEBUG=1` writes a trace to `%APPDATA%\prio\debug.log`.
`PRIO_TEST_HOTKEY=1` (add) or `2` (list) fires a shortcut at startup, for
testing without a keyboard. `prio.exe add cal` opens the capture window with
the calendar unfolded.

## Licence

[MIT](LICENSE).
