; Tauri already removes application data only when its explicit checkbox is selected.
; Updates, passive installs and ordinary uninstall preserve data and credentials.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    nsExec::ExecToStack '"$INSTDIR\${MAINBINARYNAME}.exe" --purge-credentials-for-uninstall'
    Pop $0
    Pop $1
    ${If} $0 != 0
      MessageBox MB_ICONEXCLAMATION "Ember could not remove its credentials. Close Ember and retry, or uninstall with application data preserved."
      Abort
    ${EndIf}
  ${EndIf}
!macroend
