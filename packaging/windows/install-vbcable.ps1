<#
  FreeMic helper: download VB-CABLE from VB-Audio's official server and start
  VB-Audio's own installer. Only runs when the user asks for it.

  - The package is downloaded from vb-audio.com (FreeMic does not redistribute it).
  - The installer's Authenticode signature is verified before it is started.
  - Windows asks for administrator approval (UAC); the VB-Audio setup window
    then asks you to click "Install Driver". A restart may be requested.

  -VerifyOnly downloads and verifies without starting the installer (testing).
#>
param([switch]$VerifyOnly)
$ErrorActionPreference = 'Stop'
$url = 'https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip'
$work = Join-Path $env:TEMP ("FreeMic-VBCABLE-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    Write-Host "Downloading VB-CABLE from $url ..."
    $zip = Join-Path $work 'vbcable.zip'
    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    Expand-Archive -Path $zip -DestinationPath (Join-Path $work 'x')
    $arch = if ([Environment]::Is64BitOperatingSystem) { 'VBCABLE_Setup_x64.exe' } else { 'VBCABLE_Setup.exe' }
    if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { $arch = 'VBCABLE_Setup_ARM64.exe' }
    $setup = Get-ChildItem -Path (Join-Path $work 'x') -Recurse -Filter $arch | Select-Object -First 1
    if (-not $setup) { throw "The download did not contain $arch." }
    $sig = Get-AuthenticodeSignature -FilePath $setup.FullName
    $subject = $sig.SignerCertificate.Subject
    if ($sig.Status -ne 'Valid' -or ($subject -notmatch 'Burel|VB-Audio')) {
        throw "Signature check failed for $($setup.Name): status $($sig.Status), signer '$subject'. Not starting it."
    }
    Write-Host "Signature OK: $subject"
    if ($VerifyOnly) { Write-Host "VerifyOnly: not starting the installer."; exit 0 }
    Write-Host 'Starting the VB-CABLE installer (Windows will ask for administrator approval)...'
    $p = Start-Process -FilePath $setup.FullName -WorkingDirectory $setup.DirectoryName -Verb RunAs -PassThru -Wait
    Write-Host "VB-CABLE installer finished (exit code $($p.ExitCode)). Restart Windows if it asked you to."
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
