; FreeMic installer (Inno Setup 6). Built by packaging/windows/build.ps1.
; Installs for the current user by default (no administrator rights needed).
; VB-CABLE is a separate VB-Audio product: it is never bundled, downloaded or
; started by FreeMic or this installer (a driver needs administrator approval in
; VB-Audio's own installer). If it is missing, the last page offers to open the
; VB-CABLE web page. Silent installs (/SILENT, /VERYSILENT) don't open it.
; yt-dlp.exe (official, unmodified, SHA-256 checked by build.ps1) is included
; for the YouTube page.

#ifndef AppVersion
  #define AppVersion "0.2.1"
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
VersionInfoVersion={#AppVersion}
VersionInfoProductName=FreeMic
VersionInfoDescription=FreeMic setup
VersionInfoCopyright=Copyright (c) 2026 Vaden. MIT License.

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\FreeMic.exe"; DestDir: "{app}"; Flags: ignoreversion
; Not "ignoreversion": a copy that "Update yt-dlp" made newer is kept on upgrade.
Source: "{#SourceDir}\yt-dlp.exe"; DestDir: "{app}"
Source: "{#SourceDir}\yt-dlp-LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\yt-dlp-THIRD_PARTY_LICENSES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\yt-dlp-SOURCE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD-PARTY-NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\Inter-LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs

[InstallDelete]
; Left in the program folder by FreeMic 0.2.0 (no longer used).
Type: files; Name: "{app}\install-vbcable.ps1"

[UninstallDelete]
; What yt-dlp's own updater may leave next to yt-dlp.exe.
Type: files; Name: "{app}\yt-dlp.exe.old"
Type: files; Name: "{app}\yt-dlp.exe.new"

[Icons]
Name: "{group}\FreeMic"; Filename: "{app}\FreeMic.exe"
Name: "{group}\Uninstall FreeMic"; Filename: "{uninstallexe}"
Name: "{autodesktop}\FreeMic"; Filename: "{app}\FreeMic.exe"; Tasks: desktopicon

[Registry]
; Remove the "start when I sign in" entry FreeMic may have created.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "FreeMic"; Flags: uninsdeletevalue dontcreatekey

[Run]
Filename: "https://vb-audio.com/Cable/"; Description: "Open the VB-CABLE web page (FreeMic's virtual microphone needs this free driver)"; Flags: postinstall shellexec skipifsilent nowait; Check: not VBCableInstalled
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
