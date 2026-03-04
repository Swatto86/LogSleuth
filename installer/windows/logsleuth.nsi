; LogSleuth NSIS Installer
; Supports per-user (no elevation) and per-machine (admin) installation.
; Uses NSIS MultiUser.nsh and Modern UI 2.
;
; Build from the workspace root:
;   makensis installer\windows\logsleuth.nsi
;
; The version string below is automatically updated by update-application.ps1.

Unicode True
SetCompressor /SOLID lzma

; ---------------------------------------------------------------------------
; Product metadata
; ---------------------------------------------------------------------------

!define PRODUCT_NAME      "LogSleuth"
!define PRODUCT_VERSION   "1.0.0"
!define PRODUCT_PUBLISHER "Swatto"
!define PRODUCT_URL       "https://github.com/swatto86/LogSleuth"
!define PRODUCT_EXE       "logsleuth.exe"
!define INSTALL_DIR_NAME  "LogSleuth"
!define UNINST_KEY        "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"

; ---------------------------------------------------------------------------
; MultiUser setup  (must come before !include MultiUser.nsh)
; Highest = request admin if available, fall back to user install silently.
; ---------------------------------------------------------------------------

!define MULTIUSER_EXECUTIONLEVEL       Highest
!define MULTIUSER_MUI
!define MULTIUSER_INSTALLMODE_COMMANDLINE
!define MULTIUSER_INSTALLMODE_INSTDIR  "${INSTALL_DIR_NAME}"

!include MultiUser.nsh
!include MUI2.nsh
!include LogicLib.nsh

; ---------------------------------------------------------------------------
; Output and appearance
; ---------------------------------------------------------------------------

Name    "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "..\..\LogSleuth-Setup-${PRODUCT_VERSION}.exe"

; Icon (resolved relative to this .nsi file)
!define MUI_ICON   "..\..\assets\icon.ico"
!define MUI_UNICON "..\..\assets\icon.ico"

; Welcome / Finish page branding
!define MUI_WELCOMEPAGE_TITLE       "Welcome to ${PRODUCT_NAME} ${PRODUCT_VERSION} Setup"
!define MUI_WELCOMEPAGE_TEXT        "This wizard will guide you through the installation of ${PRODUCT_NAME}, a fast cross-platform log file viewer and analyser.$\n$\nIt is recommended that you close all other applications before continuing."
!define MUI_FINISHPAGE_RUN          "$INSTDIR\${PRODUCT_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT     "Launch ${PRODUCT_NAME}"
!define MUI_FINISHPAGE_LINK         "Visit project page"
!define MUI_FINISHPAGE_LINK_LOCATION "${PRODUCT_URL}"

; Abort confirmation on cancel
!define MUI_ABORTWARNING

; ---------------------------------------------------------------------------
; Installer pages
; ---------------------------------------------------------------------------

!insertmacro MUI_PAGE_WELCOME
!insertmacro MULTIUSER_PAGE_INSTALLMODE   ; "Install for all users" / "just me"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ---------------------------------------------------------------------------
; Version info (shown in EXE properties / UAC dialog)
; ---------------------------------------------------------------------------

VIProductVersion "${PRODUCT_VERSION}.0"
VIAddVersionKey "ProductName"      "${PRODUCT_NAME}"
VIAddVersionKey "ProductVersion"   "${PRODUCT_VERSION}"
VIAddVersionKey "CompanyName"      "${PRODUCT_PUBLISHER}"
VIAddVersionKey "FileDescription"  "${PRODUCT_NAME} Installer"
VIAddVersionKey "FileVersion"      "${PRODUCT_VERSION}"
VIAddVersionKey "LegalCopyright"   "Copyright 2024 ${PRODUCT_PUBLISHER}. MIT License."

; ---------------------------------------------------------------------------
; Init callbacks
; ---------------------------------------------------------------------------

Function .onInit
    !insertmacro MULTIUSER_INIT
FunctionEnd

Function un.onInit
    !insertmacro MULTIUSER_UNINIT
FunctionEnd

; ---------------------------------------------------------------------------
; Main install section
; ---------------------------------------------------------------------------

Section "LogSleuth" SEC_MAIN

    SectionIn RO   ; required section, cannot be deselected

    SetOutPath "$INSTDIR"
    SetOverwrite on

    ; Copy application binary
    ; (path relative to this .nsi file: ..\..\target\release\logsleuth.exe)
    File "..\..\target\release\logsleuth.exe"

    ; Write uninstaller
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    ; -----------------------------------------------------------------------
    ; Start Menu shortcuts
    ; -----------------------------------------------------------------------
    CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
    CreateShortcut  "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" \
                    "$INSTDIR\${PRODUCT_EXE}" "" "$INSTDIR\${PRODUCT_EXE}" 0
    CreateShortcut  "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" \
                    "$INSTDIR\Uninstall.exe"

    ; -----------------------------------------------------------------------
    ; Add/Remove Programs registry entry
    ; SHCTX = HKLM when admin / HKCU when per-user (set by MultiUser.nsh)
    ; -----------------------------------------------------------------------
    WriteRegStr   SHCTX "${UNINST_KEY}" "DisplayName"      "${PRODUCT_NAME}"
    WriteRegStr   SHCTX "${UNINST_KEY}" "DisplayVersion"   "${PRODUCT_VERSION}"
    WriteRegStr   SHCTX "${UNINST_KEY}" "Publisher"        "${PRODUCT_PUBLISHER}"
    WriteRegStr   SHCTX "${UNINST_KEY}" "URLInfoAbout"     "${PRODUCT_URL}"
    WriteRegStr   SHCTX "${UNINST_KEY}" "UninstallString"  '"$INSTDIR\Uninstall.exe"'
    WriteRegStr   SHCTX "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
    WriteRegStr   SHCTX "${UNINST_KEY}" "DisplayIcon"      "$INSTDIR\${PRODUCT_EXE}"
    WriteRegStr   SHCTX "${UNINST_KEY}" "InstallLocation"  "$INSTDIR"
    WriteRegDWORD SHCTX "${UNINST_KEY}" "NoModify"         1
    WriteRegDWORD SHCTX "${UNINST_KEY}" "NoRepair"         1

SectionEnd

; ---------------------------------------------------------------------------
; Uninstall section
; ---------------------------------------------------------------------------

Section "Uninstall"

    ; Remove application files
    Delete "$INSTDIR\${PRODUCT_EXE}"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir  "$INSTDIR"

    ; Remove Start Menu shortcuts
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk"
    RMDir  "$SMPROGRAMS\${PRODUCT_NAME}"

    ; Remove registry entry
    DeleteRegKey SHCTX "${UNINST_KEY}"

SectionEnd
