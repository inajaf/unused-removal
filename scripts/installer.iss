; Inno Setup script for unused-removal desktop app
; Build: ISCC scripts\installer.iss  (or .\scripts\build-desktop-windows.ps1 -Installer)

#define AppName "Unused Removal"
#define AppVersion "1.0.0"
#define AppExe "unused-removal.exe"

[Setup]
AppId={{8F1D9C42-77B3-4E5A-9C1D-2A6B54F0E100}
AppName={#AppName}
AppVersion={#AppVersion}
DefaultDirName={autopf}\unused-removal
DefaultGroupName={#AppName}
OutputDir=..\target\release\installer
OutputBaseFilename=unused-removal-setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional icons:"

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent
