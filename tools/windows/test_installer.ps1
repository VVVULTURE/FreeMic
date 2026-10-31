<#
  Installer tests that run without touching the real system audio setup.

  1. Builds a TEST variant of the installer (/DTestNoCable) that acts as if
     VB-CABLE were missing. A silent install must succeed and start nothing
     (the "open the VB-CABLE web page" option is for interactive installs only).
  2. Silent install of the real installer into a temp folder: files (FreeMic,
     the bundled yt-dlp and its licenses), uninstall entry, Start menu; then
     silent uninstall: everything removed, including what yt-dlp's updater
     may leave behind.

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

# --- 1. silent install without a cable ---------------------------------------
& $iscc /Q /DTestNoCable "/DSourceDir=$root\dist\stage" "/O$work" /Ftest-setup "$root\packaging\windows\freemic.iss"
Check 'test installer built' { Test-Path "$work\test-setup.exe" }
$dir = "$work\inst-test"
$p = Start-Process "$work\test-setup.exe" -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=`"$dir`"" -PassThru -Wait
Check 'silent install without VB-CABLE succeeded' { $p.ExitCode -eq 0 -and (Test-Path "$dir\FreeMic.exe") }
$p = Start-Process "$dir\unins000.exe" -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -PassThru -Wait
Start-Sleep -Seconds 2
Check 'test uninstall clean' { $p.ExitCode -eq 0 -and -not (Test-Path $dir) -and -not (Test-Path $uninstKey) }

# --- 2. real installer: silent install + uninstall --------------------------
$setup = Get-ChildItem "$root\dist\FreeMic-*-windows-setup.exe" | Select-Object -First 1
$dir = "$work\inst"
$p = Start-Process $setup.FullName -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', "/DIR=`"$dir`"" -PassThru -Wait
Check 'install exit code 0' { $p.ExitCode -eq 0 }
Check 'program installed' { (Get-FileHash "$dir\FreeMic.exe").Hash -eq (Get-FileHash "$root\dist\stage\FreeMic.exe").Hash }
Check 'yt-dlp installed next to FreeMic' { (Get-FileHash "$dir\yt-dlp.exe").Hash -eq (Get-FileHash "$root\dist\stage\yt-dlp.exe").Hash }
Check 'yt-dlp licenses and source note installed' { (Test-Path "$dir\yt-dlp-LICENSE.txt") -and (Test-Path "$dir\yt-dlp-THIRD_PARTY_LICENSES.txt") -and (Select-String "$dir\yt-dlp-SOURCE.txt" -Pattern 'github.com/yt-dlp/yt-dlp/tree/' -Quiet) }
Check 'no PowerShell helper installed' { -not (Get-ChildItem $dir -Recurse -Filter *.ps1) }
Check 'uninstall entry registered' { (Get-ItemProperty $uninstKey).DisplayName -eq 'FreeMic' }
Check 'documentation installed' { (Test-Path "$dir\docs\INSTALL.md") -and (Test-Path "$dir\docs\images\main-window.png") -and -not (Test-Path "$dir\docs\WORKLOG.md") }
Check 'Start menu shortcut' { Test-Path "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\FreeMic\FreeMic.lnk" }
Check 'no desktop shortcut by default' { -not (Test-Path "$([Environment]::GetFolderPath('Desktop'))\FreeMic.lnk") }
$ver = (& "$dir\FreeMic.exe" --version | Out-String).Trim()
Check "installed program runs ($ver)" { $ver -match '^FreeMic \d' }
$ytv = (& "$dir\yt-dlp.exe" --ignore-config --version | Out-String).Trim()
Check "installed yt-dlp runs ($ytv)" { $ytv -match '^\d{4}\.\d{2}\.\d{2}' }
# What yt-dlp's updater may leave behind is removed on uninstall too.
'x' | Set-Content "$dir\yt-dlp.exe.old"
$p = Start-Process "$dir\unins000.exe" -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -PassThru -Wait
Start-Sleep -Seconds 2
Check 'uninstall exit code 0' { $p.ExitCode -eq 0 }
Check 'program folder removed' { -not (Test-Path $dir) }
Check 'uninstall entry removed' { -not (Test-Path $uninstKey) }
Check 'Start menu entries removed' { -not (Test-Path "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\FreeMic") }

Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "== installer: $pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
