[CmdletBinding()]
param(
    [string]$Version = "v0.0.1-local",
    [string]$Target  = "x86_64-pc-windows-msvc"
)

$ScriptDir = $PSScriptRoot

# uniffi-bindgen-go v0.7.1 targets uniffi 0.31.0, which is exactly the version
# the workspace is pinned to (uniffi = "=0.31.0").
# See https://github.com/NordSecurity/uniffi-bindgen-go/releases for available tags.
$UniffiBindgenGoTag = "v0.7.1+v0.31.0"

switch -Wildcard ($Target) {
    "*-apple-*"   { $LibExt = "dylib" }
    "*-linux-*"   { $LibExt = "so"    }
    "*-windows-*" { $LibExt = "dll"   }
    default { Write-Error "Unrecognized target triple: $Target"; exit 1 }
}

$LibPrefix = if ($LibExt -eq "dll") { "" } else { "lib" }
$LibName   = "${LibPrefix}chia_wallet_sdk.$LibExt"
$BuiltName = "${LibPrefix}chia_wallet_sdk_go.$LibExt"
$LibPath   = Join-Path $ScriptDir ".." "target" $Target "release-go" $BuiltName

if ($IsWindows -and -not $env:CMAKE_GENERATOR) {
    $env:CMAKE_GENERATOR = "Ninja"
}

Write-Host "Building native library for $Target..."
Push-Location (Join-Path $ScriptDir "..")
try {
    cargo build --profile release-go -p chia-wallet-sdk-go --target $Target
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

if (-not (Get-Command uniffi-bindgen-go -ErrorAction SilentlyContinue)) {
    Write-Host "Installing uniffi-bindgen-go $UniffiBindgenGoTag..."
    cargo install uniffi-bindgen-go `
        --git https://github.com/NordSecurity/uniffi-bindgen-go `
        --tag $UniffiBindgenGoTag
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host "Generating Go bindings..."
uniffi-bindgen-go --library $LibPath --out-dir $ScriptDir
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Mirrors patch-bindings.sh (used by local-build.sh and CI); keep the rules in sync.
# uniffi-bindgen-go generates OptionType factory constructors as OptionTypeCat(),
# OptionTypeNft(), etc., which collide with the separately-generated OptionTypeCat
# and OptionTypeNft struct types (likewise DelegatedPuzzle.oracle() vs the
# DelegatedPuzzleOracle type). Rename the constructors to resolve the conflict.
Write-Host "Patching naming collisions..."
$Generated = Join-Path $ScriptDir "chia_wallet_sdk" "chia_wallet_sdk.go"
$content = [System.IO.File]::ReadAllText($Generated)
$content = $content `
    -replace '(?m)^func OptionTypeCat\(',          'func NewOptionTypeFromCat(' `
    -replace '(?m)^func OptionTypeNft\(',          'func NewOptionTypeFromNft(' `
    -replace '(?m)^func OptionTypeRevocableCat\(', 'func NewOptionTypeFromRevocableCat(' `
    -replace '(?m)^func OptionTypeXch\(',          'func NewOptionTypeFromXch(' `
    -replace '(?m)^func DelegatedPuzzleOracle\(',  'func NewDelegatedPuzzleFromOracle('
[System.IO.File]::WriteAllText($Generated, $content)

Write-Host "Staging native library..."
$OutDir = Join-Path $ScriptDir "chia_wallet_sdk"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Copy-Item $LibPath (Join-Path $OutDir $LibName)

Write-Host ""
Write-Host "Go bindings generated in: $OutDir"
Write-Host ""
Write-Host "To build a Go project using these bindings, set CGO_LDFLAGS:"
Write-Host "  `$env:CGO_LDFLAGS = `"-L$OutDir -lchia_wallet_sdk`""
Write-Host "  go build ./..."
