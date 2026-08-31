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
    $Iscc = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if (-not $Iscc) {
        $Candidates = @(
            "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
            "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
        )
        $IsccPath = $Candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
        if (-not $IsccPath) { throw "Inno Setup 6 compiler (ISCC.exe) was not found" }
    } else {
        $IsccPath = $Iscc.Source
    }
    Write-Host "Compiling installer with Inno Setup..." -ForegroundColor Cyan
    & $IsccPath $Iss
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed with exit code $LASTEXITCODE" }
    Write-Host "Installer written to target\release\installer\" -ForegroundColor Green
}
