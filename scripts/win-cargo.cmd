@echo off
setlocal EnableExtensions
REM Clean MSVC + LLVM env, then run cargo. Avoids PATH blow-up from nested vcvars.
set "PATH=C:\Program Files\LLVM\bin;C:\Program Files\nodejs;%SystemRoot%\system32;%SystemRoot%;%SystemRoot%\System32\Wbem;%SystemRoot%\System32\WindowsPowerShell\v1.0\;%USERPROFILE%\.cargo\bin"
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
set "WHISPER_DONT_GENERATE_BINDINGS="

for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSINSTALL=%%i"
if not defined VSINSTALL (
  echo Visual Studio Build Tools with C++ workload not found.
  exit /b 1
)

call "%VSINSTALL%\VC\Auxiliary\Build\vcvars64.bat" >nul
if errorlevel 1 exit /b 1

cd /d "%~dp0.."
cargo %*
exit /b %ERRORLEVEL%
