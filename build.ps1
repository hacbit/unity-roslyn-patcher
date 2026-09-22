$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    cargo build --workspace --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    New-Item -ItemType Directory -Force dist | Out-Null
    foreach ($source in @('target/release/unity-launcher.exe','target/release/chain-probe.exe')) {
        $destination = Join-Path 'dist' (Split-Path -Leaf $source)
        # A running Unity can hold the hook DLL open. Identical files need no overwrite.
        if ((Test-Path -LiteralPath $destination) -and
            ((Get-FileHash -LiteralPath $source).Hash -eq (Get-FileHash -LiteralPath $destination).Hash)) { continue }
        Copy-Item -LiteralPath $source -Destination $destination
    }
    # Per-session immutable runtime paths: an open Unity keeps its original DLL/proxy.
    $hookHash = (Get-FileHash -LiteralPath 'target/release/unity_roslyn_hook.dll').Hash.Substring(0,16)
    $proxyHash = (Get-FileHash -LiteralPath 'target/release/compiler-proxy.exe').Hash.Substring(0,16)
    $runtimeRelative = "runtime/$hookHash-$proxyHash"
    $runtimeDestination = Join-Path 'dist' $runtimeRelative
    New-Item -ItemType Directory -Force $runtimeDestination | Out-Null
    foreach ($source in @('target/release/compiler-proxy.exe','target/release/unity_roslyn_hook.dll')) {
        $destination = Join-Path $runtimeDestination (Split-Path -Leaf $source)
        if ((Test-Path -LiteralPath $destination) -and
            ((Get-FileHash -LiteralPath $source).Hash -eq (Get-FileHash -LiteralPath $destination).Hash)) { continue }
        Copy-Item -LiteralPath $source -Destination $destination
    }
    $manifestPath = Join-Path $PSScriptRoot 'dist/runtime.json'
    $manifestTemp = $manifestPath + '.tmp'
    [IO.File]::WriteAllText($manifestTemp, (@{ directory = $runtimeRelative } | ConvertTo-Json))
    # Windows PowerShell marshals $null to an empty string for this overload.
    if (Test-Path -LiteralPath $manifestPath) { [IO.File]::Replace($manifestTemp, $manifestPath, $manifestPath + '.bak') }
    else { [IO.File]::Move($manifestTemp, $manifestPath) }
    if (!(Test-Path -LiteralPath dist/unity-launcher.json)) {
        Copy-Item -LiteralPath unity-launcher.auto.json -Destination dist/unity-launcher.json
    }
    Copy-Item -LiteralPath vendor/Detours-4.0.1/LICENSE.md -Destination dist/Detours-LICENSE.md
    Copy-Item -LiteralPath README.md -Destination dist/README.md
} finally { Pop-Location }
