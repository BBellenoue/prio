#!/bin/sh
# Installs Prio as a resident on macOS:
#  - Prio.app in ~/Applications (an LSUIElement bundle: menu bar, no Dock icon), with the
#    Cmd+Alt+A (capture) and Cmd+Alt+P (list) global shortcuts;
#  - a session agent that starts it again at login.
set -eu

# From a release archive, prio sits next to the script; from the repository, in target/release.
root=$(cd "$(dirname "$0")" && pwd)
exe="$root/prio"
[ -x "$exe" ] || exe="$root/target/release/prio"
app="$HOME/Applications/Prio.app"
label="io.github.bbellenoue.prio"
agent="$HOME/Library/LaunchAgents/$label.plist"

[ -x "$exe" ] || { echo "build it first: cargo build --release" >&2; exit 1; }


launchctl bootout "gui/$(id -u)/$label" 2>/dev/null || true
pkill -f "$app/Contents/MacOS/prio" 2>/dev/null || true

mkdir -p "$app/Contents/MacOS"
cp "$exe" "$app/Contents/MacOS/prio"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>prio</string>
    <key>CFBundleIdentifier</key><string>$label</string>
    <key>CFBundleName</key><string>Prio</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.2.0</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
# The binary is unsigned: without this, Gatekeeper refuses anything unpacked from a download.
xattr -dr com.apple.quarantine "$app" 2>/dev/null || true
echo "installed: $app"

mkdir -p "$(dirname "$agent")"
cat > "$agent" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>$label</string>
    <key>ProgramArguments</key>
    <array><string>$app/Contents/MacOS/prio</string></array>
    <key>RunAtLoad</key><true/>
</dict>
</plist>
PLIST
launchctl bootstrap "gui/$(id -u)" "$agent"
echo "session agent: $agent"
echo "Prio started. Cmd+Alt+A to capture, Cmd+Alt+P for the list."
