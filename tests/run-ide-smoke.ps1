param(
    [string]$VisualStudioDll = 'D:\clients\client_hls_2\Library\ScriptAssemblies\Unity.VisualStudio.Editor.dll'
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $repoRoot ('work/ide-smoke-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'UnitySmoke/Assets'),(Join-Path $PSScriptRoot 'UnitySmoke/Packages'),(Join-Path $PSScriptRoot 'UnitySmoke/ProjectSettings') -Destination $testRoot -Recurse
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'IdeSyncSmoke.cs') -Destination (Join-Path $testRoot 'Assets/Editor/IdeSyncSmoke.cs')
Copy-Item -LiteralPath $VisualStudioDll -Destination (Join-Path $testRoot 'Assets/Editor/Unity.VisualStudio.Editor.dll')
[IO.File]::WriteAllText((Join-Path $testRoot 'GeneratedBeforeLaunch.csproj'), '<Project><!-- AssetPostprocessor.OnGeneratedCSProject --><PropertyGroup><LangVersion>9.0</LangVersion></PropertyGroup></Project>')
[IO.File]::WriteAllText((Join-Path $testRoot 'HandWritten.csproj'), '<Project><PropertyGroup><LangVersion>9.0</LangVersion></PropertyGroup></Project>')
$log = Join-Path $testRoot 'ide-test.log'
& (Join-Path $repoRoot 'dist/unity-launcher.exe') --config (Join-Path $repoRoot 'unity-launcher.example.json') --project $testRoot --wait -- -batchmode -nographics -noUpm -quit -executeMethod IdeSyncSmoke.Run -logFile $log
if ($LASTEXITCODE -ne 0) { throw "IDE smoke failed; read $log" }
$content = Get-Content -LiteralPath $log -Raw
foreach ($marker in @('ROSLYN_IDE_EXISTING_OK','ROSLYN_IDE_CALLBACK_OK','ROSLYN_IDE_REGENERATION_OK')) {
    if (!$content.Contains($marker)) { throw "Missing $marker in $log" }
}
Write-Host "PASS: existing projects, callbacks, actual VS regeneration. Artifacts: $testRoot"
