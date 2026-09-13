# Installs Prio as a resident:
#  - started at login (the Startup folder), with no argument, so the resident process takes the
#    Ctrl+Alt+A (capture) and Ctrl+Alt+P (list) global shortcuts and shows a notification icon;
#  - a "Prio" shortcut in the Start menu (wakes the resident, or starts it when absent).
# From a release zip, prio.exe sits next to the script; from the repository, in target\release.
$exe = Join-Path $PSScriptRoot "prio.exe"
if (-not (Test-Path $exe)) { $exe = Join-Path $PSScriptRoot "target\release\prio.exe" }
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

# Restart the resident on the new build.
Get-Process priority, prio -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500
Start-Process $exe
Write-Host "Prio started. Ctrl+Alt+A to capture, Ctrl+Alt+P for the list."
