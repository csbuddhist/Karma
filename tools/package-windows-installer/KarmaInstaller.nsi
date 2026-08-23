Unicode true

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef FILE_VERSION
  !error "FILE_VERSION is required"
!endif
!ifndef BUNDLE_DIR
  !error "BUNDLE_DIR is required"
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE is required"
!endif
!ifndef ICON_FILE
  !error "ICON_FILE is required"
!endif

!define PRODUCT_NAME "Karma Family Protection"
!define PRODUCT_PUBLISHER "Karma"
!define PRODUCT_REGISTRY_KEY "Software\Karma"
!define PRODUCT_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Karma"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$PROGRAMFILES64\Karma"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
CRCCheck on
ShowInstDetails show
ShowUninstDetails show
Icon "${ICON_FILE}"
UninstallIcon "${ICON_FILE}"

VIProductVersion "${FILE_VERSION}"
VIAddVersionKey /LANG=2052 "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=2052 "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey /LANG=2052 "FileDescription" "Karma Windows x64 安装程序"
VIAddVersionKey /LANG=2052 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=2052 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=2052 "LegalCopyright" "MIT License"

!define MUI_ABORTWARNING
!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"
!define MUI_FINISHPAGE_TEXT "Karma 已完成安装并启动保护服务。请使用桌面或开始菜单中的快捷方式打开管理控制台。"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  SetRegView 64
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "Karma 仅支持 64 位 Windows。"
    Abort
  ${EndIf}

  ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\KarmaService" "ImagePath"
  ${If} $0 != ""
    MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "检测到已安装的 Karma。是否先卸载现有版本并继续安装？卸载需要输入 Karma 管理员密码。" IDYES uninstall_existing
    Abort

uninstall_existing:
    ReadRegStr $1 HKLM "${PRODUCT_REGISTRY_KEY}" "InstallDirectory"
    ${If} $1 == ""
      StrCpy $1 "$PROGRAMFILES64\Karma"
    ${EndIf}
    IfFileExists "$1\Uninstall-Karma-Launcher.exe" 0 existing_uninstaller_missing

    ExecWait '"$1\Uninstall-Karma-Launcher.exe" /S' $2
    ${If} $2 != 0
      MessageBox MB_OK|MB_ICONSTOP "管理员密码验证失败或现有版本卸载未完成。安装已取消，Karma 保持原有安装状态。"
      Abort
    ${EndIf}

    ReadRegStr $3 HKLM "SYSTEM\CurrentControlSet\Services\KarmaService" "ImagePath"
    ${If} $3 != ""
      MessageBox MB_OK|MB_ICONSTOP "现有 KarmaService 未能移除，安装已取消。"
      Abort
    ${EndIf}
    Goto existing_uninstall_done

existing_uninstaller_missing:
    MessageBox MB_OK|MB_ICONSTOP "找不到现有版本的 Uninstall-Karma-Launcher.exe，无法自动卸载。安装已取消。"
    Abort

existing_uninstall_done:
  ${EndIf}
FunctionEnd

Section "Install" SecInstall
  SetRegView 64
  SetShellVarContext all
  SetOutPath "$INSTDIR"

  File /r "${BUNDLE_DIR}\*.*"
  WriteUninstaller "$INSTDIR\Uninstall-Karma-Launcher.exe"

  DetailPrint "正在校验安装包并注册 KarmaService..."
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\Install-Karma.ps1" -InstallDirectory "$INSTDIR"'
  Pop $0
  Pop $1
  ${If} $0 != "0"
    DetailPrint "$1"
    MessageBox MB_OK|MB_ICONSTOP "Karma 安装失败。安装日志中保留了诊断信息。"
    Abort
  ${EndIf}

  CreateDirectory "$SMPROGRAMS\Karma Family Protection"
  CreateShortcut "$SMPROGRAMS\Karma Family Protection\Karma 管理控制台.lnk" "$INSTDIR\karma-ui.exe" "" "$INSTDIR\karma-ui.exe" 0
  CreateShortcut "$SMPROGRAMS\Karma Family Protection\卸载 Karma.lnk" "$INSTDIR\Uninstall-Karma-Launcher.exe"
  CreateShortcut "$DESKTOP\Karma 家庭保护.lnk" "$INSTDIR\karma-ui.exe" "" "$INSTDIR\karma-ui.exe" 0

  WriteRegStr HKLM "${PRODUCT_REGISTRY_KEY}" "InstallDirectory" "$INSTDIR"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\karma-ui.exe"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall-Karma-Launcher.exe"'
  WriteRegDWORD HKLM "${PRODUCT_UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${PRODUCT_UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetRegView 64
  SetShellVarContext all

  IfFileExists "$INSTDIR\Uninstall-Karma.ps1" 0 uninstall_missing
  ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\Uninstall-Karma.ps1" -InstallDirectory "$INSTDIR"' $0
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONSTOP "管理员密码验证失败或卸载未完成。Karma 保持安装状态。"
    Abort
  ${EndIf}

  Delete "$DESKTOP\Karma 家庭保护.lnk"
  RMDir /r "$SMPROGRAMS\Karma Family Protection"
  DeleteRegKey HKLM "${PRODUCT_UNINSTALL_KEY}"
  DeleteRegKey /ifempty HKLM "${PRODUCT_REGISTRY_KEY}"
  Goto uninstall_done

uninstall_missing:
  MessageBox MB_OK|MB_ICONSTOP "找不到密码授权卸载脚本。为防止绕过保护，卸载已取消。"
  Abort

uninstall_done:
SectionEnd
