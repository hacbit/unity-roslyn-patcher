@echo off
setlocal
rem Default: launcher EXE, JSON and this CMD are in the same directory.
rem If you move this CMD, change TOOL_DIR to the launcher directory below.
set "TOOL_DIR=%~dp0."
rem Relative compiler paths in the JSON are resolved from the JSON directory.
set "CONFIG_PATH=%TOOL_DIR%\unity-launcher.json"
rem Default: this CMD is in the Unity project root. Otherwise set an absolute path.
set "PROJECT_DIR=%~dp0."
rem Example: set "TOOL_DIR=D:\Tools\unity-roslyn-launcher"
rem Example: set "PROJECT_DIR=D:\Projects\MyUnityProject"
"%TOOL_DIR%\unity-launcher.exe" --config "%CONFIG_PATH%" --project "%PROJECT_DIR%" %*
if errorlevel 1 pause
endlocal
