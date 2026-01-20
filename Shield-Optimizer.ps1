param(
    [switch]$Demo,
    [switch]$ForceAdbDownload
)

# Set script-level flag for ForceAdbDownload
$Script:ForceAdbDownload = $ForceAdbDownload

<#
.SYNOPSIS
    Android TV Optimizer (v62 - Multi-Device Support)
.DESCRIPTION
    Supports Nvidia Shield TV, Onn 4K Pro, Chromecast with Google TV, and other Android TV devices.

    v62 Changes:
    - Added device detection (Shield vs Google TV)
    - Device-specific app lists and optimizations
    - Improved launcher detection for Google TV devices
    - Added device profile display

    Previous fixes:
    - Socket leak fix, ADB output trimming, IP validation
    - Cursor/window error handling, ESC key support
    - Apply All Defaults, session summary, abort functionality
    - Keyboard shortcuts, colored status tags
#>

$Script:Version = "v62-beta"
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# --- CONFIGURATION & DATA MODELS ---

# Device type enum
$Script:DeviceType = @{
    Shield = "Shield"
    GoogleTV = "GoogleTV"
    Unknown = "Unknown"
}

# ============================================================================
# APP LISTS - Format: Package, Name, Method, Risk, OptDesc, RestDesc, DefOpt, DefRest
# ============================================================================

# Apps common to ALL Android TV devices
$Script:CommonAppList = @(
    # [SAFE - Universal]
    @("com.google.android.tvrecommendations", "Sponsored Content", "DISABLE", "Safe", "Removes 'Sponsored' rows from home.", "Restores sponsored content rows.", "Y", "Y"),
    @("com.google.android.feedback", "Google Feedback", "DISABLE", "Safe", "Stops feedback data collection.", "Restores Google feedback services.", "Y", "Y"),
    @("com.android.printspooler", "Print Spooler", "DISABLE", "Safe", "Disables unused print service.", "Restores print service.", "Y", "Y"),
    @("com.android.gallery3d", "Android Gallery", "DISABLE", "Safe", "Removes legacy photo viewer.", "Restores legacy photo viewer.", "Y", "Y"),

    # [DEAD APPS - Universal]
    @("com.google.android.videos", "Google Play Movies", "UNINSTALL", "Safe", "Removes defunct app.", "Restores defunct app.", "Y", "Y"),
    @("com.google.android.music", "Google Play Music", "UNINSTALL", "Safe", "Removes defunct app.", "Restores defunct app.", "Y", "Y"),

    # [STREAMING APPS - Optional]
    @("com.netflix.ninja", "Netflix", "UNINSTALL", "Safe", "Streaming App.", "Restores Netflix.", "N", "N"),
    @("com.amazon.amazonvideo.livingroom", "Amazon Prime Video", "UNINSTALL", "Safe", "Streaming App.", "Restores Prime Video.", "N", "N"),
    @("com.wbd.stream", "HBO Max / Discovery", "UNINSTALL", "Safe", "Streaming App.", "Restores HBO/Max.", "N", "N"),
    @("com.hulu.livingroomplus", "Hulu", "UNINSTALL", "Safe", "Streaming App.", "Restores Hulu.", "N", "N"),
    @("tv.twitch.android.app", "Twitch", "UNINSTALL", "Safe", "Streaming App.", "Restores Twitch.", "N", "N"),
    @("com.disney.disneyplus.prod", "Disney+", "UNINSTALL", "Safe", "Streaming App.", "Restores Disney+.", "N", "N"),
    @("com.spotify.tv.android", "Spotify", "UNINSTALL", "Safe", "Streaming App.", "Restores Spotify.", "N", "N"),
    @("com.google.android.youtube.tvmusic", "YouTube Music", "UNINSTALL", "Safe", "Streaming App.", "Restores YouTube Music.", "N", "N"),

    # [MEDIUM RISK - Universal]
    @("com.android.dreams.basic", "Basic Daydream", "DISABLE", "Medium", "Disables basic screensaver.", "Restores basic screensaver.", "N", "Y"),
    @("com.android.providers.tv", "Live Channels Provider", "DISABLE", "Medium", "Disables Live TV provider.", "Restores Live Channels support.", "N", "Y"),
    @("com.google.android.backdrop", "Ambient Mode", "DISABLE", "Medium", "Disables ambient/screensaver.", "Restores ambient mode.", "N", "Y"),

    # [HIGH RISK - Universal]
    @("com.google.android.katniss", "Google Assistant", "DISABLE", "High Risk", "Breaks voice search.", "Restores voice search.", "N", "Y"),
    @("com.google.android.speech.pumpkin", "Google Speech Services", "DISABLE", "High Risk", "Breaks voice dictation.", "Restores speech services.", "N", "Y"),
    @("com.google.android.apps.mediashell", "Chromecast Built-in", "DISABLE", "High Risk", "Breaks casting to device.", "Restores Chromecast.", "N", "Y"),
    @("com.google.android.tts", "Google Text-to-Speech", "DISABLE", "High Risk", "Breaks accessibility features.", "Restores text-to-speech.", "N", "Y"),
    @("com.google.android.play.games", "Google Play Games", "DISABLE", "Medium Risk", "May break cloud saves.", "Restores Play Games.", "N", "Y")
)

# NVIDIA Shield-specific apps
$Script:ShieldAppList = @(
    # [SHIELD SAFE]
    @("com.nvidia.stats", "Nvidia Telemetry", "DISABLE", "Safe", "Stops Nvidia data collection.", "Restores Nvidia telemetry.", "Y", "Y"),
    @("com.nvidia.diagtools", "Nvidia Diagnostics", "DISABLE", "Safe", "Stops diagnostic logging.", "Restores diagnostic tools.", "Y", "Y"),

    # [SHIELD MEDIUM]
    @("com.nvidia.ocs", "Nvidia Content Service", "DISABLE", "Medium", "Background optimization service.", "Restores content optimization.", "N", "Y"),
    @("com.nvidia.shieldtech.hooks", "Nvidia System Hooks", "DISABLE", "Medium", "Shield-specific system hooks.", "Restores system hooks.", "N", "Y"),
    @("com.nvidia.tegrazone3", "Nvidia Games", "DISABLE", "Medium Risk", "May break GeForce NOW.", "Restores Nvidia Games app.", "N", "Y"),

    # [SHIELD HIGH RISK]
    @("com.nvidia.ota", "Nvidia System Updater", "DISABLE", "High Risk", "Stops Shield OS updates.", "Restores system updates.", "N", "Y"),
    @("com.plexapp.mediaserver.smb", "Plex Media Server", "DISABLE", "Advanced", "Breaks local Plex hosting.", "Restores Plex Server.", "N", "Y"),

    # [SHIELD LAUNCHER]
    @("com.google.android.tvlauncher", "Stock Launcher", "DISABLE", "High Risk", "Requires custom launcher first!", "Restores stock home screen.", "N", "Y")
)

# Google TV-specific apps (Onn 4K, Chromecast, etc.)
$Script:GoogleTVAppList = @(
    # [GOOGLE TV SAFE]
    @("com.google.android.leanbacklauncher.recommendations", "Home Recommendations", "DISABLE", "Safe", "Removes extra recommendation rows.", "Restores home recommendations.", "Y", "Y"),
    @("com.walmart.otto", "Walmart App", "UNINSTALL", "Safe", "Removes Walmart bloatware.", "Restores Walmart app.", "Y", "Y"),

    # [GOOGLE TV MEDIUM]
    @("com.google.android.tungsten.overscan", "Setup Wizard Helper", "DISABLE", "Medium", "Post-setup service.", "Restores setup components.", "N", "Y"),

    # [ONN-SPECIFIC - Amlogic devices]
    @("com.droidlogic.launcher.provider", "Droidlogic Launcher Provider", "DISABLE", "Medium", "Onn launcher data provider. Disable with launcher.", "Restores Onn launcher provider.", "N", "Y"),

    # [GOOGLE TV HOME HANDLERS - Must be disabled together via Launcher Wizard]
    # WARNING: These declare HOME intent - disabling only one causes loops!
    # Use Launcher Wizard which handles all HOME handlers automatically
    @("com.google.android.tungsten.setupwraith", "Setup Wraith (HOME)", "DISABLE", "High Risk", "HOME handler! Use Launcher Wizard instead.", "Restores setup wizard HOME handler.", "N", "Y"),

    # [GOOGLE TV LAUNCHER - Handle via Launcher Wizard]
    @("com.google.android.apps.tv.launcherx", "Google TV Home", "DISABLE", "High Risk", "Use Launcher Wizard to safely disable!", "Restores Google TV home.", "N", "Y")
)

$Script:PerfList = @(
    @{Key="window_animation_scale"; Name="Animation Speed"; Desc="Controls UI transition speed."; OptVal="0.5"; RestVal="1.0"},
    @{Key="background_process_limit"; Name="Background Process Limit"; Desc="Limits multitasking to free RAM."; OptVal="2"; RestVal="Standard"}
)

$Script:Launchers = @(
    @{Name="Projectivy Launcher"; Pkg="com.spocky.projengmenu"},
    @{Name="FLauncher"; Pkg="me.efesser.flauncher"},
    @{Name="ATV Launcher"; Pkg="com.sweech.launcher"},
    @{Name="Wolf Launcher"; Pkg="com.wolf.firelauncher"}
)

# --- DEVICE DETECTION ---

# Detect device type based on brand and model
function Get-DeviceType ($Target) {
    try {
        $brand = (& $Script:AdbPath -s $Target shell getprop ro.product.brand 2>&1 | Out-String).Trim().ToLower()
        $model = (& $Script:AdbPath -s $Target shell getprop ro.product.model 2>&1 | Out-String).Trim().ToLower()
        $device = (& $Script:AdbPath -s $Target shell getprop ro.product.device 2>&1 | Out-String).Trim().ToLower()

        # NVIDIA Shield detection
        if ($brand -eq "nvidia" -or $model -match "shield" -or $device -match "foster|darcy|mdarcy|sif") {
            return $Script:DeviceType.Shield
        }

        # Onn (Walmart) detection
        if ($brand -eq "onn" -or $model -match "onn" -or $device -match "ott_") {
            return $Script:DeviceType.GoogleTV
        }

        # Chromecast / Google TV detection
        if ($brand -eq "google" -or $model -match "chromecast|sabrina|boreal") {
            return $Script:DeviceType.GoogleTV
        }

        # Generic Google TV detection - check for launcherx
        $hasLauncherX = & $Script:AdbPath -s $Target shell pm list packages com.google.android.apps.tv.launcherx 2>&1 | Out-String
        if ($hasLauncherX -match "launcherx") {
            return $Script:DeviceType.GoogleTV
        }

        # Default to Unknown
        return $Script:DeviceType.Unknown
    }
    catch {
        return $Script:DeviceType.Unknown
    }
}

# Get device-friendly name
function Get-DeviceTypeName ($Type) {
    switch ($Type) {
        "Shield"   { return "Nvidia Shield" }
        "GoogleTV" { return "Google TV" }
        default    { return "Android TV" }
    }
}

# Get combined app list for device type
function Get-AppListForDevice ($Type) {
    $combined = @()

    # Add common apps first
    $combined += $Script:CommonAppList

    # Add device-specific apps
    switch ($Type) {
        "Shield" {
            $combined += $Script:ShieldAppList
        }
        "GoogleTV" {
            $combined += $Script:GoogleTVAppList
        }
        default {
            # For unknown devices, include both but skip the launchers
            foreach ($app in $Script:ShieldAppList) {
                if ($app[0] -notmatch "tvlauncher") { $combined += ,$app }
            }
            foreach ($app in $Script:GoogleTVAppList) {
                if ($app[0] -notmatch "launcherx") { $combined += ,$app }
            }
        }
    }

    return $combined
}

# Show device profile information
function Show-DeviceProfile ($Target, $DeviceInfo) {
    Write-Header "Device Profile"

    $typeName = Get-DeviceTypeName $DeviceInfo.Type
    Write-Host " Device:  " -NoNewline -ForegroundColor Gray
    Write-Host "$($DeviceInfo.Name)" -ForegroundColor Cyan

    Write-Host " Model:   " -NoNewline -ForegroundColor Gray
    Write-Host "$($DeviceInfo.Model)" -ForegroundColor White

    Write-Host " Profile: " -NoNewline -ForegroundColor Gray
    Write-Host "$typeName" -ForegroundColor Yellow

    Write-Host " Serial:  " -NoNewline -ForegroundColor Gray
    Write-Host "$($DeviceInfo.Serial)" -ForegroundColor DarkGray

    # Get Android version
    try {
        $androidVer = (& $Script:AdbPath -s $Target shell getprop ro.build.version.release 2>&1 | Out-String).Trim()
        Write-Host " Android: " -NoNewline -ForegroundColor Gray
        Write-Host "$androidVer" -ForegroundColor White
    } catch {}

    # Show which app list will be used
    $appList = Get-AppListForDevice $DeviceInfo.Type
    Write-Host " Apps:    " -NoNewline -ForegroundColor Gray
    Write-Host "$($appList.Count) apps in optimization list" -ForegroundColor Green

    Write-Host ""
}

# --- UTILITY FUNCTIONS ---

function Write-Header ($Text)   { Write-Host "`n=== $Text ===" -ForegroundColor Cyan }
function Write-SubHeader ($Text){ Write-Host "`n--- $Text ---" -ForegroundColor DarkCyan }
function Write-Success ($Text)  { Write-Host " [OK] $Text" -ForegroundColor Green }
function Write-Warn ($Text)     { Write-Host " [!!] $Text" -ForegroundColor Yellow }
function Write-ErrorMsg ($Text) { Write-Host " [ERROR] $Text" -ForegroundColor Red }
function Write-Info ($Text)     { Write-Host " [INFO] $Text" -ForegroundColor Gray }
function Write-Dim ($Text)      { Write-Host " $Text" -ForegroundColor DarkGray }
function Write-Separator        { Write-Host "`n────────────────────────────────────────────────────────────────────────────────`n" -ForegroundColor DarkGray }

# --- DEMO MODE ---
function Show-DemoScreens {
    Clear-Host
    Write-Host "DEMO MODE - Screenshot Gallery" -ForegroundColor Magenta
    Write-Host "All screens displayed statically for screenshot capture.`n" -ForegroundColor DarkGray

    # ============================================================
    # SCREEN 1: Main Menu with 2 fake devices
    # ============================================================
    Write-Separator
    Write-Host " Android TV Optimizer $Script:Version - Main Menu" -ForegroundColor Cyan
    Write-Host " ================================================" -ForegroundColor DarkCyan

    # Fake devices
    Write-Host "  > " -NoNewline -ForegroundColor Cyan
    Write-OptionWithHighlight -Text "[1] Shield TV Pro" -Selected $true
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[2] Living Room TV" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[S]can Network" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[C]onnect IP" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[R]eport All" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "Re[f]resh" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "Restart [A]DB" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[H]elp" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[Q]uit" -Selected $false

    Write-Host " ================================================" -ForegroundColor DarkCyan
    Write-Host " Info: " -NoNewline -ForegroundColor Yellow
    Write-Host "Nvidia Shield | Shield TV Pro (2019) | 192.168.1.100:5555" -ForegroundColor White
    Write-Host " [Arrows: Move] [Keys: Select] [Enter: OK] [ESC: Back]" -ForegroundColor DarkGray

    # ============================================================
    # SCREEN 2: Action Menu
    # ============================================================
    Write-Separator
    Write-Host " Action Menu: Shield TV Pro (Nvidia Shield)" -ForegroundColor Cyan
    Write-Host " ================================================" -ForegroundColor DarkCyan

    Write-Host "  > " -NoNewline -ForegroundColor Cyan
    Write-OptionWithHighlight -Text "[O]ptimize" -Selected $true
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[R]estore" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "R[e]port" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[L]auncher Setup" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[P]rofile" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "Re[c]overy" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "Re[b]oot" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[D]isconnect" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "Bac[k]" -Selected $false

    Write-Host " ================================================" -ForegroundColor DarkCyan
    Write-Host " Info: " -NoNewline -ForegroundColor Yellow
    Write-Host "Debloat apps and tune performance for Nvidia Shield." -ForegroundColor White

    # ============================================================
    # SCREEN 3: Health Report
    # ============================================================
    Write-Separator
    Write-Header "Health Report: Shield TV Pro (Nvidia Shield)"

    Write-SubHeader "System Info"
    Write-Host " Platform:  " -NoNewline -ForegroundColor Gray
    Write-Host "tegra" -ForegroundColor Cyan
    Write-Host " Android:   " -NoNewline -ForegroundColor Gray
    Write-Host "11" -ForegroundColor White

    Write-SubHeader "Vitals"
    Write-Host " Temp:    " -NoNewline -ForegroundColor Gray
    Write-Host "42.3`°C" -ForegroundColor Green
    Write-Host " RAM:     " -NoNewline -ForegroundColor Gray
    Write-Host "68% (1890 / 2780 MB)" -ForegroundColor Yellow
    Write-Host " Swap:    " -NoNewline -ForegroundColor Gray
    Write-Host "128 MB" -ForegroundColor White
    Write-Host " Storage: " -NoNewline -ForegroundColor Gray
    Write-Host "8.2G / 14G (59%)" -ForegroundColor Green

    Write-SubHeader "Settings Check"
    Write-Host " Animation Speed: " -NoNewline -ForegroundColor Gray
    Write-Host "0.5" -ForegroundColor Cyan
    Write-Host " Process Limit:   " -NoNewline -ForegroundColor Gray
    Write-Host "2" -ForegroundColor Cyan

    Write-SubHeader "Bloat Check"
    Write-Host " [ACTIVE BLOAT] Sponsored Content" -ForegroundColor Yellow
    Write-Host " [ACTIVE BLOAT] Google Feedback" -ForegroundColor Yellow

    # ============================================================
    # SCREEN 4: Launcher Wizard
    # ============================================================
    Write-Separator
    Write-Host " Select Launcher" -ForegroundColor Cyan
    Write-Host " ================================================" -ForegroundColor DarkCyan

    Write-Host "  > " -NoNewline -ForegroundColor Cyan
    Write-OptionWithHighlight -Text "[P]rojectivy Launcher [ACTIVE]" -Selected $true
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[F]Launcher [INSTALLED]" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[A]TV Launcher [MISSING]" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[W]olf Launcher [MISSING]" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[S]tock Launcher (Google TV) [DISABLED]" -Selected $false
    Write-Host "    " -NoNewline
    Write-OptionWithHighlight -Text "[B]ack" -Selected $false

    Write-Host " ================================================" -ForegroundColor DarkCyan
    Write-Host " Info: " -NoNewline -ForegroundColor Yellow
    Write-Host "Install or Enable Projectivy Launcher" -ForegroundColor White

    # ============================================================
    # SCREEN 5: Optimize Flow - Sample App
    # ============================================================
    Write-Separator
    Write-Header "Application Management (Optimize) - Nvidia Shield"
    Write-Host ""
    Write-Host "Remove: " -NoNewline
    Write-Host "Sponsored Content" -ForegroundColor Cyan -NoNewline
    Write-Host " [Safe]" -ForegroundColor Green
    Write-Dim "    Removes 'Sponsored' rows from home."
    Write-Host "    >> Action:  " -NoNewline -ForegroundColor Gray
    Write-Host " [ DISABLE ] " -NoNewline -ForegroundColor Cyan
    Write-Host "   UNINSTALL   " -NoNewline -ForegroundColor DarkGray
    Write-Host "   SKIP   " -NoNewline -ForegroundColor DarkGray
    Write-Host "   ABORT   " -ForegroundColor DarkGray
    Write-Host ""
    Write-Dim "Google Play Movies ... [NOT INSTALLED]"
    Write-Dim "Google Play Music ... [ALREADY DISABLED]"
    Write-Host ""
    Write-Host "Remove: " -NoNewline
    Write-Host "Nvidia Telemetry" -ForegroundColor Cyan -NoNewline
    Write-Host " [Safe]" -ForegroundColor Green
    Write-Dim "    Stops Nvidia data collection."
    Write-Host "    >> Action:  " -NoNewline -ForegroundColor Gray
    Write-Host " [ DISABLE ] " -NoNewline -ForegroundColor Cyan
    Write-Host "   UNINSTALL   " -NoNewline -ForegroundColor DarkGray
    Write-Host "   SKIP   " -NoNewline -ForegroundColor DarkGray
    Write-Host "   ABORT   " -ForegroundColor DarkGray

    # ============================================================
    # SCREEN 6: Summary Screen
    # ============================================================
    Write-Separator
    Write-Header "Summary"
    Write-Host " Disabled:    8 apps" -ForegroundColor Green
    Write-Host " Uninstalled: 2 apps" -ForegroundColor Green
    Write-Host " Skipped:     5 apps" -ForegroundColor Gray
    Write-Host " Failed:      0 apps" -ForegroundColor Gray

    Write-Header "Finished"
    Write-Host "Reboot Device Now?  " -NoNewline -ForegroundColor Gray
    Write-Host "   YES   " -NoNewline -ForegroundColor DarkGray
    Write-Host " [ NO ] " -ForegroundColor Cyan

    Write-Separator
    Write-Host "END OF DEMO" -ForegroundColor Magenta
    Write-Host ""
}

# FIX #10: Helper function to execute ADB commands with proper error checking
function Invoke-AdbCommand {
    param(
        [string]$Target,
        [string]$Command,
        [switch]$Silent
    )

    try {
        $output = & $Script:AdbPath -s $Target shell $Command 2>&1
        $outputStr = ($output | Out-String).Trim()

        # Check for common ADB error patterns
        $hasError = $outputStr -match "Exception|Error|Failure|failed|not found|inaccessible"

        if ($hasError -and -not $Silent) {
            return @{ Success = $false; Output = $outputStr; Error = $outputStr }
        }

        return @{ Success = $true; Output = $outputStr; Error = $null }
    }
    catch {
        return @{ Success = $false; Output = ""; Error = $_.Exception.Message }
    }
}

# FIX #8: $PSScriptRoot fallback when dot-sourced
function Get-ScriptDirectory {
    if ($PSScriptRoot -and $PSScriptRoot -ne "") {
        return $PSScriptRoot
    }
    return (Get-Location).Path
}

function Check-Adb {
    $ScriptDir = Get-ScriptDirectory
    $AdbExe = "$ScriptDir\adb.exe"
    $Script:AdbPath = ""

    # Force download if flag is set - skip all checks and download fresh
    if ($Script:ForceAdbDownload) {
        Write-Warn "Forcing ADB re-download..."
        # Kill ADB server first so files aren't locked
        if (Test-Path $AdbExe) {
            try { & $AdbExe kill-server 2>$null } catch {}
        }
        try { Stop-Process -Name "adb" -Force -ErrorAction SilentlyContinue } catch {}
        Start-Sleep -Milliseconds 500
        Remove-Item $AdbExe -Force -ErrorAction SilentlyContinue
        Remove-Item "$ScriptDir\AdbWinApi.dll" -Force -ErrorAction SilentlyContinue
        Remove-Item "$ScriptDir\AdbWinUsbApi.dll" -Force -ErrorAction SilentlyContinue
        # Fall through to download section
    }
    elseif (Test-Path $AdbExe) {
        $Script:AdbPath = $AdbExe
    }
    elseif (Get-Command "adb.exe" -ErrorAction SilentlyContinue) {
        $Script:AdbPath = (Get-Command "adb.exe").Source
    }

    if ($Script:AdbPath -eq "") {
        if (-not $Script:ForceAdbDownload) { Write-Warn "ADB missing. Downloading..." }
        $Url = "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"
        $Zip = "$ScriptDir\adb_temp.zip"; $Ext = "$ScriptDir\adb_temp_extract"
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -Uri $Url -OutFile $Zip -UseBasicParsing
            Expand-Archive -Path $Zip -DestinationPath $Ext -Force
            Move-Item "$Ext\platform-tools\adb.exe" $ScriptDir -Force
            Move-Item "$Ext\platform-tools\AdbWinApi.dll" $ScriptDir -Force
            Move-Item "$Ext\platform-tools\AdbWinUsbApi.dll" $ScriptDir -Force
            Remove-Item $Zip -Force; Remove-Item $Ext -Recurse -Force
            $Script:AdbPath = $AdbExe
            Write-Success "ADB Installed."
        } catch { Write-ErrorMsg "ADB Setup Failed."; Exit }
    } else { Write-Success "Found ADB." }
}

# UX #E: Add ADB Server Restart function
function Restart-AdbServer {
    Write-Info "Restarting ADB server..."
    try {
        & $Script:AdbPath kill-server 2>$null
        Start-Sleep -Milliseconds 500
        & $Script:AdbPath start-server 2>$null
        Write-Success "ADB server restarted."
    }
    catch {
        Write-ErrorMsg "Failed to restart ADB server."
    }
    Start-Sleep -Milliseconds 500
}

# UX #G: Add Disconnect Device function
function Disconnect-Device {
    param([string]$Serial)
    Write-Info "Disconnecting $Serial..."
    try {
        & $Script:AdbPath disconnect $Serial 2>$null
        Write-Success "Disconnected."
    }
    catch {
        Write-ErrorMsg "Failed to disconnect."
    }
}

function Get-Devices {
    $raw = & $Script:AdbPath devices
    $devs = @()
    foreach ($line in $raw) {
        if ($line -match "^(\S+)\s+(device|offline|unauthorized)") {
            $s = $matches[1]
            $st = $matches[2]

            $n = "Unknown Device"
            $mod = "Unknown Model"
            $devType = $Script:DeviceType.Unknown

            if ($st -eq "device") {
                try {
                    # Batch all property queries into ONE adb call for speed
                    $props = (& $Script:AdbPath -s $s shell "settings get global device_name; getprop ro.product.brand; getprop ro.product.model; getprop ro.product.device; getprop ro.product.manufacturer" 2>&1 | Out-String) -split "`n"

                    $devName = if ($props[0]) { $props[0].Trim() } else { "" }
                    $brand = if ($props[1]) { $props[1].Trim() } else { "" }
                    $mCode = if ($props[2]) { $props[2].Trim() } else { "" }
                    $device = if ($props[3]) { $props[3].Trim() } else { "" }
                    $manufacturer = if ($props[4]) { $props[4].Trim() } else { "" }

                    # Determine device type from manufacturer/brand
                    if ($manufacturer -match "NVIDIA" -or $brand -match "NVIDIA") {
                        $devType = $Script:DeviceType.Shield
                    }
                    elseif ($manufacturer -match "Google|Amlogic" -or $brand -match "onn|google" -or $device -match "ott_|sabrina|boreal") {
                        $devType = $Script:DeviceType.GoogleTV
                    }
                    else {
                        $devType = $Script:DeviceType.Unknown
                    }

                    # Get device name
                    if ($devName -and $devName -notmatch "Exception|Error|null") {
                        $n = $devName
                    } elseif ($brand) {
                        $n = "$brand Device"
                    } else {
                        $n = "Android TV"
                    }

                    # Get model name based on device type
                    if ($devType -eq $Script:DeviceType.Shield) {
                        switch ($device.ToLower()) {
                            "mdarcy" { $mod = "Shield TV Pro (2019)" }
                            "sif"    { $mod = "Shield TV (2019 Tube)" }
                            "darcy"  { $mod = "Shield TV (2017)" }
                            "foster" { $mod = "Shield TV (2015)" }
                            default  { $mod = "Shield TV ($mCode)" }
                        }
                    }
                    elseif ($devType -eq $Script:DeviceType.GoogleTV) {
                        switch -Regex ($device.ToLower()) {
                            "ott_"      { $mod = "Onn 4K ($mCode)" }
                            "sabrina"   { $mod = "Chromecast with Google TV" }
                            "boreal"    { $mod = "Google TV Streamer (2024)" }
                            default     { $mod = "Google TV ($mCode)" }
                        }
                    }
                    else {
                        $mod = "Android TV ($mCode)"
                    }
                }
                catch {
                    $n = "Android TV"
                    $mod = "Unknown Model"
                }
            } elseif ($st -eq "offline") {
                $n = "Offline"; $mod = "Rebooting..."
            } elseif ($st -eq "unauthorized") {
                $n = "Unauthorized"; $mod = "Check TV Screen"
            }

            $devs += [PSCustomObject]@{
                ID = $devs.Count + 1
                Serial = $s
                Name = $n
                Status = $st
                Model = $mod
                Type = $devType
            }
        }
    }
    return $devs
}

# FIX #1: Socket leak fix and #14: Results feedback and #H: Timeout improvement
function Scan-Network {
    Write-Info "Scanning local subnet for Android TV devices (timeout: 200ms)..."
    $arp = arp -a | Select-String "dynamic"
    $foundCount = 0

    foreach ($entry in $arp) {
        if ($entry -match "(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})") {
            $ip = $matches[1]
            $sock = $null
            try {
                $sock = New-Object System.Net.Sockets.TcpClient
                # FIX #H: Increased timeout from 50ms to 200ms
                $asyncResult = $sock.BeginConnect($ip, 5555, $null, $null)
                if ($asyncResult.AsyncWaitHandle.WaitOne(200, $false)) {
                    if ($sock.Connected) {
                        Write-Success "Found device at $ip"
                        & $Script:AdbPath connect $ip | Out-Null
                        $foundCount++
                    }
                }
            }
            catch {
                # Silently continue on connection errors
            }
            finally {
                # FIX #1: Always dispose socket to prevent leak
                if ($sock) {
                    try { $sock.Close() } catch {}
                    try { $sock.Dispose() } catch {}
                }
            }
        }
    }

    # FIX #14: Show results feedback
    if ($foundCount -eq 0) {
        Write-Warn "No devices found on network. Ensure Network Debugging is enabled on your Android TV device."
    } else {
        Write-Success "Scan complete. Found $foundCount device(s)."
    }
    Start-Sleep -Milliseconds 500
}

function Open-PlayStore ($Target, $PkgId) {
    Write-Header "Opening Play Store..."
    Write-Warn "ACTION REQUIRED ON TV!"
    Write-Host "Please pick up your remote and click 'Install' or 'Enable' on the screen." -ForegroundColor Yellow
    try {
        & $Script:AdbPath -s $Target shell am start -a android.intent.action.VIEW -d "market://details?id=$PkgId" | Out-Null
        Write-Success "Command sent to Shield. Waiting for you..."
        Pause
    } catch { Write-ErrorMsg "Failed to open Play Store." }
}

function Show-Help {
    Clear-Host
    Write-Header "HELP & TROUBLESHOOTING"
    Write-Host "1. PRE-REQUISITES" -ForegroundColor Cyan
    Write-Host "   - Enable Developer Options: Settings > Device Preferences > About > Build (Click 7 times)"
    Write-Host "   - Enable USB Debugging: Settings > Device Preferences > Developer Options > USB Debugging"
    Write-Host "   - Network Debugging: Enable this if connecting via WiFi."

    Write-Host "`n2. CONNECTING" -ForegroundColor Cyan
    Write-Host "   - Use [Scan Network] to auto-discover Android TV devices."
    Write-Host "   - If scan fails, use [Connect IP] to enter the address manually."
    Write-Host "   - If a device shows 'UNAUTHORIZED', check your TV screen to accept the connection."

    Write-Host "`n3. SUPPORTED DEVICES" -ForegroundColor Cyan
    Write-Host "   - Nvidia Shield TV (all models)"
    Write-Host "   - Onn 4K Pro (Walmart)"
    Write-Host "   - Chromecast with Google TV"
    Write-Host "   - Google TV Streamer (2024)"
    Write-Host "   - Other Android TV devices"

    Write-Host "`n4. MODES" -ForegroundColor Cyan
    Write-Host "   - OPTIMIZE: Disables bloatware (device-specific). Choose Disable or Uninstall."
    Write-Host "   - RESTORE: Re-enables or re-installs apps."
    Write-Host "   - LAUNCHER: Install Projectivy, FLauncher, or manage stock launcher."
    Write-Host "   - RECOVERY: Emergency restore - re-enables all disabled packages."

    Write-Host "`n5. KEYBOARD SHORTCUTS" -ForegroundColor Cyan
    Write-Host "   - Arrow Keys: Navigate menus"
    Write-Host "   - 1-9: Quick select devices"
    Write-Host "   - Letters: Quick select options (shown as [S]can, [Q]uit, etc.)"
    Write-Host "   - Enter: Confirm selection"
    Write-Host "   - ESC: Cancel / Go back"

    Read-Host "`nPress Enter to return..."
}

# Helper to print menu option with colored shortcut key and status tags
function Write-OptionWithHighlight ($Text, [bool]$Selected) {
    # Parse the text for [X] shortcut pattern and status tags
    $remaining = $Text

    while ($remaining.Length -gt 0) {
        # Look for brackets
        $bracketStart = $remaining.IndexOf('[')

        if ($bracketStart -lt 0) {
            # No more brackets, print the rest
            if ($Selected) {
                Write-Host "$remaining " -NoNewline -ForegroundColor Black -BackgroundColor Cyan
            } else {
                Write-Host $remaining -NoNewline -ForegroundColor Gray
            }
            break
        }

        # Print text before bracket
        if ($bracketStart -gt 0) {
            $before = $remaining.Substring(0, $bracketStart)
            if ($Selected) {
                Write-Host $before -NoNewline -ForegroundColor Black -BackgroundColor Cyan
            } else {
                Write-Host $before -NoNewline -ForegroundColor Gray
            }
        }

        # Find closing bracket
        $bracketEnd = $remaining.IndexOf(']', $bracketStart)
        if ($bracketEnd -lt 0) {
            # No closing bracket, print rest normally
            if ($Selected) {
                Write-Host "$($remaining.Substring($bracketStart)) " -NoNewline -ForegroundColor Black -BackgroundColor Cyan
            } else {
                Write-Host $remaining.Substring($bracketStart) -NoNewline -ForegroundColor Gray
            }
            break
        }

        # Extract bracket content
        $bracketContent = $remaining.Substring($bracketStart + 1, $bracketEnd - $bracketStart - 1)

        # Determine color based on content
        $bracketColor = switch -Regex ($bracketContent) {
            "^[A-Z0-9]$"    { "Yellow" }      # Single char shortcut key
            "ACTIVE"        { "Green" }
            "INSTALLED"     { "Cyan" }
            "ENABLED"       { "Cyan" }
            "MISSING"       { "Red" }
            "DISABLED"      { "DarkYellow" }
            "NOT FOUND"     { "DarkGray" }
            default         { "White" }
        }

        # Print the bracketed content with color
        if ($Selected) {
            Write-Host "[" -NoNewline -ForegroundColor Black -BackgroundColor Cyan
            Write-Host $bracketContent -NoNewline -ForegroundColor $bracketColor -BackgroundColor Cyan
            Write-Host "]" -NoNewline -ForegroundColor Black -BackgroundColor Cyan
        } else {
            Write-Host "[" -NoNewline -ForegroundColor DarkGray
            Write-Host $bracketContent -NoNewline -ForegroundColor $bracketColor
            Write-Host "]" -NoNewline -ForegroundColor DarkGray
        }

        # Move past this bracket
        $remaining = $remaining.Substring($bracketEnd + 1)
    }

    Write-Host ""  # Newline at end
}

# --- NEW VERTICAL MENU SYSTEM ---
# FIX #13: ESC key support, FIX #3: Cursor error handling, UX #C: Number/letter key shortcuts
# StaticStartIndex: Items before this index use numbers (1-9), items from this index use letters
# Shortcuts: Optional array of shortcut letters for static items (e.g., @("S","C","R","F","A","H","Q"))
function Read-Menu ($Title, $Options, $Descriptions, $DefaultIndex=0, $StaticStartIndex=-1, $Shortcuts=$null) {
    $idx = $DefaultIndex
    $max = $Options.Count - 1

    # If StaticStartIndex not specified, all items use letters
    if ($StaticStartIndex -lt 0) { $StaticStartIndex = 0 }

    # Build shortcut mapping: key char -> index
    $shortcutMap = @{}
    $shortcutDisplay = @{}  # index -> display char

    for ($i = 0; $i -lt $Options.Count; $i++) {
        if ($i -lt $StaticStartIndex) {
            # Device - use number
            $shortcutDisplay[$i] = "$($i + 1)"
        } else {
            # Static option - use provided shortcut or first letter
            $staticIdx = $i - $StaticStartIndex
            if ($Shortcuts -and $staticIdx -lt $Shortcuts.Count) {
                $char = $Shortcuts[$staticIdx].ToUpper()
            } else {
                # Fallback: use first letter of option
                $char = $Options[$i].Substring(0,1).ToUpper()
            }
            $shortcutDisplay[$i] = $char
            $shortcutMap[$char] = $i
        }
    }

    # FIX #3: Hide Cursor with proper error handling
    $origCursor = 25
    try {
        $origCursor = $Host.UI.RawUI.CursorSize
        $Host.UI.RawUI.CursorSize = 0
    } catch {
        # Silently continue if cursor manipulation fails (Windows Terminal, VS Code)
    }

    while ($true) {
        Clear-Host
        # Colorful title
        Write-Host " $Title" -ForegroundColor Cyan
        Write-Host " ================================================" -ForegroundColor DarkCyan

        for ($i=0; $i -lt $Options.Count; $i++) {
            $shortcut = $shortcutDisplay[$i]
            $optText = $Options[$i]

            # Format option text with embedded shortcut: "Skip" -> "[S]kip", "Refresh" -> "Re[f]resh"
            # But don't insert inside existing brackets like [DISABLED]
            $displayText = $null

            # Find first occurrence of shortcut letter that's NOT inside existing brackets
            $foundPos = -1
            $inBracket = $false
            for ($c = 0; $c -lt $optText.Length; $c++) {
                if ($optText[$c] -eq '[') { $inBracket = $true }
                elseif ($optText[$c] -eq ']') { $inBracket = $false }
                elseif (-not $inBracket -and $optText[$c].ToString().ToUpper() -eq $shortcut.ToUpper()) {
                    $foundPos = $c
                    break
                }
            }

            if ($foundPos -ge 0) {
                $actualChar = $optText[$foundPos]
                $displayText = $optText.Substring(0, $foundPos) + "[$actualChar]" + $optText.Substring($foundPos + 1)
            } else {
                # Letter not found outside brackets, prefix it
                $displayText = "[$shortcut] $optText"
            }

            if ($i -eq $idx) {
                # Selected Item - highlight with arrow and background
                Write-Host "  > " -NoNewline -ForegroundColor Cyan
                # Print with colored shortcut key
                Write-OptionWithHighlight -Text $displayText -Selected $true
            } else {
                # Normal Item
                Write-Host "    " -NoNewline
                Write-OptionWithHighlight -Text $displayText -Selected $false
            }
        }

        Write-Host " ================================================" -ForegroundColor DarkCyan
        Write-Host " Info: " -NoNewline -ForegroundColor Yellow
        if ($Descriptions[$idx]) {
            Write-Host "$($Descriptions[$idx])" -ForegroundColor White
        } else {
            Write-Host "Select an option." -ForegroundColor DarkGray
        }

        # Show hint
        Write-Host " [" -NoNewline -ForegroundColor DarkGray
        Write-Host "Arrows" -NoNewline -ForegroundColor DarkCyan
        Write-Host ": Move] [" -NoNewline -ForegroundColor DarkGray
        Write-Host "Keys" -NoNewline -ForegroundColor Yellow
        Write-Host ": Select] [" -NoNewline -ForegroundColor DarkGray
        Write-Host "Enter" -NoNewline -ForegroundColor Green
        Write-Host ": OK] [" -NoNewline -ForegroundColor DarkGray
        Write-Host "ESC" -NoNewline -ForegroundColor Red
        Write-Host ": Back]" -ForegroundColor DarkGray

        $key = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")

        # FIX #13: ESC key to cancel (VirtualKeyCode 27)
        if ($key.VirtualKeyCode -eq 27) {
            try { $Host.UI.RawUI.CursorSize = $origCursor } catch {}
            return -1  # Return -1 to indicate cancellation
        }
        elseif ($key.VirtualKeyCode -eq 38) { # Up
            $idx--; if ($idx -lt 0) { $idx = $max }
        }
        elseif ($key.VirtualKeyCode -eq 40) { # Down
            $idx++; if ($idx -gt $max) { $idx = 0 }
        }
        elseif ($key.VirtualKeyCode -eq 13) { # Enter
            try { $Host.UI.RawUI.CursorSize = $origCursor } catch {}
            return $idx
        }
        # Number key shortcuts (1-9) for devices
        elseif ($key.VirtualKeyCode -ge 49 -and $key.VirtualKeyCode -le 57) {
            $numIdx = $key.VirtualKeyCode - 49  # 49 = '1', so index 0
            if ($numIdx -lt $StaticStartIndex -and $numIdx -le $max) {
                try { $Host.UI.RawUI.CursorSize = $origCursor } catch {}
                return $numIdx
            }
        }
        # Numpad 1-9 for devices
        elseif ($key.VirtualKeyCode -ge 97 -and $key.VirtualKeyCode -le 105) {
            $numIdx = $key.VirtualKeyCode - 97  # 97 = Numpad1, so index 0
            if ($numIdx -lt $StaticStartIndex -and $numIdx -le $max) {
                try { $Host.UI.RawUI.CursorSize = $origCursor } catch {}
                return $numIdx
            }
        }
        # Letter keys for static options - check shortcut map
        elseif ($key.VirtualKeyCode -ge 65 -and $key.VirtualKeyCode -le 90) {
            $pressedKey = [string][char]$key.VirtualKeyCode
            if ($shortcutMap.ContainsKey($pressedKey)) {
                try { $Host.UI.RawUI.CursorSize = $origCursor } catch {}
                return $shortcutMap[$pressedKey]
            }
        }
        # Also check if pressed number matches a shortcut (for menus like process limit)
        elseif ($StaticStartIndex -eq 0 -and $key.VirtualKeyCode -ge 49 -and $key.VirtualKeyCode -le 57) {
            $pressedKey = [string][char]$key.VirtualKeyCode  # '1' = 49, etc.
            if ($shortcutMap.ContainsKey($pressedKey)) {
                try { $Host.UI.RawUI.CursorSize = $origCursor } catch {}
                return $shortcutMap[$pressedKey]
            }
        }
    }
}

# FIX #13: ESC key support for toggle
function Read-Toggle ($Prompt, $Options, $DefaultIndex=0) {
    # Horizontal toggle for [ YES ] NO using cursor positioning
    $idx = $DefaultIndex
    $max = $Options.Count - 1

    # Save starting cursor position
    $startPos = $null
    try { $startPos = $Host.UI.RawUI.CursorPosition } catch {}

    while ($true) {
        # Reset cursor to start position if possible
        if ($startPos) {
            try { $Host.UI.RawUI.CursorPosition = $startPos } catch {}
        }

        # Build the entire line as a string first
        $line = "$Prompt "
        for ($i=0; $i -lt $Options.Count; $i++) {
            if ($i -eq $idx) {
                $line += " [ $($Options[$i]) ] "
            } else {
                $line += "   $($Options[$i])   "
            }
        }
        $line += "          "  # Padding to clear old text

        # Write prompt
        Write-Host $Prompt -NoNewline -ForegroundColor Gray
        Write-Host " " -NoNewline

        # Write options with colors
        for ($i=0; $i -lt $Options.Count; $i++) {
            if ($i -eq $idx) {
                Write-Host " [ $($Options[$i]) ] " -NoNewline -ForegroundColor Cyan
            } else {
                Write-Host "   $($Options[$i])   " -NoNewline -ForegroundColor DarkGray
            }
        }
        Write-Host "          " -NoNewline  # Clear any trailing characters

        $key = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")

        # FIX #13: ESC key to cancel
        if ($key.VirtualKeyCode -eq 27) {
            Write-Host ""
            return -1  # Return -1 to indicate cancellation
        }
        elseif ($key.VirtualKeyCode -eq 37) { # Left
            $idx--; if ($idx -lt 0) { $idx = $max }
        }
        elseif ($key.VirtualKeyCode -eq 39) { # Right
            $idx++; if ($idx -gt $max) { $idx = 0 }
        }
        elseif ($key.VirtualKeyCode -eq 13) { # Enter
            Write-Host ""
            return $idx
        }
    }
}

# --- REPORT GENERATOR ---
# Universal diagnostics for Shield, Google TV, and other Android TV devices
function Run-Report ($Target, $Name, $DeviceType = "Unknown") {
    $typeName = Get-DeviceTypeName $DeviceType
    Write-Header "Health Report: $Name ($typeName)"

    # --- TEMPERATURE ---
    # Try multiple methods for different device types
    $Temp = "N/A"

    # Method 1: thermalservice - look for CPU, soc_thermal, or first valid temp
    try {
        $dump = & $Script:AdbPath -s $Target shell dumpsys thermalservice 2>&1 | Out-String
        # Try CPU/CPU0 first (Shield), then soc_thermal (Onn/Amlogic), then any Type=0 reading
        if ($dump -match "mValue=([\d\.]+).*mName=CPU\d*,") {
            $Temp = [math]::Round([float]$matches[1], 1)
        }
        elseif ($dump -match "mValue=([\d\.]+).*mName=soc_thermal") {
            $Temp = [math]::Round([float]$matches[1], 1)
        }
        elseif ($dump -match "Temperature\{mValue=([\d\.]+),.*mType=0") {
            # Type 0 = CPU/SoC temperature
            $Temp = [math]::Round([float]$matches[1], 1)
        }
    } catch {}

    # Method 2: thermal zones (generic Linux)
    if ($Temp -eq "N/A") {
        try {
            $thermal = & $Script:AdbPath -s $Target shell "cat /sys/class/thermal/thermal_zone0/temp 2>/dev/null" 2>&1 | Out-String
            if ($thermal -match "^\d+") {
                $tempVal = [int]($thermal.Trim())
                if ($tempVal -gt 1000) { $tempVal = $tempVal / 1000 }  # millidegrees to degrees
                if ($tempVal -gt 0 -and $tempVal -lt 150) {  # Sanity check
                    $Temp = [math]::Round($tempVal, 1)
                }
            }
        } catch {}
    }

    # Method 3: battery temperature (fallback)
    if ($Temp -eq "N/A") {
        try {
            $battery = & $Script:AdbPath -s $Target shell dumpsys battery 2>&1 | Out-String
            if ($battery -match "temperature:\s*(\d+)") {
                $Temp = [math]::Round([int]$matches[1] / 10, 1)
            }
        } catch {}
    }

    # --- RAM ---
    $Total = 0; $Free = 0; $Swap = 0

    # Try dumpsys meminfo first
    try {
        $mem = & $Script:AdbPath -s $Target shell dumpsys meminfo 2>&1 | Out-String
        if ($mem -match "Total RAM:\s+([0-9,]+)\s*K") { $Total = [math]::Round(($matches[1] -replace ",","") / 1024, 0) }
        if ($mem -match "Free RAM:\s+([0-9,]+)\s*K") { $Free = [math]::Round(($matches[1] -replace ",","") / 1024, 0) }
        if ($mem -match "ZRAM:.*used for\s+([0-9,]+)\s*K") { $Swap = [math]::Round(($matches[1] -replace ",","") / 1024, 0) }
    } catch {}

    # Fallback to /proc/meminfo
    if ($Total -eq 0) {
        try {
            $procMem = & $Script:AdbPath -s $Target shell cat /proc/meminfo 2>&1 | Out-String
            if ($procMem -match "MemTotal:\s+(\d+)\s*kB") { $Total = [math]::Round([int]$matches[1] / 1024, 0) }
            if ($procMem -match "MemAvailable:\s+(\d+)\s*kB") { $Free = [math]::Round([int]$matches[1] / 1024, 0) }
            elseif ($procMem -match "MemFree:\s+(\d+)\s*kB") { $Free = [math]::Round([int]$matches[1] / 1024, 0) }
        } catch {}
    }

    if ($Total -eq 0) { $Total = 2048 }  # Default fallback
    $Used = $Total - $Free
    $Pct = if ($Total -gt 0) { [math]::Round(($Used / $Total) * 100, 0) } else { 0 }

    # --- STORAGE ---
    $StorageUsed = "N/A"; $StorageTotal = "N/A"; $StoragePct = 0
    try {
        $df = & $Script:AdbPath -s $Target shell df -h /data 2>&1 | Out-String
        # Match df output: Filesystem Size Used Avail Use% Mounted
        # Format varies: mount point can be at start or end, look for size columns + percentage
        foreach ($line in ($df -split "`n")) {
            if ($line -match "/data" -and $line -match "(\d+\.?\d*[GMKT]?)\s+(\d+\.?\d*[GMKT]?)\s+(\d+\.?\d*[GMKT]?)\s+(\d+)%") {
                $StorageTotal = $matches[1]
                $StorageUsed = $matches[2]
                $StoragePct = [int]$matches[4]
                break
            }
        }
    } catch {}

    # --- CPU/PLATFORM ---
    $Platform = "Unknown"
    try {
        $Platform = (& $Script:AdbPath -s $Target shell getprop ro.board.platform 2>&1 | Out-String).Trim()
        if (-not $Platform -or $Platform -match "Exception") { $Platform = "Unknown" }
    } catch {}

    # --- ANDROID VERSION ---
    $AndroidVer = "Unknown"
    try {
        $AndroidVer = (& $Script:AdbPath -s $Target shell getprop ro.build.version.release 2>&1 | Out-String).Trim()
    } catch {}

    Write-SubHeader "System Info"
    Write-Host " Platform:  " -NoNewline -ForegroundColor Gray
    Write-Host "$Platform" -ForegroundColor Cyan
    Write-Host " Android:   " -NoNewline -ForegroundColor Gray
    Write-Host "$AndroidVer" -ForegroundColor White

    Write-SubHeader "Vitals"
    Write-Host " Temp:    " -NoNewline -ForegroundColor Gray
    if ($Temp -ne "N/A") {
        $tempColor = if ([float]$Temp -gt 70) { "Red" } elseif ([float]$Temp -gt 50) { "Yellow" } else { "Green" }
        Write-Host "${Temp}°C" -ForegroundColor $tempColor
    } else {
        Write-Host "N/A" -ForegroundColor DarkGray
    }

    Write-Host " RAM:     " -NoNewline -ForegroundColor Gray
    $ramColor = if ($Pct -gt 85) { "Red" } elseif ($Pct -gt 70) { "Yellow" } else { "Green" }
    Write-Host "$Pct% ($Used / $Total MB)" -ForegroundColor $ramColor

    Write-Host " Swap:    " -NoNewline -ForegroundColor Gray
    Write-Host "$Swap MB" -ForegroundColor White

    Write-Host " Storage: " -NoNewline -ForegroundColor Gray
    if ($StorageUsed -ne "N/A") {
        $storColor = if ($StoragePct -gt 90) { "Red" } elseif ($StoragePct -gt 75) { "Yellow" } else { "Green" }
        Write-Host "$StorageUsed / $StorageTotal ($StoragePct%)" -ForegroundColor $storColor
    } else {
        Write-Host "N/A" -ForegroundColor DarkGray
    }

    Write-SubHeader "Settings Check"
    $anim = (& $Script:AdbPath -s $Target shell settings get global window_animation_scale 2>&1 | Out-String).Trim()
    $proc = (& $Script:AdbPath -s $Target shell settings get global background_process_limit 2>&1 | Out-String).Trim()
    if ($proc -eq "null" -or $proc -eq "" -or $proc -match "Exception") { $proc = "Standard" }
    if ($anim -match "Exception" -or $anim -eq "null") { $anim = "1.0" }

    Write-Host " Animation Speed: " -NoNewline -ForegroundColor Gray
    Write-Host "$anim" -ForegroundColor Cyan
    Write-Host " Process Limit:   " -NoNewline -ForegroundColor Gray
    Write-Host "$proc" -ForegroundColor Cyan

    Write-SubHeader "Bloat Check"
    $clean = $true

    # Get device-specific app list
    $appList = Get-AppListForDevice $DeviceType

    # Query enabled packages once
    $enabledPkgs = (& $Script:AdbPath -s $Target shell pm list packages -e 2>&1 | Out-String)

    foreach ($app in $appList) {
        $pkg = $app[0]; $appName = $app[1]; $risk = $app[3]
        if ($risk -match "Safe" -or $risk -match "Medium") {
            # Use exact match with word boundary
            if ($enabledPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)") {
                Write-Host " [ACTIVE BLOAT] $appName" -ForegroundColor Yellow
                $clean = $false
            }
        }
    }
    if ($clean) { Write-Success "System is clean." }
    Write-Host "`n"
}

# --- LAUNCHER WIZARD ---
# FIX #D: Check stock launcher status before offering disable
# Known stock launcher packages (varies by device/Android version)
$Script:StockLaunchers = @(
    "com.google.android.tvlauncher",           # Standard Google TV Launcher
    "com.google.android.apps.tv.launcherx",    # Newer Google TV Launcher (Chromecast, Onn)
    "com.google.android.leanbacklauncher",     # Older Android TV Launcher
    "com.amazon.tv.launcher"                   # Fire TV Launcher (if sideloaded)
)

# Safe HOME handlers that should NEVER be disabled (they won't cause loops)
$Script:SafeHomeHandlers = @(
    "com.android.tv.settings",                 # Settings app - safe fallback
    "com.android.settings"                     # Generic settings
)

# Discover all HOME-capable apps on the device
function Get-HomeHandlers ($Target) {
    $handlers = @()
    try {
        $output = & $Script:AdbPath -s $Target shell "cmd package query-activities -a android.intent.action.MAIN -c android.intent.category.HOME" 2>&1 | Out-String

        # Parse output - look for "packageName=com.example.app" lines
        # Each Activity block has a packageName field under ActivityInfo
        foreach ($line in ($output -split "`n")) {
            $trimmed = $line.Trim()
            # Match: packageName=com.example.app
            if ($trimmed -match "^packageName=([a-zA-Z][a-zA-Z0-9_.]+)$") {
                $pkg = $matches[1]
                if ($handlers -notcontains $pkg) {
                    $handlers += $pkg
                }
            }
        }
    }
    catch {
        Write-ErrorMsg "Failed to query HOME handlers: $_"
    }
    return $handlers
}

# Get the current HOME role holder (Android 10+)
function Get-HomeRoleHolder ($Target) {
    try {
        $output = & $Script:AdbPath -s $Target shell "cmd role get-role-holders android.app.role.HOME" 2>&1 | Out-String
        if ($output -match "([a-zA-Z0-9_.]+)") {
            return $matches[1].Trim()
        }
    }
    catch {}
    return $null
}

# Set a package as the HOME role holder
function Set-HomeRoleHolder ($Target, $Package) {
    try {
        $result = & $Script:AdbPath -s $Target shell "cmd role add-role-holder android.app.role.HOME $Package" 2>&1 | Out-String
        return -not ($result -match "Exception|Error|failed")
    }
    catch {
        return $false
    }
}

# Disable all stock HOME handlers, keeping only the custom launcher and safe fallbacks
function Disable-AllStockLaunchers {
    param(
        [string]$Target,
        [string]$CustomLauncherPkg
    )

    Write-Info "Discovering all HOME-capable apps..."
    $homeHandlers = @(Get-HomeHandlers -Target $Target)

    # Also check known problematic HOME handlers that might not appear in query
    $knownHomeHandlers = $Script:StockLaunchers + @(
        "com.google.android.tungsten.setupwraith",
        "com.droidlogic.launcher.provider"
    )

    # Get DISABLED packages - we'll skip anything already disabled
    # Note: pm list packages -e is unreliable (includes disabled-user packages on some devices)
    $disabledPkgs = & $Script:AdbPath -s $Target shell "pm list packages -d" 2>&1 | Out-String
    $installedPkgs = & $Script:AdbPath -s $Target shell "pm list packages" 2>&1 | Out-String

    foreach ($pkg in $knownHomeHandlers) {
        # Add if: installed AND not already disabled AND not already in list
        $isInstalled = $installedPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)"
        $isDisabled = $disabledPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)"
        if ($isInstalled -and -not $isDisabled -and $homeHandlers -notcontains $pkg) {
            $homeHandlers += $pkg
        }
    }

    if ($null -eq $homeHandlers -or $homeHandlers.Count -eq 0) {
        Write-ErrorMsg "Could not detect HOME handlers. Aborting for safety."
        return $false
    }

    # Build safe list: custom launcher + safe handlers
    $safeList = @($CustomLauncherPkg) + $Script:SafeHomeHandlers

    # Identify what will be disabled
    $toDisable = @()
    $toKeep = @()

    foreach ($pkg in $homeHandlers) {
        if ($safeList -contains $pkg) {
            $toKeep += $pkg
        }
        elseif ($Script:Launchers.Pkg -contains $pkg) {
            # Another custom launcher - keep it
            $toKeep += $pkg
        }
        else {
            $toDisable += $pkg
        }
    }

    if ($toDisable.Count -eq 0) {
        Write-Info "No stock launchers to disable."
        return $true
    }

    # Show user what will happen
    Write-Header "HOME Handler Analysis"
    Write-Host ""
    Write-Host " Found $($homeHandlers.Count) HOME-capable apps:" -ForegroundColor Gray
    Write-Host ""

    foreach ($pkg in $toKeep) {
        $label = if ($pkg -eq $CustomLauncherPkg) { "your custom launcher" }
                 elseif ($Script:SafeHomeHandlers -contains $pkg) { "safe fallback" }
                 else { "other custom launcher" }
        Write-Host "   " -NoNewline
        Write-Host "KEEP   " -NoNewline -ForegroundColor Green
        Write-Host "$pkg" -NoNewline -ForegroundColor Cyan
        Write-Host " ($label)" -ForegroundColor DarkGray
    }
    foreach ($pkg in $toDisable) {
        Write-Host "   " -NoNewline
        Write-Host "WILL DISABLE  " -NoNewline -ForegroundColor Yellow
        Write-Host "$pkg" -ForegroundColor White
    }
    Write-Host ""

    # Confirm
    $confirm = Read-Toggle -Prompt "Disable $($toDisable.Count) HOME handler(s)?" -Options @("YES", "NO") -DefaultIndex 0
    if ($confirm -ne 0) {
        Write-Info "Cancelled."
        return $false
    }

    # First, ensure custom launcher is the HOME role holder
    Write-Info "Setting $CustomLauncherPkg as HOME role holder..."
    $roleSet = Set-HomeRoleHolder -Target $Target -Package $CustomLauncherPkg
    if (-not $roleSet) {
        Write-Warn "Could not set HOME role (may require manual selection)."
    }

    # Disable each stock handler
    $disabledCount = 0
    $disabledPkgs = @()

    foreach ($pkg in $toDisable) {
        Write-Info "Disabling: $pkg"
        $result = Invoke-AdbCommand -Target $Target -Command "pm disable-user --user 0 $pkg"
        if ($result.Success -and $result.Output -notmatch "Failure") {
            Write-Success "Disabled: $pkg"
            $disabledCount++
            $disabledPkgs += $pkg
        }
        else {
            Write-ErrorMsg "Failed to disable: $pkg"
        }
    }

    Write-Host ""
    Write-Success "Disabled $disabledCount of $($toDisable.Count) HOME handlers."

    if ($disabledCount -gt 0) {
        Write-Info "Press the Home button - $CustomLauncherPkg should now activate."
    }

    return $true
}

# Restore all stock HOME handlers (re-enable everything that was disabled)
function Restore-AllStockLaunchers {
    param([string]$Target)

    Write-Info "Discovering disabled HOME handlers..."

    # Get all HOME handlers (includes disabled ones via -u flag check)
    $homeHandlers = @(Get-HomeHandlers -Target $Target)
    $disabledPkgs = (& $Script:AdbPath -s $Target shell pm list packages -d 2>&1 | Out-String)

    # Find which HOME handlers are currently disabled
    $toRestore = @()
    foreach ($pkg in $homeHandlers) {
        if ($disabledPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)") {
            $toRestore += $pkg
        }
    }

    # Also check known stock launchers that might not show in query when disabled
    $knownToCheck = $Script:StockLaunchers + @(
        "com.google.android.tungsten.setupwraith",
        "com.droidlogic.launcher.provider"
    )

    foreach ($pkg in $knownToCheck) {
        if ($disabledPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)") {
            if ($toRestore -notcontains $pkg) {
                $toRestore += $pkg
            }
        }
    }

    if ($toRestore.Count -eq 0) {
        Write-Info "No disabled stock launchers found."
        return $true
    }

    Write-Header "Restore Stock Launchers"
    Write-Host ""
    Write-Host " Found $($toRestore.Count) disabled HOME handler(s):" -ForegroundColor Gray
    Write-Host ""
    foreach ($pkg in $toRestore) {
        Write-Host "   " -NoNewline
        Write-Host "WILL ENABLE  " -NoNewline -ForegroundColor Cyan
        Write-Host "$pkg" -ForegroundColor White
    }
    Write-Host ""

    $confirm = Read-Toggle -Prompt "Re-enable $($toRestore.Count) package(s)?" -Options @("YES", "NO") -DefaultIndex 0
    if ($confirm -ne 0) {
        Write-Info "Cancelled."
        return $false
    }

    $restoredCount = 0
    foreach ($pkg in $toRestore) {
        Write-Info "Enabling: $pkg"
        $result = Invoke-AdbCommand -Target $Target -Command "pm enable $pkg"
        if ($result.Success) {
            Write-Success "Enabled: $pkg"
            $restoredCount++
        }
        else {
            Write-ErrorMsg "Failed to enable: $pkg"
        }
    }

    Write-Host ""
    Write-Success "Restored $restoredCount of $($toRestore.Count) packages."
    Write-Info "Press Home button and select your preferred launcher."

    return $true
}

# Helper to detect the current default launcher on the device
function Get-CurrentLauncher ($Target) {
    try {
        # Query for the current home activity
        $result = & $Script:AdbPath -s $Target shell cmd package resolve-activity --brief -a android.intent.action.MAIN -c android.intent.category.HOME 2>&1
        $output = ($result | Out-String).Trim()
        # Output format: "priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\ncom.example.launcher/.MainActivity"
        # We want the package name (before the /)
        if ($output -match "([a-zA-Z0-9_.]+)/") {
            return $matches[1]
        }
    } catch {}
    return $null
}

function Setup-Launcher ($Target) {
    Write-Header "Custom Launcher Wizard"
    Write-Info "Detecting current launcher..."

    $installedPkgs = (& $Script:AdbPath -s $Target shell pm list packages 2>&1 | Out-String)
    $disabledPkgs = (& $Script:AdbPath -s $Target shell pm list packages -d 2>&1 | Out-String)

    # Detect the currently active launcher
    $currentLauncher = Get-CurrentLauncher -Target $Target
    if ($currentLauncher) {
        Write-Info "Current launcher: $currentLauncher"
    }

    # Find which stock launcher is installed on this device
    $stockLauncherPkg = $null
    $stockLauncherInstalled = $false
    $stockLauncherDisabled = $false

    # First check our known list
    foreach ($stockPkg in $Script:StockLaunchers) {
        if ($installedPkgs -match "package:$([regex]::Escape($stockPkg))(\r|\n|$)") {
            $stockLauncherPkg = $stockPkg
            $stockLauncherInstalled = $true
            $stockLauncherDisabled = $disabledPkgs -match "package:$([regex]::Escape($stockPkg))(\r|\n|$)"
            break
        }
    }

    # If no known stock launcher found but we detected a current launcher that's not a custom one, use it
    if (-not $stockLauncherInstalled -and $currentLauncher) {
        $isCustomLauncher = $false
        foreach ($l in $Script:Launchers) {
            if ($currentLauncher -eq $l.Pkg) { $isCustomLauncher = $true; break }
        }
        if (-not $isCustomLauncher) {
            $stockLauncherPkg = $currentLauncher
            $stockLauncherInstalled = $true
            $stockLauncherDisabled = $false
            Write-Info "Detected stock launcher: $currentLauncher"
        }
    }

    $lOpts = @(); $lDescs = @(); $launchers = @()

    foreach ($l in $Script:Launchers) {
        $status = "MISSING"
        $isCurrent = ($currentLauncher -eq $l.Pkg)
        # FIX #12: Use exact match
        if ($installedPkgs -match "package:$([regex]::Escape($l.Pkg))(\r|\n|$)") {
            $status = if ($isCurrent) { "ACTIVE" } else { "INSTALLED" }
        }
        $lOpts += "$($l.Name) [$status]"
        $lDescs += "Install or Enable $($l.Name)"
        $launchers += $l
    }

    # UX #D: Show stock launcher status (only if installed)
    if ($stockLauncherInstalled) {
        $stockDetail = switch ($stockLauncherPkg) {
            "com.google.android.tvlauncher" { "Google TV" }
            "com.google.android.apps.tv.launcherx" { "Google TV" }
            "com.google.android.leanbacklauncher" { "Android TV" }
            default { $stockLauncherPkg }
        }
        if ($stockLauncherDisabled) {
            $lOpts += "Stock Launcher ($stockDetail) [DISABLED]"
            $lDescs += "Re-enable $stockLauncherPkg"
        } else {
            $isCurrent = ($currentLauncher -eq $stockLauncherPkg)
            $status = if ($isCurrent) { "ACTIVE" } else { "ENABLED" }
            $lOpts += "Stock Launcher ($stockDetail) [$status]"
            $lDescs += "Package: $stockLauncherPkg"
        }
    } else {
        $lOpts += "Stock Launcher [NOT FOUND]"
        $lDescs += "No standard stock launcher detected"
    }
    $lOpts += "Back"; $lDescs += "Return to Action Menu"

    # Shortcuts: P=Projectivy, F=FLauncher, A=ATV, W=Wolf, S=Stock, B=Back
    $launcherShortcuts = @("P", "F", "A", "W", "S", "B")
    $sel = Read-Menu -Title "Select Launcher" -Options $lOpts -Descriptions $lDescs -Shortcuts $launcherShortcuts

    # Handle ESC or Back
    if ($sel -eq -1 -or $lOpts[$sel] -match "^Back") { return }

    if ($lOpts[$sel] -match "Stock|Google TV|Android TV") {
        if (-not $stockLauncherInstalled) {
            Write-Warn "No stock launcher found on this device."
            return
        }
        if ($stockLauncherDisabled) {
            # Use comprehensive restore that re-enables ALL disabled HOME handlers
            $null = Restore-AllStockLaunchers -Target $Target
        } else {
            Write-Info "Stock Launcher is already enabled."
            Write-Info "Press Home button and select it as default."
        }
        return
    }

    # Safety check: ensure selection is within bounds of custom launchers array
    if ($sel -ge $launchers.Count) {
        Write-Warn "Invalid selection."
        return
    }

    $choice = $launchers[$sel]
    # FIX #12: Use exact match
    if (-not ($installedPkgs -match "package:$([regex]::Escape($choice.Pkg))(\r|\n|$)")) {
        $toggleIdx = Read-Toggle -Prompt "Not Installed. Open Play Store?" -Options @("YES", "NO") -DefaultIndex 0
        if ($toggleIdx -eq 0) {
            Open-PlayStore -Target $Target -PkgId $choice.Pkg
        }
    } else {
        # Check if already the active launcher
        if ($currentLauncher -eq $choice.Pkg) {
            Write-Success "$($choice.Name) is already the active launcher."

            # Offer to disable remaining HOME handlers for cleaner experience
            $toggleIdx = Read-Toggle -Prompt "Disable all stock HOME handlers?" -Options @("YES", "NO") -DefaultIndex 1
            if ($toggleIdx -eq 0) {
                $null = Disable-AllStockLaunchers -Target $Target -CustomLauncherPkg $choice.Pkg
            }
            return
        }

        # Custom launcher is installed but not active
        Write-Success "$($choice.Name) is installed."

        # Check if any stock launcher is still active
        $isStockActive = $false
        foreach ($stockPkg in $Script:StockLaunchers) {
            if ($currentLauncher -eq $stockPkg) {
                $isStockActive = $true
                break
            }
        }

        if ($isStockActive) {
            # Offer to disable ALL stock HOME handlers (not just the main launcher)
            $toggleIdx = Read-Toggle -Prompt "Disable stock launchers to make $($choice.Name) the default?" -Options @("YES", "NO") -DefaultIndex 0
            if ($toggleIdx -eq 0) {
                $null = Disable-AllStockLaunchers -Target $Target -CustomLauncherPkg $choice.Pkg
            } else {
                Write-Info "Press Home button on your remote and select $($choice.Name) as default."
            }
        } else {
            Write-Info "Press Home button on your remote and select it as default."
        }
    }
}

# Helper to show task summary (used for completion and abort)
function Show-TaskSummary ($Mode, [switch]$Aborted) {
    if ($Aborted) {
        Write-Header "Aborted - Partial Summary"
    } else {
        Write-Header "Summary"
    }

    if ($Mode -eq "Optimize") {
        Write-Host " Disabled:    $($Script:Summary.Disabled) apps" -ForegroundColor Green
        Write-Host " Uninstalled: $($Script:Summary.Uninstalled) apps" -ForegroundColor Green
    } else {
        Write-Host " Restored: $($Script:Summary.Restored) apps" -ForegroundColor Green
    }
    Write-Host " Skipped:  $($Script:Summary.Skipped) apps" -ForegroundColor Gray
    if ($Script:Summary.Failed -gt 0) {
        Write-Host " Failed:   $($Script:Summary.Failed) apps" -ForegroundColor Red
    }
}

# --- ENGINE: UNIFIED TASK RUNNER ---
# UX #A: Apply All Defaults, UX #B: Summary tracking
function Run-Task ($Target, $Mode, $DeviceType = "Unknown") {
    $typeName = Get-DeviceTypeName $DeviceType
    Write-Header "Application Management ($Mode) - $typeName"

    # UX #B: Track summary statistics
    $Script:Summary = @{
        Disabled = 0
        Uninstalled = 0
        Restored = 0
        Skipped = 0
        Failed = 0
    }

    # UX #A: Ask about Apply All Defaults
    $applyAll = $false
    $applyIdx = Read-Toggle -Prompt "Apply all default actions without prompting?" -Options @("NO (Review Each)", "YES (Use Defaults)") -DefaultIndex 0
    if ($applyIdx -eq 1) { $applyAll = $true }
    if ($applyIdx -eq -1) { return }  # ESC pressed

    # FIX #11: Query packages once, cache results
    Write-Info "Querying installed packages..."
    $installedPkgs = (& $Script:AdbPath -s $Target shell pm list packages 2>&1 | Out-String)      # Currently installed for user
    $allPkgs = (& $Script:AdbPath -s $Target shell pm list packages -u 2>&1 | Out-String)         # All packages including uninstalled
    $disabledPkgs = (& $Script:AdbPath -s $Target shell pm list packages -d 2>&1 | Out-String)    # Disabled packages

    # Get device-specific app list
    $appList = Get-AppListForDevice $DeviceType
    Write-Info "Processing $($appList.Count) apps for $typeName..."

    foreach ($app in $appList) {
        $pkg = $app[0]; $name = $app[1]; $defMethod = $app[2]; $risk = $app[3]
        $optDesc = $app[4]; $restDesc = $app[5]
        $defOpt = $app[6]; $defRest = $app[7]

        # FIX #12: Use exact match with word boundary
        $existsOnSystem = $allPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)"           # Package exists (may be uninstalled)
        $isInstalledForUser = $installedPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)" # Actually installed for user 0
        $isDisabled = $disabledPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)"

        $skip = $false
        $canUninstall = $isInstalledForUser
        $canDisable = $existsOnSystem -and -not $isDisabled

        if ($Mode -eq "Optimize") {
            if (-not $existsOnSystem) { Write-Dim "$name ... [NOT INSTALLED]"; $skip=$true }
            elseif ($isDisabled) { Write-Dim "$name ... [ALREADY DISABLED]"; $skip=$true }
            elseif (-not $isInstalledForUser -and $defMethod -eq "UNINSTALL") { Write-Dim "$name ... [ALREADY UNINSTALLED]"; $skip=$true }
            $desc = $optDesc
        } else {
            if ($isInstalledForUser -and -not $isDisabled) { Write-Dim "$name ... [ALREADY ACTIVE]"; $skip=$true }
            $desc = $restDesc
        }

        if (-not $skip) {
            if ($Mode -eq "Optimize") { $verb = "Remove" } else { $verb = "Restore" }
            Write-Host -NoNewline "${verb}: "
            Write-Host "$name" -ForegroundColor Cyan -NoNewline
            if ($risk -match "Safe") { $c="Green" } elseif ($risk -match "Medium") { $c="Yellow" } else { $c="Red" }
            Write-Host " [$risk]" -ForegroundColor $c
            Write-Dim "    $desc"

            if ($Mode -eq "Restore") {
                if ($existsOnSystem) { Write-Host "    [Status: Disabled]" -ForegroundColor DarkGray }
                else { Write-Host "    [Status: Missing]" -ForegroundColor Yellow }
            }

            if ($Mode -eq "Optimize") {
                # Build options based on what's actually possible
                if ($defMethod -eq "DISABLE" -or -not $canUninstall) {
                    # Default to disable, or can only disable (not uninstall)
                    if ($canUninstall) {
                        $opts = @("DISABLE", "UNINSTALL", "SKIP", "ABORT")
                    } else {
                        $opts = @("DISABLE", "SKIP", "ABORT")
                    }
                    if ($defOpt -eq "Y") { $defIdx = 0 } else { $defIdx = ($opts.Count - 2) }
                } else {
                    # Default to uninstall
                    $opts = @("UNINSTALL", "DISABLE", "SKIP", "ABORT")
                    if ($defOpt -eq "Y") { $defIdx = 0 } else { $defIdx = 2 }
                }

                # UX #A: Apply defaults or prompt
                if ($applyAll) {
                    $toggleResult = $defIdx
                } else {
                    $toggleResult = Read-Toggle -Prompt "    >> Action:" -Options $opts -DefaultIndex $defIdx
                    if ($toggleResult -eq -1) { $toggleResult = ($opts.Count - 1) }  # ESC = ABORT
                }
                $selStr = $opts[$toggleResult]

                # Handle ABORT
                if ($selStr -eq "ABORT") {
                    Write-Warn "Process aborted by user."
                    Show-TaskSummary -Mode $Mode -Aborted
                    Pause
                    return
                }

                if ($selStr -ne "SKIP") {
                    if ($selStr -eq "UNINSTALL") {
                        Write-Host "    Uninstalling..." -NoNewline -ForegroundColor Gray
                        $result = Invoke-AdbCommand -Target $Target -Command "pm uninstall --user 0 $pkg"
                        if ($result.Success -and $result.Output -notmatch "Failure") {
                            Write-Host "`r" -NoNewline
                            Write-Success "Uninstalled: $name"
                            $Script:Summary.Uninstalled++
                        } else {
                            Write-Host ""
                            Write-ErrorMsg "Uninstall failed: $($result.Output)"
                            $Script:Summary.Failed++
                        }
                    } else {
                        Write-Host "    Disabling..." -NoNewline -ForegroundColor Gray
                        $result = Invoke-AdbCommand -Target $Target -Command "pm disable-user --user 0 $pkg"
                        if ($result.Success -and $result.Output -notmatch "Failure") {
                            Write-Host "`r" -NoNewline
                            Write-Success "Disabled: $name"
                            $Script:Summary.Disabled++
                        } else {
                            Write-Host ""
                            Write-ErrorMsg "Disable failed: $($result.Output)"
                            $Script:Summary.Failed++
                        }
                    }
                } else {
                    $Script:Summary.Skipped++
                }
            } else {
                $opts = @("RESTORE", "SKIP", "ABORT")
                if ($defRest -eq "Y") { $defIdx = 0 } else { $defIdx = 1 }

                # UX #A: Apply defaults or prompt
                if ($applyAll) {
                    $toggleResult = $defIdx
                } else {
                    $toggleResult = Read-Toggle -Prompt "    >> Action:" -Options $opts -DefaultIndex $defIdx
                    if ($toggleResult -eq -1) { $toggleResult = 2 }  # ESC = ABORT
                }
                $selStr = $opts[$toggleResult]

                # Handle ABORT
                if ($selStr -eq "ABORT") {
                    Write-Warn "Process aborted by user."
                    Show-TaskSummary -Mode $Mode -Aborted
                    Pause
                    return
                }

                if ($selStr -eq "RESTORE") {
                    Write-Host "    Attempting Recovery..." -NoNewline -ForegroundColor Gray
                    if ($isInstalled) {
                        # FIX #10: Use helper with error checking
                        $result = Invoke-AdbCommand -Target $Target -Command "pm enable $pkg"
                        if ($result.Success) {
                            Write-Success "Re-enabled."
                            $Script:Summary.Restored++
                        } else {
                            Write-ErrorMsg "Enable failed: $($result.Error)"
                            $Script:Summary.Failed++
                        }
                    } else {
                        # FIX #6: Renamed $res to $installResult to avoid shadowing
                        $installResult = Invoke-AdbCommand -Target $Target -Command "cmd package install-existing $pkg"
                        if ($installResult.Output -match "Package .* doesn't exist") {
                            Write-Host " [FILE MISSING]" -ForegroundColor Red
                            $playIdx = Read-Toggle -Prompt "    >> Open Play Store?" -Options @("YES", "NO") -DefaultIndex 0
                            if ($playIdx -eq 0) {
                                Open-PlayStore -Target $Target -PkgId $pkg
                                $Script:Summary.Restored++
                            } else {
                                $Script:Summary.Skipped++
                            }
                        } else {
                            # FIX #10: Also enable after install-existing
                            $enableResult = Invoke-AdbCommand -Target $Target -Command "pm enable $pkg"
                            if ($enableResult.Success) {
                                Write-Success "Restored."
                                $Script:Summary.Restored++
                            } else {
                                Write-ErrorMsg "Restore failed: $($enableResult.Error)"
                                $Script:Summary.Failed++
                            }
                        }
                    }
                } elseif ($selStr -eq "SKIP") {
                    $Script:Summary.Skipped++
                }
            }
        }
    }

    Write-Header "Performance Settings ($Mode)"

    # 1. Animation
    $currAnim = (& $Script:AdbPath -s $Target shell settings get global window_animation_scale | Out-String).Trim()
    # FIX #9: Fixed string interpolation - use subexpression
    if ($Mode -eq "Optimize") { $tAnim = "0.5" } else { $tAnim = "1.0" }

    Write-Host "Animation Speed" -NoNewline
    if ($currAnim -eq $tAnim) { Write-Host " [ALREADY $tAnim]" -ForegroundColor DarkGray }
    else {
        Write-Host " [Current: ${currAnim}]" -ForegroundColor Yellow

        if ($applyAll) {
            $idx = 0  # Default to YES
        } else {
            # FIX #9: Use subexpression for variable in string
            $idx = Read-Toggle -Prompt "    >> Set to $($tAnim)?" -Options @("YES", "NO", "ABORT") -DefaultIndex 0
            if ($idx -eq -1) { $idx = 2 }  # ESC = ABORT
        }

        # Handle ABORT
        if ($idx -eq 2) {
            Write-Warn "Process aborted by user."
            Show-TaskSummary -Mode $Mode -Aborted
            Pause
            return
        }

        if ($idx -eq 0) {
            # FIX #10: Use helper with error checking
            $result1 = Invoke-AdbCommand -Target $Target -Command "settings put global window_animation_scale $tAnim"
            $result2 = Invoke-AdbCommand -Target $Target -Command "settings put global transition_animation_scale $tAnim"
            $result3 = Invoke-AdbCommand -Target $Target -Command "settings put global animator_duration_scale $tAnim"

            if ($result1.Success -and $result2.Success -and $result3.Success) {
                Write-Success "Applied."
            } else {
                Write-ErrorMsg "Failed to apply animation settings."
            }
        }
    }

    # 2. Process Limit
    $currProc = (& $Script:AdbPath -s $Target shell settings get global background_process_limit | Out-String).Trim()
    if ($currProc -eq "null" -or $currProc -eq "") { $currProc = "Standard" }

    Write-Host "`nBackground Process Limit" -NoNewline
    Write-Host " [Current: $currProc]" -ForegroundColor Gray

    if ($Mode -eq "Optimize") {
        if ($applyAll) {
            # Default to "At Most 2" (index 2)
            $sel = 2
        } else {
            $procOpts = @("Standard", "At Most 1", "At Most 2", "At Most 3", "At Most 4", "Skip", "Abort")
            $procDescs = @("Unlimited background apps.", "Aggressive RAM saving.", "Balanced RAM saving.", "Moderate.", "Light limit.", "Do not change.", "Cancel and return to menu.")
            # Shortcuts: S=Standard, 1-4 for limits, K=sKip, X=abort
            $procShortcuts = @("S", "1", "2", "3", "4", "K", "X")
            $sel = Read-Menu -Title "Select Process Limit" -Options $procOpts -Descriptions $procDescs -DefaultIndex 2 -Shortcuts $procShortcuts
            if ($sel -eq -1) { $sel = 6 }  # ESC = Abort
        }

        # Handle Abort
        if ($sel -eq 6) {
            Write-Warn "Process aborted by user."
            Show-TaskSummary -Mode $Mode -Aborted
            Pause
            return
        }

        $val = $null
        if ($sel -eq 1) { $val = 1 }
        elseif ($sel -eq 2) { $val = 2 }
        elseif ($sel -eq 3) { $val = 3 }
        elseif ($sel -eq 4) { $val = 4 }

        if ($sel -eq 0) { # Standard
            $result = Invoke-AdbCommand -Target $Target -Command "settings delete global background_process_limit"
            if ($result.Success) {
                Write-Success "Reset to Standard."
            } else {
                Write-ErrorMsg "Failed to reset process limit."
            }
        } elseif ($val) {
            $result = Invoke-AdbCommand -Target $Target -Command "settings put global background_process_limit $val"
            if ($result.Success) {
                Write-Success "Set to $val processes."
            } else {
                Write-ErrorMsg "Failed to set process limit."
            }
        }
    } else {
        if ($currProc -ne "Standard") {
            if ($applyAll) {
                $idx = 0  # Default to YES
            } else {
                $idx = Read-Toggle -Prompt "    >> Reset to Standard?" -Options @("YES", "NO", "ABORT") -DefaultIndex 0
                if ($idx -eq -1) { $idx = 2 }  # ESC = ABORT
            }

            # Handle ABORT
            if ($idx -eq 2) {
                Write-Warn "Process aborted by user."
                Show-TaskSummary -Mode $Mode -Aborted
                Pause
                return
            }

            if ($idx -eq 0) {
                $result = Invoke-AdbCommand -Target $Target -Command "settings delete global background_process_limit"
                if ($result.Success) {
                    Write-Success "Reset."
                } else {
                    Write-ErrorMsg "Failed to reset process limit."
                }
            }
        }
    }

    Show-TaskSummary -Mode $Mode

    Write-Header "Finished"
    $r = Read-Toggle -Prompt "Reboot Device Now?" -Options @("YES", "NO") -DefaultIndex 1
    if ($r -eq 0) {
        # FIX #15: Show reboot confirmation
        Write-Info "Rebooting device..."
        & $Script:AdbPath -s $Target reboot
        Write-Success "Reboot command sent."
    }
}

# FIX #4: IP Validation function
function Test-ValidIP ($IP) {
    if ($IP -match "^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(:\d+)?$") {
        $octets = @([int]$matches[1], [int]$matches[2], [int]$matches[3], [int]$matches[4])
        foreach ($octet in $octets) {
            if ($octet -lt 0 -or $octet -gt 255) { return $false }
        }
        return $true
    }
    return $false
}

# --- PANIC RECOVERY ---
# Re-enable all disabled packages (emergency restore)
function Run-PanicRecovery ($Target) {
    Write-Header "Panic Recovery Mode"
    Write-Warn "This will re-enable ALL disabled packages on the device."
    Write-Host ""

    $confirm = Read-Toggle -Prompt "Are you sure you want to proceed?" -Options @("YES", "NO") -DefaultIndex 1
    if ($confirm -ne 0) {
        Write-Info "Recovery cancelled."
        return
    }

    Write-Info "Fetching disabled packages..."
    $disabledPkgs = & $Script:AdbPath -s $Target shell pm list packages -d 2>&1 | Out-String

    $packages = @()
    foreach ($line in ($disabledPkgs -split "`n")) {
        if ($line -match "^package:(.+)$") {
            $packages += $matches[1].Trim()
        }
    }

    if ($packages.Count -eq 0) {
        Write-Success "No disabled packages found. Nothing to restore."
        return
    }

    Write-Info "Found $($packages.Count) disabled packages. Re-enabling..."
    $restored = 0
    $failed = 0

    foreach ($pkg in $packages) {
        try {
            $result = Invoke-AdbCommand -Target $Target -Command "pm enable $pkg"
            if ($result.Success) {
                Write-Success "Enabled: $pkg"
                $restored++
            } else {
                Write-ErrorMsg "Failed: $pkg"
                $failed++
            }
        }
        catch {
            Write-ErrorMsg "Error: $pkg"
            $failed++
        }
    }

    Write-Header "Recovery Summary"
    Write-Host " Restored: $restored packages" -ForegroundColor Green
    if ($failed -gt 0) {
        Write-Host " Failed:   $failed packages" -ForegroundColor Red
    }
    Write-Info "You may need to reboot for changes to take effect."
}

# --- REBOOT OPTIONS ---
function Show-RebootMenu ($Target) {
    Write-Header "Reboot Options"

    $rOpts = @("Normal Reboot", "Recovery Mode", "Bootloader", "Cancel")
    $rDescs = @(
        "Standard device restart.",
        "Boot into recovery mode (for advanced users).",
        "Boot into bootloader/fastboot (for advanced users).",
        "Return without rebooting."
    )
    $rShortcuts = @("N", "R", "B", "C")

    $sel = Read-Menu -Title "Reboot Device" -Options $rOpts -Descriptions $rDescs -Shortcuts $rShortcuts

    if ($sel -eq -1 -or $sel -eq 3) { return }

    $confirm = Read-Toggle -Prompt "Confirm reboot?" -Options @("YES", "NO") -DefaultIndex 1
    if ($confirm -ne 0) { return }

    switch ($sel) {
        0 {
            Write-Info "Rebooting device..."
            & $Script:AdbPath -s $Target reboot
            Write-Success "Reboot command sent."
        }
        1 {
            Write-Info "Booting to recovery mode..."
            & $Script:AdbPath -s $Target reboot recovery
            Write-Success "Recovery mode command sent."
        }
        2 {
            Write-Info "Booting to bootloader..."
            & $Script:AdbPath -s $Target reboot bootloader
            Write-Success "Bootloader command sent."
        }
    }
}

# --- MAIN MENU ---

# Demo mode: display all screens and exit
if ($Demo) {
    Show-DemoScreens
    exit
}

Clear-Host; Check-Adb

# FIX #5: Window resize with error handling
try {
    $currentSize = (Get-Host).UI.RawUI.WindowSize
    if ($currentSize -and $currentSize.Width -lt 80) {
        $Host.UI.RawUI.WindowSize = New-Object Management.Automation.Host.Size(100, 35)
    }
} catch {
    # Silently continue - Windows Terminal and VS Code don't support window resizing
}

# UX #F: Show version in header
Write-Header "ANDROID TV OPTIMIZER $Script:Version"

while ($true) {
    $devs = @(Get-Devices)
    $mOpts = @(); $mDescs = @()

    if ($devs.Count -gt 0) {
        foreach ($d in $devs) {
            $status = $d.Status
            $typeName = Get-DeviceTypeName $d.Type
            if ($status -eq "device") { $txt = $d.Name } else { $txt = "$($d.Name) [$status]" }
            $mOpts += $txt
            $mDescs += "$typeName | $($d.Model) | $($d.Serial)"
        }
    }

    # Static options start after devices (devices use numbers, everything else uses letters)
    $staticStart = $devs.Count

    $mOpts += "Scan Network"; $mDescs += "Auto-discover Android TV devices on local network."
    $mOpts += "Connect IP"; $mDescs += "Manually connect to a specific IP address."
    $mOpts += "Report All"; $mDescs += "Run Health Check on ALL connected devices."
    $mOpts += "Refresh"; $mDescs += "Reload device list."
    # UX #E: ADB Server Restart option
    $mOpts += "Restart ADB"; $mDescs += "Kill and restart ADB server (fixes connection issues)."
    $mOpts += "Help"; $mDescs += "View instructions and troubleshooting."
    $mOpts += "Quit"; $mDescs += "Exit Optimizer."

    # UX #F: Show version in menu title
    # Pass StaticStartIndex so devices use numbers, options use letters
    # Shortcuts: S=Scan, C=Connect, R=Report, F=reFresh, A=ADB, H=Help, Q=Quit
    $mainShortcuts = @("S", "C", "R", "F", "A", "H", "Q")
    $sel = Read-Menu -Title "Android TV Optimizer $Script:Version - Main Menu" -Options $mOpts -Descriptions $mDescs -StaticStartIndex $staticStart -Shortcuts $mainShortcuts

    # Handle ESC
    if ($sel -eq -1) { continue }

    $selText = $mOpts[$sel]

    if ($selText -eq "Scan Network") { Scan-Network; continue }
    if ($selText -eq "Connect IP") {
        $i = Read-Host "Enter IP Address (e.g., 192.168.1.100)"
        # FIX #4: Validate IP before connecting
        if (Test-ValidIP $i) {
            Write-Info "Connecting to $i..."
            & $Script:AdbPath connect $i
        } else {
            Write-ErrorMsg "Invalid IP address format: $i"
            Start-Sleep -Milliseconds 1500
        }
        continue
    }
    if ($selText -eq "Refresh") { continue }
    if ($selText -eq "Report All") {
        foreach ($d in $devs) {
            if ($d.Status -eq "device") {
                Run-Report -Target $d.Serial -Name $d.Name -DeviceType $d.Type
            }
        }
        Pause; continue
    }
    # UX #E: Handle ADB restart
    if ($selText -eq "Restart ADB") { Restart-AdbServer; continue }
    if ($selText -eq "Help") { Show-Help; continue }
    if ($selText -eq "Quit") { Exit }

    if ($sel -lt $devs.Count) {
        $target = $devs[$sel]

        if ($target.Status -ne "device") {
            Write-Warn "Cannot manage device in state: $($target.Status)"
            Pause; continue
        }

        # Get device type name for display
        $deviceTypeName = Get-DeviceTypeName $target.Type

        # Action menu with device type info
        $aOpts = @("Optimize", "Restore", "Report", "Launcher Setup", "Profile", "Recovery", "Reboot", "Disconnect", "Back")
        $aDescs = @(
            "Debloat apps and tune performance for $deviceTypeName.",
            "Undo optimizations and fix missing apps.",
            "Check Temp, RAM, Storage, and bloat status.",
            "Install Projectivy or switch launchers.",
            "View device profile and detected settings.",
            "Emergency: Re-enable ALL disabled packages.",
            "Restart device (normal, recovery, or bootloader).",
            "Disconnect this device from ADB.",
            "Return to Main Menu."
        )
        # Shortcuts: O=Optimize, R=Restore, E=rEport, L=Launcher, P=Profile, C=reCovery, B=reboot, D=Disconnect, K=bacK
        $actionShortcuts = @("O", "R", "E", "L", "P", "C", "B", "D", "K")
        $aSel = Read-Menu -Title "Action Menu: $($target.Name) ($deviceTypeName)" -Options $aOpts -Descriptions $aDescs -Shortcuts $actionShortcuts

        # Handle ESC
        if ($aSel -eq -1) { continue }

        $act = $aOpts[$aSel]

        if ($act -eq "Optimize") { Run-Task -Target $target.Serial -Mode "Optimize" -DeviceType $target.Type }
        if ($act -eq "Restore") { Run-Task -Target $target.Serial -Mode "Restore" -DeviceType $target.Type }
        if ($act -eq "Report") { Run-Report -Target $target.Serial -Name $target.Name -DeviceType $target.Type; Pause }
        if ($act -eq "Launcher Setup") { Setup-Launcher -Target $target.Serial; Pause }
        if ($act -eq "Profile") { Show-DeviceProfile -Target $target.Serial -DeviceInfo $target; Pause }
        if ($act -eq "Recovery") { Run-PanicRecovery -Target $target.Serial; Pause }
        if ($act -eq "Reboot") { Show-RebootMenu -Target $target.Serial; Pause }
        if ($act -eq "Disconnect") { Disconnect-Device -Serial $target.Serial; Pause }
    }
}
