<#
  Tests FreeMic's real WASAPI microphone capture without the user's physical
  microphone: VB-CABLE's "CABLE Output" (a virtual capture device) is used as
  FreeMic's microphone, a 330 Hz tone is rendered into "CABLE Input" by
  fmtest, and FreeMic's mix goes to the null output (a WAV file), so nothing
  is ever sent to a real device. The cable's clock and the null output's
  clock differ, which also exercises the drift correction on a real device.

  Safety: aborts if any application has a session on CABLE Output (so no
  other program can hear the tone). The FreeMic window is off-screen.
#>
param([int]$Seconds = 12,
      [string]$Exe = "$PSScriptRoot\..\..\target\release\freemic.exe",
      [string]$Fmtest = "$PSScriptRoot\..\..\target\release\fmtest.exe")
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path; $Fmtest = (Resolve-Path $Fmtest).Path
$out = Join-Path $env:TEMP "freemic-capture-$(Get-Random)"
New-Item -ItemType Directory -Path "$out\data" | Out-Null
$pass = 0; $fail = 0
function Check($name, [scriptblock]$cond) {
    $ok = $false; try { $ok = [bool](& $cond) } catch { }
    if ($ok) { Write-Host "PASS: $name"; $script:pass++ } else { Write-Host "FAIL: $name"; $script:fail++ }
}

$list = & $Fmtest list
$inCable = $false; $busy = @(); $cableId = $null
foreach ($l in $list) {
    if ($l -match '^\s{2}\S') {
        $inCable = $l -match 'CABLE Output'
        if ($inCable -and $l -match 'id=(\S+)') { $cableId = $Matches[1] }
    } elseif ($inCable -and $l -match 'session pid (\d+) \((.*)\) state (\w+)' -and $Matches[1] -ne '0') { $busy += $l.Trim() }
}
if (-not $cableId) { throw "CABLE Output not found" }
if ($busy.Count -gt 0) { throw "ABORT: applications are attached to CABLE Output: $($busy -join '; ')" }
Write-Host "CABLE Output has no application sessions - safe to test."

& $Fmtest tone "$out\tone330.wav" $Seconds 330 0.3 | Out-Null
$settings = @{ mic_volume = 100; master_volume = 100; mic_device = @{ id = $cableId; name = 'CABLE Output (VB-Audio Virtual Cable)' } }
$settings | ConvertTo-Json | Out-File -Encoding ascii "$out\data\settings.json"
'{"version":1,"next_id":1,"sounds":[]}' | Out-File -Encoding ascii "$out\data\soundboard.json"

$env:FREEMIC_DATA_DIR = "$out\data"; $env:FREEMIC_TEST_OUTPUT = 'null'; $env:FREEMIC_NULL_OUT = "$out\mix.wav"
$env:FREEMIC_TEST_WINDOW = '1'; $env:FREEMIC_NO_HOTKEYS = '1'; $env:FREEMIC_STATS = '1'
$app = Start-Process -FilePath $Exe -PassThru
try {
    $log = "$out\data\freemic.log"
    for ($i = 0; $i -lt 50; $i++) { if ((Test-Path $log) -and (Select-String -Path $log -Pattern 'mic started' -Quiet)) { break }; Start-Sleep -Milliseconds 200 }
    Check 'FreeMic captures CABLE Output through WASAPI' { Select-String -Path $log -Pattern 'mic started: CABLE Output' -Quiet }
    Check 'mix goes to the null output (no device)' { Select-String -Path $log -Pattern 'output started: Null output' -Quiet }
    Start-Sleep -Seconds 1
    & $Fmtest play "CABLE Input" "$out\tone330.wav" | Out-Null
    Start-Sleep -Seconds 1
    & $Exe --quit | Out-Null
    Check 'FreeMic exited' { $app.WaitForExit(10000) }
} finally {
    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    Remove-Item Env:FREEMIC_DATA_DIR, Env:FREEMIC_TEST_OUTPUT, Env:FREEMIC_NULL_OUT, Env:FREEMIC_TEST_WINDOW, Env:FREEMIC_NO_HOTKEYS, Env:FREEMIC_STATS -ErrorAction SilentlyContinue
}
$an = & $Fmtest analyze "$out\mix.wav" 330
$an | Set-Content "$out\analysis.txt"; $an | ForEach-Object { Write-Host $_ }
$vals = foreach ($l in $an) { if ($l -match 't=\s*(\d+)s rms [\d.]+\s+330Hz ([\d.]+)') { [double]$Matches[2] } }
# Every full second of the tone must be at the expected level (first/last are partial).
$good = @($vals | Where-Object { [math]::Abs($_ - 0.30) -lt 0.02 }).Count
Check "mic signal arrives at the right level ($good of $Seconds s at 0.30 +/- 0.02)" { $good -ge $Seconds - 2 }
$lv = $vals | Where-Object { $_ -gt 0.2 } | Measure-Object -Minimum -Maximum
Write-Host ("330 Hz level over the full seconds: min {0:N3}, max {1:N3}" -f $lv.Minimum, $lv.Maximum)
Check 'no clipping' { ($an -join "`n") -match 'clipped_samples 0' }
$stats = Select-String -Path "$out\data\freemic.log" -Pattern 'stats: mic' | ForEach-Object Line
$after = @($stats | Select-Object -Skip 2)
Write-Host "last stats: $($stats[-1])"
$first = if ($after.Count -gt 0 -and $after[0] -match 'mic_underruns (\d+)') { [int]$Matches[1] } else { -1 }
Check 'no mic underruns after start-up' { $stats[-1] -match "mic_underruns $first " }
Write-Host "== capture: $pass passed, $fail failed  (artifacts: $out)"
if ($fail -gt 0) { exit 1 }
