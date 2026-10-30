<#
  Portable zip test: extract to %TEMP%, start FreeMic with the null audio
  backend (no devices are opened) off-screen, quit it over the command pipe,
  and check that all data went to the "data" folder next to FreeMic.exe and
  nothing to %APPDATA% or HKCU\Software.
#>
param([string]$Zip = (Get-ChildItem "$PSScriptRoot\..\..\dist\FreeMic-*-windows-portable.zip" | Select-Object -First 1).FullName)
$ErrorActionPreference = 'Stop'
$pass = 0; $fail = 0
function Check($name, [scriptblock]$cond) {
    $ok = $false; try { $ok = [bool](& $cond) } catch { }
    if ($ok) { Write-Host "PASS: $name"; $script:pass++ } else { Write-Host "FAIL: $name"; $script:fail++ }
}
if (Test-Path "$env:APPDATA\FreeMic") { throw "$env:APPDATA\FreeMic exists already; this test needs it absent" }
$dir = Join-Path $env:TEMP "freemic-portable-$(Get-Random)"
Expand-Archive $Zip -DestinationPath $dir
$exe = (Get-ChildItem $dir -Recurse -Filter FreeMic.exe | Select-Object -First 1).FullName
$home_ = Split-Path $exe
Check 'zip contains portable.txt' { Test-Path "$home_\portable.txt" }
Check 'zip contains the VB-CABLE helper' { Test-Path "$home_\install-vbcable.ps1" }
$env:FREEMIC_BACKEND = 'null'; $env:FREEMIC_TEST_WINDOW = '1'; $env:FREEMIC_NO_HOTKEYS = '1'
try {
    $app = Start-Process $exe -PassThru
    Start-Sleep -Seconds 3
    Check 'FreeMic running' { -not $app.HasExited }
    & $exe --quit | Out-Null
    Check 'quit over the command pipe' { $app.WaitForExit(10000) }
} finally {
    Remove-Item Env:FREEMIC_BACKEND, Env:FREEMIC_TEST_WINDOW, Env:FREEMIC_NO_HOTKEYS -ErrorAction SilentlyContinue
    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force }
}
Check 'settings saved next to the program' { Test-Path "$home_\data\settings.json" }
Check 'log says portable mode' { Select-String -Path "$home_\data\freemic.log" -Pattern 'portable: true' -Quiet }
Check 'nothing written to %APPDATA%' { -not (Test-Path "$env:APPDATA\FreeMic") }
Check 'nothing written to HKCU\Software\FreeMic' { -not (Test-Path 'HKCU:\Software\FreeMic') }
Check 'no autostart entry' { -not (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).FreeMic }
Remove-Item $dir -Recurse -Force
Write-Host "== portable: $pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
