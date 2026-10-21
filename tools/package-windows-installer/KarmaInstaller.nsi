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
!ifndef CLEANUP_FILE
  !error "CLEANUP_FILE is required"
!endif
!ifndef VC_REDIST_FILE
  !error "VC_REDIST_FILE is required"
!endif

!define PRODUCT_NAME "Karma Family Protection"
!define PRODUCT_PUBLISHER "Karma"
!define PRODUCT_REGISTRY_KEY "Software\Karma"
!define PRODUCT_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Karma"

Var UpdateExisting
Var ExistingDirectory

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
VIAddVersionKey /LANG=2052 "LegalCopyright" "GNU GPL v3.0"

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
  StrCpy $UpdateExisting 0
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "Karma 仅支持 64 位 Windows。"
    Abort
  ${EndIf}

  ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Services\KarmaService" "ImagePath"
  ${If} $0 != ""
    ReadRegStr $ExistingDirectory HKLM "${PRODUCT_REGISTRY_KEY}" "InstallDirectory"
    ${If} $ExistingDirectory == ""
      StrCpy $ExistingDirectory "$PROGRAMFILES64\Karma"
    ${EndIf}
    nsExec::ExecToStack '"$ExistingDirectory\KarmaControl.exe" version'
    Pop $5
    Pop $6
    ${If} $5 == "0"
      # 0.2.4+ uses the authenticated service updater. Failure must never
      # fall back to uninstalling protection. 新版升级失败不能退回卸载。
      StrCpy $UpdateExisting 1
      Return
    ${ElseIf} $5 != "2"
      MessageBox MB_OK|MB_ICONSTOP "Cannot identify the installed updater. Setup cancelled. / 无法识别已安装版本的升级接口，安装已取消。"
      Abort
    ${EndIf}
    # Only legacy KarmaControl returns usage exit code 2 for `version`.
    # 仅旧版不支持 version 命令时返回 2，允许走旧的授权卸载流程。
    MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "检测到已安装的 Karma。是否先卸载现有版本并继续安装？卸载需要输入 Karma 管理员密码。" IDYES uninstall_existing
    Abort

uninstall_existing:
    ReadRegStr $1 HKLM "${PRODUCT_REGISTRY_KEY}" "InstallDirectory"
    ${If} $1 == ""
      StrCpy $1 "$PROGRAMFILES64\Karma"
    ${EndIf}
    IfFileExists "$1\Uninstall-Karma.ps1" 0 existing_uninstaller_missing

    # 直接同步运行已安装的密码授权卸载脚本：控制台窗口可见，可以输入
    # 管理员密码，并且返回真实的退出码。不能调用 Uninstall-Karma-Launcher.exe：
    # NSIS 卸载器会把自身复制到临时目录异步执行，原进程总是立即退出 0，
    # 安装器既等不到卸载完成，也拿不到真实的失败结果。
    #
    # 旧版 KarmaControl 会把“服务未运行”误报为密码验证失败，因此先确保
    # 服务已启动（已在运行时 sc start 返回 1056，可安全忽略）。
    nsExec::ExecToStack '"$SYSDIR\sc.exe" start KarmaService'
    Pop $5
    Sleep 2000

    ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "$1\Uninstall-Karma.ps1" -InstallDirectory "$1"' $2
    ${If} $2 != 0
      MessageBox MB_OK|MB_ICONSTOP "管理员密码验证失败或现有版本卸载未完成。安装已取消，Karma 保持原有安装状态。"
      Abort
    ${EndIf}

    StrCpy $4 0
wait_for_existing_service_removal:
    ReadRegStr $3 HKLM "SYSTEM\CurrentControlSet\Services\KarmaService" "ImagePath"
    ${If} $3 == ""
      Goto existing_cleanup
    ${EndIf}
    IntOp $4 $4 + 1
    ${If} $4 < 60
      Sleep 500
      Goto wait_for_existing_service_removal
    ${EndIf}
    MessageBox MB_OK|MB_ICONSTOP "现有 KarmaService 在 30 秒内未能移除。请关闭“服务”管理器或其他可能占用该 Service 的工具后重试。安装已取消。"
    Abort

    # 旧版卸载脚本可能遗留运行中的 karma-ui 等进程和被占用的文件；
    # 解包前强制结束并清空安装目录，避免出现“无法打开要写入的文件”。
existing_cleanup:
    InitPluginsDir
    File /oname=$PLUGINSDIR\Karma-Cleanup.ps1 "${CLEANUP_FILE}"
    ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\Karma-Cleanup.ps1" -InstallDirectory "$1"' $6
    ${If} $6 != 0
      MessageBox MB_OK|MB_ICONSTOP "旧安装目录 $1 仍被 Karma 进程占用，无法清理。请结束 karma-ui 等进程后重试安装。安装已取消。"
      Abort
    ${EndIf}
    Goto existing_uninstall_done

existing_uninstaller_missing:
    MessageBox MB_OK|MB_ICONSTOP "找不到现有版本的 Uninstall-Karma.ps1，无法自动卸载。请先从“应用和功能”中卸载 Karma，再运行本安装器。安装已取消。"
    Abort

existing_uninstall_done:
  ${EndIf}
FunctionEnd

Section "Install" SecInstall
  SetRegView 64
  SetShellVarContext all
  ${If} $UpdateExisting == 1
    InitPluginsDir
    SetOutPath "$PLUGINSDIR\KarmaUpdate"
    File /r "${BUNDLE_DIR}\*.*"
    nsExec::ExecToStack '"$SYSDIR\sc.exe" start KarmaService'
    Pop $5
    Pop $6
    ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\KarmaUpdate\Update-Karma.ps1" -BundleDirectory "$PLUGINSDIR\KarmaUpdate" -InstallDirectory "$ExistingDirectory" -ExpectedVersion "${VERSION}"' $0
    ${If} $0 != 0
      MessageBox MB_OK|MB_ICONSTOP "Update rejected or not confirmed. See console status. Protection remains registered. / 升级被拒绝或尚未确认，请查看控制台状态。保护服务仍保持注册。"
      Abort
    ${EndIf}
    Goto install_done
  ${EndIf}
  SetOutPath "$INSTDIR"

  File /r "${BUNDLE_DIR}\*.*"
  WriteUninstaller "$INSTDIR\Uninstall-Karma-Launcher.exe"

  # 微软 VC++ x64 运行库只解包到临时插件目录：Install-Karma.ps1 先校验其微软签名，
  # 仅在本机运行库缺失或版本过旧时静默安装，安装目录中不保留安装包。
  InitPluginsDir
  File /oname=$PLUGINSDIR\vc_redist.x64.exe "${VC_REDIST_FILE}"

  DetailPrint "正在校验安装包、检查 Microsoft Visual C++ 运行库并注册 KarmaService..."
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\Install-Karma.ps1" -InstallDirectory "$INSTDIR" -VCRedistPath "$PLUGINSDIR\vc_redist.x64.exe"'
  Pop $0
  Pop $1
  Delete "$PLUGINSDIR\vc_redist.x64.exe"
  ${If} $0 != "0"
    DetailPrint "$1"
    MessageBox MB_OK|MB_ICONSTOP "Karma 安装失败（可能是 Microsoft Visual C++ 运行库未能安装）。安装日志中保留了诊断信息。"
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
install_done:
SectionEnd

Section "Uninstall"
  SetRegView 64
  SetShellVarContext all

  IfFileExists "$INSTDIR\Uninstall-Karma.ps1" 0 uninstall_missing
  ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\Uninstall-Karma.ps1" -InstallDirectory "$INSTDIR"' $0
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONSTOP "Super password required, or administrator password after the waiting period; uninstall incomplete. / 需要超级密码，或等待期满后的管理员密码；卸载未完成。Karma 保持安装状态。"
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
