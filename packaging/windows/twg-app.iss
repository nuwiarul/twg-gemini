#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

[Setup]
AppId={{7D1F5A8A-6F8B-4B94-9E71-04D0B5C7F2E1}
AppName=Tactical Wall Game 3D
AppVersion={#MyAppVersion}
AppPublisher=TWG Team
DefaultDirName={localappdata}\TWG-3D
DefaultGroupName=Tactical Wall Game 3D
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\..\dist
OutputBaseFilename=TWG-3D-Setup-x64-v{#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\..\dist\twg-app-windows-x64\twg-app.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\twg-app-windows-x64\TWG-3D.bat"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\twg-app-windows-x64\README.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Tactical Wall Game 3D"; Filename: "{app}\TWG-3D.bat"; WorkingDir: "{localappdata}\TWG-3D"
Name: "{group}\{cm:UninstallProgram,Tactical Wall Game 3D}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Tactical Wall Game 3D"; Filename: "{app}\TWG-3D.bat"; WorkingDir: "{localappdata}\TWG-3D"; Tasks: desktopicon

[Run]
Filename: "{app}\TWG-3D.bat"; Description: "Launch Tactical Wall Game 3D"; Flags: nowait postinstall skipifsilent shellexec
