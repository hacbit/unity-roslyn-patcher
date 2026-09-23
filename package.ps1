param(
    [string]$OutputPath,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($PSScriptRoot)
$dist = Join-Path $root 'dist'
if (-not $SkipBuild) {
    & (Join-Path $root 'build.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Launcher build failed' }
}

$manifestPath = Join-Path $dist 'runtime.json'
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw 'Missing dist/runtime.json; run build.ps1 first' }
$runtimeRelative = (Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json).directory
if ($runtimeRelative -notmatch '^runtime/[0-9A-Fa-f]{16}-[0-9A-Fa-f]{16}$') {
    throw "Invalid runtime directory in manifest: $runtimeRelative"
}
foreach ($required in @(
    'unity-launcher.exe', 'unity-launcher.json', 'runtime.json',
    "$runtimeRelative/compiler-proxy.exe", "$runtimeRelative/unity_roslyn_hook.dll"
)) {
    if (-not (Test-Path -LiteralPath (Join-Path $dist $required) -PathType Leaf)) {
        throw "Missing package input: $required"
    }
}
Copy-Item -LiteralPath (Join-Path $root 'dist.gitignore') -Destination (Join-Path $dist '.gitignore')

if (-not $OutputPath) { $OutputPath = Join-Path $dist 'unity-roslyn-launcher.zip' }
if (-not [IO.Path]::IsPathRooted($OutputPath)) { $OutputPath = Join-Path $root $OutputPath }
$OutputPath = [IO.Path]::GetFullPath($OutputPath)
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($OutputPath)) | Out-Null
$temporary = "$OutputPath.tmp"
$runtimePrefix = "$runtimeRelative/"
$files = @(Get-ChildItem -LiteralPath $dist -Recurse -File | Where-Object {
    $relative = [IO.Path]::GetRelativePath($dist, $_.FullName).Replace('\', '/')
    $relative -notmatch '\.zip(?:\.tmp)?$' -and
    $relative -ne 'runtime.json.bak' -and
    (-not $relative.StartsWith('runtime/') -or $relative.StartsWith($runtimePrefix))
})
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = $null
try {
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    $archive = [IO.Compression.ZipFile]::Open($temporary, [IO.Compression.ZipArchiveMode]::Create)
    foreach ($file in $files) {
        $relative = [IO.Path]::GetRelativePath($dist, $file.FullName).Replace('\', '/')
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
            $archive, $file.FullName, $relative, [IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
    $archive.Dispose()
    $archive = $null
    Move-Item -LiteralPath $temporary -Destination $OutputPath -Force
} finally {
    if ($null -ne $archive) { $archive.Dispose() }
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
}

Write-Output "Package: $OutputPath"
Write-Output "Runtime: $runtimeRelative"
