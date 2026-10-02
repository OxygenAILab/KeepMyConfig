# Build and package the Windows release binary.
# GitHub@OxygenAILab | OxygenAILab@StarsailsClover

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    cargo build --release -p keepmyconfig-cli

    $version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' |
        Select-Object -First 1).Matches.Groups[1].Value
    $releaseDir = Join-Path $root "release"
    New-Item -ItemType Directory -Force $releaseDir | Out-Null

    $binary = Join-Path $root "target\release\keepmyconfig.exe"
    $packageDir = Join-Path $releaseDir "keepmyconfig-x86_64-pc-windows-msvc"
    New-Item -ItemType Directory -Force $packageDir | Out-Null
    Copy-Item $binary (Join-Path $packageDir "keepmyconfig.exe") -Force
    Copy-Item (Join-Path $root "README.md") $packageDir -Force
    Copy-Item (Join-Path $root "README.zh-CN.md") $packageDir -Force

    $zip = Join-Path $releaseDir "keepmyconfig-x86_64-pc-windows-msvc.zip"
    if (Test-Path $zip) { Remove-Item -LiteralPath $zip -Force }
    Compress-Archive -Path (Join-Path $packageDir "*") -DestinationPath $zip

    Write-Host "Built KeepMyConfig $version -> $zip"
}
finally {
    Pop-Location
}

