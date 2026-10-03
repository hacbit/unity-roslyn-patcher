#Requires -Version 7.0
param(
    # Existing callers can still use OutputPath for the lightweight ZIP.
    [string]$OutputPath,
    [string]$BundledOutputPath,
    # Defaults to the launcher Cargo.toml version; release tags may override it.
    [string]$Version,
    [string]$DotnetRoot,
    [string]$SdkVersion = '10.0.401',
    [switch]$LiteOnly,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($PSScriptRoot)
$dist = Join-Path $root 'dist'
if (-not $Version) { $Version = & (Join-Path $root 'tools/Get-PackageVersion.ps1') }
$Version = $Version -replace '^v', ''
if ($Version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$') { throw "Invalid package version: $Version" }
if (-not $SkipBuild) {
    & (Join-Path $root 'build.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Launcher build failed' }
}
function Require-File([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Missing package input: $Path" }
}
function Archive-Path([string]$Path) {
    if (-not [IO.Path]::IsPathRooted($Path)) { $Path = Join-Path $root $Path }
    $Path = [IO.Path]::GetFullPath($Path)
    if ([IO.Path]::GetExtension($Path) -ne '.zip') { throw "Output must be a .zip file: $Path" }
    return $Path
}
$manifestPath = Join-Path $dist 'runtime.json'
Require-File $manifestPath
$runtimeRelative = (Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json).directory
if ($runtimeRelative -notmatch '^runtime/[0-9A-Fa-f]{16}-[0-9A-Fa-f]{16}$') {
    throw "Invalid runtime directory in manifest: $runtimeRelative"
}
# Explicit allowlist excludes local configs, diagnostic tools, old runtimes and archives.
$commonFiles = [ordered]@{
    'unity-launcher.exe' = Join-Path $dist 'unity-launcher.exe'
    'runtime.json' = $manifestPath
    "$runtimeRelative/compiler-proxy.exe" = Join-Path $dist "$runtimeRelative/compiler-proxy.exe"
    "$runtimeRelative/unity_roslyn_hook.dll" = Join-Path $dist "$runtimeRelative/unity_roslyn_hook.dll"
    'Open-Unity.cmd' = Join-Path $root 'Open-Unity.example.cmd'
    'README.md' = Join-Path $root 'README.md'
    'Detours-LICENSE.md' = Join-Path $root 'vendor/Detours-4.0.1/LICENSE.md'
    '.gitignore' = Join-Path $root 'dist.gitignore'
}
foreach ($path in $commonFiles.Values) { Require-File $path }
Require-File (Join-Path $root 'unity-launcher.auto.json')
$liteConfig = Get-Content -LiteralPath (Join-Path $root 'unity-launcher.auto.json') -Raw -Encoding UTF8
if (-not $OutputPath) { $OutputPath = Join-Path $dist "unity-roslyn-launcher-v$Version-lite.zip" }
$OutputPath = Archive-Path $OutputPath
if (-not $BundledOutputPath) {
    $stem = [IO.Path]::GetFileNameWithoutExtension($OutputPath) -replace '-lite$', ''
    $BundledOutputPath = Join-Path ([IO.Path]::GetDirectoryName($OutputPath)) "$stem-bundled.zip"
}
$BundledOutputPath = Archive-Path $BundledOutputPath
if (-not $LiteOnly -and $OutputPath -eq $BundledOutputPath) { throw 'Archive output paths must differ' }
$bundledFiles = [ordered]@{}
if (-not $LiteOnly) {
    if ($SdkVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'SdkVersion must be a stable SDK version, e.g. 10.0.401' }
    if (-not $DotnetRoot) {
        $command = Get-Command dotnet.exe -ErrorAction SilentlyContinue
        if (-not $command) { throw 'Pass -DotnetRoot <SDK or extracted RoslynCompiler directory>, or use -LiteOnly' }
        $DotnetRoot = Split-Path -Parent $command.Source
    }
    $DotnetRoot = (Resolve-Path -LiteralPath $DotnetRoot).Path
    $compilerRelative = "sdk/$SdkVersion/Roslyn/bincore"
    $compilerRoot = Join-Path $DotnetRoot $compilerRelative
    foreach ($name in @('dotnet.exe', 'LICENSE.txt', 'ThirdPartyNotices.txt',
        "$compilerRelative/csc.dll", "$compilerRelative/csc.deps.json", "$compilerRelative/csc.runtimeconfig.json",
        "$compilerRelative/Microsoft.CodeAnalysis.dll", "$compilerRelative/Microsoft.CodeAnalysis.CSharp.dll")) {
        Require-File (Join-Path $DotnetRoot $name)
    }
    $runtimeConfig = Get-Content -LiteralPath (Join-Path $compilerRoot 'csc.runtimeconfig.json') -Raw | ConvertFrom-Json
    $framework = $runtimeConfig.runtimeOptions.framework
    if ($framework.name -ne 'Microsoft.NETCore.App') { throw 'Unsupported compiler runtime framework' }
    $minimum = [version]$framework.version
    # Copy only one compatible runtime and matching hostfxr; omit SDK tools/reference packs.
    $runtime = Get-ChildItem -LiteralPath (Join-Path $DotnetRoot 'shared/Microsoft.NETCore.App') -Directory |
        Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
        Where-Object { ([version]$_.Name).Major -eq $minimum.Major -and
            ([version]$_.Name).Minor -eq $minimum.Minor -and ([version]$_.Name) -ge $minimum } |
        Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
    if (-not $runtime) { throw "Missing compatible Microsoft.NETCore.App $minimum runtime" }
    $runtimeVersion = $runtime.Name
    Require-File (Join-Path $runtime.FullName 'coreclr.dll')
    Require-File (Join-Path $runtime.FullName 'hostpolicy.dll')
    Require-File (Join-Path $DotnetRoot "host/fxr/$runtimeVersion/hostfxr.dll")
    $compilerVersion = & (Join-Path $DotnetRoot 'dotnet.exe') exec (Join-Path $compilerRoot 'csc.dll') -version
    if ($LASTEXITCODE -ne 0) { throw 'Bundled compiler preflight failed' }
    $languages = & (Join-Path $DotnetRoot 'dotnet.exe') exec (Join-Path $compilerRoot 'csc.dll') '-langversion:?'
    $language = ($liteConfig | ConvertFrom-Json).lang_version
    if (-not ($languages | Where-Object {
        (($_ -split '\s+')[0] -replace '\.0$', '') -eq ($language -replace '\.0$', '')
    })) {
        throw "Compiler does not support configured C# $language"
    }
    foreach ($name in @('dotnet.exe', 'LICENSE.txt', 'ThirdPartyNotices.txt')) {
        $bundledFiles["RoslynCompiler/$name"] = Join-Path $DotnetRoot $name
    }
    foreach ($directory in @($compilerRelative, "host/fxr/$runtimeVersion", "shared/Microsoft.NETCore.App/$runtimeVersion")) {
        foreach ($file in Get-ChildItem -LiteralPath (Join-Path $DotnetRoot $directory) -Recurse -File) {
            $relative = [IO.Path]::GetRelativePath($DotnetRoot, $file.FullName).Replace('\', '/')
            $bundledFiles["RoslynCompiler/$relative"] = $file.FullName
        }
    }
    $bundledConfig = [ordered]@{
        dotnet = 'RoslynCompiler/dotnet.exe'
        csc = "RoslynCompiler/$compilerRelative/csc.dll"
        lang_version = $language
    } | ConvertTo-Json
    Write-Output "Bundled: SDK $SdkVersion / .NET $runtimeVersion / Roslyn $compilerVersion"
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
function Write-Package([string]$Path, $ExtraFiles, [string]$Config) {
    New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($Path)) | Out-Null
    $temporary = "$Path.$([guid]::NewGuid().ToString('N')).tmp"
    $archive = $null
    try {
        $archive = [IO.Compression.ZipFile]::Open($temporary, [IO.Compression.ZipArchiveMode]::Create)
        foreach ($files in @($commonFiles, $ExtraFiles)) {
            foreach ($entry in $files.GetEnumerator()) {
                [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                    $archive, $entry.Value, $entry.Key, [IO.Compression.CompressionLevel]::Optimal) | Out-Null
            }
        }
        $entry = $archive.CreateEntry('unity-launcher.json')
        $writer = [IO.StreamWriter]::new($entry.Open(), [Text.UTF8Encoding]::new($false))
        try { $writer.WriteLine($Config) } finally { $writer.Dispose() }
        $archive.Dispose()
        $archive = $null
        Move-Item -LiteralPath $temporary -Destination $Path -Force
    } finally {
        if ($null -ne $archive) { $archive.Dispose() }
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
    Write-Output "Package: $Path"
}
Write-Package $OutputPath ([ordered]@{}) $liteConfig
if (-not $LiteOnly) { Write-Package $BundledOutputPath $bundledFiles $bundledConfig }
Write-Output "Runtime: $runtimeRelative"
