param([string]$Unity)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $repoRoot ('work/overrides-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'UnitySmoke/Assets'),(Join-Path $PSScriptRoot 'UnitySmoke/Packages'),(Join-Path $PSScriptRoot 'UnitySmoke/ProjectSettings') -Destination $testRoot -Recurse
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Overrides') -Destination (Join-Path $testRoot 'Assets') -Recurse
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'OverridesSmoke.cs') -Destination (Join-Path $testRoot 'Assets/Editor/OverridesSmoke.cs')
$config = Get-Content (Join-Path $repoRoot 'unity-launcher.example.json') -Raw | ConvertFrom-Json
$config.lang_version = '12'
if ($Unity) { $config.unity = (Resolve-Path -LiteralPath $Unity).Path }
$configPath = Join-Path $testRoot 'launcher.json'
$config | ConvertTo-Json | Set-Content -LiteralPath $configPath -Encoding Ascii
$log = Join-Path $testRoot 'overrides.log'
& (Join-Path $repoRoot 'dist/unity-launcher.exe') --config $configPath --project $testRoot --wait -- -batchmode -nographics -noUpm -executeMethod OverridesSmoke.Run -logFile $log
if ($LASTEXITCODE -ne 0) { throw "Overrides smoke failed: $log" }
if (!(Get-Content $log -Raw).Contains('ROSLYN_OVERRIDES_HOT_AND_PLAYER_OK')) { throw 'Missing final verification marker' }
Write-Host "PASS: default 12, per-assembly 14/9.0, IDE, live override addition/removal, Player compilation. Artifacts: $testRoot"
