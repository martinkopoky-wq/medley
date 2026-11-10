Unicode True
!include "MUI2.nsh"
Name "Medley"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Medley"
InstallDirRegKey HKCU "Software\Medley" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "Medley"
VIAddVersionKey "FileDescription" "Medley Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Medley contributors"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
Section "Medley"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File "${PAYLOAD}\medley.exe"
  File "${PAYLOAD}\*.dll"
  File "${PAYLOAD}\LICENSE-*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Medley"
  CreateShortcut "$SMPROGRAMS\Medley\Medley.lnk" "$INSTDIR\medley.exe"
  CreateShortcut "$SMPROGRAMS\Medley\Uninstall Medley.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Medley" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Medley" "DisplayName" "Medley"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Medley" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Medley" "Publisher" "Medley contributors"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Medley" "DisplayIcon" "$INSTDIR\medley.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Medley" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Medley" "QuietUninstallString" '$\"$INSTDIR\Uninstall.exe$\" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Medley" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Medley" "NoRepair" 1
SectionEnd
Section "Uninstall"
  SetShellVarContext current
  Delete "$INSTDIR\medley.exe"
  !include "${DELETE_RUNTIME}"
  Delete "$INSTDIR\LICENSE-MIT"
  Delete "$INSTDIR\LICENSE-APACHE"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Medley\Medley.lnk"
  Delete "$SMPROGRAMS\Medley\Uninstall Medley.lnk"
  RMDir "$SMPROGRAMS\Medley"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Medley"
  DeleteRegKey HKCU "Software\Medley"
  ; User settings, library and credentials live elsewhere and are preserved.
SectionEnd
