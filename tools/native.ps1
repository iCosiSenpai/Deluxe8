<#
.SYNOPSIS
  Native Windows x64 build tool for Racing 8 DX Recomp (the BUILD TOOLS only; the game pipeline is never run here).

.DESCRIPTION
  Reproduces the upstream CI recipe (.github/workflows/windows-engine.yml of the pinned racing8 commit) on a fresh checkout:
    deps      - fetch every admin-free dependency into .cache/ (glslang, CMake, LLVM clang, vcpkg + static zlib,
                 Qt 6.7.3 via aqtinstall, Vulkan headers + vulkan-1.lib carved out of the LunarG SDK installer,
                 the racing8 source at the pinned commit, the suyu fork with our patches)
    engine    - build mk8d-engine.exe + kartrecomp-export.exe (Ninja + MSVC Release, exactly the CI flags) and stage them
    toolchain - assemble the redistributable llvm-mingw toolchain (packaging/windows/make-toolchain.ps1) and self-test it
    launcher  - build kartrecomp.exe / kartrecomp-setup.exe / kartrecomp-build.exe (Qt 6.7.3 + static zlib) and windeployqt them
    plugins   - FSR 3.1 (shader blobs generated in WSL, like the Linux CI job), FSR 4, and DLSS (LOCAL USE ONLY) + the plugin tests
    assemble  - lay out out/native/win-x64 exactly like the release zip, run the release contents test, zip it
    all       - everything above, in order

  Requires: Visual Studio 2022/2026 with MSVC x64 + Ninja + CMake tools, git, curl, 7z, PowerShell 7, and (for the FSR 3
  shader step) WSL with an Ubuntu distro that has gcc/g++/python3/glslangValidator. Nothing needs admin: Qt, Vulkan and
  every SDK are installed under .cache/ and everything is staged under out/native/.

.EXAMPLE
  pwsh tools/native.ps1 all
  pwsh tools/native.ps1 plugins
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('deps', 'engine', 'toolchain', 'launcher', 'plugins', 'assemble', 'all')]
    [string]$Command = 'all'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# ------------------------------------------------------------------ pinned inputs (the ground truth)
$Racing8Url    = 'https://github.com/galarianpikachu2021-hue/Racing-8-DX-Recomp.git'
$Racing8Commit = '9c1a32406d2de055f87f15edc63d8ed2b211db96'   # release v0.2.4
$ForkUrl       = 'https://github.com/dougchansan/suyu-v0.0.4.git'
$ForkSha       = '5949cab3ba93233ddd1c319bfc6f010f6cfa910a'   # packaging/windows/fetch-fork.sh
$QtVersion     = '6.7.3'
$QtArch        = 'win64_msvc2019_64'   # aqt's argument name; the installed folder drops the "win64_" prefix
$QtDir         = 'msvc2019_64'         # ...\qt\6.7.3\msvc2019_64
$VcpkgUrl      = 'https://github.com/microsoft/vcpkg.git'
$VcpkgCommit   = '4702e381dbbd9b54d3f7e1eb565cd8981c1814cd'   # classic mode still works here
$VulkanSdkUrl  = 'https://sdk.lunarg.com/sdk/download/1.4.363.0/windows/vulkan-sdk.exe'
$VulkanSdkSha  = 'c02932bd9a968b22d404536108adaacac908c081cd6b0113f5bfefc523dd0a6a'
$LlvmVersion   = '23.1.5'
$LlvmUrl       = "https://github.com/llvm/llvm-project/releases/download/llvmorg-$LlvmVersion/clang%2Bllvm-$LlvmVersion-x86_64-pc-windows-msvc.tar.xz"
$LlvmSha       = '62fdada7f6fa2b73856486dfbbdba3d2c1882991621615bcb55c60207611c6d4'   # same pins as engine.ps1
$GlslangVersion = '16.6.0'
$GlslangUrl    = "https://github.com/KhronosGroup/glslang/releases/download/$GlslangVersion/glslang-$GlslangVersion-windows-x86_64-release.zip"
$GlslangSha    = '82bf434e69b9bb4829de7e2b4bc2c5e7a7861e53d66cf75e5cc70f5f694a8d9b'
$CMakeVersion  = '4.5.0'
$CMakeUrl      = "https://github.com/Kitware/CMake/releases/download/v$CMakeVersion/cmake-$CMakeVersion-windows-x86_64.zip"
$CMakeSha      = '301b1728317ccd84e984b02e366a8293735bc5ac618757d3d8f58b578f062411'
$Fsr4SdkTag    = 'v2.4.0'
$Fsr4SdkUrl    = 'https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK.git'
$DlssSdkUrl    = 'https://github.com/NVIDIA/DLSS.git'        # public NGX SDK; the result is for this machine only

# ------------------------------------------------------------------ layout (writes stay inside .cache\, build\native-*, out\native\)
$Root      = Split-Path -Parent $PSScriptRoot
$Cache     = Join-Path $Root '.cache'
$Toolchain = Join-Path $Cache 'toolchain'                  # shared with tools/engine.ps1 (idempotent)
$Native    = Join-Path $Cache 'native'
$Repo      = Join-Path $Native 'racing8'                   # the pinned upstream clone (ground truth for the build)
$Fork      = Join-Path $Native 'fork'
$Vcpkg     = Join-Path $Native 'vcpkg'
$Qt        = Join-Path $Native 'qt'
$Vk        = Join-Path $Native 'vksdk'
$Prepared  = Join-Path $Native 'ffx-prepared'
$EngineBuild  = Join-Path $Root 'build/native-engine'
$LauncherBuild = Join-Path $Root 'build/native-launcher'
$Stage     = Join-Path $Root 'build/native-stage'
$OutRoot   = Join-Path $Root 'out/native'

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
# .ps1 scripts need NAMED parameters, which only splat from a hashtable (an array splat would pass them positionally).
# They run in-process for correct [string[]] parameter binding (pwsh -File cannot bind multi-value parameters),
# but their environment is guarded: the upstream tests clobber $env:PATH (test_toolchain.ps1 sets it to System32
# only) and that must not leak back into the driver process.
function Invoke-Ps1 {
    param([string]$Script, [string[]]$Positional = @(), [hashtable]$Named = @{})
    $savedPath = $env:PATH
    & $Script @Positional @Named
    $env:PATH = $savedPath
    if ($LASTEXITCODE -ne 0) { throw "$Script failed with exit code $LASTEXITCODE" }
}
function Invoke-Git([string]$Dir) { Invoke-Native git.exe (@('-C', $Dir) + $args) }

# a build dir configured for a different source path (repo moved, or a dir reused across checkouts) confuses CMake
function Reset-BuildDir {
    param([string]$Dir, [string]$Source)
    $cache = Join-Path $Dir 'CMakeCache.txt'
    if (Test-Path $cache) {
        # note: the variable must NOT be called $home — that collides with the read-only automatic $HOME
        $cmakeHome = Select-String -Path $cache -Pattern '^CMAKE_HOME_DIRECTORY:INTERNAL=(.+)$' |
            ForEach-Object { $_.Matches[0].Groups[1].Value -replace '\\', '/' }
        if ($cmakeHome -and $cmakeHome -ne ($Source -replace '\\', '/')) {
            Write-Host "   build dir was configured for $cmakeHome; re-creating it for $Source"
            Remove-Item -Recurse -Force $Dir
        }
    }
}

function Get-VsInstallPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { throw 'Visual Studio (2022/2026) with "Desktop development with C++" is required.' }
    $path = & $vswhere -latest -prerelease -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $path) { throw 'No Visual Studio installation with the MSVC x64 toolset was found.' }
    return $path
}

# vcvars64 environment (captured once per process) + the tools we need on PATH
function Enter-DevEnvironment {
    $vs = Get-VsInstallPath
    if (-not $env:NATIVE_VSENV) {
        $vcvars = Join-Path $vs 'VC/Auxiliary/Build/vcvars64.bat'
        cmd.exe /c "`"$vcvars`" >nul 2>&1 && set" | ForEach-Object {
            if ($_ -match '^([^=]+)=(.*)$') {
                # some inherited vars (HOME, ...) are read-only in PowerShell; skipping them is harmless
                try { Set-Item -Path "env:$($Matches[1])" -Value $Matches[2] -ErrorAction Stop } catch { }
            }
        }
        $env:NATIVE_VSENV = '1'
    }
    $ninja = Join-Path $vs 'Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja'
    if (-not (Test-Path (Join-Path $ninja 'ninja.exe'))) { throw "Missing VS component (C++ CMake tools for Windows): $ninja" }
    $extra = @(
        (Join-Path $Toolchain 'cmake/bin'),
        (Join-Path $Toolchain 'glslang/bin'),
        (Join-Path $Toolchain 'llvm/bin'),
        $ninja
    ) | Where-Object { Test-Path $_ }
    $env:PATH = (($extra -join ';') + ';' + $env:PATH)
}

# ------------------------------------------------------------------ deps
function Install-Glslang {
    if (-not (Test-Path (Join-Path $Toolchain 'glslang/bin/glslangValidator.exe'))) {
        Write-Step 'glslangValidator'
        New-Item -ItemType Directory -Force (Join-Path $Toolchain 'dl') | Out-Null
        $zip = Join-Path $Toolchain 'dl/glslang.zip'
        Get-Pinned $GlslangUrl $zip $GlslangSha
        Expand-Archive -Force $zip (Join-Path $Toolchain 'glslang')
        Copy-Item (Join-Path $Toolchain 'glslang/bin/glslang.exe') (Join-Path $Toolchain 'glslang/bin/glslangValidator.exe')
    }
}
function Install-CMake {
    if (-not (Test-Path (Join-Path $Toolchain 'cmake/bin/cmake.exe'))) {
        Write-Step "CMake $CMakeVersion (portable; VS ships an older one)"
        New-Item -ItemType Directory -Force (Join-Path $Toolchain 'dl') | Out-Null
        $zip = Join-Path $Toolchain 'dl/cmake.zip'
        Get-Pinned $CMakeUrl $zip $CMakeSha
        Expand-Archive -Force $zip $Toolchain
        Move-Item (Join-Path $Toolchain "cmake-$CMakeVersion-windows-x86_64") (Join-Path $Toolchain 'cmake')
    }
}
function Install-Clang {
    # full LLVM: the module guard test links a stub module with clang --target=x86_64-pc-windows-msvc
    if (-not (Test-Path (Join-Path $Toolchain 'llvm/bin/clang.exe'))) {
        Write-Step "LLVM $LlvmVersion (~900 MB, one time)"
        New-Item -ItemType Directory -Force (Join-Path $Toolchain 'dl') | Out-Null
        $archive = Join-Path $Toolchain 'dl/llvm.tar.xz'
        Get-Pinned $LlvmUrl $archive $LlvmSha
        Invoke-Native "$env:SystemRoot\System32\tar.exe" @('-xf', $archive, '-C', $Toolchain)
        Move-Item (Join-Path $Toolchain "clang+llvm-$LlvmVersion-x86_64-pc-windows-msvc") (Join-Path $Toolchain 'llvm')
    }
}
function Install-Vcpkg {
    # VS 2026's bundled vcpkg is manifest-mode only; a real checkout supports classic "install" (what the CI runs).
    if (-not (Test-Path (Join-Path $Vcpkg 'vcpkg.exe'))) {
        Write-Step 'vcpkg'
        if (-not (Test-Path (Join-Path $Vcpkg '.git'))) {
            Invoke-Native git.exe @('clone', '--depth', '1', '--filter=blob:none', $VcpkgUrl, $Vcpkg)
        }
        Invoke-Git $Vcpkg fetch --depth 1 origin $VcpkgCommit
        Invoke-Git $Vcpkg checkout -f $VcpkgCommit
        Invoke-Native (Join-Path $Vcpkg 'bootstrap-vcpkg.bat') @('-disableMetrics')
    }
    if (-not (Test-Path (Join-Path $Vcpkg 'installed/x64-windows-static-md/lib/zlib.lib')) -and
        -not (Test-Path (Join-Path $Vcpkg 'installed/x64-windows-static-md/lib/libzs.lib'))) {
        Write-Step 'vcpkg: zlib:x64-windows-static-md'
        Invoke-Native (Join-Path $Vcpkg 'vcpkg.exe') @('install', 'zlib:x64-windows-static-md')
    }
}
function Install-Qt {
    if (-not (Test-Path (Join-Path $Qt "$QtVersion/$QtDir/bin/qmake.exe"))) {
        Write-Step "Qt $QtVersion $QtArch via aqtinstall (~1 GB, one time)"
        $venv = Join-Path $Native 'venv'
        if (-not (Test-Path (Join-Path $venv 'Scripts/python.exe'))) {
            Invoke-Native py @('-m', 'venv', $venv)
        }
        Invoke-Native (Join-Path $venv 'Scripts/python.exe') @('-m', 'pip', 'install', '-q', 'aqtinstall')
        Invoke-Native (Join-Path $venv 'Scripts/aqt.exe') @('install-qt', 'windows', 'desktop', $QtVersion, $QtArch, '-O', $Qt)
    }
}
function Install-VulkanSdk {
    # The LunarG installer wants admin, so we download it and carve the 7z block chain by hand (7z only sees the
    # first NSIS block). What the plugin builds need: Include\vulkan\*.h and Lib\vulkan-1.lib.
    if ((Test-Path (Join-Path $Vk 'extracted/Include/vulkan/vulkan.h')) -and (Test-Path (Join-Path $Vk 'extracted/Lib/vulkan-1.lib'))) { return }
    Write-Step 'Vulkan SDK (headers + vulkan-1.lib, carved from the LunarG installer)'
    New-Item -ItemType Directory -Force $Vk | Out-Null
    $installer = Join-Path $Vk 'vulkan-sdk-installer.exe'
    if (-not (Test-Path $installer)) {
        Invoke-Native curl.exe @('-fsSL', '--retry', '3', '-o', $installer, $VulkanSdkUrl)
        if ((Get-FileHash $installer -Algorithm SHA256).Hash.ToLower() -ne $VulkanSdkSha) { throw "checksum mismatch for $installer" }
    }
    $wanted = @('Include\vulkan\vulkan.h', 'Lib\vulkan-1.lib')
    $found = @{}
    $cur = $installer
    for ($i = 1; $i -le 12 -and $found.Count -lt $wanted.Count; $i++) {
        $listing = & 7z.exe l $cur 2>&1
        $names = @($listing | Select-String -Pattern '^\d{4}-\d\d-\d\d\s+\d\d:\d\d:\d\d\s+\S+\s+\d+\s+\d+\s+(.+)$' |
            ForEach-Object { $_.Matches[0].Groups[1].Value })
        foreach ($w in $wanted) {
            if (-not $found.ContainsKey($w) -and $names -contains $w) {
                # whole Include dir (all headers), but only the one file from the huge Lib block
                $filter = if ($w -eq 'Lib\vulkan-1.lib') { $w } else { (Split-Path -Parent $w) }
                & 7z.exe x $cur -y "-o$Vk/extracted" $filter | Out-Null
                if ($LASTEXITCODE -eq 0 -and (Test-Path (Join-Path $Vk "extracted/$w"))) { $found[$w] = $true }
            }
        }
        if ($found.Count -lt $wanted.Count) {
            $tail = $listing | Select-String -Pattern '^Tail Size = (\d+)$' | Select-Object -First 1
            if (-not $tail) { break }
            $n = [long]$tail.Matches[0].Groups[1].Value
            if ($n -lt 1000) { break }
            $bytes = [IO.File]::ReadAllBytes($cur)
            $next = Join-Path $Vk "block$i.bin"
            [IO.File]::WriteAllBytes($next, $bytes[($bytes.Length - $n)..($bytes.Length - 1)])
            $cur = $next
        }
    }
    foreach ($w in $wanted) { if (-not $found[$w]) { throw "could not carve $w out of the LunarG SDK installer" } }
}
function Get-Racing8 {
    if (-not (Test-Path (Join-Path $Repo '.git'))) {
        Write-Step "Fetch racing8 @ $($Racing8Commit.Substring(0, 12))"
        New-Item -ItemType Directory -Force $Repo | Out-Null
        Invoke-Git $Repo init -q
        Invoke-Git $Repo remote add origin $Racing8Url
        Invoke-Git $Repo fetch --depth 1 origin $Racing8Commit
        Invoke-Git $Repo -c advice.detachedHead=false checkout -q $Racing8Commit
    }
    $have = git.exe -C $Repo rev-parse HEAD 2>$null
    if ($have -ne $Racing8Commit) { throw "racing8 at $Repo is not at the pinned commit $Racing8Commit" }
}
function Get-Fork {
    # packaging/windows/fetch-fork.sh, reimplemented in PowerShell so no bash is needed
    Write-Step "Fetch the suyu fork @ $($ForkSha.Substring(0, 12)) + patches"
    if (-not (Test-Path (Join-Path $Fork '.git'))) {
        New-Item -ItemType Directory -Force $Fork | Out-Null
        Invoke-Native git.exe @('clone', '--filter=blob:none', '--no-checkout', $ForkUrl, $Fork)
    }
    $patches = @(Get-ChildItem (Join-Path $Repo 'patches/suyu') -Filter '*.patch' | Sort-Object Name)
    # patches are applied all-or-nothing: if the LAST one reverses cleanly, the tree is the patched state and we
    # leave every file untouched (re-touching them would force ninja to recompile the whole engine)
    $head = git.exe -C $Fork rev-parse HEAD 2>$null
    $needReset = ($head -ne $ForkSha)
    if (-not $needReset) {
        $needReset = -not (git.exe -C $Fork apply --check --reverse $patches[-1].FullName 2>$null)
        if ($needReset) { Write-Host '   last patch does not reverse cleanly; resetting the tree' }
    } else {
        Write-Host '   fork HEAD is not the pinned commit; resetting the tree'
    }
    if ($needReset) {
        Invoke-Git $Fork -c advice.detachedHead=false checkout -q -f $ForkSha
        # back to a pristine upstream tree: drop the files the patches add (untracked) and any local edits;
        # submodule work trees are git repos themselves, so a single -f leaves them alone
        Invoke-Git $Fork clean -fdx
        Invoke-Git $Fork submodule update --init --recursive --depth 1
        foreach ($p in $patches) {
            Write-Host "   applying $(Split-Path $p -Leaf)"
            Invoke-Git $Fork apply --whitespace=nowarn $p.FullName
        }
    } else {
        Write-Host '   patches already applied'
    }
}
function Get-EngineVersion {
    $pin = Join-Path $Repo 'packaging/windows/engine-version.pin'
    if (Test-Path $pin) { return ((Get-Content $pin -Raw) -replace '\s', '') }
    # the CI: sha256 of the concatenated patches/suyu/*.patch (glob order), first 10 hex chars, + "-abi6"
    $bytes = New-Object byte[] 0
    foreach ($p in @(Get-ChildItem (Join-Path $Repo 'patches/suyu') -Filter '*.patch' | Sort-Object Name)) {
        $bytes += [IO.File]::ReadAllBytes($p.FullName)
    }
    $hash = ([Security.Cryptography.SHA256]::HashData($bytes) | ForEach-Object { $_.ToString('x2') }) -join ''
    return ($hash.Substring(0, 10) + '-abi6')
}

# ------------------------------------------------------------------ engine
function Invoke-Engine {
    Write-Step 'Engine (suyu fork, Ninja + MSVC Release)'
    Enter-DevEnvironment
    $version = Get-EngineVersion
    Write-Host "   ENGINE_VERSION = $version"
    New-Item -ItemType Directory -Force $EngineBuild | Out-Null
    Reset-BuildDir $EngineBuild $Fork
    Invoke-Native cmake.exe @(
        '-S', $Fork, '-B', $EngineBuild, '-G', 'Ninja', '-DCMAKE_BUILD_TYPE=Release',
        '-DENABLE_QT=OFF', '-DYUZU_TESTS=OFF', '-DENABLE_WEB_SERVICE=OFF',
        '-DSUYU_NO_JIT=ON', '-DKARTRECOMP_ENGINE=ON', "-DKARTRECOMP_ENGINE_VERSION=$version"
    )
    Invoke-Native cmake.exe @('--build', $EngineBuild, '--target', 'mk8d-engine', 'kartrecomp-export', '--', '-k', '0')

    # stage exactly like the CI "Stage" step
    $stage = Join-Path $Stage 'engine'
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item (Join-Path $EngineBuild 'bin/mk8d-engine.exe'), (Join-Path $EngineBuild 'bin/kartrecomp-export.exe') $stage
    Get-ChildItem (Join-Path $EngineBuild 'bin') -Filter *.dll | Copy-Item -Destination $stage
    Set-Content (Join-Path $stage 'ENGINE_VERSION') -Value $version -NoNewline
    $mi = Get-ChildItem (Join-Path $EngineBuild '_deps') -Recurse -Filter LICENSE -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match 'mimalloc' } | Select-Object -First 1
    if (-not $mi) { throw 'mimalloc LICENSE not found in the build tree' }
    Copy-Item $mi.FullName (Join-Path $stage 'mimalloc-LICENSE.txt')
    Write-Step 'Module guard test'
    Invoke-Ps1 (Join-Path $Repo 'tests/test_module_guard.ps1') @() @{ EngineDir = $stage }
    Get-ChildItem $stage | ForEach-Object { "{0,8:N1} MB  {1}" -f ($_.Length / 1MB), $_.Name } | Write-Host
}

# ------------------------------------------------------------------ toolchain
function Invoke-Toolchain {
    Write-Step 'Toolchain (llvm-mingw, checksum-pinned download)'
    $out = Join-Path $Stage 'toolchain'
    Invoke-Ps1 (Join-Path $Repo 'packaging/windows/make-toolchain.ps1') @() @{ Out = $out }
    Write-Step 'Toolchain test'
    Invoke-Ps1 (Join-Path $Repo 'tests/windows/test_toolchain.ps1') @() @{ Toolchain = $out }
}

# ------------------------------------------------------------------ launcher
function Invoke-Launcher {
    Write-Step 'Launcher (Qt 6.7.3 + zlib static-md)'
    Enter-DevEnvironment
    $env:PATH = (Join-Path $Qt "$QtVersion/$QtDir/bin") + ';' + $env:PATH
    $env:VCPKG_INSTALLATION_ROOT = $Vcpkg
    New-Item -ItemType Directory -Force $LauncherBuild | Out-Null
    Reset-BuildDir $LauncherBuild $Repo
    Invoke-Native cmake.exe @(
        '-S', $Repo, '-B', $LauncherBuild, '-G', 'Ninja', '-DCMAKE_BUILD_TYPE=Release',
        "-DCMAKE_TOOLCHAIN_FILE=$Vcpkg/scripts/buildsystems/vcpkg.cmake",
        '-DVCPKG_TARGET_TRIPLET=x64-windows-static-md',
        "-DCMAKE_PREFIX_PATH=$(Join-Path $Qt "$QtVersion/$QtDir")"
    )
    Invoke-Native cmake.exe @('--build', $LauncherBuild)
    $stage = Join-Path $Stage 'launcher'
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item (Join-Path $LauncherBuild 'app/kartrecomp.exe'), (Join-Path $LauncherBuild 'app/kartrecomp-setup.exe'),
        (Join-Path $LauncherBuild 'app/kartrecomp-build.exe') $stage
    Invoke-Native (Join-Path $Qt "$QtVersion/$QtDir/bin/windeployqt.exe") @('--release', '--no-translations', (Join-Path $stage 'kartrecomp.exe'))
}

# ------------------------------------------------------------------ plugins
function Invoke-FfxPrepared {
    # the CI "ffx-prepared" job is Linux: FidelityFX SDK v1.1.4 + the FSR 3 shader blobs, plain SPIR-V.
    # We run the upstream prepare-sdk.sh in WSL. The upstream checkout may have CRLF line endings (autocrlf),
    # which WSL's bash cannot execute, so the scripts are copied with LF endings first.
    $wsl = Get-Command wsl.exe -ErrorAction SilentlyContinue
    if (-not $wsl) { throw 'wsl.exe not found: the FSR 3 shader step needs WSL (gcc, glslangValidator, python3)' }
    if ((Test-Path (Join-Path $Prepared 'shaders')) -and (Test-Path (Join-Path $Prepared 'sdk/sdk/include'))) {
        Write-Host '   ffx-prepared already present'; return
    }
    Write-Step 'FSR 3 SDK preparation + shader blobs (WSL)'
    $work = Join-Path $Native 'ffx-work'
    New-Item -ItemType Directory -Force $work | Out-Null
    $scripts = Join-Path $work 'fsr3-scripts'
    New-Item -ItemType Directory -Force $scripts | Out-Null
    foreach ($f in 'prepare-sdk.sh', 'gen-ffx-shaders.sh', 'port-ffx-sc.py') {
        $text = (Get-Content (Join-Path $Repo "plugins/fsr3/$f") -Raw) -replace "`r`n", "`n"
        [IO.File]::WriteAllText((Join-Path $scripts $f), $text)
    }
    $wslRoot = '/mnt/' + $Root.Substring(0, 1).ToLower() + ($Root.Substring(2) -replace '\\', '/')
    $wrapper = Join-Path $work 'ffx-prepare.sh'
    # single-quoted here-string: $NATIVE must reach bash literally
    $wtext = (@'
#!/bin/bash
set -e
NATIVE=__WSL_NATIVE__
cd "$NATIVE/ffx-work/fsr3-scripts"
./prepare-sdk.sh "$NATIVE/ffx-work"
mkdir -p "$NATIVE/ffx-prepared/sdk/sdk"
cp -r "$NATIVE/ffx-work/sdk/sdk/include" "$NATIVE/ffx-work/sdk/sdk/src" "$NATIVE/ffx-prepared/sdk/sdk/"
cp -r "$NATIVE/ffx-work/shaders" "$NATIVE/ffx-prepared/shaders"
'@) -replace "`r`n", "`n"
    $wtext = $wtext.Replace('__WSL_NATIVE__', "$wslRoot/.cache/native")
    [IO.File]::WriteAllText($wrapper, $wtext)
    $distro = (& wsl.exe -l -q 2>$null | Where-Object { $_ }) | Select-Object -First 1
    if (-not $distro) { throw 'no WSL distro found' }
    Invoke-Native wsl.exe @('-d', $distro, '--', 'bash', "$wslRoot/.cache/native/ffx-work/ffx-prepare.sh")
    if (-not (Test-Path (Join-Path $Prepared 'shaders'))) { throw 'ffx-prepared did not produce shaders' }
}

function Invoke-Plugins {
    Write-Step 'Plugins (FSR 3.1, FSR 4, DLSS)'
    Invoke-FfxPrepared
    Enter-DevEnvironment
    $env:VULKAN_SDK = Join-Path $Vk 'extracted'
    foreach ($p in "$env:VULKAN_SDK/Include/vulkan/vulkan.h", "$env:VULKAN_SDK/Lib/vulkan-1.lib") {
        if (-not (Test-Path $p)) { throw "VULKAN_SDK incomplete: missing $p" }
    }
    $plugins = Join-Path $Stage 'plugins'
    $dlss = Join-Path $Stage 'dlss'
    New-Item -ItemType Directory -Force $plugins | Out-Null
    Invoke-Ps1 (Join-Path $Repo 'plugins/fsr3/build-windows.ps1') @() @{ Prepared = $Prepared; Out = $plugins }
    Invoke-Ps1 (Join-Path $Repo 'plugins/fsr4/build-windows.ps1') @() @{ Out = $plugins; SdkDir = (Join-Path $Native 'fsr4-sdk') }
    New-Item -ItemType Directory -Force $dlss | Out-Null
    Invoke-Ps1 (Join-Path $Repo 'plugins/dlss/build-windows.ps1') @() @{ Out = $dlss; SdkDir = (Join-Path $Native 'dlss-sdk') }
    Write-Step 'Plugin tests'
    Invoke-Ps1 (Join-Path $Repo 'tests/windows/test_plugins.ps1') @($plugins) @{ Expect = @('fsr3', 'fsr4') }
    Invoke-Ps1 (Join-Path $Repo 'tests/windows/test_plugins.ps1') @($dlss) @{ Expect = @('dlss') }
}

# ------------------------------------------------------------------ assemble
function Invoke-Assemble {
    Write-Step 'Assemble -> out/native/win-x64 (the release zip layout)'
    $engine = Join-Path $Stage 'engine'
    $launcher = Join-Path $Stage 'launcher'
    $toolchain = Join-Path $Stage 'toolchain'
    $plugins = Join-Path $Stage 'plugins'
    foreach ($d in $engine, $launcher, $toolchain, $plugins) {
        if (-not (Test-Path $d)) { throw "missing stage $d (run the matching command first)" }
    }
    $out = Join-Path $OutRoot 'win-x64'
    # assemble.ps1 deletes the whole output folder first. A kartrecomp.exe started from there keeps its portable "userdata"
    # inside it (the built game, saves, and the NSPs it MOVED in), so that folder is parked beside it and put back afterwards;
    # the release test and the zip never see it.
    $userdata = Join-Path $out 'userdata'
    $parked = Join-Path $OutRoot 'userdata.parked'
    if (Test-Path $parked) { throw "$parked exists (left by an interrupted assemble): move it back to $userdata first" }
    if (Test-Path $userdata) {
        Write-Host "   parking $userdata during the assemble"
        Move-Item $userdata $parked
    }
    try {
        Invoke-Ps1 (Join-Path $Repo 'packaging/windows/assemble.ps1') @() @{
            Engine = $engine; Launcher = $launcher; Toolchain = $toolchain; Plugins = $plugins; Out = $out
        }
        Invoke-AssembleChecks $out
    } finally {
        if (Test-Path $parked) {
            if (-not (Test-Path $out)) { New-Item -ItemType Directory -Force $out | Out-Null }
            Move-Item $parked $userdata
        }
    }
}

function Invoke-AssembleChecks([string]$out) {
    # DLSS links NVIDIA's static NGX library: local use only, never in the release folder
    $dlss = Join-Path $Stage 'dlss'
    if (Test-Path $dlss) {
        $dlssOut = Join-Path $OutRoot 'dlss-local/plugins'
        New-Item -ItemType Directory -Force $dlssOut | Out-Null
        Copy-Item "$dlss/*" $dlssOut -Recurse -Force
        @'
DLSS plugin for THIS machine only (links NVIDIA's static NGX library and ships NVIDIA's runtime DLLs).
It must never be distributed: the release contents test fails any folder that contains it.
Copy the four files into <game>\engine\plugins\ to use them.
'@ | Set-Content (Join-Path $OutRoot 'dlss-local/DLSS-README.txt')
    }
    Write-Step 'Release contents test'
    Invoke-Ps1 (Join-Path $Repo 'tests/windows/test_release_contents.ps1') @($out) @{ RequirePlugins = $true }
    Write-Step 'Zip (like the CI bundle job)'
    $zip = Join-Path $OutRoot 'KartRecomp-windows-x64.zip'
    if (Test-Path $zip) { Remove-Item -Force $zip }
    Compress-Archive -Path $out -DestinationPath $zip -CompressionLevel Optimal
    "{0}  {1}" -f (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower(), (Split-Path $zip -Leaf) |
        Out-File -Encoding ascii ($zip + '.sha256')
    "{0:N0} MB" -f ((Get-Item $zip).Length / 1MB) | Write-Host
}

# ------------------------------------------------------------------ commands
function Invoke-Deps {
    Install-Glslang; Install-CMake; Install-Clang; Install-Vcpkg; Install-Qt; Install-VulkanSdk; Get-Racing8; Get-Fork
    Write-Host 'deps ready.'
}

switch ($Command) {
    'deps'      { Invoke-Deps }
    'engine'    { Invoke-Engine }
    'toolchain' { Invoke-Toolchain }
    'launcher'  { Invoke-Launcher }
    'plugins'   { Invoke-Plugins }
    'assemble'  { Invoke-Assemble }
    'all'       { Invoke-Deps; Invoke-Engine; Invoke-Toolchain; Invoke-Launcher; Invoke-Plugins; Invoke-Assemble }
}
