param([switch]$DisablePackageManager)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$testRoot = Join-Path $repoRoot ('work/smoke-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'UnitySmoke/Assets'),(Join-Path $PSScriptRoot 'UnitySmoke/Packages'),(Join-Path $PSScriptRoot 'UnitySmoke/ProjectSettings') -Destination $testRoot -Recurse
$launcher = Join-Path $repoRoot 'dist/unity-launcher.exe'
$config = Join-Path $repoRoot 'unity-launcher.example.json'
$log = Join-Path $testRoot 'editor-test.log'
$unityArgs = @('-batchmode','-nographics','-executeMethod','SmokeBuild.HotReload','-logFile',$log)
if ($DisablePackageManager) { $unityArgs += '-noUpm' }
& $launcher --config $config --project $testRoot --wait -- @unityArgs
if ($LASTEXITCODE -ne 0) { throw "Unity smoke failed; read $log" }
$content = Get-Content -LiteralPath $log -Raw
foreach ($marker in @('ROSLYN_EDITOR_OK','ROSLYN_HOT_RELOAD_OK','ROSLYN_BUILD_OK')) {
    if (!$content.Contains($marker)) { throw "Missing $marker in $log" }
}
$playerLog = Join-Path $testRoot 'player-test.log'
$player = Start-Process -FilePath (Join-Path $testRoot 'Build/Smoke.exe') -ArgumentList '-batchmode','-nographics','-logFile',('"' + $playerLog + '"') -WindowStyle Hidden -PassThru
if (!$player.WaitForExit(30000)) { Stop-Process -Id $player.Id; throw 'Smoke Player timed out' }
$player.Refresh()
if ($player.ExitCode -ne 0) { throw "Player failed: $($player.ExitCode)" }
if (!(Get-Content -LiteralPath $playerLog -Raw).Contains('ROSLYN_PLAYER_OK: 42, 3')) { throw 'Missing player success marker' }
Write-Host "PASS: Unity initial compile, hot reload, Player build and execution. Artifacts: $testRoot"
