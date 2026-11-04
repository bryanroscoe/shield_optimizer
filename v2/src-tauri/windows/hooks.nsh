; NSIS installer hooks, included by Tauri's installer template (tauri-cli 2.11).
;
; Releases up to 2.2.0 were called "Shield Optimizer". Tauri keys the NSIS
; install on the product name, so the 2.3.0 update installed "ATV Optimizer"
; beside the old copy instead of over it, and the old copy kept its Start menu
; and desktop shortcuts and its Add/Remove Programs entry. This removes that
; copy after the new one is in place.
;
; It deletes only the files the old installer put there (by name), the old
; shortcuts that still point at the old program, and the old registry keys.
; It never runs the old uninstaller and never touches app data: settings live
; in %LOCALAPPDATA%\com.shieldoptimizer.app (the WebView profile the app copies
; on first launch) and the data folder %LOCALAPPDATA%\ShieldOptimizer, and both
; are left exactly as they are.

!define ATVOPT_LEGACY_PRODUCT "Shield Optimizer"
!define ATVOPT_LEGACY_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${ATVOPT_LEGACY_PRODUCT}"
; MANUFACTURER is pinned to this through bundle.publisher in tauri.conf.json.
!define ATVOPT_LEGACY_MANUKEY "Software\shieldoptimizer"

; $R0 install folder, $R1 main binary, $R2 scratch.
; IsShortcutTarget clobbers $0-$3, so only $R registers carry state here.
!macro ATVOPT_REMOVE_LEGACY_SHORTCUT LINK
  ${If} ${FileExists} "${LINK}"
    !insertmacro IsShortcutTarget "${LINK}" "$R0\$R1"
    Pop $R2
    ${If} $R2 = 1
      !insertmacro UnpinShortcut "${LINK}"
      Delete "${LINK}"
      DetailPrint "Removed old shortcut ${LINK}"
    ${EndIf}
  ${EndIf}
!macroend

!macro ATVOPT_REMOVE_LEGACY_INSTALL ROOT CONTEXT
  ClearErrors
  ReadRegStr $R0 ${ROOT} "${ATVOPT_LEGACY_UNINSTKEY}" "InstallLocation"
  ${IfNot} ${Errors}
  ${AndIf} $R0 != ""
    ; Written as "$\"$INSTDIR$\"": drop the quotes.
    StrCpy $R2 $R0 1
    ${If} $R2 == '"'
      StrCpy $R0 $R0 "" 1
      StrCpy $R0 $R0 -1
    ${EndIf}
    ReadRegStr $R1 ${ROOT} "${ATVOPT_LEGACY_UNINSTKEY}" "MainBinaryName"
    ${If} $R1 == ""
      StrCpy $R1 "shield-optimizer-v2.exe"
    ${EndIf}

    ${If} $R0 == $INSTDIR
      ; Both products in one folder: resources and uninstall.exe are now this
      ; install's, so only the old program, entry and shortcuts go.
      ${If} $R1 != "${MAINBINARYNAME}.exe"
        Delete "$R0\$R1"
      ${EndIf}
      DetailPrint "Old ${ATVOPT_LEGACY_PRODUCT} shared this folder; kept its shared files"
    ${Else}
      Delete "$R0\$R1"
      ${If} ${FileExists} "$R0\$R1"
        ; Still running, or not ours to delete. Leave the old install whole
        ; so it can still be uninstalled normally.
        DetailPrint "Could not remove old ${ATVOPT_LEGACY_PRODUCT} at $R0; leaving it"
        Goto atvopt_legacy_done_${ROOT}
      ${EndIf}
      Delete "$R0\resources\scrcpy-server-v3.1"
      RMDir "$R0\resources"
      Delete "$R0\uninstall.exe"
      RMDir "$R0"
      DetailPrint "Removed old ${ATVOPT_LEGACY_PRODUCT} at $R0"
    ${EndIf}

    ; No release set a Start menu folder, so the shortcut sits at the top.
    SetShellVarContext ${CONTEXT}
    !insertmacro ATVOPT_REMOVE_LEGACY_SHORTCUT "$SMPROGRAMS\${ATVOPT_LEGACY_PRODUCT}.lnk"
    !insertmacro ATVOPT_REMOVE_LEGACY_SHORTCUT "$DESKTOP\${ATVOPT_LEGACY_PRODUCT}.lnk"
    !insertmacro SetContext

    DeleteRegKey ${ROOT} "${ATVOPT_LEGACY_UNINSTKEY}"
    DeleteRegKey ${ROOT} "${ATVOPT_LEGACY_MANUKEY}\${ATVOPT_LEGACY_PRODUCT}"
    DeleteRegKey /ifempty ${ROOT} "${ATVOPT_LEGACY_MANUKEY}"
  ${EndIf}
  atvopt_legacy_done_${ROOT}:
  ClearErrors
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro ATVOPT_REMOVE_LEGACY_INSTALL HKCU current
  ; Every release so far installed per user; a per-machine copy can only be
  ; removed by an elevated installer, and the steps above fail harmlessly
  ; without rights.
  !insertmacro ATVOPT_REMOVE_LEGACY_INSTALL HKLM all
!macroend
