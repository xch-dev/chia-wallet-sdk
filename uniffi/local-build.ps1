[CmdletBinding()]
param(
    [string]$Version = "0.0.4-local",
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ScriptDir = $PSScriptRoot

function Show-Usage {
    Write-Host "Usage: .\local-build.ps1 [-Version <version>] [-Target <target>]"
    Write-Host "  -Version   NuGet package version (default: 0.0.4-local)"
    Write-Host "  -Target    Rust target triple (default: x86_64-pc-windows-msvc)"
    exit 1
}

# Derive library filename and .NET RID from the target triple
switch -Wildcard ($Target) {
    "*-apple-*" {
        $LibExt = "dylib"
        switch -Wildcard ($Target) {
            "aarch64-*" { $DotnetRid = "osx-arm64" }
            "x86_64-*"  { $DotnetRid = "osx-x64" }
            default     { Write-Error "Unsupported macOS arch in target: $Target"; exit 1 }
        }
    }
    "*-linux-*" {
        $LibExt = "so"
        switch -Wildcard ($Target) {
            "aarch64-*" { $DotnetRid = "linux-arm64" }
            "x86_64-*"  { $DotnetRid = "linux-x64" }
            default     { Write-Error "Unsupported Linux arch in target: $Target"; exit 1 }
        }
    }
    "*-windows-*" {
        $LibExt = "dll"
        switch -Wildcard ($Target) {
            "aarch64-*" { $DotnetRid = "win-arm64" }
            "x86_64-*"  { $DotnetRid = "win-x64" }
            default     { Write-Error "Unsupported Windows arch in target: $Target"; exit 1 }
        }
    }
    default {
        Write-Error "Unrecognized target triple: $Target"
        exit 1
    }
}

# Windows DLLs have no "lib" prefix; all other platforms do.
$LibPrefix = if ($LibExt -eq "dll") { "" } else { "lib" }
$LibName = "${LibPrefix}chia_wallet_sdk.$LibExt"
$LibPath = Join-Path $ScriptDir ".." "target" $Target "release" $LibName

# Force Ninja generator on Windows to avoid MSBuild's VCTargetsPath detection,
# which fails with MSB4136/System.MarvinHash on some VS BuildTools installs.
if ($IsWindows -and -not $env:CMAKE_GENERATOR) {
    $env:CMAKE_GENERATOR = "Ninja"
}

Write-Host "Building native library for $Target..."
Push-Location (Join-Path $ScriptDir "..")
try {
    cargo build --release -p chia-wallet-sdk-cs --target $Target
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

Write-Host "Generating C# bindings..."
uniffi-bindgen-cs `
    --library `
    --out-dir "$ScriptDir\cs" `
    --config "$ScriptDir\uniffi.toml" `
    $LibPath
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Staging native library..."
$NativeDir = Join-Path $ScriptDir "cs" "runtimes" $DotnetRid "native"
New-Item -ItemType Directory -Force -Path $NativeDir | Out-Null
Copy-Item $LibPath $NativeDir

Write-Host "Packing NuGet (version: $Version)..."
dotnet pack "$ScriptDir\cs\ChiaWalletSdk.csproj" `
    -c Release `
    -o "$ScriptDir\nuget-out" `
    -p:Version=$Version
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host ""
Write-Host "Package ready: $ScriptDir\nuget-out\ChiaWalletSdk.$Version.nupkg"
Write-Host ""
Write-Host "To register the local feed (once):"
Write-Host "  dotnet nuget add source $ScriptDir\nuget-out --name chia-local"
Write-Host ""
Write-Host "To add to a project:"
Write-Host "  dotnet add package ChiaWalletSdk --version $Version"
