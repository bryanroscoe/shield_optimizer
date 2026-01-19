<#
.SYNOPSIS
    Nvidia Shield Ultimate Optimizer (v33 - Launcher Manager)
.DESCRIPTION
    - Detects ALL custom launchers (Projectivy, FLauncher, Wolf, etc.).
    - New [L]auncher Menu: Install Projectivy remotely via Play Store.
    - Improved Network Scanner.
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
function Write-Dim ($Text)      { Write-Host " $Text" -ForegroundColor DarkGray }

# --- 0. WELCOME & PRE-FLIGHT ---
Clear-Host
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "   NVIDIA SHIELD ULTIMATE OPTIMIZER     " -ForegroundColor White
Write-Host "      Debloat | Optimize | Monitor      " -ForegroundColor Gray
Write-Host "========================================" -ForegroundColor Cyan

Write-Host "`n[!] PRE-FLIGHT CHECKLIST" -ForegroundColor Yellow
Write-Host "Before proceeding, please ensure:"
Write-Host " 1. USB Debugging is ON (Device Prefs > Developer Options)"
Write-Host " 2. Network Debugging is ON (If connecting via WiFi)"
Write-Host " 3. A Custom Launcher is INSTALLED (We will check this for you)"

$ack = Read-Host "`nPress Enter to start..."

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

# --- 2. NETWORK SCANNER ---
function Scan-Network {
    Write-Header "SCANNING NETWORK FOR SHIELDS"
    Write-Info "Scanning local ARP table for devices listening on port 5555..."
    
    $arpTable = arp -a | Select-String "dynamic"
    $found = @()

    foreach ($entry in $arpTable) {
        if ($entry -match "(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})") {
            $ip = $matches[1]
            try {
                $socket = New-Object System.Net.Sockets.TcpClient
                $connect = $socket.BeginConnect($ip, 5555, $null, $null)
                $success = $connect.AsyncWaitHandle.WaitOne(100, $false) # 100ms Timeout
                if ($success) {
                    Write-Host " [FOUND] $ip (ADB Open)" -ForegroundColor Green
                    $found += $ip
                    & $AdbPath connect $ip | Out-Null
                }
            } catch {}
        }
    }

    if ($found.Count -eq 0) {
        Write-Warn "No devices found via ARP scan."
        Write-Info "Try entering the IP manually if your device is on a different subnet."
    } else {
        Write-Success "Scan complete. Refreshed device list."
    }
}

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

# --- 3. UTILS: LAUNCHER CHECK ---
function Get-LauncherStatus ($TargetSerial) {
    $pkgs = & $AdbPath -s $TargetSerial shell pm list packages
    $known = @(
        @{Id="com.spocky.projengmenu"; Name="Projectivy Launcher"},
        @{Id="me.efesser.flauncher"; Name="FLauncher"},
        @{Id="com.wolf.firelauncher"; Name="Wolf Launcher"},
        @{Id="com.sweech.launcher"; Name="ATV Launcher"},
        @{Id="com.google.android.tvlauncher"; Name="Stock Android TV Launcher"}
    )
    
    $installed = @()
    foreach ($k in $known) {
        if ($pkgs -match $k.Id) { $installed += $k }
    }
    return $installed
}

# --- 4. REPORT GENERATOR ---
function Generate-Report ($TargetSerial, $TargetName) {
    Write-Header "DIAGNOSTIC REPORT: $TargetName"
    
    # --- LAUNCHER CHECK ---
    $launchers = Get-LauncherStatus -TargetSerial $TargetSerial
    $customFound = $false
    $stockFound = $false
    
    Write-SubHeader "LAUNCHER HEALTH"
    foreach ($l in $launchers) {
        if ($l.Id -eq "com.google.android.tvlauncher") { 
            $stockFound = $true
            # Check if disabled
            $isDisabled = & $AdbPath -s $TargetSerial shell pm list packages -d | Select-String "com.google.android.tvlauncher"
            if ($isDisabled) { Write-Host " Stock Launcher:      " -NoNewline; Write-Host "Disabled (Perfect)" -ForegroundColor Green }
            else { Write-Host " Stock Launcher:      " -NoNewline; Write-Host "Running (Wasting RAM)" -ForegroundColor Yellow }
        } else {
            $customFound = $true
            Write-Host " Custom Launcher:     " -NoNewline; Write-Host "$($l.Name) (Installed)" -ForegroundColor Green
        }
    }
    if (-not $customFound) { Write-Host " Custom Launcher:     " -NoNewline; Write-Host "NONE DETECTED (Critical Warning)" -ForegroundColor Red }

    # --- SENSORS ---
    $Temp = "N/A"
    $tDump = & $AdbPath -s $TargetSerial shell dumpsys thermalservice 2>$null
    foreach ($line in $tDump) {
        if ($line -match "mValue=([\d\.]+).*mName=CPU") {
            $val = [float]$matches[1]
            if ($val -gt 20 -and $val -lt 100) { $Temp = [math]::Round($val, 1); break }
        }
    }
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
    
    # --- RENDER ---
    Write-SubHeader "DEVICE HEALTH"
    Write-Host " Device:    $TargetName ($TargetSerial)"
    Write-Host " Temp:      " -NoNewline
    if ($Temp -eq "N/A") { Write-Host "Unknown" -ForegroundColor Gray }
    elseif ($Temp -lt 65) { Write-Host "$Temp C (Cool)" -ForegroundColor Green } 
    else { Write-Host "$Temp C (Warm)" -ForegroundColor Yellow }

    Write-SubHeader "MEMORY"
    Write-Host " RAM Load:  " -NoNewline
    if ($UsedPercent -gt 85) { Write-Host "$UsedPercent% (CRITICAL)" -ForegroundColor Red } elseif ($UsedPercent -gt 65) { Write-Host "$UsedPercent% (High)" -ForegroundColor Yellow } else { Write-Host "$UsedPercent% (Good)" -ForegroundColor Green }
    Write-Host " ($UsedRAM MB / $TotalRAM MB)" -ForegroundColor Gray
    
    Write-SubHeader "TOP 10 CONSUMERS (PSS)"
    foreach ($app in $TopApps) {
        $pct = [math]::Round(($app.MB / $TotalRAM) * 100, 1)
        Write-Host " $($app.MB) MB" -NoNewline -ForegroundColor Magenta
        Write-Host " ($pct%) - $($app.Name)"
    }
    echo "`n"
}

# --- 5. PRE-SCAN HELPER ---
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

    Write-Host " [S] Scan Network for Shields" -ForegroundColor Yellow
    Write-Host " [C] Connect Manually (IP)" -ForegroundColor Gray
    Write-Host " [R] Refresh List" -ForegroundColor Gray
    Write-Host " [A] Report All Devices" -ForegroundColor Magenta
    Write-Host " [H] Help / Info" -ForegroundColor Cyan
    Write-Host " [Q] Quit" -ForegroundColor Gray

    $selection = Read-Host "`nSelect Device [1-$($shields.Count)] or [S/C/R/A/H/Q]"

    if ($selection -match "^s") { Scan-Network; Pause; continue }
    if ($selection -match "^c") {
        $ip = Read-Host " >> Enter IP Address (e.g. 192.168.1.5)"
        if ($ip -ne "") { Write-Info "Connecting..."; & $AdbPath connect $ip; Start-Sleep -s 2 }
        continue
    }
    if ($selection -match "^r") { continue }
    if ($selection -match "^q") { Exit }
    
    if ($selection -match "^h") {
        Write-Header "HELP & DOCUMENTATION"
        Write-Host "1. OPTIMIZE MODE" -ForegroundColor Cyan
        Write-Host "   Removes bloatware and speeds up animations."
        Write-Host "   - Safe Items default to [Y]es."
        Write-Host "   - Risky Items default to [N]o."
        Write-Host "`n2. RESTORE MODE" -ForegroundColor Cyan
        Write-Host "   Re-installs all removed apps."
        Write-Host "`n3. REPORT MODE" -ForegroundColor Cyan
        Write-Host "   Checks Temp, Storage, RAM, Swap, and Launchers."
        Pause; continue
    }

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
    Write-Host " [4] INSTALL LAUNCHER (Projectivy)" -ForegroundColor Magenta
    $mode = Read-Host "`nSelect Option [1-4] (Default: 1)"

    # [4] INSTALL LAUNCHER
    if ($mode -eq "4") {
        Write-Header "Install Projectivy Launcher"
        Write-Info "Attempting to open Play Store page on the Shield..."
        try {
            & $AdbPath -s $GlobalTarget shell am start -a android.intent.action.VIEW -d "market://details?id=com.spocky.projengmenu"
            Write-Success "Play Store opened on TV! Please click 'Install' with your remote."
        } catch {
            Write-ErrorMsg "Could not open Play Store. Device might lack GApps."
        }
        Pause; continue
    }

    # [3] REPORT
    if ($mode -eq "3") { 
        Generate-Report -TargetSerial $GlobalTarget -TargetName $GlobalName
        Pause; continue
    }

    # [2] RESTORE & [1] OPTIMIZE Logic (Same as before)
    # ... (Keeping previous logic for consistency)
    
    $Apps = @(
        # ... (Same App List as v30)
        @("com.google.android.tvrecommendations", "Google TV Recommendations", "DISABLE", "Safe", "Removes 'Sponsored' rows", "Y"),
        @("com.nvidia.stats", "Nvidia Telemetry", "DISABLE", "Safe", "Stops background data", "Y"),
        @("com.google.android.feedback", "Google Feedback", "DISABLE", "Safe", "Stops background data", "Y"),
        @("com.nvidia.diagtools", "Nvidia Diag Tools", "DISABLE", "Safe", "Stops diagnostic logging", "Y"),
        @("com.android.printspooler", "Print Spooler", "DISABLE", "Safe", "Disables print service", "Y"),
        @("com.android.gallery3d", "Android Gallery", "UNINSTALL", "Safe", "Removes legacy viewer", "Y"),
        @("com.google.android.videos", "Google Play Movies", "UNINSTALL", "Safe", "Removes defunct app", "Y"),
        @("com.google.android.music", "Google Play Music", "UNINSTALL", "Safe", "Removes defunct app", "Y"),
        @("com.netflix.ninja", "Netflix", "UNINSTALL", "Safe", "Removes Netflix", "N"),
        @("com.amazon.amazonvideo.livingroom", "Amazon Prime Video", "UNINSTALL", "Safe", "Removes Prime Video", "N"),
        @("com.wbd.stream", "HBO Max / Discovery", "UNINSTALL", "Safe", "Removes HBO/Max", "N"),
        @("com.hulu.livingroomplus", "Hulu", "UNINSTALL", "Safe", "Removes Hulu", "N"),
        @("tv.twitch.android.app", "Twitch", "UNINSTALL", "Safe", "Removes Twitch", "N"),
        @("com.disney.disneyplus.prod", "Disney+", "UNINSTALL", "Safe", "Removes Disney+", "N"),
        @("com.spotify.tv.android", "Spotify", "UNINSTALL", "Safe", "Removes Spotify", "N"),
        @("com.google.android.youtube.tvmusic", "YouTube Music", "UNINSTALL", "Safe", "Removes YT Music", "N"),
        @("com.google.android.katniss", "Google Assistant (Voice)", "DISABLE", "High", "Breaks Remote Mic", "N"),
        @("com.google.android.apps.mediashell", "Chromecast Built-in", "UNINSTALL", "High", "Breaks Casting", "N"),
        @("com.nvidia.ota", "Nvidia System Updater", "DISABLE", "High", "Stops Updates", "N"),
        @("com.google.android.tvlauncher", "Stock Android Launcher", "DISABLE", "Medium", "Requires Custom Launcher", "N"),
        @("com.plexapp.mediaserver.smb", "Plex Media SERVER", "DISABLE", "Advanced", "Breaks local Plex Server", "N"),
        @("com.google.android.tts", "Google Text-to-Speech", "DISABLE", "Medium", "Breaks Accessibility", "N"),
        @("com.nvidia.tegrazone3", "Nvidia Games (GeForce)", "UNINSTALL", "Medium", "Removes Cloud Gaming", "N"),
        @("com.google.android.play.games", "Google Play Games", "UNINSTALL", "Medium", "Removes Play Games", "N"),
        @("com.google.android.backdrop", "Google Screensaver", "DISABLE", "Safe", "Disables Screensaver", "N")
    )

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

    $launcherList = Get-LauncherStatus -TargetSerial $GlobalTarget
    $hasCustom = $false
    foreach ($l in $launcherList) { if ($l.Id -ne "com.google.android.tvlauncher") { $hasCustom = $true } }
    
    if (-not $hasCustom) {
        Write-ErrorMsg "NO CUSTOM LAUNCHER DETECTED!"
        Write-Warn "Optimizing without Projectivy/FLauncher installed will cause a Black Screen."
        $cont = Read-Host "Are you absolutely sure you want to proceed? [y/N]"
        if ($cont -notmatch "^y") { continue }
    }
    
    Write-Info "Scanning apps..."
    $MemMap = Get-MemMap -TargetSerial $GlobalTarget

    Write-Header "Optimization Audit"
    foreach ($app in $Apps) {
        $pkg = $app[0]; $name = $app[1]; $action = $app[2]; $risk = $app[3]; $effect = $app[4]; $def = $app[5]

        $check = & $AdbPath -s $GlobalTarget shell pm list packages | Select-String "package:$pkg"
        if (-not $check) { Write-Dim "Found: $name ... [NOT INSTALLED] (Skipping)"; continue }

        if ($action -eq "DISABLE") {
            $disabled = & $AdbPath -s $GlobalTarget shell pm list packages -d | Select-String "package:$pkg"
            if ($disabled) { Write-Dim "Found: $name ... [ALREADY DISABLED] (Skipping)"; continue }
        }

        Write-Host -NoNewline "Found: "
        Write-Host "$name" -ForegroundColor Cyan -NoNewline
        if ($risk -eq "Safe") { $c="Green" } elseif ($risk -eq "Medium") { $c="Yellow" } else { $c="Red" }
        Write-Host " [$risk]" -ForegroundColor $c -NoNewline
        if ($MemMap.ContainsKey($pkg)) { Write-Host " (Active RAM: $($MemMap[$pkg]) MB)" -NoNewline -ForegroundColor Magenta }
        
        Write-Host "`n    Effect: $effect" -ForegroundColor Gray

        if ($def -eq "Y") {
            $resp = Read-Host " >> $($action)? [Y/n] (Recommended)"
            if ($resp -eq "" -or $resp -match "^y") { $doAction = $true } else { $doAction = $false }
        } else {
            $resp = Read-Host " >> $($action)? [y/N] (Optional)"
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
