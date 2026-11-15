<#
.SYNOPSIS
  Deluxe8 engine build tool for Windows (x64).

.DESCRIPTION
  Builds the tailored Deluxe8 engine from the pinned Eden revision in engine/upstream.json:
    toolchain  - fetch admin-free toolchain into .cache/toolchain (LLVM clang-cl, glslang)
    fetch      - clone/fetch the pinned upstream revision into .cache/engine/eden
    patch      - reset to the pinned revision and apply engine/patches/*.patch, link engine/src
    configure  - CMake (Ninja + clang-cl, VS 2026 MSVC STL + Windows SDK)
    build      - compile
    stage      - copy binaries to out/engine/win-x64 (deluxe8-engine.exe, deluxe8-room.exe)
    all        - everything above, in order
    export-patches - (dev) regenerate engine/patches from commits on the 'deluxe8' branch
    clean      - delete the build directory

.EXAMPLE
  pwsh tools/engine.ps1 all
  pwsh tools/engine.ps1 build -Config RelWithDebInfo
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('toolchain', 'fetch', 'patch', 'configure', 'build', 'stage', 'all', 'export-patches', 'clean')]
    [string]$Command = 'all',
    [ValidateSet('Release', 'RelWithDebInfo', 'Debug')]
    [string]$Config = 'Release',
    # Discard local (uncommitted or unexported) changes in the engine tree when patching.
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Root      = Split-Path -Parent $PSScriptRoot
$Cache     = Join-Path $Root '.cache'
$Toolchain = Join-Path $Cache 'toolchain'
$SrcDir    = Join-Path $Cache 'engine/eden'
$BuildDir  = Join-Path $Root 'build/engine-win-x64'
$StageDir  = Join-Path $Root 'out/engine/win-x64'
$PatchDir  = Join-Path $Root 'engine/patches'
$OurSrc    = Join-Path $Root 'engine/src/deluxe8'
$Upstream  = Get-Content (Join-Path $Root 'engine/upstream.json') -Raw | ConvertFrom-Json

# Toolchain downloads, pinned by SHA-256 (the digests GitHub publishes for these release assets). Same pins in native.ps1.
$LlvmVersion    = '23.1.5'
$LlvmUrl        = "https://github.com/llvm/llvm-project/releases/download/llvmorg-$LlvmVersion/clang%2Bllvm-$LlvmVersion-x86_64-pc-windows-msvc.tar.xz"
$LlvmSha        = '62fdada7f6fa2b73856486dfbbdba3d2c1882991621615bcb55c60207611c6d4'
$GlslangVersion = '16.6.0'
$GlslangUrl     = "https://github.com/KhronosGroup/glslang/releases/download/$GlslangVersion/glslang-$GlslangVersion-windows-x86_64-release.zip"
$GlslangSha     = '82bf434e69b9bb4829de7e2b4bc2c5e7a7861e53d66cf75e5cc70f5f694a8d9b'
$CMakeVersion   = '4.5.0'
$CMakeUrl       = "https://github.com/Kitware/CMake/releases/download/v$CMakeVersion/cmake-$CMakeVersion-windows-x86_64.zip"
$CMakeSha       = '301b1728317ccd84e984b02e366a8293735bc5ac618757d3d8f58b578f062411'

function Write-Step([string]$Message) { Write-Host "==> $Message" -ForegroundColor Cyan }
function Invoke-Native {
    param([string]$Exe, [string[]]$Arguments)
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed with exit code $LASTEXITCODE" }
}
# Downloads $Url to $OutFile unless a file with the pinned SHA-256 is already there; a mismatch deletes the file and stops.
function Get-Pinned([string]$Url, [string]$OutFile, [string]$Sha256) {
    $have = (Test-Path $OutFile) -and ((Get-FileHash $OutFile -Algorithm SHA256).Hash.ToLower() -eq $Sha256)
    if (-not $have) {
        Invoke-Native curl.exe @('-fsSL', '--retry', '3', '-o', $OutFile, $Url)
        $got = (Get-FileHash $OutFile -Algorithm SHA256).Hash.ToLower()
        if ($got -ne $Sha256) {
            Remove-Item -Force $OutFile
            throw "checksum mismatch for $Url (expected $Sha256, got $got)"
        }
    }
}
function Invoke-EngineGit { Invoke-Native git.exe (@("-C", $SrcDir) + $args) }

# ---------------------------------------------------------------- toolchain
function Get-VsInstallPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { throw 'Visual Studio (2022/2026) with "Desktop development with C++" is required.' }
    $path = & $vswhere -latest -prerelease -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $path) { throw 'No Visual Studio installation with the MSVC x64 toolset was found.' }
    return $path
}

function Install-Toolchain {
    Write-Step 'Toolchain'
    New-Item -ItemType Directory -Force (Join-Path $Toolchain 'dl') | Out-Null

    $llvm = Join-Path $Toolchain 'llvm'
    if (-not (Test-Path (Join-Path $llvm 'bin/clang-cl.exe'))) {
        $archive = Join-Path $Toolchain 'dl/llvm.tar.xz'
        Write-Host "   downloading LLVM $LlvmVersion (~900 MB)"
        Get-Pinned $LlvmUrl $archive $LlvmSha
        Invoke-Native "$env:SystemRoot\System32\tar.exe" @('-xf', $archive, '-C', $Toolchain)
        Move-Item (Join-Path $Toolchain "clang+llvm-$LlvmVersion-x86_64-pc-windows-msvc") $llvm
    }

    $glslang = Join-Path $Toolchain 'glslang'
    if (-not (Test-Path (Join-Path $glslang 'bin/glslangValidator.exe'))) {
        $archive = Join-Path $Toolchain 'dl/glslang.zip'
        Write-Host "   downloading glslang $GlslangVersion"
        Get-Pinned $GlslangUrl $archive $GlslangSha
        Expand-Archive -Force $archive $glslang
        Copy-Item (Join-Path $glslang 'bin/glslang.exe') (Join-Path $glslang 'bin/glslangValidator.exe')
    }

    # clang-cl + C++20 modules need CMake >= 4.4 (VS 2026 ships 4.3.x): use a portable CMake.
    $cmake = Join-Path $Toolchain 'cmake'
    if (-not (Test-Path (Join-Path $cmake 'bin/cmake.exe'))) {
        $archive = Join-Path $Toolchain 'dl/cmake.zip'
        Write-Host "   downloading CMake $CMakeVersion"
        Get-Pinned $CMakeUrl $archive $CMakeSha
        Expand-Archive -Force $archive $Toolchain
        Move-Item (Join-Path $Toolchain "cmake-$CMakeVersion-windows-x86_64") $cmake
    }

    $vs = Get-VsInstallPath
    $ninja = Join-Path $vs 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja'
    if (-not (Test-Path $ninja)) { throw "Missing VS component (C++ CMake tools for Windows): $ninja" }
    Write-Host "   VS:      $vs"
    Write-Host "   clang:   $(& (Join-Path $llvm 'bin/clang-cl.exe') --version | Select-Object -First 1)"
    Write-Host "   cmake:   $(& (Join-Path $cmake 'bin/cmake.exe') --version | Select-Object -First 1)"
}

function Enter-DevEnvironment {
    $vs = Get-VsInstallPath
    $vcvars = Join-Path $vs 'VC/Auxiliary/Build/vcvars64.bat'
    if (-not $env:DELUXE8_VSENV) {
        cmd.exe /c "`"$vcvars`" >nul 2>&1 && set" | ForEach-Object {
            if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "env:$($Matches[1])" -Value $Matches[2] }
        }
        $env:DELUXE8_VSENV = '1'
    }
    $extra = @(
        (Join-Path $Toolchain 'llvm/bin'),
        (Join-Path $Toolchain 'glslang/bin'),
        (Join-Path $Toolchain 'cmake/bin'),
        (Join-Path $vs 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja')
    )
    $env:PATH = ($extra -join ';') + ';' + $env:PATH
}

# ---------------------------------------------------------------- sources
function Get-UpstreamSource {
    Write-Step "Fetch $($Upstream.name) @ $($Upstream.commit.Substring(0, 12))"
    if (-not (Test-Path (Join-Path $SrcDir '.git'))) {
        New-Item -ItemType Directory -Force $SrcDir | Out-Null
        Invoke-EngineGit init -q
        Invoke-EngineGit remote add origin $Upstream.url
    }
    $have = git.exe -C $SrcDir cat-file -t $Upstream.commit 2>$null
    if ($have -ne 'commit') { Invoke-EngineGit fetch --depth 1 origin $Upstream.commit }
}

function Set-EnginePatches {
    Write-Step 'Apply Deluxe8 patch queue'
    $dirty = git.exe -C $SrcDir status --porcelain --untracked-files=no
    if ($dirty -and -not $Force) { throw "Engine tree has local changes. Commit + export-patches first, or pass -Force.`n$dirty" }

    Invoke-EngineGit -c advice.detachedHead=false checkout -q -f -B deluxe8 $Upstream.commit
    $patches = @(Get-ChildItem -Path $PatchDir -Filter '*.patch' -ErrorAction SilentlyContinue | Sort-Object Name)
    if ($patches.Count -gt 0) {
        # one argument per patch file: an array passed as a single element would reach git as one joined path
        $files = [string[]]$patches.FullName
        Invoke-EngineGit -c user.name=Deluxe8 -c user.email=build@deluxe8.invalid am --3way --keep-cr @files
    }
    Write-Host "   $($patches.Count) patch(es) applied"

    # Our own sources live in this repository; link them into the tree.
    $link = Join-Path $SrcDir 'src/deluxe8'
    if (Test-Path $link) { (Get-Item $link).Delete() }
    New-Item -ItemType Junction -Path $link -Target $OurSrc | Out-Null
    $exclude = Join-Path $SrcDir '.git/info/exclude'
    if (-not (Select-String -Path $exclude -Pattern '^/src/deluxe8$' -Quiet -ErrorAction SilentlyContinue)) {
        Add-Content -Path $exclude -Value '/src/deluxe8'
    }
}

function Export-EnginePatches {
    Write-Step 'Export patches from branch deluxe8'
    New-Item -ItemType Directory -Force $PatchDir | Out-Null
    Get-ChildItem $PatchDir -Filter '*.patch' | Remove-Item
    Invoke-EngineGit format-patch --quiet --no-signature --zero-commit -N -o $PatchDir "$($Upstream.commit)..deluxe8"
    Get-ChildItem $PatchDir -Filter '*.patch' | ForEach-Object { Write-Host "   $($_.Name)" }
}

# ---------------------------------------------------------------- build
function Invoke-Configure {
    Write-Step "Configure ($Config)"
    Enter-DevEnvironment
    $clang = (Join-Path $Toolchain 'llvm/bin/clang-cl.exe') -replace '\\', '/'
    $cmakeArgs = @(
        '-S', $SrcDir, '-B', $BuildDir, '-G', 'Ninja',
        "-DCMAKE_BUILD_TYPE=$Config",
        "-DCMAKE_C_COMPILER=$clang", "-DCMAKE_CXX_COMPILER=$clang",
        "-DCPM_SOURCE_CACHE=$((Join-Path $Cache 'cpm') -replace '\\', '/')",
        '-DENABLE_QT=OFF', '-DYUZU_CMD=ON', '-DYUZU_ROOM=ON', '-DYUZU_ROOM_STANDALONE=ON',
        '-DYUZU_TESTS=OFF', '-DBUILD_TESTING=OFF', '-DENABLE_WERROR=OFF',
        '-DENABLE_UPDATE_CHECKER=OFF', '-DENABLE_LSFG=ON', '-DENABLE_RESHADE=ON',
        '-DDELUXE8=ON'
    )
    Invoke-Native cmake $cmakeArgs
}

function Invoke-Build {
    Write-Step 'Build'
    Enter-DevEnvironment
    Invoke-Native cmake @('--build', $BuildDir, '--config', $Config)
}

function Invoke-Stage {
    Write-Step "Stage -> $StageDir"
    $bin = Join-Path $BuildDir 'bin'
    New-Item -ItemType Directory -Force $StageDir | Out-Null
    Copy-Item (Join-Path $bin 'eden-cli.exe') (Join-Path $StageDir 'deluxe8-engine.exe') -Force
    if (Test-Path (Join-Path $bin 'eden-room.exe')) {
        Copy-Item (Join-Path $bin 'eden-room.exe') (Join-Path $StageDir 'deluxe8-room.exe') -Force
    }
    Get-ChildItem $bin -Filter '*.dll' | Copy-Item -Destination $StageDir -Force
    Copy-Item (Join-Path $SrcDir 'LICENSE.txt') (Join-Path $StageDir 'LICENSE-eden.txt') -Force
    Get-ChildItem $StageDir | Format-Table Name, Length -AutoSize | Out-String | Write-Host
}

switch ($Command) {
    'toolchain'      { Install-Toolchain }
    'fetch'          { Get-UpstreamSource }
    'patch'          { Set-EnginePatches }
    'configure'      { Invoke-Configure }
    'build'          { Invoke-Build }
    'stage'          { Invoke-Stage }
    'export-patches' { Export-EnginePatches }
    'clean'          { if (Test-Path $BuildDir) { Remove-Item -Recurse -Force $BuildDir } }
    'all'            { Install-Toolchain; Get-UpstreamSource; Set-EnginePatches; Invoke-Configure; Invoke-Build; Invoke-Stage }
}
