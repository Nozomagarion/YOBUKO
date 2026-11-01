; Copyright (C) 2026 The Suhina contributors
; SPDX-License-Identifier: GPL-3.0-or-later
;
; The Windows installer for YOBUKO (Inno Setup 6), built by scripts/windows_installer.sh from a
; package folder holding suhina.exe, suhina-cli.exe and suhina-plugin-host.exe side by side.
; It installs for the current user only (no administrator prompt) into
; %LOCALAPPDATA%\Programs\YOBUKO, adds a Start menu entry, optionally a desktop shortcut, and an
; uninstaller listed in Windows' installed apps. The three programs stay side by side so the app
; still finds its plug-in host and the in-app updater can still replace them.
;
; Defines passed by the build script: AppVersion, ShortVersion (numbers only), SourceDir, OutputDir,
; OutputName.

#ifndef AppVersion
  #error AppVersion is required
#endif

[Setup]
AppId={{6F1B6E2A-3C3B-4C55-9A57-59B0C0B4D0A1}
AppName=YOBUKO
AppVersion={#AppVersion}
AppVerName=YOBUKO {#AppVersion}
AppPublisher=YOBUKO (based on Suhina and rbxport)
AppPublisherURL=https://github.com/Nozomagarion/YOBUKO
AppSupportURL=https://github.com/Nozomagarion/YOBUKO
VersionInfoVersion={#ShortVersion}
DefaultDirName={autopf}\YOBUKO
DefaultGroupName=YOBUKO
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile={#SourceDir}\LICENSE
UninstallDisplayIcon={app}\suhina.exe
UninstallDisplayName=YOBUKO
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\YOBUKO"; Filename: "{app}\suhina.exe"
Name: "{group}\{cm:UninstallProgram,YOBUKO}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\YOBUKO"; Filename: "{app}\suhina.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\suhina.exe"; Description: "{cm:LaunchProgram,YOBUKO}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Files the in-app updater leaves behind while swapping programs.
Type: files; Name: "{app}\*.old"
Type: files; Name: "{app}\*.old?"
