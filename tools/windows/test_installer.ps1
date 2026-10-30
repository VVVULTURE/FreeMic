<#
  Installer tests that run without touching the real system audio setup.

  1. Builds a TEST variant of the installer (/DTestNoCable) that acts as if
     VB-CABLE were missing and whose install-vbcable.ps1 is a stub that only
     writes a marker file. A silent install must NOT run it (no download, no
     UAC prompt during unattended installs).
  2. Silent install of the real installer into a temp folder: files, uninstall
     entry, Start menu; then silent uninstall: everything removed.

  Requires dist\stage from packaging\windows\build.ps1. Installs only into
  %TEMP% and only for the current user.
#>
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
$work = Join-Path $env:TEMP "freemic-insttest-$(Get-Random)"
New-Item -ItemType Directory -Path $work | Out-Null
$pass = 0; $fail = 0
function Check($name, [scriptblock]$cond) {
    $ok = $false; try { $ok = [bool](& $cond) } catch { }
    if ($ok) { Write-Host "PASS: $name"; $script:pass++ } else { Write-Host "FAIL: $name"; $script:fail++ }
}
$uninstKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{6A0E5E2B-9B67-4C4C-9E0B-5F3A61F2C1D7}_is1'
if (Test-Path $uninstKey) { throw "FreeMic is already installed for this user; uninstall it first" }

# --- 1. silent install never starts the VB-CABLE download ---------------------
$stage = "$work\stage"
Copy-Item "$root\dist\stage" $stage -Recurse
$marker = "$work\vbcable-stub-ran.txt"
"Set-Content -Path '$marker' -Value 'stub ran'" | Set-Content "$stage\install-vbcable.ps1"
& $iscc /Q /DTestNoCable "/DSourceDir=$stage" "/O$work" /Ftest-setup "$root\packaging\windows\freemic.iss"
Check 'test installer built' { Test-Path "$work\test-setup.exe" }
$dir = "$work\inst-test"
$p = Start-Process "$work\test-setup.exe" -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=`"$dir`"" -PassThru -Wait
Check 'test install succeeded' { $p.ExitCode -eq 0 -and (Test-Path "$dir\FreeMic.exe") }
Check 'silent install did not start the VB-CABLE download' { -not (Test-Path $marker) }
$p = Start-Process "$dir\unins000.exe" -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -PassThru -Wait
Start-Sleep -Seconds 2
Check 'test uninstall clean' { $p.ExitCode -eq 0 -and -not (Test-Path $dir) -and -not (Test-Path $uninstKey) }

# --- 2. real installer: silent install + uninstall --------------------------
$setup = Get-ChildItem "$root\dist\FreeMic-*-windows-setup.exe" | Select-Object -First 1
$dir = "$work\inst"
$p = Start-Process $setup.FullName -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=`"$dir`"" -PassThru -Wait
Check 'install exit code 0' { $p.ExitCode -eq 0 }
Check 'program installed' { (Get-FileHash "$dir\FreeMic.exe").Hash -eq (Get-FileHash "$root\dist\stage\FreeMic.exe").Hash }
Check 'uninstall entry registered' { (Get-ItemProperty $uninstKey).DisplayName -eq 'FreeMic' }
Check 'documentation installed' { (Test-Path "$dir\docs\INSTALL.md") -and (Test-Path "$dir\docs\images\main-window.png") -and -not (Test-Path "$dir\docs\WORKLOG.md") }
Check 'Start menu shortcut' { Test-Path "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\FreeMic\FreeMic.lnk" }
Check 'no desktop shortcut by default' { -not (Test-Path "$([Environment]::GetFolderPath('Desktop'))\FreeMic.lnk") }
$ver = (& "$dir\FreeMic.exe" --version | Out-String).Trim()
Check "installed program runs ($ver)" { $ver -match '^FreeMic \d' }
$p = Start-Process "$dir\unins000.exe" -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -PassThru -Wait
Start-Sleep -Seconds 2
Check 'uninstall exit code 0' { $p.ExitCode -eq 0 }
Check 'program folder removed' { -not (Test-Path $dir) }
Check 'uninstall entry removed' { -not (Test-Path $uninstKey) }
Check 'Start menu entries removed' { -not (Test-Path "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\FreeMic") }

Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "== installer: $pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
