# Installe Prio en mode resident:
#  - demarrage a l'ouverture de session (dossier Startup), sans argument => process resident,
#    raccourcis globaux Ctrl+Alt+A (ajouter) et Ctrl+Alt+P (liste), icone de zone de notification ;
#  - un raccourci "Prio" dans le menu Demarrer (reveille le resident, ou le lance s'il est absent).
$exe = Join-Path $PSScriptRoot "target\release\prio.exe"
$menu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$startup = Join-Path $menu "Startup"
$sh = New-Object -ComObject WScript.Shell

# anciens raccourcis (nom "Priorites", .lnk a touche de raccourci Windows)
foreach ($old in @((Join-Path $menu "Priorite - Ajouter.lnk"), (Join-Path $menu "Priorites.lnk"), (Join-Path $startup "Priorites.lnk"))) {
    if (Test-Path $old) { Remove-Item $old; Write-Host "supprime: $old" }
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
    Write-Host "cree: $($s.Path)"
}

# Redemarre le resident sur la nouvelle version.
Get-Process priority, prio -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500
Start-Process $exe
Write-Host "Prio lance. Ctrl+Alt+A ajouter, Ctrl+Alt+P liste."
