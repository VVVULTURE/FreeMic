<#
  Portable zip test: extract to %TEMP%, start FreeMic with the null audio
  backend (no devices are opened) off-screen, quit it over the command pipe,
  and check that all data went to the "data" folder next to FreeMic.exe and
  nothing to %APPDATA% or HKCU\Software, and that FreeMic found the yt-dlp
  that comes with it.
#>
param([string]$Zip = (Get-ChildItem "$PSScriptRoot\..\..\dist\FreeMic-*-windows-portable.zip" | Select-Object -First 1).FullName)
$ErrorActionPreference = 'Stop'
$pass = 0; $fail = 0
function Check($name, [scriptblock]$cond) {
    $ok = $false; try { $ok = [bool](& $cond) } catch { }
    if ($ok) { Write-Host "PASS: $name"; $script:pass++ } else { Write-Host "FAIL: $name"; $script:fail++ }
}
# The installed FreeMic's data folder (if any) must stay exactly as it is.
function Snapshot {
    if (-not (Test-Path "$env:APPDATA\FreeMic")) { return 'absent' }
    (Get-ChildItem "$env:APPDATA\FreeMic" -Recurse -Force | ForEach-Object { "$($_.FullName)|$($_.Length)|$($_.LastWriteTimeUtc.Ticks)" } | Sort-Object) -join "`n"
}
$before = Snapshot
$dir = Join-Path $env:TEMP "freemic-portable-$(Get-Random)"
Expand-Archive $Zip -DestinationPath $dir
$exe = (Get-ChildItem $dir -Recurse -Filter FreeMic.exe | Select-Object -First 1).FullName
$home_ = Split-Path $exe
Check 'zip contains portable.txt' { Test-Path "$home_\portable.txt" }
Check 'zip contains yt-dlp and its licenses' { (Test-Path "$home_\yt-dlp.exe") -and (Test-Path "$home_\yt-dlp-THIRD_PARTY_LICENSES.txt") -and (Test-Path "$home_\yt-dlp-SOURCE.txt") }
Check 'no PowerShell helper in the zip' { -not (Get-ChildItem $home_ -Recurse -Filter *.ps1) }
# Stand-in for the yt-dlp.exe that FreeMic 0.2.0 downloaded into the data folder.
New-Item -ItemType Directory -Force -Path "$home_\data\tools" | Out-Null
'left by 0.2.0' | Set-Content "$home_\data\tools\yt-dlp.exe"
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
Check 'removed the yt-dlp.exe that 0.2.0 had downloaded' { -not (Test-Path "$home_\data\tools\yt-dlp.exe") -and (Select-String -Path "$home_\data\freemic.log" -Pattern 'left by FreeMic 0.2.0' -Quiet) }
Check 'FreeMic found the bundled yt-dlp' { Select-String -Path "$home_\data\freemic.log" -SimpleMatch -Pattern "yt-dlp: $home_\yt-dlp.exe (20" -Quiet }
Check 'nothing written to %APPDATA%\FreeMic' { (Snapshot) -eq $before }
Check 'nothing written to HKCU\Software\FreeMic' { -not (Test-Path 'HKCU:\Software\FreeMic') }
Check 'no autostart entry' { -not (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).FreeMic }
Remove-Item $dir -Recurse -Force
Write-Host "== portable: $pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
