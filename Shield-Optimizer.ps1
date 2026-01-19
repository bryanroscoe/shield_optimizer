<#
.SYNOPSIS
    Nvidia Shield Ultimate Optimizer (v29 - Syntax Fix)
.DESCRIPTION
    Fixes variable expansion crash ($action?) in the Optimization loop.
#>

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# --- COLORS & UI ---
function Write-Header ($Text)   { Write-Host "`n=== $Text ===" -ForegroundColor Cyan }
function Write-SubHeader ($Text){ Write-Host "`n--- $Text ---" -ForegroundColor DarkCyan }
function Write-Success ($Text)  { Write-Host " [OK] $Text" -ForegroundColor Green }
function Write-Warn ($Text)     { Write-Host " [!!] $Text" -ForegroundColor Yellow }
function Write-ErrorMsg ($Text) { Write-Host " [ERROR] $Text" -ForegroundColor Red }
function Write-Info ($Text)     { Write-Host " [INFO] $Text" -ForegroundColor Gray }

# --- 1. PORTABLE ADB SETUP ---
Write-Header "Checking System Requirements"

$ScriptDir = $PSScriptRoot
$AdbExe = "$ScriptDir\adb.exe"
$AdbPath = ""

if (Test-Path $AdbExe) { $AdbPath = $AdbExe; Write-Success "Found Portable ADB." }
elseif (Get-Command "adb.exe" -ErrorAction SilentlyContinue) { $AdbPath = (Get-Command "adb.exe").Source; Write-Success "Found System ADB." }

if ($AdbPath -eq "") {
    Write-Warn "ADB missing. Setting up Portable ADB..."
    Stop-Process -Name "adb" -ErrorAction SilentlyContinue
    $Url = "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"
    $ZipPath = "$ScriptDir\adb_temp.zip"
    $ExtractFolder = "$ScriptDir\adb_temp_extract"

    try {
        if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
        if (Test-Path $ExtractFolder) { Remove-Item $ExtractFolder -Recurse -Force }
        
        Write-Info "Downloading..."
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri $Url -OutFile $ZipPath -UseBasicParsing

        Write-Info "Extracting..."
        Expand-Archive -Path $ZipPath -DestinationPath $ExtractFolder -Force

        $Source = "$ExtractFolder\platform-tools"
        Move-Item "$Source\adb.exe" $ScriptDir -Force
        Move-Item "$Source\AdbWinApi.dll" $ScriptDir -Force
        Move-Item "$Source\AdbWinUsbApi.dll" $ScriptDir -Force
        
        Remove-Item $ZipPath -Force
        Remove-Item $ExtractFolder -Recurse -Force
        $AdbPath = $AdbExe
        Write-Success "Portable ADB installed."
    }
    catch { Write-ErrorMsg "Setup failed."; Pause; Exit }
}

# --- 2. DEVICE SELECTION & UTILS ---
function Get-ConnectedShields {
    $raw = & $AdbPath devices
    $devices = @()
    foreach ($line in $raw) {
        if ($line -match "^(\S+)\s+device") {
            $s = $matches[1]
            $n = & $AdbPath -s $s shell settings get global device_name 2>$null
            if (-not $n) { $n = "Unknown Shield" }
            $devices += [PSCustomObject]@{ ID = $devices.Count + 1; Serial = $s; Name = $n; Status = "Ready" }
        }
        elseif ($line -match "^(\S+)\s+unauthorized") {
            $devices += [PSCustomObject]@{ ID = $devices.Count + 1; Serial = $matches[1]; Name = "UNAUTHORIZED"; Status = "Check TV" }
        }
        elseif ($line -match "^(\S+)\s+offline") {
            $devices += [PSCustomObject]@{ ID = $devices.Count + 1; Serial = $matches[1]; Name = "OFFLINE"; Status = "Device Offline" }
        }
    }
    return $devices
}

$GlobalTarget = ""

# --- 3. DIAGNOSTIC REPORT GENERATOR ---
function Generate-Report ($TargetSerial, $TargetName) {
    Write-Header "DIAGNOSTIC REPORT: $TargetName"
    
    # --- STORAGE ---
    $df = & $AdbPath -s $TargetSerial shell df -h /data
    $StorageInfo = "Unknown"
    foreach ($line in $df) {
        if ($line -match "\s+([\d\.]+[G|M])\s+([\d\.]+[G|M])\s+([\d\.]+[G|M])\s+(\d+%)") {
            $Size=$matches[1]; $Used=$matches[2]; $Free=$matches[3]; $Pct=$matches[4]
            $StorageInfo = "$Used Used / $Size Total ($Free Free)"
            $StoragePct = [int]($Pct -replace "%","")
        }
    }

    # --- SENSORS (THERMAL HAL) ---
    $Temp = "N/A"
    $tDump = & $AdbPath -s $TargetSerial shell dumpsys thermalservice 2>$null
    foreach ($line in $tDump) {
        if ($line -match "mValue=([\d\.]+).*mName=CPU") {
            $val = [float]$matches[1]
            if ($val -gt 20 -and $val -lt 100) { $Temp = [math]::Round($val, 1); break }
        }
    }
    # Fallback
    if ($Temp -eq "N/A") {
        for ($i=0; $i -lt 10; $i++) {
            $val = & $AdbPath -s $TargetSerial shell cat /sys/class/thermal/thermal_zone$i/temp 2>$null
            if ($val -match "^\d+$" -and [int]$val -gt 20000 -and [int]$val -lt 100000) {
                $Temp = [math]::Round($val / 1000, 1); break
            }
        }
    }

    # --- MEMORY (PSS PARSER) ---
    $rawMem = & $AdbPath -s $TargetSerial shell dumpsys meminfo
    $FreeRAM = 0; $SwapUsed = 0; $TotalRAM = 3072
    $RawApps = @()
    $ParsingPSS = $false

    foreach ($line in $rawMem) {
        if ($line -match "Total RAM:\s+([0-9,]+)K") { $TotalRAM = [math]::Round(($matches[1] -replace ",", "") / 1024, 0) }
        if ($line -match "Free RAM:\s+([0-9,]+)K") { $FreeRAM = [math]::Round(($matches[1] -replace ",", "") / 1024, 0) }
        if ($line -match "ZRAM:.*used for\s+([0-9,]+)K") { $SwapUsed = [math]::Round(($matches[1] -replace ",", "") / 1024, 0) }
        
        if ($line -match "Total PSS by process") { $ParsingPSS = $true; continue }
        if ($line -match "Total PSS by OOM adjustment") { $ParsingPSS = $false; continue }

        if ($ParsingPSS -and $line -match "^\s*([0-9,]+)K:\s+([a-zA-Z0-9_\.\@\-]+).*\(pid") {
            $kb = $matches[1] -replace ",", ""
            $mb = [math]::Round($kb / 1024, 0)
            $pkg = $matches[2]
            if ($pkg -notmatch "^\." -and $pkg -notmatch "^(System|Persistent|Cached|Native|Unknown|Dalvik|Stack|Ashmem|Perceptible)$") {
                $RawApps += [PSCustomObject]@{ Name = $pkg; MB = $mb }
            }
        }
    }
    
    $TopApps = $RawApps | Group-Object Name | Select-Object Name, @{N='MB'; E={ ($_.Group | Measure-Object MB -Sum).Sum }} | Sort-Object MB -Descending | Select-Object -First 10
    $UsedRAM = $TotalRAM - $FreeRAM
    $UsedPercent = [math]::Round(($UsedRAM / $TotalRAM) * 100, 0)
    
    # --- OS STATS ---
    $anim = (& $AdbPath -s $TargetSerial shell settings get global window_animation_scale | Out-String).Trim()
    $uptime = (& $AdbPath -s $TargetSerial shell uptime | Out-String).Trim()
    $build = (& $AdbPath -s $TargetSerial shell getprop ro.build.display.id | Out-String).Trim()

    # --- RENDER ---
    Write-SubHeader "DEVICE HEALTH"
    Write-Host " Device:    $TargetName ($TargetSerial)"
    Write-Host " OS Build:  $build"
    Write-Host " Uptime:    $uptime"
    Write-Host " Temp:      " -NoNewline
    if ($Temp -eq "N/A") { Write-Host "Unknown" -ForegroundColor Gray }
    elseif ($Temp -lt 65) { Write-Host "$Temp C (Cool)" -ForegroundColor Green } 
    else { Write-Host "$Temp C (Warm)" -ForegroundColor Yellow }

    Write-Host " Storage:   " -NoNewline
    if ($StoragePct -gt 90) { Write-Host "$StorageInfo (CRITICAL - Full)" -ForegroundColor Red }
    elseif ($StoragePct -gt 75) { Write-Host "$StorageInfo (Low)" -ForegroundColor Yellow }
    else { Write-Host "$StorageInfo (Good)" -ForegroundColor Green }

    Write-SubHeader "MEMORY & PERFORMANCE"
    Write-Host " RAM Load:  " -NoNewline
    if ($UsedPercent -gt 85) { Write-Host "$UsedPercent% (CRITICAL)" -ForegroundColor Red } elseif ($UsedPercent -gt 65) { Write-Host "$UsedPercent% (High)" -ForegroundColor Yellow } else { Write-Host "$UsedPercent% (Good)" -ForegroundColor Green }
    Write-Host " ($UsedRAM MB / $TotalRAM MB)" -ForegroundColor Gray
    
    Write-Host " Swap Usage:" -NoNewline
    if ($SwapUsed -lt 10) { Write-Host " $SwapUsed MB (Clean)" -ForegroundColor Green } 
    elseif ($SwapUsed -lt 100) { Write-Host " $SwapUsed MB (Used)" -ForegroundColor Yellow } 
    else { Write-Host " $SwapUsed MB (Thrashing)" -ForegroundColor Red }
    
    Write-Host " Animation: " -NoNewline
    if ($anim -eq "0.5") { Write-Host "${anim}x (Optimized)" -ForegroundColor Green } else { Write-Host "${anim}x (Stock)" -ForegroundColor Yellow }

    Write-SubHeader "TOP 10 CONSUMERS (PSS)"
    foreach ($app in $TopApps) {
        $pct = [math]::Round(($app.MB / $TotalRAM) * 100, 1)
        Write-Host " $($app.MB) MB" -NoNewline -ForegroundColor Magenta
        Write-Host " ($pct%) - $($app.Name)"
    }
    echo "`n"
}

# --- 4. PRE-SCAN HELPER ---
function Get-MemMap ($TargetSerial) {
    $MemMap = @{}
    $ParsingPSS = $false
    $rawMem = & $AdbPath -s $TargetSerial shell dumpsys meminfo
    foreach ($line in $rawMem) {
        if ($line -match "Total PSS by process") { $ParsingPSS = $true; continue }
        if ($line -match "Total PSS by OOM adjustment") { $ParsingPSS = $false; continue }
        if ($ParsingPSS -and $line -match "^\s*([0-9,]+)K:\s+([a-zA-Z0-9_\.\@\-]+).*\(pid") {
            $kb = $matches[1] -replace ",", ""
            $mb = [math]::Round($kb / 1024, 0)
            $p = $matches[2]; if (-not $MemMap.ContainsKey($p)) { $MemMap[$p] = $mb }
        }
    }
    return $MemMap
}

# --- MAIN LOOP ---
while ($true) {
    Write-Header "Device Selection"
    $shields = @(Get-ConnectedShields)

    if ($shields.Count -gt 0) {
        foreach ($dev in $shields) {
            if ($dev.Status -eq "Ready") { Write-Host " [$($dev.ID)] $($dev.Name) ($($dev.Serial))" -ForegroundColor Green }
            else { Write-Host " [$($dev.ID)] $($dev.Serial) - $($dev.Status)" -ForegroundColor Red }
        }
    } else {
        Write-Info "No devices currently connected."
    }

    Write-Host " [C] Connect to a New IP Address" -ForegroundColor Yellow
    Write-Host " [R] Refresh List" -ForegroundColor Gray
    Write-Host " [A] Report All Devices" -ForegroundColor Magenta
    Write-Host " [Q] Quit" -ForegroundColor Gray

    $selection = Read-Host "`nSelect Device [1-$($shields.Count)] or [C/R/A/Q]"

    if ($selection -match "^c") {
        $ip = Read-Host " >> Enter IP Address (e.g. 192.168.1.5)"
        if ($ip -ne "") {
            Write-Info "Attempting connection to $ip..."
            & $AdbPath connect $ip; Start-Sleep -s 2
        }
        continue
    }
    if ($selection -match "^r") { continue }
    if ($selection -match "^q") { Exit }
    
    if ($selection -match "^a") {
        Write-Header "BULK HEALTH REPORT"
        foreach ($dev in $shields) {
            if ($dev.Status -eq "Ready") {
                Generate-Report -TargetSerial $dev.Serial -TargetName $dev.Name
                Write-Host "------------------------------------------------" -ForegroundColor Gray
                Start-Sleep -s 2
            } else {
                Write-Warn "Skipping $($dev.Name) (Status: $($dev.Status))"
            }
        }
        Pause
        continue
    }

    $target = $shields | Where-Object { $_.ID -eq $selection }
    if ($target) {
        if ($target.Status -eq "Ready") {
            $GlobalTarget = $target.Serial
            $GlobalName = $target.Name
        } else {
            Write-ErrorMsg "Cannot select this device. Status: $($target.Status)"
            Pause; continue
        }
    } else {
        Write-ErrorMsg "Invalid selection."
        Pause; continue
    }

    Write-Header "Action Menu: $GlobalName"
    Write-Host " [1] OPTIMIZE (Debloat + Speed Up)" -ForegroundColor Green
    Write-Host " [2] RESTORE (Factory Defaults)" -ForegroundColor Yellow
    Write-Host " [3] REPORT (Diagnostics Only)" -ForegroundColor Cyan
    $mode = Read-Host "`nSelect Option [1-3] (Default: 1)"

    # List structure: Package, Name, Action, Risk, Effect, DefaultInput(Y/N)
    $Apps = @(
        # --- SAFE TO REMOVE (Default: Y) ---
        @("com.google.android.tvrecommendations", "Google TV Recommendations", "DISABLE", "Safe", "Removes 'Sponsored' rows from stock launcher", "Y"),
        @("com.nvidia.stats", "Nvidia Telemetry", "DISABLE", "Safe", "Stops background data collection", "Y"),
        @("com.google.android.feedback", "Google Feedback", "DISABLE", "Safe", "Stops background data collection", "Y"),
        @("com.nvidia.diagtools", "Nvidia Diag Tools", "DISABLE", "Safe", "Stops background diagnostic logging", "Y"),
        @("com.android.printspooler", "Print Spooler", "DISABLE", "Safe", "Disables useless background print service", "Y"),
        @("com.android.gallery3d", "Android Gallery", "UNINSTALL", "Safe", "Removes legacy photo viewer", "Y"),
        @("com.google.android.videos", "Google Play Movies", "UNINSTALL", "Safe", "Removes defunct Google Movies app", "Y"),
        @("com.google.android.music", "Google Play Music", "UNINSTALL", "Safe", "Removes defunct Google Music app", "Y"),
        
        # --- STREAMING APPS (Default: Y - unless you use them) ---
        @("com.netflix.ninja", "Netflix", "UNINSTALL", "Safe", "Removes the Netflix App", "Y"),
        @("com.amazon.amazonvideo.livingroom", "Amazon Prime Video", "UNINSTALL", "Safe", "Removes the Prime Video App", "Y"),
        @("com.wbd.stream", "HBO Max / Discovery", "UNINSTALL", "Safe", "Removes HBO/Max App", "Y"),
        @("com.hulu.livingroomplus", "Hulu", "UNINSTALL", "Safe", "Removes Hulu App", "Y"),
        @("tv.twitch.android.app", "Twitch", "UNINSTALL", "Safe", "Removes Twitch App", "Y"),
        @("com.disney.disneyplus.prod", "Disney+", "UNINSTALL", "Safe", "Removes Disney+ App", "Y"),
        @("com.spotify.tv.android", "Spotify", "UNINSTALL", "Safe", "Removes Spotify App", "Y"),
        @("com.google.android.youtube.tvmusic", "YouTube Music", "UNINSTALL", "Safe", "Removes YT Music App", "Y"),
        
        # --- FUNCTIONALITY BREAKERS (Default: N) ---
        @("com.google.android.katniss", "Google Assistant (Voice Search)", "DISABLE", "High", "Breaks Remote Microphone & Voice Search buttons", "N"),
        @("com.google.android.apps.mediashell", "Chromecast Built-in", "UNINSTALL", "High", "Breaks 'Casting' video from phone to Shield", "N"),
        @("com.nvidia.ota", "Nvidia System Updater", "DISABLE", "High", "Stops all Security & OS Updates", "N"),
        @("com.google.android.tvlauncher", "Stock Android Launcher", "DISABLE", "Medium", "Requires a Custom Launcher (Projectivy) or Black Screen", "N"),
        @("com.plexapp.mediaserver.smb", "Plex Media SERVER", "DISABLE", "Advanced", "Breaks hosting a Plex Server on this device", "N"),
        @("com.google.android.tts", "Google Text-to-Speech", "DISABLE", "Medium", "Breaks Accessibility features & Voice reading", "N"),
        @("com.nvidia.tegrazone3", "Nvidia Games (GeForce Now)", "UNINSTALL", "Medium", "Removes Nvidia Cloud Gaming functionality", "N"),
        @("com.google.android.play.games", "Google Play Games", "UNINSTALL", "Medium", "Removes Play Games login support", "N"),
        @("com.google.android.backdrop", "Google Screensaver", "DISABLE", "Safe", "Disables the 'Daydream' photo screensaver", "N")
    )

    if ($mode -eq "3") { 
        Generate-Report -TargetSerial $GlobalTarget -TargetName $GlobalName
        Pause; continue
    }

    if ($mode -eq "2") {
        Write-Header "Restoring Factory Defaults"
        foreach ($app in $Apps) {
            Write-Host "Restoring $($app[1])..." -NoNewline
            & $AdbPath -s $GlobalTarget shell cmd package install-existing $app[0] 2>$null | Out-Null
            & $AdbPath -s $GlobalTarget shell pm enable $app[0] 2>$null | Out-Null
            Write-Host " [DONE]" -ForegroundColor Green
        }
        & $AdbPath -s $GlobalTarget shell settings put global window_animation_scale 1.0
        & $AdbPath -s $GlobalTarget shell settings put global transition_animation_scale 1.0
        & $AdbPath -s $GlobalTarget shell settings put global animator_duration_scale 1.0
        Write-Success "Restored. Rebooting..."
        & $AdbPath -s $GlobalTarget reboot; Pause; continue
    }

    $launcher = & $AdbPath -s $GlobalTarget shell pm list packages | Select-String "com.spocky.projengmenu"
    if (-not $launcher) { Write-ErrorMsg "Projectivy Launcher NOT FOUND. Aborting."; Pause; continue }
    
    Write-Info "Scanning apps..."
    $MemMap = Get-MemMap -TargetSerial $GlobalTarget

    Write-Header "Optimization Audit"
    foreach ($app in $Apps) {
        $pkg = $app[0]; $name = $app[1]; $action = $app[2]; $risk = $app[3]; $effect = $app[4]; $def = $app[5]

        $check = & $AdbPath -s $GlobalTarget shell pm list packages | Select-String "package:$pkg"
        if (-not $check) { continue }

        if ($action -eq "DISABLE") {
            $disabled = & $AdbPath -s $GlobalTarget shell pm list packages -d | Select-String "package:$pkg"
            if ($disabled) { continue }
        }

        Write-Host -NoNewline "Found: "
        Write-Host "$name" -ForegroundColor Cyan -NoNewline
        if ($risk -eq "Safe") { $c="Green" } elseif ($risk -eq "Medium") { $c="Yellow" } else { $c="Red" }
        Write-Host " [$risk]" -ForegroundColor $c -NoNewline
        if ($MemMap.ContainsKey($pkg)) { Write-Host " (Using: $($MemMap[$pkg]) MB)" -NoNewline -ForegroundColor Magenta }
        
        Write-Host "`n    Effect: $effect" -ForegroundColor Gray

        # Dynamic Prompt based on Default
        if ($def -eq "Y") {
            # FIX: Syntax error fixed here
            $resp = Read-Host " >> $($action)? [Y/n]"
            if ($resp -eq "" -or $resp -match "^y") { $doAction = $true } else { $doAction = $false }
        } else {
            # FIX: Syntax error fixed here
            $resp = Read-Host " >> $($action)? [y/N]"
            if ($resp -match "^y") { $doAction = $true } else { $doAction = $false }
        }

        if ($doAction) {
            if ($action -eq "UNINSTALL") { & $AdbPath -s $GlobalTarget shell pm uninstall --user 0 $pkg | Out-Null }
            else { & $AdbPath -s $GlobalTarget shell pm disable-user --user 0 $pkg | Out-Null }
            Write-Success "Processed."
        }
    }

    Write-Header "Performance"
    $cur = (& $AdbPath -s $GlobalTarget shell settings get global window_animation_scale | Out-String).Trim()
    if ($cur -ne "0.5") {
        $resp = Read-Host "Set Animations to 0.5x? [Y/n]"
        if ($resp -eq "" -or $resp -match "^y") {
            & $AdbPath -s $GlobalTarget shell settings put global window_animation_scale 0.5
            & $AdbPath -s $GlobalTarget shell settings put global transition_animation_scale 0.5
            & $AdbPath -s $GlobalTarget shell settings put global animator_duration_scale 0.5
            Write-Success "Applied."
        }
    }

    Write-Header "Finished"
    $reboot = Read-Host "Reboot now to apply changes? [Y/n]"
    if ($reboot -eq "" -or $reboot -match "^y") {
        Write-Info "Rebooting..."
        & $AdbPath -s $GlobalTarget reboot
        
        Write-Header "Waiting for Device..."
        Write-Info "Waiting for device to go offline..."
        Start-Sleep -s 10
        Write-Info "Waiting for device to come online (this can take 2 mins)..."
        $retries=0
        do { 
            Start-Sleep -s 5
            $s = & $AdbPath -s $GlobalTarget get-state 2>$null
            Write-Host "." -NoNewline -ForegroundColor DarkGray
            $retries++
        } while (($s -ne "device") -and ($retries -lt 30))
        
        if ($s -eq "device") { 
            Write-Success "`nDevice Online."
            Write-Info "Waiting 30 seconds for OS to settle..."
            Start-Sleep -s 30
            Generate-Report -TargetSerial $GlobalTarget -TargetName $GlobalName
        }
    }
    Pause
}
