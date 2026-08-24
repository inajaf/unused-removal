# Build the Windows desktop binary (and optionally an installer).
# Usage: .\scripts\build-desktop-windows.ps1 [-Installer]
param(
    [switch]$Installer
)

$ErrorActionPreference = "Stop"
$AppName = "unused-removal"

Write-Host "Building release binary (feature: desktop)..." -ForegroundColor Cyan
cargo build --release --features desktop

$Bin = "target\release\$AppName.exe"
if (-not (Test-Path $Bin)) { throw "binary not found: $Bin" }
Write-Host "Built $Bin" -ForegroundColor Green

if ($Installer) {
    # Requires Inno Setup 6 (https://jrsoftware.org/isinfo.php) on PATH as ISCC.exe
    $Iss = "scripts\installer.iss"
    if (-not (Test-Path $Iss)) { throw "missing $Iss" }
    Write-Host "Compiling installer with Inno Setup..." -ForegroundColor Cyan
    ISCC $Iss
    Write-Host "Installer written to target\release\installer\" -ForegroundColor Green
}
