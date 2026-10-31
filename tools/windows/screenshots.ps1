<#
  Screenshots of FreeMic with a demo soundboard, for the docs and for checking
  the layout by eye. Uses the null audio backend (no audio device is opened),
  no global hotkeys, and an off-screen window that never takes focus.

  .\tools\windows\screenshots.ps1 -Out docs\images            # docs images
  .\tools\windows\screenshots.ps1 -Out $env:TEMP\shots -All   # every size/theme
#>
param([string]$Out = "docs\images", [switch]$All)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
$exe = "$root\target\release\freemic.exe"
$fmtest = "$root\target\release\fmtest.exe"
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path

function New-Demo([string]$dir, [bool]$withSounds) {
    if (Test-Path $dir) { Remove-Item $dir -Recurse -Force }
    New-Item -ItemType Directory -Path "$dir\sounds" | Out-Null
    $sounds = @()
    if ($withSounds) {
        $list = @(
            @{ n = 'Airhorn'; f = 440; s = 2; hk = 'control+shift+F1' },
            @{ n = 'Applause'; f = 523; s = 4; c = @(74, 108, 178); hk = 'control+shift+F2' },
            @{ n = 'Drum roll'; f = 587; s = 3; hk = 'control+shift+F3' },
            @{ n = 'Sad trombone'; f = 659; s = 4; c = @(196, 88, 64) },
            @{ n = 'Rimshot'; f = 698; s = 1 },
            @{ n = 'Crickets'; f = 784; s = 6; c = @(52, 136, 88); loop = $true },
            @{ n = 'Victory fanfare'; f = 880; s = 5; c = @(122, 90, 180) },
            @{ n = 'Boing'; f = 988; s = 1 },
            @{ n = 'Elevator music'; f = 330; s = 30; c = @(36, 132, 134); loop = $true },
            @{ n = 'Ding'; f = 1047; s = 1 }
        )
        $id = 1
        foreach ($x in $list) {
            $file = "sounds\demo$id.wav"
            & $fmtest tone "$dir\$file" $x.s $x.f 0.2 | Out-Null
            $sounds += [ordered]@{
                id = $id; name = $x.n; file = $file
                mode = $(if ($x.loop) { 'Loop' } else { 'Once' })
                color = $x.c; hotkey = $x.hk; duration = [double]$x.s
            }
            $id++
        }
    }
    $board = [ordered]@{ version = 1; sounds = $sounds; next_id = $sounds.Count + 1 }
    [IO.File]::WriteAllText("$dir\soundboard.json", ($board | ConvertTo-Json -Depth 5))
}

# yt-dlp for the YouTube screenshots: the verified copy packaging\windows\build.ps1 fetched.
$ytExe = Get-ChildItem "$root\dist\yt-dlp\*\yt-dlp.exe" -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1

function Shot([string]$name, [int]$w, [int]$h, [string]$theme, [string]$view, [string]$play, [bool]$withSounds, [bool]$muted, [string]$ytSearch = '') {
    $dir = Join-Path $env:TEMP "freemic-shot-$name"
    New-Demo $dir $withSounds
    if ($ytSearch) {
        if (-not $ytExe) { throw "run packaging\windows\build.ps1 first (it fetches yt-dlp.exe)" }
        $env:FREEMIC_YTDLP = $ytExe.FullName
    }
    $settings = [ordered]@{ version = 1; window_size = @($w, $h); theme = $theme; mic_enabled = $false; mic_muted = $muted; stop_hotkey = 'control+shift+F12' }
    [IO.File]::WriteAllText("$dir\settings.json", ($settings | ConvertTo-Json))
    $env:FREEMIC_BACKEND = 'null'; $env:FREEMIC_TEST_WINDOW = '1'; $env:FREEMIC_NO_HOTKEYS = '1'; $env:FREEMIC_DATA_DIR = $dir
    $env:FREEMIC_TEST_VIEW = $view; $env:FREEMIC_TEST_PLAY = $play; $env:FREEMIC_TEST_YT_SEARCH = $ytSearch
    $p = Start-Process $exe -PassThru
    try {
        $deadline = (Get-Date).AddSeconds(15)
        while ((Get-Date) -lt $deadline) {
            $p.Refresh()
            if ($p.MainWindowHandle -ne [IntPtr]::Zero) { break }
            Start-Sleep -Milliseconds 200
        }
        Start-Sleep -Milliseconds $(if ($ytSearch) { 25000 } elseif ($play) { 2600 } else { 1200 })
        & "$root\tools\win_capture.ps1" -ProcessId $p.Id -Out "$Out\$name.png"
    } finally {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        foreach ($v in 'FREEMIC_BACKEND', 'FREEMIC_TEST_WINDOW', 'FREEMIC_NO_HOTKEYS', 'FREEMIC_DATA_DIR', 'FREEMIC_TEST_VIEW', 'FREEMIC_TEST_PLAY', 'FREEMIC_TEST_YT_SEARCH', 'FREEMIC_YTDLP') { Remove-Item "env:$v" -ErrorAction SilentlyContinue }
        Start-Sleep -Milliseconds 300
        Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Shot 'main-window' 900 600 'Dark' '' '9' $true $false
Shot 'settings' 900 600 'Dark' 'settings' '' $true $false
Shot 'youtube' 900 600 'Dark' 'youtube' '' $true $false 'vine boom sound effect'
if ($All) {
    Shot 'main-light' 900 600 'Light' '' '4' $true $false
    Shot 'settings-light' 900 600 'Light' 'settings' '' $true $false
    Shot 'small' 480 400 'Dark' '' '5' $true $true
    Shot 'small-settings' 480 400 'Dark' 'settings' '' $true $false
    Shot 'medium' 640 480 'Light' '' '' $true $false
    Shot 'empty' 640 480 'Dark' '' '' $false $false
    Shot 'wide' 1280 760 'Dark' '' '2' $true $false
    Shot 'youtube-light' 640 480 'Light' 'youtube' '' $true $false 'airhorn'
    Shot 'youtube-setup' 520 420 'Dark' 'youtube' '' $true $false
}
