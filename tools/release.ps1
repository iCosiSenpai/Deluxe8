<#
.SYNOPSIS
  Builds a Deluxe8 release on this PC: Compat engine, launcher, NSIS installer and SHA256SUMS.txt in out/release/<version>.
  With -Thumbprint every executable is code-signed with a certificate from the Windows certificate store (a cloud or USB
  token certificate such as Certum SimplySign shows up there once its software is running).

.EXAMPLE
  pwsh tools/release.ps1                                       # unsigned (for testing only: SmartScreen will warn)
  pwsh tools/release.ps1 -Thumbprint 0123456789ABCDEF...       # signed + RFC 3161 timestamp
  pwsh tools/release.ps1 -Thumbprint ... -SkipEngine           # reuse out/engine/win-x64 from an earlier build

.NOTES
  Signing details and the alternatives (Azure Artifact Signing in CI, SignPath): docs/RELEASING.md.
#>
param(
    [string]$Thumbprint,
    [string]$TimestampUrl = 'http://timestamp.digicert.com',
    [switch]$SkipEngine
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = Split-Path $PSScriptRoot -Parent
$Launcher = Join-Path $Root 'apps/launcher'
$Engine = Join-Path $Root 'out/engine/win-x64'
$Version = (Get-Content (Join-Path $Launcher 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
$Out = Join-Path $Root "out/release/v$Version"

function Write-Step([string]$Message) { Write-Host "==> $Message" -ForegroundColor Cyan }
function Invoke-Native {
    param([string]$Exe, [string[]]$Arguments)
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed with exit code $LASTEXITCODE" }
}

$signtool = $null
if ($Thumbprint) {
    $Thumbprint = $Thumbprint -replace '\s', ''
    $cert = Get-ChildItem Cert:\CurrentUser\My, Cert:\LocalMachine\My | Where-Object Thumbprint -eq $Thumbprint | Select-Object -First 1
    if (-not $cert) { throw "No certificate with thumbprint $Thumbprint in CurrentUser\My or LocalMachine\My" }
    if (-not ($cert.EnhancedKeyUsageList.ObjectId -contains '1.3.6.1.5.5.7.3.3')) { throw "$($cert.Subject) is not a code-signing certificate" }
    if ($cert.NotAfter -lt (Get-Date)) { throw "$($cert.Subject) expired on $($cert.NotAfter)" }
    $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
        Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
    if (-not $signtool) { throw 'signtool.exe not found: install the Windows SDK (it comes with the Visual Studio C++ workload)' }
    Write-Host "   certificate: $($cert.Subject) (valid until $($cert.NotAfter.ToString('yyyy-MM-dd')))"
} else {
    Write-Warning 'No -Thumbprint: the build is unsigned and SmartScreen will warn about it.'
}

function Invoke-Sign([string]$File) {
    Invoke-Native $signtool.FullName @('sign', '/sha1', $Thumbprint, '/fd', 'sha256', '/tr', $TimestampUrl, '/td', 'sha256',
        '/d', 'Deluxe8', $File)
}

if (-not $SkipEngine) {
    Write-Step 'Compat engine'
    Invoke-Native pwsh @('-NoProfile', '-File', (Join-Path $PSScriptRoot 'engine.ps1'), 'all')
}
foreach ($exe in 'deluxe8-engine.exe', 'deluxe8-room.exe') {
    if (-not (Test-Path (Join-Path $Engine $exe))) { throw "$exe missing in ${Engine}: run without -SkipEngine" }
}
if ($signtool) {
    Write-Step 'Signing the engine'
    foreach ($exe in 'deluxe8-engine.exe', 'deluxe8-room.exe') { Invoke-Sign (Join-Path $Engine $exe) }
}

Write-Step "Launcher $Version and installer"
Push-Location $Launcher
try {
    Invoke-Native pnpm @('install', '--frozen-lockfile')
    $cli = @('tauri', 'build', '--bundles', 'nsis', '--config', 'src-tauri/tauri.bundle-windows.conf.json')
    if ($signtool) {
        # Tauri signs the launcher, the uninstaller and the installer with signtool and this certificate
        $sign = @{ bundle = @{ windows = @{ certificateThumbprint = $Thumbprint; digestAlgorithm = 'sha256'; timestampUrl = $TimestampUrl; tsp = $true } } }
        $cli += @('--config', ($sign | ConvertTo-Json -Depth 5 -Compress))
    }
    Invoke-Native pnpm $cli
} finally {
    Pop-Location
}

Write-Step "Release files in $Out"
New-Item -ItemType Directory -Force $Out | Out-Null
Get-ChildItem $Out -File | Remove-Item
$installer = Get-ChildItem (Join-Path $Launcher "src-tauri/target/release/bundle/nsis") -Filter "Deluxe8_${Version}_*-setup.exe"
if (-not $installer) { throw 'installer not found' }
Copy-Item $installer.FullName $Out
if ($signtool) {
    $check = @(Get-ChildItem $Out -Filter *.exe) + @(Get-Item (Join-Path $Engine '*.exe'))
    foreach ($f in $check) {
        $s = Get-AuthenticodeSignature $f.FullName
        '   {0}: {1}' -f $f.Name, $s.Status
        if ($s.Status -ne 'Valid') { throw "$($f.Name) is not validly signed" }
    }
}
Get-ChildItem $Out -File | Where-Object Name -ne 'SHA256SUMS.txt' |
    ForEach-Object { '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name } |
    Set-Content -Encoding ascii (Join-Path $Out 'SHA256SUMS.txt')
Get-Content (Join-Path $Out 'SHA256SUMS.txt')
Write-Host 'Before publishing: docs/RELEASING.md, "Release checklist".' -ForegroundColor Green
