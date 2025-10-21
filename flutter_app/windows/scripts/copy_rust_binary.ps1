param(
    [string]$Configuration = "Debug",
    [string]$DestDir = ""
)

Write-Host "========== Copying Rust Binary =========="

$BuildType = if ($Configuration -eq "Release" -or $Configuration -eq "Profile") { "release" } else { "debug" }

Write-Host "Build Type: $BuildType"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Resolve-Path (Join-Path $ScriptDir "..\..\..") | Select-Object -ExpandProperty Path
$RustBinaryPath = Join-Path $ProjectRoot "target\$BuildType\fungi.exe"

if ([string]::IsNullOrEmpty($DestDir)) {
    if ($env:CMAKE_BINARY_DIR) {
        $CMakeBinaryDir = $env:CMAKE_BINARY_DIR
    } else {
        $CMakeBinaryDir = Join-Path $ProjectRoot "flutter_app\build\windows\x64\runner"
    }
    $DestDir = Join-Path $CMakeBinaryDir $Configuration
}

$DestBinary = Join-Path $DestDir "fungi.exe"

Write-Host "Source: $RustBinaryPath"
Write-Host "Destination: $DestBinary"

if (-not (Test-Path $RustBinaryPath)) {
    Write-Host "Error: Rust binary not found!" -ForegroundColor Red
    $BuildCmd = if ($BuildType -eq "release") { "cargo build --release" } else { "cargo build" }
    Write-Host "Please run: $BuildCmd"
    exit 1
}

if (-not (Test-Path $DestDir)) {
    New-Item -ItemType Directory -Path $DestDir -Force | Out-Null
}

$NeedsCopy = $true

if ($BuildType -eq "debug" -and (Test-Path $DestBinary)) {
    $SourceLastWrite = (Get-Item $RustBinaryPath).LastWriteTime
    $DestLastWrite = (Get-Item $DestBinary).LastWriteTime
    
    if ($SourceLastWrite -le $DestLastWrite) {
        $NeedsCopy = $false
        Write-Host "Binary is up to date, skipping copy"
    }
}

if ($NeedsCopy) {
    if (Test-Path $DestBinary) {
        Remove-Item $DestBinary -Force
    }
    
    Copy-Item $RustBinaryPath -Destination $DestBinary -Force
    
    if ($BuildType -eq "debug") {
        Write-Host "Copied binary (debug)"
    } else {
        Write-Host "Copied binary (release)"
    }
}

Write-Host "========== Done =========="
