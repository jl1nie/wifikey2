; ============================================================
; WiFiKey2 NSIS Installer
; Usage: makensis wifikey2.nsi
; Prerequisite: wifikey-server.exe built with `cargo build --release`
;               from repo root: set CARGO_TARGET_DIR=C:\espbuild
; ============================================================

Unicode true
ManifestDPIAware true
ManifestDPIAwareness PerMonitorV2

SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"

; ---- 定数定義 ------------------------------------------------
!define APP_NAME        "WiFiKey2"
!define APP_EXE         "wifikey-server.exe"
!define APP_VERSION     "0.3.7"
!define PUBLISHER       "Minoru Tomobe"
!define APP_URL         ""
!define APP_GUID        "{com.wifikey2.server}"

; インストール先: %LOCALAPPDATA%\Programs\WiFiKey2 (カレントユーザー)
!define INSTALL_DIR     "$LOCALAPPDATA\Programs\${APP_NAME}"

; ビルド成果物のパス (cargo build --release)
!define SRC_DIR         "..\..\..\..\target\release"
!define SCRIPTS_SRC     "..\scripts"
!define ICON_FILE       "..\src-tauri\icons\icon.ico"

; ---- 出力設定 ------------------------------------------------
Name            "${APP_NAME} ${APP_VERSION}"
OutFile         "${APP_NAME}-${APP_VERSION}-setup.exe"
InstallDir      "${INSTALL_DIR}"
RequestExecutionLevel user

; ---- UI 設定 ------------------------------------------------
!define MUI_ICON                    "${ICON_FILE}"
!define MUI_UNICON                  "${ICON_FILE}"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE       "WiFiKey2 ${APP_VERSION} セットアップ"
!define MUI_WELCOMEPAGE_TEXT        "WiFiKey2 は ESP32 ベースのリモート CW キーヤーサーバーです。$\r$\n$\r$\nセットアップを開始するには「次へ」をクリックしてください。"
!define MUI_FINISHPAGE_RUN          "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT     "WiFiKey2 を起動する"

; ページ定義
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; 言語 (日本語を優先、英語フォールバック)
!insertmacro MUI_LANGUAGE "Japanese"
!insertmacro MUI_LANGUAGE "English"

; ---- バージョン情報 -----------------------------------------
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey /LANG=1041 "ProductName"      "${APP_NAME}"
VIAddVersionKey /LANG=1041 "ProductVersion"   "${APP_VERSION}"
VIAddVersionKey /LANG=1041 "CompanyName"      "${PUBLISHER}"
VIAddVersionKey /LANG=1041 "LegalCopyright"   "Copyright (c) 2026 ${PUBLISHER}"
VIAddVersionKey /LANG=1041 "FileDescription"  "WiFiKey2 Installer"
VIAddVersionKey /LANG=1041 "FileVersion"      "${APP_VERSION}.0"

; ---- インストールセクション ----------------------------------
Section "MainSection" SEC_MAIN
    SetOutPath "$INSTDIR"

    ; 既存プロセスが動いていれば終了させる
    ExecWait 'taskkill /F /IM "${APP_EXE}"' $0

    ; メインバイナリ
    File "${SRC_DIR}\${APP_EXE}"

    ; Lua スクリプト
    SetOutPath "$INSTDIR\scripts"
    File "${SCRIPTS_SRC}\*.lua"

    ; アンインストーラを生成
    SetOutPath "$INSTDIR"
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    ; スタートメニュー
    CreateDirectory "$SMPROGRAMS\${APP_NAME}"
    CreateShortcut  "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" \
                    "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
    CreateShortcut  "$SMPROGRAMS\${APP_NAME}\Uninstall.lnk" \
                    "$INSTDIR\Uninstall.exe"

    ; デスクトップショートカット
    CreateShortcut  "$DESKTOP\${APP_NAME}.lnk" \
                    "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0

    ; レジストリ (プログラムの追加と削除)
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}" \
                     "DisplayName"          "${APP_NAME}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}" \
                     "DisplayVersion"       "${APP_VERSION}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}" \
                     "Publisher"            "${PUBLISHER}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}" \
                     "InstallLocation"      "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}" \
                     "UninstallString"      '"$INSTDIR\Uninstall.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}" \
                     "DisplayIcon"          "$INSTDIR\${APP_EXE}"
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}" \
                       "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}" \
                       "NoRepair" 1

    ; インストールサイズを記録
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}" \
                       "EstimatedSize" "$0"
SectionEnd

; ---- アンインストールセクション -----------------------------
Section "Uninstall"
    ExecWait 'taskkill /F /IM "${APP_EXE}"' $0

    ; ファイル削除
    Delete "$INSTDIR\${APP_EXE}"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir /r "$INSTDIR\scripts"
    RMDir "$INSTDIR"

    ; ショートカット削除
    Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
    Delete "$SMPROGRAMS\${APP_NAME}\Uninstall.lnk"
    RMDir  "$SMPROGRAMS\${APP_NAME}"
    Delete "$DESKTOP\${APP_NAME}.lnk"

    ; レジストリ削除
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_GUID}"
SectionEnd
