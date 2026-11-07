@echo off
rem Makes this PC YOBUKO's Linux build machine (Ubuntu in WSL). Asks for administrator rights.
powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process powershell -Verb RunAs -ArgumentList '-NoProfile -ExecutionPolicy Bypass -NoExit -File \"%~dp0brancher-yobuko-linux.ps1\"'"
