#!/bin/sh
# Installe Prio en mode resident sur macOS:
#  - Prio.app dans ~/Applications (bundle LSUIElement: barre de menus, pas d'icone dans le Dock),
#    raccourcis globaux Cmd+Alt+A (ajouter) et Cmd+Alt+P (liste) ;
#  - un agent de session qui le relance a l'ouverture de session.
set -eu

# Depuis une archive de release, prio est a cote du script; depuis le depot, dans target/release.
root=$(cd "$(dirname "$0")" && pwd)
exe="$root/prio"
[ -x "$exe" ] || exe="$root/target/release/prio"
app="$HOME/Applications/Prio.app"
label="io.github.bbellenoue.prio"
agent="$HOME/Library/LaunchAgents/$label.plist"

[ -x "$exe" ] || { echo "construis d'abord: cargo build --release" >&2; exit 1; }


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
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
echo "installe: $app"

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
echo "agent de session: $agent"
echo "Prio lance. Cmd+Alt+A ajouter, Cmd+Alt+P liste."
