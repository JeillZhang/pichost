!include "MUI2.nsh"
!include "LogicLib.nsh"

!define APP_NAME "PicHost"
!ifndef INSTALLER_VERSION
  !define INSTALLER_VERSION "0.0.0" ; CI 以 -DINSTALLER_VERSION=v0.23.0 覆写
!endif
!ifndef stagingDir
  !define stagingDir "." ; 未传 -DstagingDir 时以 makensis 工作目录为基准
!endif

Name "${APP_NAME}"
OutFile "PicHost-setup-${INSTALLER_VERSION}.exe"
InstallDir "$PROGRAMFILES64\PicHost"
RequestExecutionLevel admin

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
Page custom DataRetentionPage DataRetentionPageLeave
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var KeepData

Function DataRetentionPage
    nsDialogs::Create 1018
    Pop $0
    ${NSD_CreateCheckBox} 0 0 100% 20u "Keep data in %ProgramData%\PicHost on uninstall"
    Pop $KeepData
    SetBrandingImage /IMGID=$KeepData
    nsDialogs::Show
FunctionEnd

Function DataRetentionPageLeave
    ${NSD_GetState} $KeepData $0
    ${If} $0 == ${BST_CHECKED}
        WriteRegDWORD HKCU "Software\PicHost" "KeepData" 1
    ${Else}
        WriteRegDWORD HKCU "Software\PicHost" "KeepData" 0
    ${EndIf}
FunctionEnd

Section "Install"
    SetOutPath "$INSTDIR"
    File "${stagingDir}\pichost-api.exe"
    File "${stagingDir}\pichost-worker.exe"
    File /r "${stagingDir}\dist"
    File /r "${stagingDir}\migrations"
    File /r "${stagingDir}\migrations-sqlite"
    nsExec::Exec '"$INSTDIR\pichost-api.exe" --install-service'
    WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
    nsExec::Exec '"$INSTDIR\pichost-api.exe" --uninstall-service'
    ReadRegDWORD $0 HKCU "Software\PicHost" "KeepData"
    ${If} $0 == 1
        RMDir /r "$INSTDIR"
    ${Else}
        RMDir /r "$INSTDIR"
        RMDir /r "$PROGRAMDATA\PicHost"
    ${EndIf}
    DeleteRegKey HKCU "Software\PicHost"
SectionEnd
