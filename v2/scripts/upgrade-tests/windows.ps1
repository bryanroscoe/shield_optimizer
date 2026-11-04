# One Windows upgrade scenario across the 2.5.0 rename: install a published
# release, seed and use it, upgrade it to the branch build, then check the
# migrated data, the old data, the install, the shortcuts and the registry.
#
# Every check is recorded (upgrade_check.py record/check-*) rather than thrown,
# so one run reports every failure. Evidence lands in -Evidence either way.

param(
  [Parameter(Mandatory)] [string] $Scenario,
  [Parameter(Mandatory)] [ValidateSet('nsis', 'msi')] [string] $OldKind,
  [Parameter(Mandatory)] [string] $OldInstaller,
  [Parameter(Mandatory)] [string] $OldVersion,
  [Parameter(Mandatory)] [ValidateSet('nsis-update', 'nsis-silent', 'msi')] [string] $Upgrade,
  [Parameter(Mandatory)] [string] $NewInstaller,
  [Parameter(Mandatory)] [string] $NewVersion,
  [Parameter(Mandatory)] [string] $Evidence,
  # Installed beside the old one before the upgrade (2.2.0 then 2.4.1 is the
  # side-by-side pair the 2.3.0 product rename left behind).
  [string] $MiddleInstaller = '',
  [string] $MiddleVersion = '',
  [switch] $RunningDuringInstall,
  [switch] $Retry
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Tool = Join-Path $PSScriptRoot 'upgrade_check.py'
$TelemetryHost = 'atv-optimizer-telemetry.bryanroscoe.workers.dev'
$Marker = 'shieldopt.ciUpgradeMarker'
$MarkerValue = "kept-across-rename-$Scenario"
$RunSeconds = 20

New-Item -ItemType Directory -Force -Path $Evidence | Out-Null
$Results = Join-Path $Evidence 'results.jsonl'

$Local = $env:LOCALAPPDATA
$OldData = Join-Path $Local 'ShieldOptimizer'
$NewData = Join-Path $Local 'ATVOptimizer'
$OldProfile = Join-Path $Local 'com.shieldoptimizer.app'
$NewProfile = Join-Path $Local 'com.atvoptimizer.app'
$OldProduct = if ([version]$OldVersion -lt [version]'2.3.0') { 'Shield Optimizer' } else { 'ATV Optimizer' }
$OldExeName = 'shield-optimizer-v2.exe'
$NewExeName = 'atv-optimizer.exe'
$PinDir = Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'

$parts = $NewVersion.Split('.')
$NewWixVersion = '{0}.{1}.{2}' -f $parts[0], $parts[1], ([int]$parts[2] * 1000 + 999)

function Py {
  & python -I $Tool @args
  if ($LASTEXITCODE -ne 0) { throw "upgrade_check.py $($args[0]) exited $LASTEXITCODE" }
}

function Record([string] $Name, [bool] $Ok, [string] $Detail = '', [switch] $Soft) {
  $status = if ($Ok) { 'ok' } elseif ($Soft) { 'warn' } else { 'fail' }
  if ($Detail -eq '') { $Detail = '-' }
  Py record --results $Results "--name=$Name" --status $status "--detail=$Detail"
}

function Step([string] $Text) { Write-Host "::group::$Text" }
function EndStep { Write-Host '::endgroup::' }

function Block-Telemetry {
  $hosts = Join-Path $env:SystemRoot 'System32\drivers\etc\hosts'
  Add-Content -Path $hosts -Value "`r`n0.0.0.0 $TelemetryHost`r`n:: $TelemetryHost`r`n" -Encoding ascii
  ipconfig /flushdns | Out-Null
  # Windows answers a 0.0.0.0 hosts entry with "no data" rather than the
  # address; either way nothing routable comes back.
  try {
    $addrs = @([System.Net.Dns]::GetHostAddresses($TelemetryHost) | ForEach-Object { $_.ToString() })
    $how = $addrs -join ', '
  } catch {
    $addrs = @()
    $how = "no address ($($_.Exception.Message))"
  }
  $routable = @($addrs | Where-Object { $_ -notin @('0.0.0.0', '::') })
  if ($routable.Count -gt 0) { throw "telemetry host still resolves to $($routable -join ', ')" }
  Record 'telemetry host blocked before any launch' $true $how
}

function Find-Exe([string[]] $Names) {
  foreach ($dir in @(
      (Join-Path $Local 'ATV Optimizer'), (Join-Path $Local 'Shield Optimizer'),
      (Join-Path $env:ProgramFiles 'ATV Optimizer'), (Join-Path $env:ProgramFiles 'Shield Optimizer'),
      (Join-Path ${env:ProgramFiles(x86)} 'ATV Optimizer'))) {
    foreach ($n in $Names) {
      $p = Join-Path $dir $n
      if (Test-Path -LiteralPath $p) { return $p }
    }
  }
  return $null
}

function Get-Shortcuts {
  $shell = New-Object -ComObject WScript.Shell
  $roots = @(
    @{ kind = 'startmenu'; path = [Environment]::GetFolderPath('Programs') },
    @{ kind = 'startmenu'; path = [Environment]::GetFolderPath('CommonPrograms') },
    @{ kind = 'desktop'; path = [Environment]::GetFolderPath('Desktop') },
    @{ kind = 'desktop'; path = [Environment]::GetFolderPath('CommonDesktopDirectory') },
    @{ kind = 'pin'; path = $PinDir }
  )
  foreach ($r in $roots) {
    if (-not $r.path -or -not (Test-Path -LiteralPath $r.path)) { continue }
    Get-ChildItem -LiteralPath $r.path -Filter '*.lnk' -Recurse -ErrorAction SilentlyContinue |
      Where-Object { $_.BaseName -match 'Optimizer' } |
      ForEach-Object {
        $target = $shell.CreateShortcut($_.FullName).TargetPath
        [pscustomobject]@{
          kind = $r.kind; path = $_.FullName; name = $_.BaseName; target = $target
          targetExists = [bool]($target -and (Test-Path -LiteralPath $target))
        }
      }
  }
}

function Get-UninstallEntries {
  $roots = @(
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall')
  foreach ($root in $roots) {
    if (-not (Test-Path $root)) { continue }
    Get-ChildItem $root -ErrorAction SilentlyContinue | ForEach-Object {
      $p = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
      if ($p -and "$($p.DisplayName)" -match 'Optimizer') {
        [pscustomobject]@{
          key = $_.Name; DisplayName = $p.DisplayName; DisplayVersion = $p.DisplayVersion
          InstallLocation = $p.InstallLocation; MainBinaryName = $p.MainBinaryName
          UninstallString = $p.UninstallString
        }
      }
    }
  }
}

function Save-State([string] $Label) {
  @(Get-Shortcuts) | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $Evidence "shortcuts-$Label.json")
  @(Get-UninstallEntries) | ConvertTo-Json -Depth 3 | Set-Content (Join-Path $Evidence "uninstall-entries-$Label.json")
  $reg = Join-Path $Evidence "registry-$Label.reg.txt"
  Remove-Item $reg -ErrorAction SilentlyContinue
  foreach ($key in @(
      'HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Shield Optimizer',
      'HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\ATV Optimizer',
      'HKCU\Software\shieldoptimizer', 'HKLM\Software\shieldoptimizer')) {
    $tmp = Join-Path $env:RUNNER_TEMP 'export.reg'
    $ErrorActionPreference = 'Continue'
    & reg.exe export $key $tmp /y *> $null
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE -eq 0) {
      Add-Content $reg "; ---- $key"
      Get-Content $tmp | Add-Content $reg
    } else {
      Add-Content $reg "; ---- $key (absent)"
    }
  }
  $global:LASTEXITCODE = 0
  Py listing --out (Join-Path $Evidence "listing-$Label.txt") `
    $OldData $NewData (Join-Path $Local 'Shield Optimizer') (Join-Path $Local 'ATV Optimizer') `
    (Join-Path $env:ProgramFiles 'ATV Optimizer') $PinDir
  Py listing --max-depth 3 --out (Join-Path $Evidence "listing-profiles-$Label.txt") $OldProfile $NewProfile
  Get-Process | Where-Object { $_.ProcessName -match 'optimizer|msedgewebview2' } |
    Select-Object Id, ProcessName, Path | Format-Table -AutoSize | Out-String |
    Set-Content (Join-Path $Evidence "processes-$Label.txt")
}

function Wait-WebView([string] $ProfileMark, [int] $Seconds = 30) {
  $deadline = (Get-Date).AddSeconds($Seconds)
  do {
    $procs = @(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" |
        Where-Object { $_.CommandLine -like "*$ProfileMark*" })
    if ($procs.Count -eq 0) { return $true }
    Start-Sleep -Seconds 1
  } while ((Get-Date) -lt $deadline)
  $procs | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 2
  return $false
}

function Close-App([System.Diagnostics.Process] $Proc, [string] $ProfileMark) {
  $how = 'already exited'
  if (-not $Proc.HasExited) {
    $Proc.Refresh()
    if ($Proc.MainWindowHandle -ne [IntPtr]::Zero -and $Proc.CloseMainWindow() -and $Proc.WaitForExit(20000)) {
      $how = 'closed its window'
    } else {
      $ErrorActionPreference = 'Continue'
      & taskkill.exe /PID $Proc.Id *> $null
      $ErrorActionPreference = 'Stop'
      if ($Proc.WaitForExit(15000)) { $how = 'taskkill (WM_CLOSE)' }
      else {
        Stop-Process -Id $Proc.Id -Force -ErrorAction SilentlyContinue
        $Proc.WaitForExit(10000) | Out-Null
        $how = 'killed'
      }
    }
  }
  $global:LASTEXITCODE = 0
  $clean = Wait-WebView $ProfileMark
  Write-Host "app $($Proc.Id) $how; webview exited cleanly: $clean"
  return $how
}

function Run-App([string] $Exe, [string] $ProfileMark, [string] $Label, [System.Diagnostics.Process] $Already = $null) {
  $proc = $Already
  if (-not $proc) { $proc = Start-Process -FilePath $Exe -PassThru }
  Start-Sleep -Seconds $RunSeconds
  $alive = -not $proc.HasExited
  $how = Close-App $proc $ProfileMark
  Record "$Label stayed up for ${RunSeconds}s" $alive "pid $($proc.Id), $how"
}

function Install([string] $Kind, [string] $Installer, [string[]] $NsisArgs, [string] $Label) {
  if ($Kind -eq 'msi') {
    $log = Join-Path $Evidence "msi-$Label.log"
    $p = Start-Process msiexec.exe -ArgumentList @('/i', "`"$Installer`"", '/qn', '/norestart', '/l*v', "`"$log`"") -PassThru
  } else {
    $p = Start-Process -FilePath $Installer -ArgumentList $NsisArgs -PassThru
  }
  if (-not $p.WaitForExit(600000)) { throw "$Label installer did not finish in 10 minutes" }
  $code = $p.ExitCode
  $argText = if ($Kind -eq 'msi') { '/i /qn' } else { $NsisArgs -join ' ' }
  Record "$Label installer ($Kind $argText) succeeded" ($code -eq 0 -or $code -eq 3010) "exit $code"
  if ($code -ne 0 -and $code -ne 3010) { throw "$Label installer exited $code" }
}

function Profile-Unchanged([string] $Label) {
  Py check-unchanged --results $Results --name "old profile unchanged ($Label)" `
    --manifest (Join-Path $Evidence 'manifest-old-profile.json') --root $OldProfile
}

function Data-Unchanged([string] $Label) {
  $extra = @()
  # A launch whose data copy failed runs on the legacy folder, so it adds
  # its own log there. Every file that was already there must stay as it was.
  if ($Retry) { $extra = @('--allow-added', 'logs') }
  Py check-unchanged --results $Results --name "old data folder unchanged ($Label)" `
    --manifest (Join-Path $Evidence 'manifest-old-data.json') --root $OldData @extra
}

function Count-Copied([string] $Item) {
  $n = & python -I $Tool count-log --logs (Join-Path $NewData 'logs') --pattern 'rename migration' --pattern 'copied' --pattern $Item
  return [int]$n
}

function Check-Migrated([string] $Label) {
  Py check-seeded --results $Results --name "seeded data copied byte-identical ($Label)" --old $OldData --new $NewData
  Py check-telemetry --results $Results --name "telemetry.json kept (same install id, still off) ($Label)" --new $NewData
  Py check-ls --results $Results --name "localStorage marker survived in the new profile ($Label)" `
    --kind webview2 --profile $NewProfile --key $Marker --value $MarkerValue
  # 2.5.0 rewrites notesSeenVersion itself, so only 2.2.0's lastSeenVersion
  # (which nothing writes any more) must keep the old value.
  $valueArgs = if ($OldLsKey -eq 'shieldopt.lastSeenVersion') { @('--value', $OldLsValue) } else { @() }
  Py check-ls --results $Results --name "old app's own localStorage key survived ($Label)" `
    --kind webview2 --profile $NewProfile --key $OldLsKey @valueArgs --soft
  Py check-no-temps --results $Results --name "no *.migrating-* folders left ($Label)" $Local
  Data-Unchanged $Label
  Profile-Unchanged $Label
}

$script:OldLsKey = $null
$script:OldLsValue = $null
$script:OldExe = $null
$script:OldExes = @()
$script:HadDesktop = $false
$lockProc = $null
$lockSignal = Join-Path $env:RUNNER_TEMP 'hold-locks.signal'

try {
  Step 'Preconditions'
  Block-Telemetry
  foreach ($p in @($OldData, $NewData, $OldProfile, $NewProfile)) {
    if (Test-Path -LiteralPath $p) { throw "$p exists before the test started" }
  }
  EndStep

  Step "Install $OldProduct $OldVersion ($OldKind)"
  Install $OldKind $OldInstaller @('/S') 'old'
  $script:OldExe = Find-Exe @($OldExeName)
  if (-not $script:OldExe) { throw "could not find $OldExeName after installing $OldVersion" }
  Record "old $OldVersion installed" $true $script:OldExe
  $script:OldExes = @($script:OldExe)
  EndStep

  Step 'Seed the data folder before the old app first runs'
  Py seed --data $OldData
  New-Item -ItemType Directory -Force -Path $PinDir | Out-Null
  $shell = New-Object -ComObject WScript.Shell
  $pin = $shell.CreateShortcut((Join-Path $PinDir "$OldProduct.lnk"))
  $pin.TargetPath = $script:OldExe
  $pin.Save()
  EndStep

  if ([version]$OldVersion -lt [version]'2.3.0') {
    $script:OldLsKey = 'shieldopt.lastSeenVersion'
  } else {
    $script:OldLsKey = 'shieldopt.notesSeenVersion'
  }
  $script:OldLsValue = $OldVersion

  Step "Run $OldVersion once"
  Run-App $script:OldExe 'com.shieldoptimizer.app' "old $OldVersion"
  if (-not (Test-Path -LiteralPath (Join-Path $OldProfile 'EBWebView'))) {
    throw "the old app did not create its WebView profile in $OldProfile"
  }
  # 2.2.0 writes lastSeenVersion only once its update check (a request to
  # GitHub) answers, so a slow network can leave it unwritten. Run it again.
  for ($i = 2; $i -le 3; $i++) {
    & python -I $Tool ls-has --kind webview2 --profile $OldProfile --key $script:OldLsKey | Out-Null
    if ($LASTEXITCODE -eq 0) { break }
    Run-App $script:OldExe 'com.shieldoptimizer.app' "old $OldVersion (run $i, $script:OldLsKey not written yet)"
  }
  $global:LASTEXITCODE = 0
  EndStep

  if ($MiddleInstaller) {
    Step "Install $MiddleVersion beside it and run it once"
    Install 'nsis' $MiddleInstaller @('/S') 'middle'
    $mid = Join-Path $Local "ATV Optimizer\$OldExeName"
    $sideBySide = @(Get-UninstallEntries | Where-Object { $_.DisplayName -in @('Shield Optimizer', 'ATV Optimizer') })
    Record "$MiddleVersion installed beside $OldVersion" ((Test-Path -LiteralPath $mid) -and $sideBySide.Count -eq 2) `
      (($sideBySide | ForEach-Object { "$($_.DisplayName) $($_.DisplayVersion)" }) -join '; ')
    $script:OldExes += $mid
    Run-App $mid 'com.shieldoptimizer.app' "middle $MiddleVersion"
    EndStep
  }

  Step 'Record the old state'
  Py ls-dump --kind webview2 --profile $OldProfile --out (Join-Path $Evidence 'localstorage-old.json')
  Py check-ls --results $Results --name "old app wrote $script:OldLsKey=$OldVersion on its own" `
    --kind webview2 --profile $OldProfile --key $script:OldLsKey --value $OldVersion --soft
  Py ls-inject --kind webview2 --profile $OldProfile --key $Marker --value $MarkerValue
  Py manifest --root $OldData --out (Join-Path $Evidence 'manifest-old-data.json')
  Py manifest --root $OldProfile --out (Join-Path $Evidence 'manifest-old-profile.json')
  Save-State 'before-upgrade'
  $script:HadDesktop = [bool](@(Get-Shortcuts) | Where-Object { $_.kind -eq 'desktop' })
  EndStep

  if ($Retry) {
    Step 'Lock one old data file and one old profile file so the first copy fails'
    $locked = @(
      (Get-ChildItem (Join-Path $OldData 'snapshots') -Filter '*.json' | Select-Object -First 1).FullName,
      # Only Local Storage entries are essential to the profile copy; a
      # locked cache or state file is skipped rather than failing it.
      (Join-Path $OldProfile 'EBWebView\Default\Local Storage\leveldb\CURRENT'))
    Set-Content $lockSignal 'hold'
    $lockArgs = @('-NoProfile', '-File', (Join-Path $PSScriptRoot 'hold-locks.ps1'), $lockSignal) + $locked
    $lockProc = Start-Process pwsh -PassThru -WindowStyle Hidden -ArgumentList ($lockArgs | ForEach-Object { "`"$_`"" })
    $deadline = (Get-Date).AddSeconds(30)
    while (-not (Test-Path "$lockSignal.ready") -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300 }
    if (-not (Test-Path "$lockSignal.ready")) { throw 'the lock holder did not start' }
    Record 'files locked for the first launch' $true ($locked -join ', ')
    EndStep
  }

  $oldRunning = $null
  if ($RunningDuringInstall) {
    Step 'Start the old app and leave it running'
    $oldRunning = Start-Process -FilePath $script:OldExe -PassThru
    Start-Sleep -Seconds 10
    Record 'old app running when the installer starts' (-not $oldRunning.HasExited) "pid $($oldRunning.Id)"
    EndStep
  }

  Step "Upgrade with the branch build ($Upgrade)"
  switch ($Upgrade) {
    # tauri-plugin-updater 2.10 runs the NSIS installer with the passive
    # install mode's flags, then /UPDATE /ARGS and the app's own arguments.
    'nsis-update' { Install 'nsis' $NewInstaller @('/P', '/R', '/UPDATE', '/ARGS') 'new' }
    'nsis-silent' { Install 'nsis' $NewInstaller @('/S') 'new' }
    'msi' { Install 'msi' $NewInstaller @() 'new' }
  }
  if ($oldRunning) {
    $stopped = $oldRunning.WaitForExit(5000)
    Record 'installer stopped the running old app' $stopped "pid $($oldRunning.Id)"
    if (-not $stopped) { Close-App $oldRunning 'com.shieldoptimizer.app' | Out-Null }
    # The second old run wrote to its own folders, so the baseline for "old
    # data unchanged" is taken now, once its WebView has exited too.
    $clean = Wait-WebView 'com.shieldoptimizer.app'
    Py manifest --root $OldData --out (Join-Path $Evidence 'manifest-old-data.json')
    Py manifest --root $OldProfile --out (Join-Path $Evidence 'manifest-old-profile.json')
    Record 'old WebView exited before the old folders were re-recorded' $clean -Soft
  }
  $newExe = Find-Exe @($NewExeName)
  Record "new $NewExeName installed" ([bool]$newExe) "$newExe"
  if (-not $newExe) { throw "$NewExeName not found after the upgrade" }
  EndStep

  Step 'First launch of the new build'
  $auto = $null
  if ($Upgrade -eq 'nsis-update') {
    # /R: the installer relaunches the app, as after an in-app update.
    $deadline = (Get-Date).AddSeconds(60)
    while (-not $auto -and (Get-Date) -lt $deadline) {
      $auto = Get-Process -Name 'atv-optimizer' -ErrorAction SilentlyContinue | Select-Object -First 1
      if (-not $auto) { Start-Sleep -Seconds 1 }
    }
    Record 'installer relaunched the app (/R)' ([bool]$auto) $(if ($auto) { "pid $($auto.Id)" } else { 'not seen in 60s; launching it directly' }) -Soft
  }
  Run-App $newExe 'com.atvoptimizer.app' 'new build (first launch)' $auto
  EndStep

  $logDirs = @('--logs', (Join-Path $NewData 'logs'), '--logs', (Join-Path $OldData 'logs'))

  if ($Retry) {
    Step 'Check the failed first copy, then remove the obstacle'
    Py check-log --results $Results --name 'first launch logged the failed copy' @logDirs `
      --pattern 'rename migration' --pattern 'fail' --min 1
    Py ls-dump --kind webview2 --profile $NewProfile --out (Join-Path $Evidence 'localstorage-new-after-failed.json')
    Save-State 'after-failed-launch'
    Remove-Item $lockSignal
    if (-not $lockProc.WaitForExit(20000)) { Stop-Process -Id $lockProc.Id -Force }
    $lockProc = $null
    EndStep

    Step 'Second launch: the retry'
    Run-App $newExe 'com.atvoptimizer.app' 'new build (retry launch)'
    EndStep
  }

  Step 'Check the migration'
  Py ls-dump --kind webview2 --profile $NewProfile --out (Join-Path $Evidence 'localstorage-new.json')
  Py check-log --results $Results --name 'log reports the data folder copied once' `
    --logs (Join-Path $NewData 'logs') --pattern 'rename migration' --pattern 'copied' --pattern 'data folder' --min 1 --max 1
  Py check-log --results $Results --name 'log reports the webview profile copied once' `
    --logs (Join-Path $NewData 'logs') --pattern 'rename migration' --pattern 'copied' --pattern 'webview profile' --min 1 --max 1
  if (-not $Retry) {
    Py check-log --results $Results --name 'no failed copy logged' @logDirs `
      --pattern 'rename migration' --pattern 'fail' --min 0 --max 0
  }
  Check-Migrated 'after migration'
  $lsDir = Join-Path $NewProfile 'EBWebView\Default\Local Storage\leveldb'
  Record 'new profile has a Local Storage database' (Test-Path -LiteralPath $lsDir) $lsDir
  EndStep

  Step 'Check the install'
  Save-State 'after-upgrade'
  $entries = @(Get-UninstallEntries)
  $legacyEntries = @($entries | Where-Object { $_.DisplayName -eq 'Shield Optimizer' })
  $atvEntries = @($entries | Where-Object { $_.DisplayName -eq 'ATV Optimizer' })
  Record 'no "Shield Optimizer" Add/Remove entry' ($legacyEntries.Count -eq 0) (($legacyEntries | ForEach-Object key) -join ', ')
  $versions = ($atvEntries | ForEach-Object { "$($_.DisplayVersion) at $($_.key)" }) -join '; '
  Record 'exactly one "ATV Optimizer" Add/Remove entry, at the new version' `
    ($atvEntries.Count -eq 1 -and $atvEntries[0].DisplayVersion -in @($NewVersion, $NewWixVersion)) $versions
  foreach ($root in @('HKCU', 'HKLM')) {
    $gone = -not (Test-Path "${root}:\Software\shieldoptimizer\Shield Optimizer")
    Record "$root\Software\shieldoptimizer\Shield Optimizer removed" $gone
  }
  foreach ($exe in $script:OldExes) {
    Record "old program $exe removed" (-not (Test-Path -LiteralPath $exe))
  }
  if ($OldProduct -eq 'Shield Optimizer') {
    $oldDir = Split-Path $script:OldExe
    $left = @(if (Test-Path -LiteralPath $oldDir) { Get-ChildItem -LiteralPath $oldDir -Recurse -File | ForEach-Object FullName })
    Record "old install folder $oldDir removed" ($left.Count -eq 0) ($left -join ', ')
  }
  $links = @(Get-Shortcuts)
  $legacyLinks = @($links | Where-Object { $_.kind -ne 'pin' -and $_.name -eq 'Shield Optimizer' })
  Record 'no "Shield Optimizer" Start menu or desktop shortcut' ($legacyLinks.Count -eq 0) (($legacyLinks | ForEach-Object path) -join ', ')
  $start = @($links | Where-Object { $_.kind -eq 'startmenu' -and $_.name -eq 'ATV Optimizer' -and $_.target -like "*\$NewExeName" -and $_.targetExists })
  Record "Start menu shortcut opens $NewExeName" ($start.Count -ge 1) (($links | Where-Object kind -eq 'startmenu' | ForEach-Object { "$($_.path) -> $($_.target)" }) -join '; ')
  if ($script:HadDesktop) {
    $desk = @($links | Where-Object { $_.kind -eq 'desktop' -and $_.name -eq 'ATV Optimizer' -and $_.target -like "*\$NewExeName" -and $_.targetExists })
    Record "desktop shortcut opens $NewExeName" ($desk.Count -ge 1) (($links | Where-Object kind -eq 'desktop' | ForEach-Object { "$($_.path) -> $($_.target)" }) -join '; ')
  }
  $stale = @($links | Where-Object { $_.kind -ne 'pin' -and (-not $_.targetExists -or $_.target -like "*\$OldExeName") })
  Record 'no Start menu or desktop shortcut points at a missing or old program' ($stale.Count -eq 0) (($stale | ForEach-Object { "$($_.path) -> $($_.target)" }) -join '; ')
  $pins = @($links | Where-Object kind -eq 'pin')
  $pinOk = $pins.Count -ge 1 -and -not ($pins | Where-Object { -not $_.targetExists -or $_.target -notlike "*\$NewExeName" })
  # The MSI has no hook, so nothing there retargets a pin; report it, don't fail on it.
  Record 'taskbar pin retargeted to the new program' $pinOk (($pins | ForEach-Object { "$($_.path) -> $($_.target)" }) -join '; ') -Soft:($Upgrade -eq 'msi')
  EndStep

  Step 'Next launch is a no-op'
  $dataBefore = Count-Copied 'data folder'
  $profileBefore = Count-Copied 'webview profile'
  Run-App $newExe 'com.atvoptimizer.app' 'new build (no-op launch)'
  $dataAfter = Count-Copied 'data folder'
  $profileAfter = Count-Copied 'webview profile'
  Record 'no-op launch copied nothing again' ($dataAfter -eq $dataBefore -and $profileAfter -eq $profileBefore) `
    "copy lines: data $dataBefore -> $dataAfter, profile $profileBefore -> $profileAfter"
  Check-Migrated 'after the no-op launch'
  EndStep
} catch {
  Write-Host "::error::$($_.Exception.Message)"
  Record 'harness completed without an error' $false "$($_.Exception.Message) at $($_.InvocationInfo.PositionMessage)"
} finally {
  if ($lockProc -and -not $lockProc.HasExited) {
    Remove-Item $lockSignal -ErrorAction SilentlyContinue
    Stop-Process -Id $lockProc.Id -Force -ErrorAction SilentlyContinue
  }
  Get-Process -Name 'atv-optimizer', 'shield-optimizer-v2' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  try {
    Save-State 'final'
    foreach ($pair in @(@($OldData, 'old-data-logs'), @($NewData, 'new-data-logs'))) {
      $src = Join-Path $pair[0] 'logs'
      if (Test-Path -LiteralPath $src) { Copy-Item -Recurse -Force $src (Join-Path $Evidence $pair[1]) }
    }
    if (Test-Path -LiteralPath $NewProfile) {
      Py ls-dump --kind webview2 --profile $NewProfile --out (Join-Path $Evidence 'localstorage-new-final.json')
    }
    Py manifest --root $NewData --out (Join-Path $Evidence 'manifest-new-data-final.json')
  } catch {
    Write-Host "::warning::evidence collection: $($_.Exception.Message)"
  }
}
