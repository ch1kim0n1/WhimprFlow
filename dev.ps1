# Run WhimprFlow in development on Windows: Vite UI + Tauri with hot reload.
# Equivalent of ./dev.sh on macOS/Linux.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

# whisper-rs / llama-cpp bindgen need libclang.
$llvmBin = "C:\Program Files\LLVM\bin"
if (Test-Path (Join-Path $llvmBin "libclang.dll")) {
    $env:LIBCLANG_PATH = $llvmBin
    if ($env:Path -notlike "*$llvmBin*") {
        $env:Path = "$llvmBin;$env:Path"
    }
} else {
    Write-Warning "LLVM not found at $llvmBin. Install LLVM (winget install LLVM.LLVM) if the build fails looking for libclang."
}

# Clear any stale "use Linux pregenerated bindings" flag - those fail MSVC layout tests.
Remove-Item Env:WHISPER_DONT_GENERATE_BINDINGS -ErrorAction SilentlyContinue

# Load MSVC via a clean PATH so nested vcvars calls cannot blow the environment.
$env:Path = @(
    "C:\Program Files\LLVM\bin",
    "C:\Program Files\nodejs",
    "$env:USERPROFILE\.cargo\bin",
    "$env:SystemRoot\system32",
    "$env:SystemRoot",
    "$env:SystemRoot\System32\Wbem",
    "$env:SystemRoot\System32\WindowsPowerShell\v1.0\"
) -join ";"

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
    if (Test-Path $vcvars) {
        cmd /c "`"$vcvars`" >nul && set" | ForEach-Object {
            if ($_ -match '^([^=]+)=(.*)$') {
                Set-Item -Path "Env:$($matches[1])" -Value $matches[2]
            }
        }
    }
}

if (-not (Test-Path "ui\node_modules")) {
    Write-Host "Installing UI dependencies..."
    npm --prefix ui install
}

$tauri = "ui\node_modules\.bin\tauri.CMD"
if (-not (Test-Path $tauri)) {
    throw "Tauri CLI missing at $tauri - run: npm --prefix ui install"
}

& $tauri dev @args
exit $LASTEXITCODE
