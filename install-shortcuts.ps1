# Installs Prio as a resident:
#  - started at login (the Startup folder), with no argument, so the resident process takes the
#    Ctrl+Alt+A (capture) and Ctrl+Alt+P (list) global shortcuts and shows a notification icon;
#  - a "Prio" shortcut in the Start menu (wakes the resident, or starts it when absent).
# From a release zip, prio.exe sits next to the script and is installed as it is; from the
# repository, it is built first, in target\release. Building needs the resident stopped: it
# holds a lock on the file it was started from.
$exe = Join-Path $PSScriptRoot "prio.exe"
$build = -not (Test-Path $exe)
if ($build) {
    $exe = Join-Path $PSScriptRoot "target\release\prio.exe"
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Error "no prio.exe next to the script, and no cargo to build one"
        exit 1
    }
    Get-Process priority, prio -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 500
    Push-Location $PSScriptRoot
    cargo build --release
    Pop-Location
    if ($LASTEXITCODE -ne 0) {
        Write-Error "cargo build --release failed: nothing installed, the resident is stopped"
        exit 1
    }
}
$menu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$startup = Join-Path $menu "Startup"
$sh = New-Object -ComObject WScript.Shell

# old shortcuts (named "Priorites", .lnk with a Windows hotkey)
foreach ($old in @((Join-Path $menu "Priorite - Ajouter.lnk"), (Join-Path $menu "Priorites.lnk"), (Join-Path $startup "Priorites.lnk"))) {
    if (Test-Path $old) { Remove-Item $old; Write-Host "removed: $old" }
}

foreach ($s in @(
    @{ Path = (Join-Path $startup "Prio.lnk"); Desc = "Prio (resident, Ctrl+Alt+A / Ctrl+Alt+P)" },
    @{ Path = (Join-Path $menu "Prio.lnk");    Desc = "Prio" }
)) {
    $lnk = $sh.CreateShortcut($s.Path)
    $lnk.TargetPath = $exe
    $lnk.Arguments = ""
    $lnk.WorkingDirectory = Split-Path $exe
    $lnk.Description = $s.Desc
    $lnk.Save()
    Write-Host "created: $($s.Path)"
}

# Restart the resident on the new build (already stopped when the build ran).
Get-Process priority, prio -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500
Start-Process $exe
Write-Host "Prio started. Ctrl+Alt+A to capture, Ctrl+Alt+P for the list."
