!macro NSIS_HOOK_PREINSTALL
  ; 1. 结束正在运行的旧进程，避免文件/快捷方式被占用
  nsExec::ExecToLog 'taskkill /F /IM "yt-whisper-studio.exe" /T'

  ; 2. 检测注册表中已安装的旧位置；若与本次目标目录不同，先静默卸载旧目录以保证全机唯一安装
  ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\YT-Whisper Studio" "InstallLocation"
  StrCmp $0 "" _ytw_check_done
  StrCpy $1 $0 1
  StrCmp $1 '"' 0 +3
    StrCpy $0 $0 "" 1
    StrCpy $0 $0 -1
  StrCmp $0 "$INSTDIR" _ytw_check_done
  IfFileExists "$0\uninstall.exe" 0 _ytw_check_done
    ExecWait '"$0\uninstall.exe" /S _?=$0'
    Delete "$0\uninstall.exe"
    RMDir "$0"
  _ytw_check_done:
!macroend
