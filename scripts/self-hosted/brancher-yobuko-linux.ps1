# Copyright (C) 2026 The Suhina contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Makes the owner's Windows PC also YOBUKO's Linux build machine, so GitHub's own machines are
# not used at all: Ubuntu 22.04 in WSL (a Linux that runs inside Windows), set up by
# yobuko-linux-runner.sh as two GitHub runners labelled `yobuko-linux`, started by Windows at
# each sign-in. Run by double-clicking BRANCHER-YOBUKO-LINUX.bat (it asks for administrator
# rights). Safe to run again.

# 'Continue', not 'Stop': Windows PowerShell 5 turns any line a program like wsl.exe writes to its
# error output into a failure. Each step checks its own result instead.
$ErrorActionPreference = 'Continue'
$Distro = 'YOBUKO-Linux'
$Root = 'C:\yobuko-linux'
$RootfsBase = 'https://cloud-images.ubuntu.com/wsl/jammy/current'
$Rootfs = 'ubuntu-jammy-wsl-amd64-ubuntu22.04lts.rootfs.tar.gz'
$TokenPage = 'https://github.com/Nozomagarion/YOBUKO/settings/actions/runners/new?arch=x64&os=linux'
$TaskName = 'YOBUKO Linux runner'
$env:WSL_UTF8 = '1'

function Step($text) { Write-Host ''; Write-Host "== $text" -ForegroundColor Cyan }
function Done($text) { Write-Host $text -ForegroundColor Green }

# 1. WSL itself.
Step 'WSL (Linux inside Windows)'
& wsl.exe --version *> $null
if ($LASTEXITCODE -ne 0) {
    & wsl.exe --install --no-distribution
    Write-Host ''
    Write-Host 'WSL is installed. RESTART the PC, then double-click BRANCHER-YOBUKO-LINUX.bat again.' -ForegroundColor Yellow
    Read-Host 'Press Enter to close'
    exit 0
}
& wsl.exe --update *> $null
Done 'WSL ready.'

# 2. A dedicated Ubuntu 22.04, kept apart from any other Linux on this PC.
Step "Ubuntu 22.04 ($Distro)"
$known = (& wsl.exe --list --quiet) -replace "`0", '' | ForEach-Object { $_.Trim() }
if ($known -notcontains $Distro) {
    New-Item -ItemType Directory -Force -Path "$Root\disk" | Out-Null
    $tar = Join-Path $Root $Rootfs
    Write-Host 'Downloading Ubuntu 22.04 (about 350 MB)...'
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -UseBasicParsing -Uri "$RootfsBase/$Rootfs" -OutFile $tar -ErrorAction Stop
    Invoke-WebRequest -UseBasicParsing -Uri "$RootfsBase/SHA256SUMS" -OutFile "$Root\SHA256SUMS" -ErrorAction Stop
    $line = (Get-Content "$Root\SHA256SUMS") | Where-Object { $_ -match [regex]::Escape($Rootfs) } | Select-Object -First 1
    $expected = ($line -split '\s+')[0].ToLower()
    $got = (Get-FileHash -Algorithm SHA256 $tar).Hash.ToLower()
    if (-not $expected -or $expected -ne $got) { throw "Ubuntu download damaged (SHA-256 $got, expected $expected)" }
    & wsl.exe --import $Distro "$Root\disk" $tar --version 2
    if ($LASTEXITCODE -ne 0) { throw 'wsl --import failed' }
    Remove-Item $tar, "$Root\SHA256SUMS"
}
Done "$Distro ready."

# 3. The token that lets this PC join the YOBUKO repository as a runner.
Step 'GitHub registration token'
Write-Host 'A GitHub page opens. Under "Configure", copy the code after --token'
Write-Host '(a long code of capital letters and digits, like AB12CD34...), then paste it here.'
Start-Process $TokenPage
$token = (Read-Host 'Token').Trim()
if ($token -notmatch '^[A-Za-z0-9]{20,}$') { throw 'That does not look like a token: copy only the code after --token' }

# 4. Linux tools, Rust and the two runners (stopped first when this runs again).
if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
    Stop-ScheduledTask -TaskName $TaskName
    & wsl.exe --terminate $Distro | Out-Null
}
Step 'Installing the build tools and the runners (10 to 20 minutes the first time)'
$setup = Join-Path $Root 'yobuko-linux-runner.sh'
New-Item -ItemType Directory -Force -Path $Root | Out-Null
$script = (Get-Content -Raw (Join-Path $PSScriptRoot 'yobuko-linux-runner.sh')) -replace "`r", ''
[System.IO.File]::WriteAllText($setup, $script, (New-Object System.Text.UTF8Encoding $false))
& wsl.exe -d $Distro -u root -- bash /mnt/c/yobuko-linux/yobuko-linux-runner.sh $token
if ($LASTEXITCODE -ne 0) { throw 'Linux setup failed (see the messages above)' }
# wsl.conf changed: restart the distribution so it applies.
& wsl.exe --terminate $Distro | Out-Null

# 5. Windows starts the runners at each sign-in, without a window (conhost --headless), and
#    restarts them if they stop.
Step "Starting the runners now and at each sign-in (task '$TaskName')"
$user = "$env:USERDOMAIN\$env:USERNAME"
$action = New-ScheduledTaskAction -Execute 'conhost.exe' -Argument "--headless wsl.exe -d $Distro -u runner -- /home/runner/start-runners.sh"
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
    -MultipleInstances IgnoreNew
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings `
    -Principal $principal -Force -ErrorAction Stop | Out-Null
Start-ScheduledTask -TaskName $TaskName

Write-Host ''
Done 'Done! On https://github.com/Nozomagarion/YOBUKO/settings/actions/runners you should now see'
Done 'yobuko-linux-1 and yobuko-linux-2 as "Idle" (green) within a minute.'
Read-Host 'Press Enter to close'
