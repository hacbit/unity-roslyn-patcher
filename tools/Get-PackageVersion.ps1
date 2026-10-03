$ErrorActionPreference = 'Stop'
$manifest = Get-Content -LiteralPath "$PSScriptRoot/../crates/launcher/Cargo.toml" -Raw
$package = [regex]::Match($manifest, '(?ms)^\[package\]\s*\r?\n(?<body>.*?)(?=^\[|\z)')
$version = [regex]::Match($package.Groups['body'].Value, '(?m)^version\s*=\s*"(?<version>[^"]+)"\s*$')
if (-not $version.Success) { throw 'Cannot read launcher package version from Cargo.toml' }
$version.Groups['version'].Value
