; The runtime registers its notification identity for development and installed builds.
; Keep it during updates; remove only Jarvis-owned identity on a full uninstall.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegKey HKCU "Software\Classes\AppUserModelId\${BUNDLEID}"
  ${EndIf}
!macroend
