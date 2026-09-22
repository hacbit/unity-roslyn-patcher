@echo off
rem Copy this file into the Unity project root, then double-click it.
"D:\Tools\unity-roslyn-patcher\dist\unity-launcher.exe" --project "%~dp0."
if errorlevel 1 pause
