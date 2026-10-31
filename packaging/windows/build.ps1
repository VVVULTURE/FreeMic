# Builds the Windows release: dist\FreeMic-<ver>-windows-setup.exe and
# dist\FreeMic-<ver>-windows-portable.zip. Requires Rust and Inno Setup 6.
#
# yt-dlp (the official standalone yt-dlp.exe) is included next to FreeMic.exe,
# so FreeMic never downloads program files itself. It is fetched here from
# yt-dlp's GitHub release (-YtDlpVersion, default: the newest release) and
# must match the SHA-256 published with that release.
param([string]$YtDlpVersion = '')
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $root
$ver = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
cargo build --release --bin freemic
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

# yt-dlp, verified against the release's SHA2-256SUMS (kept in dist\yt-dlp\<version> between builds)
if (-not $YtDlpVersion) {
    $YtDlpVersion = (Invoke-RestMethod 'https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest' -Headers @{ 'User-Agent' = 'FreeMic-build' }).tag_name
}
if ($YtDlpVersion -notmatch '^\d{4}\.\d{2}\.\d{2}(\.\d+)?$') { throw "unexpected yt-dlp version '$YtDlpVersion'" }
$yt = "$root\dist\yt-dlp\$YtDlpVersion"
New-Item -ItemType Directory -Force -Path $yt | Out-Null
$rel = "https://github.com/yt-dlp/yt-dlp/releases/download/$YtDlpVersion"
$src = "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$YtDlpVersion"
foreach ($f in @(@("$rel/SHA2-256SUMS", 'SHA2-256SUMS'), @("$rel/yt-dlp.exe", 'yt-dlp.exe'), @("$src/LICENSE", 'LICENSE'), @("$src/THIRD_PARTY_LICENSES.txt", 'THIRD_PARTY_LICENSES.txt'))) {
    if (-not (Test-Path "$yt\$($f[1])")) { Invoke-WebRequest $f[0] -OutFile "$yt\$($f[1])" -UseBasicParsing }
}
$want = (Get-Content "$yt\SHA2-256SUMS" | Where-Object { $_ -match '^([0-9a-f]{64})\s+\*?yt-dlp\.exe$' } | ForEach-Object { $Matches[1] }) | Select-Object -First 1
$have = (Get-FileHash "$yt\yt-dlp.exe" -Algorithm SHA256).Hash.ToLower()
if (-not $want -or $want -ne $have) {
    Remove-Item "$yt\yt-dlp.exe"
    throw "yt-dlp.exe $YtDlpVersion does not match its published SHA-256 (expected $want, got $have)"
}
Write-Host "yt-dlp $YtDlpVersion verified (sha256 $have)"

$stage = "$root\dist\stage"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item target\release\freemic.exe "$stage\FreeMic.exe"
Copy-Item "$yt\yt-dlp.exe" "$stage\yt-dlp.exe"
Copy-Item "$yt\LICENSE" "$stage\yt-dlp-LICENSE.txt"
Copy-Item "$yt\THIRD_PARTY_LICENSES.txt" "$stage\yt-dlp-THIRD_PARTY_LICENSES.txt"
@"
yt-dlp $YtDlpVersion (yt-dlp.exe, SHA-256 $have)

yt-dlp.exe is the official, unmodified standalone build from
https://github.com/yt-dlp/yt-dlp/releases/tag/$YtDlpVersion
FreeMic runs it as a separate program to download YouTube audio.

yt-dlp itself is released under the Unlicense (yt-dlp-LICENSE.txt). The
standalone build also contains Python and other components; as a whole it is
licensed under the GNU GPL version 3 or later (https://www.gnu.org/licenses/gpl-3.0.html).
The licenses of the bundled components are in yt-dlp-THIRD_PARTY_LICENSES.txt.

Source code:
  yt-dlp:   https://github.com/yt-dlp/yt-dlp/tree/$YtDlpVersion
            https://github.com/yt-dlp/yt-dlp/releases/download/$YtDlpVersion/yt-dlp.tar.gz
  bundled components: see yt-dlp-THIRD_PARTY_LICENSES.txt (each lists its project).

"Update yt-dlp" in FreeMic runs yt-dlp's own updater (yt-dlp -U), which
replaces this file with a newer official release.
"@ | Set-Content -Encoding utf8 "$stage\yt-dlp-SOURCE.txt"
Copy-Item LICENSE "$stage\LICENSE.txt"
Copy-Item THIRD-PARTY-NOTICES.md, README.md, assets\fonts\Inter-LICENSE.txt $stage
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
with VB-Audio's own installer (administrator approval required). FreeMic's Settings
screen opens the VB-CABLE page and explains the steps.

yt-dlp.exe (for the YouTube page) is included; see yt-dlp-SOURCE.txt.
"@ | Set-Content -Encoding utf8 "$pdir\portable.txt"
Compress-Archive -Path $pdir -DestinationPath "$pdir.zip"
Get-ChildItem "$root\dist" -File | Select-Object Name, @{n = 'MB'; e = { [math]::Round($_.Length / 1MB, 2) } }
