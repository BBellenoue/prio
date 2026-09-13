#!/bin/sh
# Installs Prio as a resident on Linux (X11 session):
#  - the binary in ~/.local/bin, a launcher in the applications menu;
#  - an autostart entry, so the resident takes the Ctrl+Alt+A (capture) and Ctrl+Alt+P (list)
#    global shortcuts at login.
set -eu

# From a release archive, prio sits next to the script; from the repository, in target/release.
root=$(cd "$(dirname "$0")" && pwd)
exe="$root/prio"
[ -x "$exe" ] || exe="$root/target/release/prio"
bin="$HOME/.local/bin/prio"
apps="$HOME/.local/share/applications"
autostart="$HOME/.config/autostart"

[ -x "$exe" ] || { echo "build it first: cargo build --release" >&2; exit 1; }
[ "${XDG_SESSION_TYPE:-x11}" = "x11" ] || echo "warning: session is ${XDG_SESSION_TYPE}, global shortcuts need X11" >&2

pkill -x prio 2>/dev/null || true
mkdir -p "$(dirname "$bin")" "$apps" "$autostart"
install -m 755 "$exe" "$bin"

cat > "$apps/prio.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Prio
Comment=One ordered list of priorities
Exec=$bin
Terminal=false
Categories=Utility;
StartupNotify=false
DESKTOP
cp "$apps/prio.desktop" "$autostart/prio.desktop"
echo "installed: $bin"
echo "autostart: $autostart/prio.desktop"

nohup "$bin" >/dev/null 2>&1 &
echo "Prio started. Ctrl+Alt+A to capture, Ctrl+Alt+P for the list."
