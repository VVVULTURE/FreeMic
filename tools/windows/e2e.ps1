<#
  FreeMic Windows end-to-end test through a virtual cable (VB-CABLE).

  Safety: aborts if any application (Discord, a game, ...) has an audio session
  on "CABLE Output", so test tones can never reach another program. Uses a
  synthetic 220 Hz microphone (FREEMIC_TEST_MIC=null) - the physical
  microphone is never opened. Nothing is played to speakers or headphones.
  The FreeMic window is created off-screen and never takes focus.

  The "receiving application" is fmtest record, which captures CABLE Output in
  WASAPI shared mode exactly like Discord or a game would.
#>
param([string]$Exe = "$PSScriptRoot\..\..\target\release\freemic.exe",
      [string]$Fmtest = "$PSScriptRoot\..\..\target\release\fmtest.exe")
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path; $Fmtest = (Resolve-Path $Fmtest).Path
$out = Join-Path $env:TEMP "freemic-e2e-$(Get-Random)"
New-Item -ItemType Directory -Path "$out\data" | Out-Null
$pass = 0; $fail = 0
function Check($name, [scriptblock]$cond) {
    $ok = $false; try { $ok = [bool](& $cond) } catch { }
    if ($ok) { Write-Host "PASS: $name"; $script:pass++ } else { Write-Host "FAIL: $name"; $script:fail++ }
}

function Assert-CableIdle {
    $list = & $Fmtest list
    $inCable = $false; $busy = @()
    foreach ($l in $list) {
        if ($l -match '^\s{2}\S') { $inCable = $l -match 'CABLE Output' }
        elseif ($inCable -and $l -match 'session pid (\d+) \((.*)\) state (\w+)' -and $Matches[1] -ne '0') { $busy += $l.Trim() }
    }
    if ($busy.Count -gt 0) { throw "ABORT: applications are attached to CABLE Output: $($busy -join '; ')" }
}
Assert-CableIdle
Write-Host "CABLE Output has no application sessions - safe to test."

& $Fmtest tone "$out\beep440.wav" 2 440 0.5 | Out-Null
& $Fmtest tone "$out\loop1k.wav" 1 1000 0.5 | Out-Null
$board = @{ version = 1; next_id = 3; sounds = @(
    @{ id = 1; name = 'Beep'; file = "$out\beep440.wav"; mode = 'Once'; duration = 2.0 },
    @{ id = 2; name = 'Hum'; file = "$out\loop1k.wav"; mode = 'Loop'; duration = 1.0 }) }
$board | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 "$out\data\soundboard.json"
'{"sound_volume":100,"mic_volume":100,"master_volume":100}' | Set-Content -Encoding utf8 "$out\data\settings.json"

$env:FREEMIC_DATA_DIR = "$out\data"; $env:FREEMIC_TEST_MIC = 'null'; $env:FREEMIC_TEST_WINDOW = '1'; $env:FREEMIC_NO_HOTKEYS = '1'; $env:FREEMIC_STATS = '1'
$app = Start-Process -FilePath $Exe -PassThru
try {
    $log = "$out\data\freemic.log"
    for ($i = 0; $i -lt 50; $i++) { if ((Test-Path $log) -and (Select-String -Path $log -Pattern 'output started' -Quiet)) { break }; Start-Sleep -Milliseconds 200 }
    Check 'FreeMic opened the virtual cable' { Select-String -Path $log -Pattern 'output started: CABLE Input' -Quiet }
    Check 'synthetic mic running' { Select-String -Path $log -Pattern 'mic started' -Quiet }
    & $Exe --stop | Out-Null; $code = $LASTEXITCODE
    Check 'command pipe answers' { $code -eq 0 }
    # Underrun counters once startup is over (a hiccup while the app is still launching
    # is recovered with a fade and is reported separately).
    Start-Sleep -Milliseconds 600
    $base = (Select-String -Path $log -Pattern 'stats: mic' | Select-Object -Last 1).Line
    $baseMic = if ($base -match 'mic_underruns (\d+)') { [int]$Matches[1] } else { -1 }
    Write-Host "mic underruns during startup: $baseMic"

    # Receiving application records CABLE Output while the soundboard is driven.
    $rec = Start-Process -FilePath $Fmtest -ArgumentList @('record', '"CABLE Output"', '15', "`"$out\received.wav`"") -PassThru -NoNewWindow
    Start-Sleep -Seconds 2; & $Exe --play Beep
    Start-Sleep -Seconds 4; & $Exe --play Hum
    Start-Sleep -Seconds 2; & $Exe --play Beep
    Start-Sleep -Seconds 2; & $Exe --stop
    $rec.WaitForExit(30000) | Out-Null
    Assert-CableIdle
    $an = & $Fmtest analyze "$out\received.wav" 220 440 1000
    $an | Set-Content "$out\analysis.txt"; $an | ForEach-Object { Write-Host $_ }
    $rows = @{}
    foreach ($l in $an) { if ($l -match 't=\s*(\d+)s rms ([\d.]+)\s+220Hz ([\d.]+)\s+440Hz ([\d.]+)\s+1000Hz ([\d.]+)') { $rows[[int]$Matches[1]] = @{ m = [double]$Matches[3]; b = [double]$Matches[4]; h = [double]$Matches[5] } } }
    $t0 = ($rows.Keys | Sort-Object | Where-Object { $rows[$_].b -gt 0.25 } | Select-Object -First 1)
    Write-Host "Beep first audible in second $t0"
    function Near($a, $b, $tol) { [math]::Abs($a - $b) -lt $tol }
    $r = { param($k) $rows[[int]$t0 + $k] }
    # (0.03: the onset may fall a few ms before a second boundary; a playing Beep reads 0.5)
    Check 'before Beep: mic only'  { $x = & $r -1; (Near $x.m 0.25 0.04) -and (Near $x.b 0 0.03) }
    Check 'mic + Beep mixed'       { $x = & $r 1; (Near $x.m 0.25 0.04) -and (Near $x.b 0.5 0.06) }
    Check 'Beep ended by itself'   { $x = & $r 3; (Near $x.b 0 0.01) -and (Near $x.m 0.25 0.04) }
    Check 'loop playing with mic'  { $x = & $r 5; (Near $x.h 0.5 0.06) -and (Near $x.m 0.25 0.04) }
    Check 'Beep replaced loop'     { $x = & $r 7; (Near $x.h 0 0.01) -and (Near $x.b 0.5 0.06) }
    Check 'stopped, mic continues' { $x = & $r 9; (Near $x.b 0 0.01) -and (Near $x.h 0 0.01) -and (Near $x.m 0.25 0.04) }
    Check 'no clipping'            { ($an -join "`n") -match 'clipped_samples 0' }

    # Processing load of the whole process (audio threads + UI, window placed off-screen).
    function Measure-Load($label) {
        $app.Refresh(); $c0 = $app.TotalProcessorTime.TotalMilliseconds; Start-Sleep -Seconds 10; $app.Refresh()
        $cpu = ($app.TotalProcessorTime.TotalMilliseconds - $c0) / 10000 / [Environment]::ProcessorCount * 100
        "{0}: CPU {1:N3}% of all cores ({2} logical), working set {3:N1} MB, private {4:N1} MB" -f $label, $cpu, [Environment]::ProcessorCount, ($app.WorkingSet64 / 1MB), ($app.PrivateMemorySize64 / 1MB)
    }
    $perf = @(Measure-Load 'idle (mic passthrough active)')
    Assert-CableIdle
    & $Exe --play Hum | Out-Null
    $perf += Measure-Load 'playing a looped sound + mic'
    & $Exe --stop | Out-Null
    $perf | ForEach-Object { Write-Host $_ }
    $perf | Set-Content "$out\perf.txt"

    & $Exe --quit | Out-Null
    Check 'FreeMic exited on --quit' { $app.WaitForExit(10000) }
    Check 'log shows clean exit' { Select-String -Path $log -Pattern 'FreeMic exited' -Quiet }
    $last = (Select-String -Path $log -Pattern 'stats: mic' | Select-Object -Last 1).Line
    Write-Host "last stats: $last"
    Check 'no soundboard or mic underruns' { $last -match "mic_underruns $baseMic sound_underruns 0" -and -not (Select-String -Path $log -Pattern 'underrun #' -Quiet) }
}
finally {
    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force }
    Remove-Item Env:FREEMIC_DATA_DIR, Env:FREEMIC_TEST_MIC, Env:FREEMIC_TEST_WINDOW, Env:FREEMIC_NO_HOTKEYS, Env:FREEMIC_STATS -ErrorAction SilentlyContinue
}
Write-Host "== windows: $pass passed, $fail failed  (artifacts: $out)"
if ($fail -gt 0) { exit 1 }
