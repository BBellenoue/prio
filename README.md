<p align="center">
  <img src="docs/logo.svg" alt="" width="112">
</p>

<h1 align="center">Prio</h1>

<p align="center">
  <a href="https://github.com/BBellenoue/prio/actions/workflows/ci.yml"><img src="https://github.com/BBellenoue/prio/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/BBellenoue/prio/actions/workflows/security.yml"><img src="https://github.com/BBellenoue/prio/actions/workflows/security.yml/badge.svg" alt="Security"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-MIT-blue.svg" alt="Licence: MIT"></a>
</p>

A small desktop app, Windows, macOS and Linux, for keeping one ordered list of
priorities. Two global shortcuts, drag and drop to reorder, and nothing else in
the way.

It exists because a pile of Notepad files is not a system: nothing dates
anything, nothing ages, nothing tells you who is waiting on what, and you never
go back to them. Prio keeps the capture as cheap (one shortcut, type, Enter) and
makes the list honest about what is rotting.

## Features

Shortcuts below are the Windows and Linux defaults; on macOS the same two are
**Cmd+Alt+A** and **Cmd+Alt+P**. Both are yours to change.

- **Ctrl+Alt+A**: capture a priority. Title, tier, effort, who asked, an optional deadline
  (typed, or picked in a calendar with *Tomorrow / Friday / +1 week / +1 month*),
  free tags and notes. Names and tags already used come back as clickable chips.
- **Ctrl+Alt+P**: the list. Rank, title, requester avatar, deadline pill
  (red when overdue or due today, amber inside three days), tag pills. Drag the
  rank handle to reorder; the order is the priority.
- **Three tiers**: *Haute*, *Moyenne*, *Basse*, a row of chips at capture time,
  *Moyenne* by default. The list is cut in three and the rank keeps running
  1..n across them, so a tier never hides which single task comes first.
  Dragging a card across a separator moves it to that tier: the same gesture
  ranks and promotes. An empty tier shows up as a drop target while a card is
  held.
- **Effort**: *30 min*, *2 h* or *1 jour*, on the same row of chips as the tier,
  *2 h* by default and never empty. Three bars on the card say which one, and a
  filter chip over the list answers the question that decides what you pick up:
  I have twenty minutes, what fits. The chips appear once two different efforts
  are in use, a filter on a single value filtering nothing.
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
- **Resident**: one process at logon, an icon in the notification area or the
  menu bar (left click opens the list, right click for a menu), windows created
  on the shortcut and destroyed on close. About 20 ms from the key press to the
  window.
- **Your shortcuts**: the gear in the list's title bar opens the settings,
  where each shortcut is set by pressing the combination you want (Ctrl and/or
  Alt, Cmd too on macOS, Shift if you like, then a letter, a digit, F1 to F24
  or Space). Applied
  at once; a combination another application already holds is flagged.

Dark, frameless windows drawn with the system's own font, Segoe UI on Windows,
San Francisco on macOS, Ubuntu or DejaVu on Linux; no assets shipped.

## Data

Everything lives in `%APPDATA%\prio\` on Windows, in
`~/Library/Application Support/prio/` on macOS, in `~/.local/share/prio/` on
Linux:

| File | Content |
|---|---|
| `tasks.json` | `{ "active": [...], "done": [...] }`, one object per task, readable and editable |
| `backup/tasks.YYYY-MM-DD.json` | a copy taken before the first write of each day, 30 days kept |
| `settings.json` | the two shortcuts, `Ctrl+Alt+A` and `Ctrl+Alt+P` by default, `Cmd+Alt+...` on macOS |
| `resident.tid`, `prio.sock` | how a second launch reaches the resident: a thread id on Windows, a unix socket on macOS |

Nothing leaves the machine. To sync between computers, point the folder at
a synced location with a link (both instances must not be open at once, the
last write wins):

```powershell
Move-Item "$env:APPDATA\prio" "$env:OneDrive\prio"
cmd /c mklink /J "$env:APPDATA\prio" "$env:OneDrive\prio"
```

```sh
mv ~/Library/Application\ Support/prio ~/OneDrive/prio
ln -s ~/OneDrive/prio ~/Library/Application\ Support/prio
```

## Install

Requires Windows 10 or 11, macOS 11 and later, or a Linux desktop running an
**X11 session**: under Wayland an application cannot reserve a key combination
for itself, and the two global shortcuts are the whole point.

Without a toolchain: download the archive for your system from the latest
[release](https://github.com/BBellenoue/prio/releases), unpack it anywhere
permanent, and run `install-shortcuts.ps1`, `install-macos.sh` or
`install-linux.sh` from that folder. The Linux archive is built on the current
Ubuntu, so it wants glibc 2.39 or newer; on an older distribution, build from
source.

From source, with a [Rust](https://rustup.rs) toolchain:

```powershell
cargo build --release
.\install-shortcuts.ps1
```

The script puts a shortcut in the Startup folder (so Prio starts with your
session, resident, no window), another in the Start menu, and launches it.

```sh
cargo build --release
./install-macos.sh
```

The script wraps the binary in `~/Applications/Prio.app` (an `LSUIElement`
bundle: menu bar, no Dock icon) and registers a session agent that starts it
at login.

```sh
sudo apt install libgtk-3-dev libxdo-dev libayatana-appindicator3-dev libxkbcommon-x11-dev
cargo build --release
./install-linux.sh
```

The script puts the binary in `~/.local/bin`, a launcher in the applications
menu and an autostart entry, then starts it.

To stop it: right click the icon, *Quitter*, or the *Quitter* link at the
bottom of the list.

`prio add` and `prio list` open a single window and exit when it closes, for
use from any other launcher.

## Layout

One crate. `src/main.rs` holds everything that does not depend on the system
and reads top to bottom:

- `Date`: calendar arithmetic on the standard library plus the system clock,
  no date crate
- `Task`, `Store`, `load`, `save`, `backup`: the JSON file and its daily copy
- `main`, `Resident`: the resident process and one child viewport per window
- `Add`, `List`, `card`, `calendar`: the UI, built on
  [egui](https://github.com/emilk/egui) through eframe with the wgpu backend

`src/platform/` holds the rest, one file per system behind the same functions:
data folder, local date, system fonts, screen size, global shortcuts, icon and
menu, and how a second launch reaches the resident. `mod.rs` parses a shortcut
into a neutral combination that each backend then translates, `win.rs` into
`RegisterHotKey` and a Win32 message loop, `mac.rs` into Carbon hotkeys and a
menu bar item, `linux.rs` into X11 key grabs and a GTK tray thread. `unix.rs`
carries what macOS and Linux share: the local date, the screen size, and the
socket a second launch writes to.

## Development

```sh
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

CI runs those on Windows, macOS and Linux. A second workflow runs Semgrep, Trivy,
cargo-deny and Gitleaks on every push and every Monday, with findings in the repository's
Security tab; `cargo deny check` runs the same advisory and licence checks
locally.

`PRIO_DEBUG=1` writes a trace to `debug.log`, next to `tasks.json`.
`PRIO_TEST_HOTKEY=1` (add) or `2` (list) fires a shortcut at startup, for
testing without a keyboard; `PRIO_TEST_SETTINGS=1` opens the list on its
settings panel. `prio add cal` opens the capture window with the calendar
unfolded.

The logo lives in `docs/logo.svg`; the notification area, menu bar and window
icons are the same drawing, rasterised in code (`logo_rgba`), as a template
image on macOS so it follows the system appearance. `docs/social-preview.png` is the
1280x640 card for the repository's social preview.

## Licence

[MIT](LICENSE).
