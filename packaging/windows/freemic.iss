; FreeMic installer (Inno Setup 6). Built by packaging/windows/build.ps1.
; Installs for the current user by default (no administrator rights needed).
; VB-CABLE is a separate VB-Audio product: it is never bundled. If it is not
; installed, an optional task downloads it from vb-audio.com, verifies the
; signature and starts VB-Audio's own installer (which asks for admin rights).
; Silent installs (/SILENT, /VERYSILENT) never download or start it.

#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist\stage"
#endif

[Setup]
AppId={{6A0E5E2B-9B67-4C4C-9E0B-5F3A61F2C1D7}
AppName=FreeMic
AppVersion={#AppVersion}
AppVerName=FreeMic {#AppVersion}
AppPublisher=FreeMic
DefaultDirName={autopf}\FreeMic
DefaultGroupName=FreeMic
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputBaseFilename=FreeMic-{#AppVersion}-windows-setup
OutputDir=..\..\dist
SetupIconFile=..\assets\freemic.ico
UninstallDisplayIcon={app}\FreeMic.exe
UninstallDisplayName=FreeMic
LicenseFile={#SourceDir}\LICENSE.txt
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
AppMutex=Local\FreeMic-main

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
Name: "vbcable"; Description: "Download and install the free VB-CABLE virtual audio driver from vb-audio.com (needed for the virtual microphone; Windows will ask for administrator approval)"; GroupDescription: "Virtual microphone:"; Check: not VBCableInstalled

[Files]
Source: "{#SourceDir}\FreeMic.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\install-vbcable.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD-PARTY-NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\Inter-LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\FreeMic"; Filename: "{app}\FreeMic.exe"
Name: "{group}\Uninstall FreeMic"; Filename: "{uninstallexe}"
Name: "{autodesktop}\FreeMic"; Filename: "{app}\FreeMic.exe"; Tasks: desktopicon

[Registry]
; Remove the "start when I sign in" entry FreeMic may have created.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "FreeMic"; Flags: uninsdeletevalue dontcreatekey

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\install-vbcable.ps1"""; StatusMsg: "Downloading and starting the VB-CABLE installer..."; Tasks: vbcable; Check: not WizardSilent; Flags: waituntilterminated
Filename: "{app}\FreeMic.exe"; Description: "Start FreeMic"; Flags: nowait postinstall skipifsilent

[Code]
const
  FriendlyNameKey = '{a45c254e-df1c-4efd-8020-67d146a850e0},2';

#ifdef TestNoCable
{ Test builds only (tools/windows/test_installer.ps1): behave as if no cable exists. }
function VBCableInstalled: Boolean;
begin
  Result := False;
end;
#else
{ True if a playback device named like a VB-CABLE input exists. }
function VBCableInstalled: Boolean;
var
  Subkeys: TArrayOfString;
  I: Integer;
  Base, Name: String;
begin
  Result := False;
  Base := 'SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render';
  if RegGetSubkeyNames(HKLM64, Base, Subkeys) then
    for I := 0 to GetArrayLength(Subkeys) - 1 do
      if RegQueryStringValue(HKLM64, Base + '\' + Subkeys[I] + '\Properties', FriendlyNameKey, Name) then
        if Pos('CABLE', Uppercase(Name)) > 0 then
        begin
          Result := True;
          Exit;
        end;
end;
#endif

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    DataDir := ExpandConstant('{userappdata}\FreeMic');
    { Silent uninstalls keep the user's data; interactive ones ask (default: keep). }
    if DirExists(DataDir) and not UninstallSilent then
      if MsgBox('Also delete your FreeMic soundboard, imported sounds and settings?' + #13#10 + DataDir,
                mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(DataDir, True, True, True);
  end;
end;
