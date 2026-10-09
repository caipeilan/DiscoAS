; Spotify support is optional. Installing or upgrading DiscoAS never deploys
; Spicetify or changes Spotify: users opt in from the application settings.

!include "WinVer.nsh"

LangString DISCOAS_UNSUPPORTED_SYSTEM 2052 "DiscoAS 需要 Windows 10 1809 或更新版本／Windows 11 的 x64 系统。"
LangString DISCOAS_UNSUPPORTED_SYSTEM 1028 "DiscoAS 需要 Windows 10 1809 或更新版本／Windows 11 的 x64 系統。"
LangString DISCOAS_UNSUPPORTED_SYSTEM 1033 "DiscoAS requires x64 Windows 10 version 1809 or later, or Windows 11."
LangString DISCOAS_WEBVIEW2_VERSION_ERROR 2052 "错误：WebView2 未更新到所需版本，请重试安装。"
LangString DISCOAS_WEBVIEW2_VERSION_ERROR 1028 "錯誤：WebView2 未更新至所需版本，請重試安裝。"
LangString DISCOAS_WEBVIEW2_VERSION_ERROR 1033 "Error: WebView2 did not reach the required version. Please retry installation."

; The custom template calls this in .onInit, before runtime installation or
; removing an earlier DiscoAS version. Silent installs must also exit cleanly.
!macro DISCOAS_VALIDATE_SYSTEM
  ${IfNot} ${IsNativeAMD64}
    MessageBox MB_OK|MB_ICONSTOP "$(DISCOAS_UNSUPPORTED_SYSTEM)" /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
  ${OrIfNot} ${AtLeastBuild} 17763
  ${OrIf} ${IsServerOS}
    MessageBox MB_OK|MB_ICONSTOP "$(DISCOAS_UNSUPPORTED_SYSTEM)" /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; An upgrade must keep pairing, extensions and existing Spicetify intact.
  ${If} $UpdateMode <> 1
    nsExec::ExecToLog /TIMEOUT=120000 '"$INSTDIR\discoas.exe" --remove-spotify-support'
    Pop $0
  ${EndIf}
!macroend
