<#
  YouTube end-to-end test of the real FreeMic.exe from the portable zip
  (needs the internet).

  FreeMic runs with the null audio backend (no audio device is opened), no
  global hotkeys, and an off-screen window that never takes focus. It looks up
  a video from a link inside a mix and adds it to the soundboard exactly like
  clicking "Add to soundboard" (FREEMIC_TEST_YT_ADD), using the yt-dlp.exe
  that comes with FreeMic.

  Also checks what made Microsoft Defender flag FreeMic 0.2.0: FreeMic must
  not create any program file, and Defender must not report anything new.
#>
param([string]$Zip = (Get-ChildItem "$PSScriptRoot\..\..\dist\FreeMic-*-windows-portable.zip" | Select-Object -First 1).FullName)
$ErrorActionPreference = 'Stop'
$pass = 0; $fail = 0
function Check($name, [scriptblock]$cond) {
    $ok = $false; try { $ok = [bool](& $cond) } catch { }
    if ($ok) { Write-Host "PASS: $name"; $script:pass++ } else { Write-Host "FAIL: $name"; $script:fail++ }
}
$video = 'jNQXAC9IVRw'   # "Me at the zoo", 19 s
$work = Join-Path $env:TEMP "freemic-yttest-$(Get-Random)"
Expand-Archive $Zip -DestinationPath $work
$exe = (Get-ChildItem $work -Recurse -Filter FreeMic.exe | Select-Object -First 1).FullName
$home_ = Split-Path $exe
$data = "$work\data"
New-Item -ItemType Directory -Path $data | Out-Null
$detectionsBefore = @(Get-MpThreatDetection -ErrorAction SilentlyContinue).Count
$programsBefore = @(Get-ChildItem $work -Recurse -Include *.exe, *.dll, *.ps1, *.bat, *.cmd | ForEach-Object FullName)
$started = Get-Date

$env:FREEMIC_BACKEND = 'null'; $env:FREEMIC_TEST_WINDOW = '1'; $env:FREEMIC_NO_HOTKEYS = '1'; $env:FREEMIC_DATA_DIR = $data
$env:FREEMIC_TEST_VIEW = 'youtube'; $env:FREEMIC_TEST_YT_ADD = $video
try {
    $app = Start-Process $exe -PassThru
    $board = "$data\soundboard.json"
    $deadline = (Get-Date).AddSeconds(120)
    while ((Get-Date) -lt $deadline -and -not ((Test-Path $board) -and (Select-String $board -Pattern $video -Quiet))) { Start-Sleep -Milliseconds 500 }
    $took = ((Get-Date) - $started).TotalSeconds
    Check "added to the soundboard ($([math]::Round($took, 1)) s)" { Select-String $board -Pattern $video -Quiet }
    Check 'named after the video title' { (Get-Content $board -Raw | ConvertFrom-Json).sounds.name -contains 'Me at the zoo' }
    Check "audio stored as sounds\youtube\$video.m4a" { (Get-Item "$data\sounds\youtube\$video.m4a").Length -gt 100000 }
    Check 'exactly one file downloaded' { @(Get-ChildItem "$data\sounds\youtube" -Recurse -File).Count -eq 1 }
    Check 'used the yt-dlp that comes with FreeMic' { Select-String "$data\freemic.log" -SimpleMatch -Pattern "yt-dlp: $home_\yt-dlp.exe (20" -Quiet }
    Start-Sleep -Seconds 1
    Check 'no yt-dlp process left running' { -not (Get-Process yt-dlp -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$work*" }) }
    & $exe --quit | Out-Null
    Check 'FreeMic exited on --quit' { $app.WaitForExit(10000) }
} finally {
    foreach ($v in 'FREEMIC_BACKEND', 'FREEMIC_TEST_WINDOW', 'FREEMIC_NO_HOTKEYS', 'FREEMIC_DATA_DIR', 'FREEMIC_TEST_VIEW', 'FREEMIC_TEST_YT_ADD') { Remove-Item "env:$v" -ErrorAction SilentlyContinue }
    if ($app -and -not $app.HasExited) { Stop-Process -Id $app.Id -Force }
}
$programsAfter = @(Get-ChildItem $work -Recurse -Include *.exe, *.dll, *.ps1, *.bat, *.cmd | ForEach-Object FullName)
Check 'FreeMic created no program files' { -not (Compare-Object $programsBefore $programsAfter) }
Start-Sleep -Seconds 5
$new = @(Get-MpThreatDetection -ErrorAction SilentlyContinue | Where-Object { $_.InitialDetectionTime -ge $started })
Check "Microsoft Defender reported nothing new (detections before: $detectionsBefore)" { $new.Count -eq 0 }
$new | ForEach-Object { Write-Host "  Defender: $($_.ThreatID) $($_.Resources -join ', ')" }
Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "== youtube (Windows): $pass passed, $fail failed"
if ($fail -gt 0) { exit 1 }
