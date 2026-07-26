[CmdletBinding()]
param(
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ScriptDir = $PSScriptRoot

# uniffi-bindgen-cpp v0.8.1 targets uniffi 0.29.4, which is exactly the version
# the workspace is pinned to (uniffi = "=0.29.4"). No patch-version mismatch here.
# See https://github.com/NordSecurity/uniffi-bindgen-cpp/tags for available tags.
$UniffiBindgenCppTag = "v0.8.1+v0.29.4"

switch -Wildcard ($Target) {
    "*-apple-*"   { $LibExt = "dylib" }
    "*-linux-*"   { $LibExt = "so"    }
    "*-windows-*" { $LibExt = "dll"   }
    default { Write-Error "Unrecognized target triple: $Target"; exit 1 }
}

$LibPrefix = if ($LibExt -eq "dll") { "" } else { "lib" }
$LibName   = "${LibPrefix}chia_wallet_sdk.$LibExt"
$LibPath   = Join-Path $ScriptDir ".." "target" $Target "release-cpp" $LibName

Write-Host "Building native library for $Target..."
Push-Location (Join-Path $ScriptDir "..")
try {
    cargo build --profile release-cpp -p chia-wallet-sdk-cpp --target $Target
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

if (-not (Get-Command uniffi-bindgen-cpp -ErrorAction SilentlyContinue)) {
    Write-Host "Installing uniffi-bindgen-cpp $UniffiBindgenCppTag..."
    cargo install uniffi-bindgen-cpp `
        --git https://github.com/NordSecurity/uniffi-bindgen-cpp `
        --tag $UniffiBindgenCppTag
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host "Generating C++ bindings..."
$OutDir = Join-Path $ScriptDir "chia_wallet_sdk"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
uniffi-bindgen-cpp --library $LibPath --out-dir $OutDir
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Patch two known uniffi-bindgen-cpp code-generation defects so the output compiles:
#   1. Clvm methods `bool`/`int` are emitted with their (reserved) C++ keyword names;
#      rename them to `bool_`/`int_`.
#   2. The forward declarations for VdfInfo/VdfProof are emitted with all-caps acronyms
#      (`VDFInfo`/`VDFProof`) while every other reference uses `VdfInfo`/`VdfProof`.
Write-Host "Patching generated bindings..."
$Hpp = Join-Path $OutDir "chia_wallet_sdk.hpp"
$Cpp = Join-Path $OutDir "chia_wallet_sdk.cpp"
$hppText = [System.IO.File]::ReadAllText($Hpp)
$hppText = $hppText `
    -replace 'std::shared_ptr<Program> bool\(', 'std::shared_ptr<Program> bool_(' `
    -replace 'std::shared_ptr<Program> int\(',  'std::shared_ptr<Program> int_(' `
    -replace 'struct VDFInfo;',  'struct VdfInfo;' `
    -replace 'struct VDFProof;', 'struct VdfProof;'
[System.IO.File]::WriteAllText($Hpp, $hppText)
$cppText = [System.IO.File]::ReadAllText($Cpp)
$cppText = $cppText `
    -replace 'Clvm::bool\(', 'Clvm::bool_(' `
    -replace 'Clvm::int\(',  'Clvm::int_('
[System.IO.File]::WriteAllText($Cpp, $cppText)

Write-Host "Staging native library..."
Copy-Item $LibPath $OutDir

Write-Host ""
Write-Host "C++ bindings generated in: $OutDir"
Write-Host "Compile chia_wallet_sdk.cpp with C++20 and link against chia_wallet_sdk."
