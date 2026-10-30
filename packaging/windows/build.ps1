# Builds the Windows release: dist\FreeMic-<ver>-windows-setup.exe and
# dist\FreeMic-<ver>-windows-portable.zip. Requires Rust and Inno Setup 6.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $root
$ver = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
cargo build --release --bin freemic
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
$stage = "$root\dist\stage"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item target\release\freemic.exe "$stage\FreeMic.exe"
Copy-Item packaging\windows\install-vbcable.ps1 $stage
Copy-Item LICENSE "$stage\LICENSE.txt"
Copy-Item THIRD-PARTY-NOTICES.md, README.md $stage
# User documentation (the development work log stays out)
New-Item -ItemType Directory -Path "$stage\docs\images" | Out-Null
Get-ChildItem docs\*.md | Where-Object Name -ne 'WORKLOG.md' | Copy-Item -Destination "$stage\docs"
Copy-Item docs\images\*.png "$stage\docs\images"

# Installer
$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 (ISCC.exe) not found" }
& $iscc /Q "/DAppVersion=$ver" "/DSourceDir=$stage" packaging\windows\freemic.iss
if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }

# Portable zip: same files plus portable.txt, which keeps all data in .\data
$pname = "FreeMic-$ver-windows-portable"
$pdir = "$root\dist\$pname"
Remove-Item -Recurse -Force $pdir, "$pdir.zip" -ErrorAction SilentlyContinue
Copy-Item -Recurse $stage $pdir
@"
FreeMic portable

This file switches FreeMic to portable mode: settings, the soundboard and imported
sounds are stored in the "data" folder next to FreeMic.exe, and nothing is written
to your user profile or the registry (unless you enable "Start FreeMic when I sign in").

FreeMic still needs a virtual audio cable driver for its microphone: VB-CABLE from
vb-audio.com. Drivers cannot be portable - VB-CABLE must be installed once on each PC
(administrator approval required). FreeMic's Settings screen can download and start
the official VB-CABLE installer for you.
"@ | Set-Content -Encoding utf8 "$pdir\portable.txt"
Compress-Archive -Path $pdir -DestinationPath "$pdir.zip"
Get-ChildItem "$root\dist" -File | Select-Object Name, @{n = 'MB'; e = { [math]::Round($_.Length / 1MB, 2) } }
