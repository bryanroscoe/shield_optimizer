; NSIS installer hooks, included by Tauri's installer template (tauri-cli 2.11).
;
; Releases up to 2.2.0 were called "Shield Optimizer". Tauri keys the NSIS
; install on the product name, so the 2.3.0 update installed "ATV Optimizer"
; beside the old copy instead of over it, and the old copy kept its Start menu
; and desktop shortcuts and its Add/Remove Programs entry. These hooks stop
; the old program, remove that copy after the new one is in place, and point
; the user's shortcuts and taskbar pins at the new program.
;
; They delete only the files the old installer put there (by name), the old
; shortcuts that still point at the old program, and the old registry keys.
; They never run the old uninstaller and never touch app data: settings live
; in %LOCALAPPDATA%\com.shieldoptimizer.app (the WebView profile the app copies
; on first launch) and the data folder %LOCALAPPDATA%\ShieldOptimizer, and both
; are left exactly as they are.
;
; The MSI has no hooks; MSI installs were always upgraded in place.

!define ATVOPT_LEGACY_PRODUCT "Shield Optimizer"
!define ATVOPT_LEGACY_EXE "shield-optimizer-v2.exe"
!define ATVOPT_LEGACY_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${ATVOPT_LEGACY_PRODUCT}"
; MANUFACTURER is pinned to this through bundle.publisher in tauri.conf.json.
!define ATVOPT_LEGACY_MANUKEY "Software\shieldoptimizer"
!define ATVOPT_PINS "$APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar"

; Registers: $R0 old install folder, $R1 old program, $R2 scratch,
; $R4/$R5 whether an old Start menu/desktop shortcut was removed,
; $R6/$R7 the taskbar pin search. The template's shortcut macros clobber
; $0-$5, so only $R registers carry state here.

; Close the old program, which every 2.x before 2.5.0 ran as
; shield-optimizer-v2.exe (the template only checks for the new name), so its
; files can be removed. Asks first unless the install is silent or passive.
!macro NSIS_HOOK_PREINSTALL
  !if "${INSTALLMODE}" == "currentUser"
    nsis_tauri_utils::FindProcessCurrentUser "${ATVOPT_LEGACY_EXE}"
  !else
    nsis_tauri_utils::FindProcess "${ATVOPT_LEGACY_EXE}"
  !endif
  Pop $R0
  ${If} $R0 = 0
    ${IfNot} ${Silent}
    ${AndIf} $PassiveMode <> 1
      ; Cancel stops the install, as the template's own running-app check does.
      MessageBox MB_OKCANCEL|MB_ICONINFORMATION "An earlier version of ATV Optimizer (or Shield Optimizer) is running. Select OK to close it." IDOK atvopt_close_old
      Abort
    ${EndIf}
    atvopt_close_old:
    !if "${INSTALLMODE}" == "currentUser"
      nsis_tauri_utils::KillProcessCurrentUser "${ATVOPT_LEGACY_EXE}"
    !else
      nsis_tauri_utils::KillProcess "${ATVOPT_LEGACY_EXE}"
    !endif
    Pop $R0
    Sleep 500
  ${EndIf}
!macroend

; Not unpinned first: IStartMenuPinnedList::RemoveFromList would also drop
; the taskbar pin, which ATVOPT_RETARGET_PINS has already moved over.
!macro ATVOPT_REMOVE_LEGACY_SHORTCUT LINK FLAG
  ${If} ${FileExists} "${LINK}"
    !insertmacro IsShortcutTarget "${LINK}" "$R0\$R1"
    Pop $R2
    ${If} $R2 = 1
      Delete "${LINK}"
      ; Without rights (the HKLM pass) the shortcut stays, and no
      ; replacement is needed.
      ${IfNot} ${FileExists} "${LINK}"
        StrCpy ${FLAG} 1
        DetailPrint "Removed old shortcut ${LINK}"
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

; Point LINK at this install's program, starting in $INSTDIR (the template's
; SetShortcutTarget sets only the path, and the old "Start in" folder may be
; gone), with this identifier's AppUserModelID.
!macro ATVOPT_POINT_LNK_HERE LINK
  !insertmacro ComHlpr_CreateInProcInstance ${CLSID_ShellLink} ${IID_IShellLink} r0 ""
  ${If} $0 P<> 0
    ${IUnknown::QueryInterface} $0 '("${IID_IPersistFile}",.r1)'
    ${If} $1 P<> 0
      ${IPersistFile::Load} $1 '("${LINK}", ${STGM_READWRITE})'
      ${IShellLink::SetPath} $0 '(w "$INSTDIR\${MAINBINARYNAME}.exe")'
      ${IShellLink::SetWorkingDirectory} $0 '(w "$INSTDIR")'
      ${IPersistFile::Save} $1 '("${LINK}",1)'
      ${IUnknown::Release} $1 ""
    ${EndIf}
    ${IUnknown::Release} $0 ""
  ${EndIf}
  !insertmacro SetLnkAppUserModelId "${LINK}"
!macroend

; Point taskbar pins whose target is OLD at the new program.
!macro ATVOPT_RETARGET_PINS OLD
  ClearErrors
  FindFirst $R6 $R7 "${ATVOPT_PINS}\*.lnk"
  ${DoWhile} $R7 != ""
    !insertmacro IsShortcutTarget "${ATVOPT_PINS}\$R7" "${OLD}"
    Pop $R2
    ${If} $R2 = 1
      !insertmacro ATVOPT_POINT_LNK_HERE "${ATVOPT_PINS}\$R7"
      DetailPrint "Pointed taskbar pin $R7 at ${MAINBINARYNAME}.exe"
    ${EndIf}
    FindNext $R6 $R7
  ${Loop}
  FindClose $R6
  ClearErrors
!macroend

; The identifier changed in 2.5.0, and the template only retargets an
; existing shortcut, so give this install's shortcuts the new
; AppUserModelID and a "Start in" folder that exists.
!macro ATVOPT_REFRESH_AUMID LINK
  ${If} ${FileExists} "${LINK}"
    !insertmacro IsShortcutTarget "${LINK}" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $R2
    ${If} $R2 = 1
      !insertmacro ATVOPT_POINT_LNK_HERE "${LINK}"
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
      StrCpy $R1 "${ATVOPT_LEGACY_EXE}"
    ${EndIf}

    ${If} $R0 == $INSTDIR
    ${OrIf} ${FileExists} "$R0\${MAINBINARYNAME}.exe"
      ; Both products in one folder: resources and uninstall.exe are now this
      ; install's, so only the old program, entry and shortcuts go.
      ${If} $R1 != "${MAINBINARYNAME}.exe"
        Delete "$R0\$R1"
      ${EndIf}
      DetailPrint "Old ${ATVOPT_LEGACY_PRODUCT} shared a folder with this install; kept the shared files"
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

    ; Pins first (per user, in the installer's context), then the old
    ; shortcuts. No release set a Start menu folder, so they sit at the top.
    !insertmacro ATVOPT_RETARGET_PINS "$R0\$R1"
    SetShellVarContext ${CONTEXT}
    !insertmacro ATVOPT_REMOVE_LEGACY_SHORTCUT "$SMPROGRAMS\${ATVOPT_LEGACY_PRODUCT}.lnk" $R4
    !insertmacro ATVOPT_REMOVE_LEGACY_SHORTCUT "$DESKTOP\${ATVOPT_LEGACY_PRODUCT}.lnk" $R5
    !insertmacro SetContext

    DeleteRegKey ${ROOT} "${ATVOPT_LEGACY_UNINSTKEY}"
    DeleteRegKey ${ROOT} "${ATVOPT_LEGACY_MANUKEY}\${ATVOPT_LEGACY_PRODUCT}"
    DeleteRegKey /ifempty ${ROOT} "${ATVOPT_LEGACY_MANUKEY}"
  ${EndIf}
  atvopt_legacy_done_${ROOT}:
  ClearErrors
!macroend

!macro NSIS_HOOK_POSTINSTALL
  StrCpy $R4 0
  StrCpy $R5 0
  !insertmacro ATVOPT_REMOVE_LEGACY_INSTALL HKCU current
  ; Every release so far installed per user; a per-machine copy can only be
  ; removed by an elevated installer, and the steps above fail harmlessly
  ; without rights.
  !insertmacro ATVOPT_REMOVE_LEGACY_INSTALL HKLM all

  ; An update only retargets an existing "ATV Optimizer" shortcut. A user who
  ; launched the old copy from its shortcuts would be left with none.
  ; CreateShortcut's "Start in" is the output folder.
  SetOutPath $INSTDIR
  ${If} $R4 = 1
  ${AndIfNot} ${FileExists} "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    DetailPrint "Created $SMPROGRAMS\${PRODUCTNAME}.lnk in place of the old shortcut"
  ${EndIf}
  ${If} $R5 = 1
  ${AndIfNot} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
    CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
    DetailPrint "Created $DESKTOP\${PRODUCTNAME}.lnk in place of the old shortcut"
  ${EndIf}

  ; 2.3.0 to 2.4.1 ran from this folder as shield-optimizer-v2.exe; the
  ; template retargets their Start menu and desktop shortcuts but not pins.
  ${If} $OldMainBinaryName != ""
  ${AndIf} $OldMainBinaryName != "${MAINBINARYNAME}.exe"
    !insertmacro ATVOPT_RETARGET_PINS "$INSTDIR\$OldMainBinaryName"
  ${EndIf}
  !insertmacro ATVOPT_REFRESH_AUMID "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  !insertmacro ATVOPT_REFRESH_AUMID "$DESKTOP\${PRODUCTNAME}.lnk"
!macroend
