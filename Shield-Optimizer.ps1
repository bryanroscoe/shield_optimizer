<#
.SYNOPSIS
    Nvidia Shield Ultimate Optimizer (v23 - Precision)
.DESCRIPTION
    Fixes memory over-reporting (PSS vs RSS).
    Scans multiple thermal zones for temperature.
    Reduced boot wait time.
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

# --- 2. DEVICE SELECTION ---
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
    }
    return $devices
}

Write-Header "Device Selection"
$shields = @(Get-ConnectedShields)

# Network Connect Fallback
if ($shields.Count -eq 0) {
    Write-Warn "No USB devices found."
    Write-Host "If your Shield is on WiFi, enter the IP address below."
    $ip = Read-Host " >> IP Address (e.g. 192.168.1.5)"
    if ($ip -ne "") {
        Write-Info "Attempting to connect to $ip..."
        & $AdbPath connect $ip
        Start-Sleep -s 2
        $shields = @(Get-ConnectedShields)
    }
}

if ($shields.Count -eq 0) { Write-ErrorMsg "Still no devices found. Check IP/Network."; Pause; Exit }

foreach ($dev in $shields) {
    if ($dev.Status -eq "Ready") { Write-Host " [$($dev.ID)] $($dev.Name) ($($dev.Serial))" -ForegroundColor Green }
    else { Write-Host " [$($dev.ID)] $($dev.Serial) - $($dev.Status)" -ForegroundColor Red }
}

$selection = Read-Host "`nEnter Device Number [1-$($shields.Count)]"
$target = $shields | Where-Object { $_.ID -eq $selection }
if (-not $target) { Write-ErrorMsg "Invalid selection."; Exit }
if ($target.Status -ne "Ready") { Write-ErrorMsg "Device Unauthorized."; Pause; Exit }
$T = $target.Serial

# --- 3. DIAGNOSTIC REPORT ---
function Generate-Report {
    Write-Header "GENERATING DIAGNOSTIC REPORT"
    Write-Info "Gathering telemetry..."

    # --- SENSORS (MULTI-ZONE SCAN) ---
    $Temp = "N/A"
    # Scan thermal zones 0 through 9
    for ($i=0; $i -lt 10; $i++) {
        $val = & $AdbPath -s $T shell cat /sys/class/thermal/thermal_zone$i/temp 2>$null
        # Filter for valid readings (Shields usually report 30000-80000 range)
        if ($val -match "^\d+$" -and [int]$val -gt 20000 -and [int]$val -lt 100000) {
            $Temp = [math]::Round($val / 1000, 1)
            break # Stop at first valid sensor
        }
    }

    # --- MEMORY (PSS PARSER) ---
    $rawMem = & $AdbPath -s $T shell dumpsys meminfo
    $FreeRAM = 0; $SwapUsed = 0; $TotalRAM = 3072
    $RawApps = @()
    $ParsingPSS = $false

    foreach ($line in $rawMem) {
        # Global Stats
        if ($line -match "Total RAM:\s+([0-9,]+)K") { $TotalRAM = [math]::Round(($matches[1] -replace ",", "") / 1024, 0) }
        if ($line -match "Free RAM:\s+([0-9,]+)K") { $FreeRAM = [math]::Round(($matches[1] -replace ",", "") / 1024, 0) }
        if ($line -match "ZRAM:.*used for\s+([0-9,]+)K") { $SwapUsed = [math]::Round(($matches[1] -replace ",", "") / 1024, 0) }
        
        # State Machine: Only start parsing apps when we hit the PSS section
        if ($line -match "Total PSS by process") { $ParsingPSS = $true; continue }
        if ($line -match "Total PSS by OOM adjustment") { $ParsingPSS = $false; continue } # Stop parsing

        if ($ParsingPSS -and $line -match "^\s*([0-9,]+)K:\s+([a-zA-Z0-9_\.\@\-]+).*\(pid") {
            $kb = $matches[1] -replace ",", ""
            $mb = [math]::Round($kb / 1024, 0)
            $pkg = $matches[2]
            
            # Filter garbage categories
            if ($pkg -notmatch "^\." -and $pkg -notmatch "^(System|Persistent|Cached|Native|Unknown|Dalvik|Stack|Ashmem|Perceptible)$") {
                $RawApps += [PSCustomObject]@{ Name = $pkg; MB = $mb }
            }
        }
    }
    
    # Group by Package Name (Sum PSS)
    $GroupedApps = $RawApps | Group-Object Name | Select-Object Name, @{N='MB'; E={ ($_.Group | Measure-Object MB -Sum).Sum }}
    $TopApps = $GroupedApps | Sort-Object MB -Descending | Select-Object -First 10

    $UsedRAM = $TotalRAM - $FreeRAM
    $UsedPercent = [math]::Round(($UsedRAM / $TotalRAM) * 100, 0)
    
    # --- OS STATS ---
    $anim = (& $AdbPath -s $T shell settings get global window_animation_scale | Out-String).Trim()
    $uptime = (& $AdbPath -s $T shell uptime | Out-String).Trim()
    $build = (& $AdbPath -s $T shell getprop ro.build.display.id | Out-String).Trim()

    # --- RENDER REPORT ---
    Write-SubHeader "DEVICE HEALTH"
    Write-Host " Device:    $($target.Name) ($T)"
    Write-Host " OS Build:  $build"
    Write-Host " Uptime:    $uptime"
    Write-Host " Temp:      " -NoNewline
    if ($Temp -eq "N/A") { Write-Host "Unknown (Sensor Unreadable)" -ForegroundColor Gray }
    elseif ($Temp -lt 65) { Write-Host "$Temp C (Cool)" -ForegroundColor Green } 
    else { Write-Host "$Temp C (Warm)" -ForegroundColor Yellow }

    Write-SubHeader "MEMORY & PERFORMANCE"
    Write-Host " RAM Load:  " -NoNewline
    if ($UsedPercent -gt 85) { Write-Host "$UsedPercent% (CRITICAL)" -ForegroundColor Red } elseif ($UsedPercent -gt 65) { Write-Host "$UsedPercent% (High)" -ForegroundColor Yellow } else { Write-Host "$UsedPercent% (Good)" -ForegroundColor Green }
    Write-Host " ($UsedRAM MB / $TotalRAM MB)" -ForegroundColor Gray
    
    Write-Host " Swap Usage:" -NoNewline
    if ($SwapUsed -lt 10) { Write-Host " $SwapUsed MB (Clean)" -ForegroundColor Green } else { Write-Host " $SwapUsed MB (Thrashing)" -ForegroundColor Red }
    
    Write-Host " Animation: " -NoNewline
    if ($anim -eq "0.5") { Write-Host "${anim}x (Optimized)" -ForegroundColor Green } else { Write-Host "${anim}x (Stock)" -ForegroundColor Yellow }

    Write-SubHeader "TOP 10 MEMORY CONSUMERS (PSS)"
    foreach ($app in $TopApps) {
        $pct = [math]::Round(($app.MB / $TotalRAM) * 100, 1)
        Write-Host " $($app.MB) MB" -NoNewline -ForegroundColor Magenta
        Write-Host " ($pct%) - $($app.Name)"
    }
    echo "`n"
}

# --- 4. PRE-SCAN ---
Write-Info "Initializing..."
$MemMap = @{}
$rawMem = & $AdbPath -s $T shell dumpsys meminfo
foreach ($line in $rawMem) {
    # Scan for PSS only
    if ($line -match "Total PSS by process") { $ParsingPSS = $true; continue }
    if ($line -match "Total PSS by OOM adjustment") { $ParsingPSS = $false; continue }
    if ($ParsingPSS -and $line -match "^\s*([0-9,]+)K:\s+([a-zA-Z0-9_\.\@\-]+).*\(pid") {
        $kb = $matches[1] -replace ",", ""
        $mb = [math]::Round($kb / 1024, 0)
        $p = $matches[2]; if (-not $MemMap.ContainsKey($p)) { $MemMap[$p] = $mb }
    }
}

# --- 5. MENU ---
Write-Header "Main Menu"
Write-Host " [1] OPTIMIZE (Debloat + Speed Up)" -ForegroundColor Green
Write-Host " [2] RESTORE (Factory Defaults)" -ForegroundColor Yellow
Write-Host " [3] REPORT (Diagnostics Only)" -ForegroundColor Cyan
$mode = Read-Host "`nSelect Option [1-3] (Default: 1)"

# FORMAT: Package, Name, Action, Risk
$Apps = @(
    @("com.google.android.katniss", "Google Assistant", "DISABLE", "Safe"),
    @("com.google.android.tvrecommendations", "Google TV Recommendations", "DISABLE", "Safe"),
    @("com.google.android.tts", "Google Text-to-Speech", "DISABLE", "Safe"),
    @("com.nvidia.stats", "Nvidia Telemetry/Stats", "DISABLE", "Safe"),
    @("com.google.android.feedback", "Google Feedback Agent", "DISABLE", "Safe"),
    @("com.nvidia.diagtools", "Nvidia Diag Tools", "DISABLE", "Safe"),
    @("com.google.android.tvlauncher", "Stock Android Launcher", "DISABLE", "Medium"),
    @("com.nvidia.ota", "Nvidia System Updater", "DISABLE", "Medium"),
    @("com.plexapp.mediaserver.smb", "Plex Media SERVER", "DISABLE", "Advanced"),
    @("com.google.android.apps.mediashell", "Chromecast Built-in", "UNINSTALL", "Safe"),
    @("com.netflix.ninja", "Netflix", "UNINSTALL", "Safe"),
    @("com.amazon.amazonvideo.livingroom", "Amazon Prime Video", "UNINSTALL", "Safe"),
    @("com.google.android.videos", "Google Play Movies/TV", "UNINSTALL", "Safe"),
    @("com.google.android.music", "Google Play Music", "UNINSTALL", "Safe"),
    @("com.wbd.stream", "HBO Max / Discovery", "UNINSTALL", "Safe"),
    @("com.hulu.livingroomplus", "Hulu", "UNINSTALL", "Safe"),
    @("tv.twitch.android.app", "Twitch", "UNINSTALL", "Safe"),
    @("com.disney.disneyplus.prod", "Disney+", "UNINSTALL", "Safe"),
    @("com.spotify.tv.android", "Spotify", "UNINSTALL", "Safe"),
    @("com.amazon.venezia", "Amazon App Store", "UNINSTALL", "Safe")
)

# === MODES ===
if ($mode -eq "3") { Generate-Report; Pause; Exit }

if ($mode -eq "2") {
    Write-Header "Restoring Factory Defaults"
    foreach ($app in $Apps) {
        Write-Host "Restoring $($app[1])..." -NoNewline
        & $AdbPath -s $T shell cmd package install-existing $app[0] 2>$null | Out-Null
        & $AdbPath -s $T shell pm enable $app[0] 2>$null | Out-Null
        Write-Host " [DONE]" -ForegroundColor Green
    }
    & $AdbPath -s $T shell settings put global window_animation_scale 1.0
    & $AdbPath -s $T shell settings put global transition_animation_scale 1.0
    & $AdbPath -s $T shell settings put global animator_duration_scale 1.0
    Write-Success "Restored. Rebooting..."
    & $AdbPath -s $T reboot; Pause; Exit
}

# OPTIMIZE MODE
$launcher = & $AdbPath -s $T shell pm list packages | Select-String "com.spocky.projengmenu"
if (-not $launcher) { Write-ErrorMsg "Projectivy Launcher NOT FOUND. Aborting."; Exit }

Write-Header "Optimization Audit (Enter = YES)"
foreach ($app in $Apps) {
    $pkg = $app[0]; $name = $app[1]; $action = $app[2]; $risk = $app[3]

    $check = & $AdbPath -s $T shell pm list packages | Select-String "package:$pkg"
    if (-not $check) { continue } # Skip missing

    if ($action -eq "DISABLE") {
        $disabled = & $AdbPath -s $T shell pm list packages -d | Select-String "package:$pkg"
        if ($disabled) { continue } # Skip already disabled
    }

    Write-Host -NoNewline "Found: "
    Write-Host "$name" -ForegroundColor Cyan -NoNewline
    if ($risk -eq "Safe") { $c="Green" } elseif ($risk -eq "Medium") { $c="Yellow" } else { $c="Red" }
    Write-Host " [$risk]" -ForegroundColor $c -NoNewline
    if ($MemMap.ContainsKey($pkg)) { Write-Host " (Using: $($MemMap[$pkg]) MB)" -NoNewline -ForegroundColor Magenta }

    $resp = Read-Host " >> $($action)? [Y/n]"
    if ($resp -eq "" -or $resp -match "^y") {
        if ($action -eq "UNINSTALL") { & $AdbPath -s $T shell pm uninstall --user 0 $pkg | Out-Null }
        else { & $AdbPath -s $T shell pm disable-user --user 0 $pkg | Out-Null }
        Write-Success "Processed."
    }
}

Write-Header "Performance"
$cur = (& $AdbPath -s $T shell settings get global window_animation_scale | Out-String).Trim()
if ($cur -ne "0.5") {
    $resp = Read-Host "Set Animations to 0.5x? [Y/n]"
    if ($resp -eq "" -or $resp -match "^y") {
        & $AdbPath -s $T shell settings put global window_animation_scale 0.5
        & $AdbPath -s $T shell settings put global transition_animation_scale 0.5
        & $AdbPath -s $T shell settings put global animator_duration_scale 0.5
        Write-Success "Applied."
    }
}

Write-Header "Finished"
$reboot = Read-Host "Reboot now to apply changes? [Y/n]"
if ($reboot -eq "" -or $reboot -match "^y") {
    Write-Info "Rebooting..."
    & $AdbPath -s $T reboot
    
    Write-Header "Waiting for Device..."
    $retries=0
    do { Start-Sleep -s 5; $s = & $AdbPath -s $T get-state 2>$null; Write-Host "." -NoNewline -ForegroundColor DarkGray; $retries++ } while (($s -ne "device") -and ($retries -lt 30))
    
    if ($s -eq "device") { 
        Write-Success "`nDevice Online."
        Write-Info "Waiting 30 seconds for OS to settle..."
        Start-Sleep -s 30
        Generate-Report 
    }
}
Pause