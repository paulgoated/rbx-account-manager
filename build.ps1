# Builds dist\RBXAccountManager.exe with the Rust GNU toolchain (no Visual Studio needed).
# Usage:  powershell -ExecutionPolicy Bypass -File build.ps1
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
Set-Location $root
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $root 'target' }

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not found. Install Rust with: rustup-init.exe --default-host x86_64-pc-windows-gnu"
}

# webbrowser (pulled in by egui) links against shlwapi, which the self-contained GNU
# toolchain doesn't ship. The windows-targets import library contains those imports,
# so a copy of it named libshlwapi.a is enough. Generated here, never committed.
$lib = Join-Path $root 'linklibs\libshlwapi.a'
if (-not (Test-Path $lib)) {
    cargo fetch --quiet
    $src = Get-ChildItem "$env:USERPROFILE\.cargo\registry\src\*\windows_x86_64_gnu-0.52.6\lib\libwindows.0.52.0.a" | Select-Object -First 1
    if (-not $src) { throw "windows_x86_64_gnu 0.52.6 not found in the cargo registry" }
    New-Item -ItemType Directory -Force (Split-Path $lib) | Out-Null
    Copy-Item $src.FullName $lib
}

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$exe = Join-Path $dist 'RBXAccountManager.exe'
Copy-Item (Join-Path $env:CARGO_TARGET_DIR 'release\RBXAccountManager.exe') $exe -Force
& (Join-Path $root 'set-icon.ps1') -Exe $exe -Ico (Join-Path $root 'app.ico')
"Built: $exe"
