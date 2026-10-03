#Requires -Version 7.0
param(
    [string]$LitePath,
    [string]$BundledPath,
    # Optional local dry-run through the actual CMD; requires an installed Unity version.
    [string]$UnityVersion
)
$ErrorActionPreference = 'Stop'
$version = & "$PSScriptRoot/../tools/Get-PackageVersion.ps1"
if (-not $LitePath) { $LitePath = "$PSScriptRoot/../dist/unity-roslyn-launcher-v$version-lite.zip" }
if (-not $BundledPath) { $BundledPath = "$PSScriptRoot/../dist/unity-roslyn-launcher-v$version-bundled.zip" }
Add-Type -AssemblyName System.IO.Compression.FileSystem
foreach ($path in @($LitePath, $BundledPath)) {
    $archive = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $path).Path)
    try {
        $names = @($archive.Entries.FullName)
        foreach ($required in @('unity-launcher.exe', 'unity-launcher.json', 'Open-Unity.cmd', 'runtime.json', 'Detours-LICENSE.md')) {
            if ($required -notin $names) { throw "Missing $required in $path" }
        }
        $reader = [IO.StreamReader]::new($archive.GetEntry('runtime.json').Open())
        try { $runtime = ($reader.ReadToEnd() | ConvertFrom-Json).directory } finally { $reader.Dispose() }
        foreach ($required in @("$runtime/compiler-proxy.exe", "$runtime/unity_roslyn_hook.dll")) {
            if ($required -notin $names) { throw "Missing $required in $path" }
        }
        if (@($names | Where-Object { $_.StartsWith('runtime/') }).Count -ne 2) { throw 'Unexpected runtime payload' }
        if ($names -match '(chain-probe|\.bak$|\.zip$)') { throw 'Unexpected development payload' }
        if ($path -eq $LitePath -and $names -match '^RoslynCompiler/') { throw 'Lightweight package includes compiler' }
    } finally { $archive.Dispose() }
}
# Extraction to a different directory verifies relative paths and runtime independence.
$testRoot = Join-Path ([IO.Path]::GetFullPath("$PSScriptRoot/../work")) "package test $([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $testRoot -Force | Out-Null
[IO.Compression.ZipFile]::ExtractToDirectory((Resolve-Path -LiteralPath $BundledPath).Path, $testRoot)
$config = Get-Content -LiteralPath (Join-Path $testRoot 'unity-launcher.json') -Raw | ConvertFrom-Json
if ([IO.Path]::IsPathRooted($config.dotnet) -or [IO.Path]::IsPathRooted($config.csc)) { throw 'Bundled config is not portable' }
$dotnet = Join-Path $testRoot $config.dotnet
$csc = Join-Path $testRoot $config.csc
$compilerRoot = Join-Path $testRoot 'RoslynCompiler'
foreach ($name in @('LICENSE.txt', 'ThirdPartyNotices.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $compilerRoot $name))) { throw "Missing license: $name" }
}
if (Test-Path -LiteralPath (Join-Path $compilerRoot 'packs')) { throw 'Reference packs must not ship' }
$runtime = Get-ChildItem -LiteralPath (Join-Path $compilerRoot 'shared/Microsoft.NETCore.App') -Directory
if (@($runtime).Count -ne 1) { throw 'Expected exactly one bundled runtime' }
$source = Join-Path $testRoot 'Fixture.cs'
Set-Content -LiteralPath $source -Encoding utf8 -Value 'public class Fixture { public int Value { get; set => field = value; } }'
& $dotnet exec $csc /nologo /noconfig /nostdlib /target:library "/langversion:$($config.lang_version)" "/out:$testRoot/Fixture.dll" "/reference:$($runtime.FullName)/System.Private.CoreLib.dll" "/reference:$($runtime.FullName)/System.Runtime.dll" $source
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $testRoot 'Fixture.dll'))) { throw 'Extracted compiler failed C# 14 fixture' }
if ($UnityVersion) {
    New-Item -ItemType Directory -Path (Join-Path $testRoot 'Assets'),(Join-Path $testRoot 'ProjectSettings') | Out-Null
    Set-Content -LiteralPath (Join-Path $testRoot 'ProjectSettings/ProjectVersion.txt') -Value "m_EditorVersion: $UnityVersion"
    Push-Location $testRoot
    try {
        # Feed ENTER if CMD pauses on failure, so a broken launcher cannot hang CI.
        $output = '' | & $env:ComSpec /d /c 'Open-Unity.cmd --dry-run' 2>&1
        $output | Write-Output
        if ($LASTEXITCODE -ne 0 -or -not ($output -match 'Preflight OK; no project changes or launch\.')) { throw 'One-click CMD dry-run failed' }
    } finally { Pop-Location }
}
Write-Output "Package validation passed; extracted fixture: $testRoot"
