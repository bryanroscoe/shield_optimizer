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

$Script:Version = "v63-crossplatform"
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# --- PLATFORM DETECTION ---
$Script:Platform = switch ($true) {
    $IsWindows { "Windows" }
    $IsMacOS   { "macOS" }
    $IsLinux   { "Linux" }
    default    { if ($env:OS -match "Windows") { "Windows" } else { "Unknown" } }
}
$Script:IsUnix = $Script:Platform -in @("macOS", "Linux")

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
    @("com.google.android.feedback", "Google Feedback", "DISABLE", "Safe", "Stops feedback data collection.", "Restores Google feedback services.", "Y", "Y"),
    @("com.android.printspooler", "Print Spooler", "DISABLE", "Safe", "Disables unused print service.", "Restores print service.", "Y", "Y"),
    @("com.android.gallery3d", "Android Gallery", "DISABLE", "Safe", "Removes legacy photo viewer.", "Restores legacy photo viewer.", "Y", "Y"),

    # [DEAD APPS - Universal]
    @("com.google.android.videos", "Google Play Movies", "UNINSTALL", "Safe", "Removes defunct app.", "Restores defunct app.", "Y", "Y"),
    @("com.google.android.music", "Google Play Music", "UNINSTALL", "Safe", "Removes defunct app.", "Restores defunct app.", "Y", "Y"),

    # [STREAMING APPS - Package names vary by device, include all variants]
    @("com.netflix.ninja", "Netflix", "UNINSTALL", "Safe", "Streaming App.", "Restores Netflix.", "N", "N"),
    @("com.amazon.amazonvideo.livingroom", "Amazon Prime Video", "UNINSTALL", "Safe", "Streaming App.", "Restores Prime Video.", "N", "N"),
    @("com.amazon.amazonvideo.livingroom.nvidia", "Amazon Prime Video (Shield)", "UNINSTALL", "Safe", "Streaming App.", "Restores Prime Video.", "N", "N"),
    @("com.wbd.stream", "Max (HBO)", "UNINSTALL", "Safe", "Streaming App.", "Restores Max.", "N", "N"),
    @("com.discovery.discoveryplus.androidtv", "Discovery+", "UNINSTALL", "Safe", "Streaming App.", "Restores Discovery+.", "N", "N"),
    @("com.hulu.livingroomplus", "Hulu", "UNINSTALL", "Safe", "Streaming App.", "Restores Hulu.", "N", "N"),
    @("tv.twitch.android.app", "Twitch", "UNINSTALL", "Safe", "Streaming App.", "Restores Twitch.", "N", "N"),
    @("com.disney.disneyplus", "Disney+", "UNINSTALL", "Safe", "Streaming App.", "Restores Disney+.", "N", "N"),
    @("com.disney.disneyplus.prod", "Disney+ (Alt)", "UNINSTALL", "Safe", "Streaming App.", "Restores Disney+.", "N", "N"),
    @("com.spotify.tv.android", "Spotify", "UNINSTALL", "Safe", "Streaming App.", "Restores Spotify.", "N", "N"),
    @("com.google.android.youtube.tvmusic", "YouTube Music", "UNINSTALL", "Safe", "Streaming App.", "Restores YouTube Music.", "N", "N"),
    @("com.apple.atve.androidtv.appletv", "Apple TV", "UNINSTALL", "Safe", "Streaming App.", "Restores Apple TV.", "N", "N"),
    @("com.cbs.ott", "Paramount+", "UNINSTALL", "Safe", "Streaming App.", "Restores Paramount+.", "N", "N"),
    @("com.crunchyroll.crunchyroid", "Crunchyroll", "UNINSTALL", "Safe", "Streaming App.", "Restores Crunchyroll.", "N", "N"),
    @("air.com.vudu.air.DownloaderTablet", "Vudu", "UNINSTALL", "Safe", "Streaming App.", "Restores Vudu.", "N", "N"),

    # [MEDIUM RISK - Universal]
    @("com.android.dreams.basic", "Basic Daydream", "DISABLE", "Medium", "Disables basic screensaver.", "Restores basic screensaver.", "N", "Y"),
    @("com.android.providers.tv", "Live Channels Provider", "DISABLE", "Medium", "Disables Live TV provider.", "Restores Live Channels support.", "N", "Y"),

    # [HIGH RISK - Universal]
    @("com.google.android.katniss", "Google Assistant", "DISABLE", "High Risk", "Breaks voice search.", "Restores voice search.", "N", "Y"),
    @("com.google.android.apps.mediashell", "Chromecast Built-in", "DISABLE", "High Risk", "Breaks casting to device.", "Restores Chromecast.", "N", "Y"),
    @("com.google.android.tts", "Google Text-to-Speech", "DISABLE", "High Risk", "Breaks accessibility features.", "Restores text-to-speech.", "N", "Y"),
    @("com.google.android.play.games", "Google Play Games", "DISABLE", "Medium Risk", "May break cloud saves.", "Restores Play Games.", "N", "Y"),

    # [HOME HANDLERS - Present on both Shield and Google TV]
    @("com.google.android.tungsten.setupwraith", "Setup Wraith (HOME)", "DISABLE", "High Risk", "HOME handler! Use Launcher Wizard instead.", "Restores setup wizard HOME handler.", "N", "Y")
)

# NVIDIA Shield-specific apps
$Script:ShieldAppList = @(
    # [SHIELD SAFE - Telemetry]
    @("com.nvidia.stats", "Nvidia Telemetry", "DISABLE", "Safe", "Stops Nvidia data collection.", "Restores Nvidia telemetry.", "Y", "Y"),
    @("com.nvidia.diagtools", "Nvidia Diagnostics", "DISABLE", "Safe", "Stops diagnostic logging.", "Restores diagnostic tools.", "Y", "Y"),
    @("com.nvidia.feedback", "Nvidia Feedback", "DISABLE", "Safe", "Stops Nvidia feedback collection.", "Restores Nvidia feedback.", "Y", "Y"),
    @("com.google.android.tvrecommendations", "Sponsored Content", "DISABLE", "Safe", "Removes 'Sponsored' rows from home.", "Restores sponsored content rows.", "Y", "Y"),

    # [SHIELD MEDIUM]
    @("com.nvidia.osc", "Nvidia OSC", "DISABLE", "Medium", "Background optimization service.", "Restores optimization service.", "N", "Y"),
    @("com.nvidia.shieldtech.hooks", "Nvidia System Hooks", "DISABLE", "Medium", "Shield-specific system hooks.", "Restores system hooks.", "N", "Y"),
    @("com.nvidia.tegrazone3", "Nvidia Games", "DISABLE", "Medium Risk", "May break GeForce NOW.", "Restores Nvidia Games app.", "N", "Y"),
    @("com.nvidia.nvgamecast", "Nvidia GameStream", "DISABLE", "Medium", "GameStream casting service.", "Restores GameStream.", "N", "Y"),
    @("com.google.android.backdrop", "Ambient Mode", "DISABLE", "Medium", "Disables ambient/screensaver.", "Restores ambient mode.", "N", "Y"),
    @("com.google.android.speech.pumpkin", "Google Speech Services", "DISABLE", "High Risk", "Breaks voice dictation.", "Restores speech services.", "N", "Y"),

    # [SHIELD HIGH RISK]
    @("com.nvidia.ota", "Nvidia System Updater", "DISABLE", "High Risk", "Stops Shield OS updates.", "Restores system updates.", "N", "Y"),
    @("com.plexapp.mediaserver.smb", "Plex Media Server", "DISABLE", "Advanced", "Breaks local Plex hosting.", "Restores Plex Server.", "N", "Y"),

    # [SHIELD LAUNCHER]
    @("com.google.android.tvlauncher", "Stock Launcher", "DISABLE", "High Risk", "Requires custom launcher first!", "Restores stock home screen.", "N", "Y")
)

# Google TV-specific apps (Onn 4K, Chromecast, etc.)
$Script:GoogleTVAppList = @(
    # [GOOGLE TV SAFE - Bloatware]
    @("com.walmart.otto", "Walmart App", "UNINSTALL", "Safe", "Removes Walmart bloatware.", "Restores Walmart app.", "Y", "Y"),
    @("com.google.android.leanbacklauncher.recommendations", "Home Recommendations", "DISABLE", "Safe", "Removes extra recommendation rows.", "Restores home recommendations.", "Y", "Y"),

    # [GOOGLE TV MEDIUM]
    @("com.google.android.tungsten.overscan", "Overscan Calibrator", "DISABLE", "Medium", "Post-setup overscan tool.", "Restores overscan calibrator.", "N", "Y"),

    # [ONN-SPECIFIC - Amlogic devices]
    @("com.droidlogic.launcher.provider", "Droidlogic Launcher Provider", "DISABLE", "Medium", "Onn launcher data provider. Disable with launcher.", "Restores Onn launcher provider.", "N", "Y"),

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
    Write-Host "  ► " -NoNewline -ForegroundColor Cyan
    Write-OptionWithHighlight -Text "[1] Shield TV Pro" -Selected $true -WithClosingArrow $true
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

    Write-Host "  ► " -NoNewline -ForegroundColor Cyan
    Write-OptionWithHighlight -Text "[O]ptimize" -Selected $true -WithClosingArrow $true
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

    Write-SubHeader "Top Memory Users"
    Write-Host "  142.3 MB  " -NoNewline -ForegroundColor Yellow
    Write-Host "com.google.android.tvlauncher" -ForegroundColor Gray
    Write-Host "   89.7 MB  " -NoNewline -ForegroundColor White
    Write-Host "com.google.android.youtube.tv" -ForegroundColor Gray
    Write-Host "   67.2 MB  " -NoNewline -ForegroundColor White
    Write-Host "com.netflix.ninja" -ForegroundColor Gray
    Write-Host "   45.1 MB  " -NoNewline -ForegroundColor White
    Write-Host "com.google.android.tvrecommendations" -ForegroundColor Gray
    Write-Host "   32.8 MB  " -NoNewline -ForegroundColor White
    Write-Host "com.nvidia.tegrazone3" -ForegroundColor Gray

    Write-SubHeader "Bloat Check"
    Write-Host " [ACTIVE BLOAT] Sponsored Content" -ForegroundColor Yellow
    Write-Host " [ACTIVE BLOAT] Google Feedback" -ForegroundColor Yellow
    Write-Host " [ACTIVE BLOAT] Walmart App" -ForegroundColor Yellow

    # ============================================================
    # SCREEN 4: Launcher Wizard
    # ============================================================
    Write-Separator
    Write-Host " Select Launcher" -ForegroundColor Cyan
    Write-Host " ================================================" -ForegroundColor DarkCyan

    Write-Host "  ► " -NoNewline -ForegroundColor Cyan
    Write-OptionWithHighlight -Text "[P]rojectivy Launcher [ACTIVE]" -Selected $true -WithClosingArrow $true
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
    Write-Host " [Safe]" -ForegroundColor Green -NoNewline
    Write-Host " (45.1 MB)" -ForegroundColor DarkGray
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
    Write-Host " [Safe]" -ForegroundColor Green -NoNewline
    Write-Host " (12.3 MB)" -ForegroundColor DarkGray
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

# Get memory usage for a specific package (returns MB or $null if not running)
function Get-AppMemoryUsage {
    param([string]$Target, [string]$Package)
    try {
        $mem = & $Script:AdbPath -s $Target shell "dumpsys meminfo $Package 2>/dev/null | grep 'TOTAL PSS'" 2>&1 | Out-String
        if ($mem -match "TOTAL PSS:\s+(\d+)") {
            $kb = [int]$matches[1]
            return [math]::Round($kb / 1024, 1)
        }
    } catch {}
    return $null
}

# Get top memory-consuming apps (returns array of @{Name; Package; MB})
function Get-TopMemoryApps {
    param([string]$Target, [int]$Count = 5)
    $apps = @()
    try {
        # dumpsys meminfo outputs sorted by PSS - parse the summary section
        $mem = & $Script:AdbPath -s $Target shell "dumpsys meminfo" 2>&1 | Out-String

        # Look for lines in "Total PSS by process:" section
        # Format: "123,456K: com.example.app (pid 1234)"
        $inSection = $false
        foreach ($line in ($mem -split "`n")) {
            if ($line -match "Total PSS by process:") { $inSection = $true; continue }
            if ($inSection -and $line -match "^\s*$") { break }  # Empty line ends section
            if ($inSection -and $line -match "^\s*([\d,]+)K:\s+([a-zA-Z0-9_.]+)") {
                $kb = [int]($matches[1] -replace ",", "")
                $pkg = $matches[2]
                # Skip system processes, focus on apps
                if ($pkg -match "^com\." -or $pkg -match "^tv\." -or $pkg -match "^me\.") {
                    $apps += @{ Package = $pkg; MB = [math]::Round($kb / 1024, 1) }
                    if ($apps.Count -ge $Count) { break }
                }
            }
        }
    } catch {}
    return $apps
}

# FIX #8: $PSScriptRoot fallback when dot-sourced
function Get-ScriptDirectory {
    if ($PSScriptRoot -and $PSScriptRoot -ne "") {
        return $PSScriptRoot
    }
    return (Get-Location).Path
}

# --- CROSS-PLATFORM ADB HELPERS ---
function Get-AdbConfig {
    $config = @{
        BinaryName = "adb"
        ExtraFiles = @()
        DownloadUrl = ""
    }

    switch ($Script:Platform) {
        "Windows" {
            $config.BinaryName = "adb.exe"
            $config.ExtraFiles = @("AdbWinApi.dll", "AdbWinUsbApi.dll")
            $config.DownloadUrl = "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"
        }
        "macOS" {
            $config.BinaryName = "adb"
            $config.ExtraFiles = @()
            $config.DownloadUrl = "https://dl.google.com/android/repository/platform-tools-latest-darwin.zip"
        }
        "Linux" {
            $config.BinaryName = "adb"
            $config.ExtraFiles = @()
            $config.DownloadUrl = "https://dl.google.com/android/repository/platform-tools-latest-linux.zip"
        }
        default {
            # Fallback to Windows
            $config.BinaryName = "adb.exe"
            $config.ExtraFiles = @("AdbWinApi.dll", "AdbWinUsbApi.dll")
            $config.DownloadUrl = "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"
        }
    }

    return $config
}

function Stop-AdbProcess {
    if ($Script:IsUnix) {
        # Unix: use pkill or killall
        try {
            $null = & pkill -f "adb" 2>&1
        } catch {
            try {
                $null = & killall adb 2>&1
            } catch {}
        }
    } else {
        # Windows: use Stop-Process
        try {
            Stop-Process -Name "adb" -Force -ErrorAction SilentlyContinue
        } catch {}
    }
}

function Check-Adb {
    $ScriptDir = Get-ScriptDirectory
    $adbConfig = Get-AdbConfig
    $AdbExe = Join-Path $ScriptDir $adbConfig.BinaryName
    $Script:AdbPath = ""

    # Force download if flag is set - skip all checks and download fresh
    if ($Script:ForceAdbDownload) {
        Write-Warn "Forcing ADB re-download..."
        # Kill ADB server first so files aren't locked
        if (Test-Path $AdbExe) {
            try { & $AdbExe kill-server 2>$null } catch {}
        }
        Stop-AdbProcess
        Start-Sleep -Milliseconds 500
        Remove-Item $AdbExe -Force -ErrorAction SilentlyContinue
        foreach ($extraFile in $adbConfig.ExtraFiles) {
            Remove-Item (Join-Path $ScriptDir $extraFile) -Force -ErrorAction SilentlyContinue
        }
        # Fall through to download section
    }
    elseif (Test-Path $AdbExe) {
        $Script:AdbPath = $AdbExe
    }
    elseif (Get-Command $adbConfig.BinaryName -ErrorAction SilentlyContinue) {
        $Script:AdbPath = (Get-Command $adbConfig.BinaryName).Source
    }

    if ($Script:AdbPath -eq "") {
        if (-not $Script:ForceAdbDownload) { Write-Warn "ADB missing. Downloading..." }
        $Url = $adbConfig.DownloadUrl
        $Zip = Join-Path $ScriptDir "adb_temp.zip"
        $Ext = Join-Path $ScriptDir "adb_temp_extract"
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -Uri $Url -OutFile $Zip -UseBasicParsing
            Expand-Archive -Path $Zip -DestinationPath $Ext -Force
            Move-Item (Join-Path $Ext "platform-tools" $adbConfig.BinaryName) $ScriptDir -Force
            foreach ($extraFile in $adbConfig.ExtraFiles) {
                Move-Item (Join-Path $Ext "platform-tools" $extraFile) $ScriptDir -Force
            }
            Remove-Item $Zip -Force; Remove-Item $Ext -Recurse -Force
            # Make executable on Unix
            if ($Script:IsUnix) {
                & chmod +x $AdbExe
            }
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

# --- CROSS-PLATFORM NETWORK HELPERS ---
function Get-ArpTable {
    $ips = @()

    switch ($Script:Platform) {
        "Windows" {
            # Windows: arp -a with "dynamic" filter
            $arpOutput = arp -a 2>$null | Select-String "dynamic"
            foreach ($line in $arpOutput) {
                if ($line -match "(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})") {
                    $ips += $matches[1]
                }
            }
        }
        "macOS" {
            # macOS: arp -a shows IPs in parentheses like (192.168.1.100)
            # Filter out "permanent" entries (multicast/broadcast)
            $arpOutput = arp -a 2>$null
            foreach ($line in $arpOutput) {
                # Skip permanent entries (multicast/broadcast)
                if ($line -match "permanent") { continue }
                if ($line -match "\((\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})\)") {
                    $ips += $matches[1]
                }
            }
        }
        "Linux" {
            # Linux: try ip neigh first, then fall back to arp -n
            $arpOutput = $null
            try {
                $arpOutput = & ip neigh 2>$null
            } catch {
                try {
                    $arpOutput = arp -n 2>$null
                } catch {}
            }
            if ($arpOutput) {
                foreach ($line in $arpOutput) {
                    if ($line -match "^(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})") {
                        $ips += $matches[1]
                    }
                }
            }
        }
        default {
            # Fallback: try Windows-style arp
            $arpOutput = arp -a 2>$null | Select-String "dynamic"
            foreach ($line in $arpOutput) {
                if ($line -match "(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})") {
                    $ips += $matches[1]
                }
            }
        }
    }

    # Filter out non-unicast addresses (multicast 224-239, broadcast, localhost)
    $ips = $ips | Where-Object {
        $firstOctet = [int]($_ -split '\.')[0]
        $firstOctet -ge 1 -and $firstOctet -le 223 -and $firstOctet -ne 127
    }

    return $ips
}

# Get local subnet for scanning
function Get-LocalSubnet {
    $subnet = $null
    try {
        # Get the default gateway's network
        if ($Script:Platform -eq "macOS") {
            # macOS: route -n get default
            $routeOutput = route -n get default 2>$null | Out-String
            if ($routeOutput -match "gateway:\s*(\d+\.\d+\.\d+\.\d+)") {
                $gateway = $matches[1]
                $octets = $gateway -split '\.'
                $subnet = "$($octets[0]).$($octets[1]).$($octets[2])"
            }
        } elseif ($Script:Platform -eq "Linux") {
            # Linux: ip route (modern) or route -n (legacy)
            $routeOutput = ip route 2>$null | Out-String
            if ($routeOutput -match "default via (\d+\.\d+\.\d+\.\d+)") {
                $gateway = $matches[1]
                $octets = $gateway -split '\.'
                $subnet = "$($octets[0]).$($octets[1]).$($octets[2])"
            } elseif (-not $subnet) {
                # Fallback to legacy route command
                $routeOutput = route -n 2>$null | Out-String
                if ($routeOutput -match "0\.0\.0\.0\s+(\d+\.\d+\.\d+\.\d+)") {
                    $gateway = $matches[1]
                    $octets = $gateway -split '\.'
                    $subnet = "$($octets[0]).$($octets[1]).$($octets[2])"
                }
            }
        } else {
            # Windows: use Get-NetRoute
            $gateway = (Get-NetRoute -DestinationPrefix "0.0.0.0/0" -ErrorAction SilentlyContinue | Select-Object -First 1).NextHop
            if ($gateway) {
                $octets = $gateway -split '\.'
                $subnet = "$($octets[0]).$($octets[1]).$($octets[2])"
            }
        }
    } catch {}

    # Fallback to common subnets
    if (-not $subnet) {
        $subnet = "192.168.1"
    }
    return $subnet
}

# FIX #1: Socket leak fix and #14: Results feedback and #H: Timeout improvement
function Scan-Network {
    Write-Info "Scanning local subnet for Android TV devices..."

    # Get local subnet
    $subnet = Get-LocalSubnet
    Write-Host " Subnet: $subnet.x" -ForegroundColor Gray

    # Build list of IPs to scan: ARP table + full subnet sweep
    $ipsToScan = @{}

    # First, add IPs from ARP table (known devices)
    $arpIps = Get-ArpTable
    foreach ($ip in $arpIps) {
        $ipsToScan[$ip] = $true
    }

    # Add common device IPs in the subnet (1-254)
    # Prioritize common DHCP ranges
    $priorityRanges = @(100..150) + @(2..99) + @(151..254)
    foreach ($i in $priorityRanges) {
        $ip = "$subnet.$i"
        if (-not $ipsToScan.ContainsKey($ip)) {
            $ipsToScan[$ip] = $true
        }
    }

    $allIps = @($ipsToScan.Keys | Select-Object -First 254)
    $total = $allIps.Count
    $foundCount = 0
    $foundIps = @()

    Write-Host " Scanning $total addresses (parallel)..." -ForegroundColor Gray

    # Parallel scan using batched async connections
    $batchSize = 50  # Check 50 IPs at once
    $timeout = 80    # 80ms timeout per batch

    for ($batchStart = 0; $batchStart -lt $total; $batchStart += $batchSize) {
        $batchEnd = [Math]::Min($batchStart + $batchSize - 1, $total - 1)
        $batch = $allIps[$batchStart..$batchEnd]

        # Show progress
        $progress = [Math]::Min($batchEnd + 1, $total)
        Write-Host "`r Scanning... $progress/$total" -NoNewline -ForegroundColor DarkGray

        # Start all connections in this batch
        $connections = @()
        foreach ($ip in $batch) {
            try {
                $sock = New-Object System.Net.Sockets.TcpClient
                $asyncResult = $sock.BeginConnect($ip, 5555, $null, $null)
                $connections += @{ IP = $ip; Socket = $sock; Async = $asyncResult }
            } catch {
                # Skip this IP
            }
        }

        # Wait for timeout then check results
        Start-Sleep -Milliseconds $timeout

        # Check which connections succeeded
        foreach ($conn in $connections) {
            try {
                if ($conn.Async.IsCompleted -and $conn.Socket.Connected) {
                    $foundIps += $conn.IP
                }
            } catch {}
            finally {
                try { $conn.Socket.Close() } catch {}
                try { $conn.Socket.Dispose() } catch {}
            }
        }
    }

    Write-Host "`r                              `r" -NoNewline  # Clear progress line

    # Connect to found devices
    foreach ($ip in $foundIps) {
        Write-Success "Found device at $ip"
        & $Script:AdbPath connect $ip 2>$null | Out-Null
        $foundCount++
    }

    # FIX #14: Show results feedback
    if ($foundCount -eq 0) {
        Write-Warn "No devices found. Ensure Network Debugging is enabled on your Android TV."
        Write-Host " Tip: You can also use 'Connect IP' to enter the address manually." -ForegroundColor Gray
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
function Write-OptionWithHighlight ($Text, [bool]$Selected, [bool]$WithClosingArrow = $false) {
    # ANSI escape codes
    $esc = [char]27
    $bold = "$esc[1m"
    $reset = "$esc[0m"

    # Set styles based on selection state (DRY)
    $textColor = if ($Selected) { "Cyan" } else { "Gray" }
    $bracketCharColor = if ($Selected) { "Cyan" } else { "DarkGray" }
    $prefix = if ($Selected) { $bold } else { "" }
    $suffix = if ($Selected) { $reset } else { "" }

    # Helper to write styled text
    $writeStyled = { param($content, $color) Write-Host "$prefix$content$suffix" -NoNewline -ForegroundColor $color }

    $remaining = $Text

    while ($remaining.Length -gt 0) {
        $bracketStart = $remaining.IndexOf('[')

        if ($bracketStart -lt 0) {
            & $writeStyled $remaining $textColor
            break
        }

        if ($bracketStart -gt 0) {
            & $writeStyled $remaining.Substring(0, $bracketStart) $textColor
        }

        $bracketEnd = $remaining.IndexOf(']', $bracketStart)
        if ($bracketEnd -lt 0) {
            & $writeStyled $remaining.Substring($bracketStart) $textColor
            break
        }

        $bracketContent = $remaining.Substring($bracketStart + 1, $bracketEnd - $bracketStart - 1)

        # Determine bracket content color based on type
        $contentColor = switch -Regex ($bracketContent) {
            "^[A-Z0-9]$"    { "Yellow" }      # Single char shortcut key
            "ACTIVE"        { "Green" }
            "INSTALLED"     { "Cyan" }
            "ENABLED"       { "Cyan" }
            "MISSING"       { "Red" }
            "DISABLED"      { "DarkYellow" }
            "NOT FOUND"     { "DarkGray" }
            default         { $textColor }
        }

        & $writeStyled "[" $bracketCharColor
        & $writeStyled $bracketContent $contentColor
        & $writeStyled "]" $bracketCharColor

        $remaining = $remaining.Substring($bracketEnd + 1)
    }

    # Add closing arrow for selected items
    if ($Selected -and $WithClosingArrow) {
        Write-Host " ◄" -NoNewline -ForegroundColor Cyan
    }

    Write-Host ""
}

# --- CROSS-PLATFORM CONSOLE HELPERS ---
function Hide-Cursor {
    # Use ANSI escape sequence (works on all modern terminals including Windows Terminal, macOS, Linux)
    [Console]::Write("$([char]27)[?25l")
}

function Show-Cursor {
    # Use ANSI escape sequence (works on all modern terminals)
    [Console]::Write("$([char]27)[?25h")
}

function Get-KeyInput {
    # Cross-platform key input handler
    # Returns a hashtable with: Key (normalized name), Char (character if printable)
    $key = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")

    $result = @{
        Key = $null
        Char = $null
    }

    # Try VirtualKeyCode first (works on Windows, may work on Unix with PS7+)
    $vk = $key.VirtualKeyCode
    $char = $key.Character
    $handled = $false

    # Handle special keys via VirtualKeyCode
    switch ($vk) {
        27 { $result.Key = "Escape"; $handled = $true }
        13 { $result.Key = "Enter"; $handled = $true }
        38 { $result.Key = "Up"; $handled = $true }      # UpArrow
        40 { $result.Key = "Down"; $handled = $true }    # DownArrow
        37 { $result.Key = "Left"; $handled = $true }    # LeftArrow
        39 { $result.Key = "Right"; $handled = $true }   # RightArrow
    }

    # Handle letters and numbers via Character (more reliable cross-platform)
    if (-not $handled -and $char) {
        if ($char -match '[0-9]') {
            $result.Key = "Number"
            $result.Char = $char
            $handled = $true
        }
        elseif ($char -match '[a-zA-Z]') {
            $result.Key = "Letter"
            $result.Char = $char.ToString().ToUpper()
            $handled = $true
        }
    }

    # If VirtualKeyCode didn't give us a result, try character-based detection (Unix fallback)
    if (-not $handled -and $Script:IsUnix) {
        $char = $key.Character
        $charCode = [int]$char

        switch ($charCode) {
            27 { # ESC or start of escape sequence
                # Brief delay to allow escape sequence to fully arrive
                Start-Sleep -Milliseconds 30
                # Check if it's an arrow key sequence (ESC [ A/B/C/D)
                if ([Console]::KeyAvailable) {
                    $seq1 = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
                    if ($seq1.Character -eq '[') {
                        Start-Sleep -Milliseconds 20
                        if ([Console]::KeyAvailable) {
                            $seq2 = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
                            switch ($seq2.Character) {
                                'A' { $result.Key = "Up" }
                                'B' { $result.Key = "Down" }
                                'C' { $result.Key = "Right" }
                                'D' { $result.Key = "Left" }
                                default { $result.Key = "Escape" }
                            }
                        } else {
                            $result.Key = "Escape"
                        }
                    } else {
                        $result.Key = "Escape"
                    }
                } else {
                    $result.Key = "Escape"
                }
            }
            13 { $result.Key = "Enter" }
            10 { $result.Key = "Enter" }  # Linux newline
            default {
                if ($char -match '[0-9]') {
                    $result.Key = "Number"
                    $result.Char = $char
                }
                elseif ($char -match '[a-zA-Z]') {
                    $result.Key = "Letter"
                    $result.Char = $char.ToString().ToUpper()
                }
                else {
                    $result.Char = $char
                }
            }
        }
    }
    elseif (-not $handled) {
        $result.Char = $key.Character
    }

    return $result
}

# --- NEW VERTICAL MENU SYSTEM ---
# FIX #13: ESC key support, FIX #3: Cursor error handling, UX #C: Number/letter key shortcuts
# Flicker-free: only redraws changed lines instead of full screen
function Read-Menu ($Title, $Options, $Descriptions, $DefaultIndex=0, $StaticStartIndex=-1, $Shortcuts=$null) {
    $idx = $DefaultIndex
    $prevIdx = -1
    $max = $Options.Count - 1

    # ANSI escape codes
    $esc = [char]27
    $clearLine = "$esc[2K"

    # Helper: move cursor to row (1-based)
    function MoveTo($row) { [Console]::Write("$esc[$row;1H") }

    # If StaticStartIndex not specified, all items use letters
    if ($StaticStartIndex -lt 0) { $StaticStartIndex = 0 }

    # Build shortcut mapping and display text cache
    $shortcutMap = @{}
    $shortcutDisplay = @{}
    $displayTexts = @{}
    $separatorIndices = @{}  # Track separator rows

    for ($i = 0; $i -lt $Options.Count; $i++) {
        # Handle separators
        if ($Options[$i] -eq "---") {
            $separatorIndices[$i] = $true
            $displayTexts[$i] = "---"
            $shortcutDisplay[$i] = ""
            continue
        }

        if ($i -lt $StaticStartIndex) {
            # Count non-separator items before this for numbering
            $numBefore = 0
            for ($j = 0; $j -lt $i; $j++) {
                if ($Options[$j] -ne "---") { $numBefore++ }
            }
            $shortcutDisplay[$i] = "$($numBefore + 1)"
        } else {
            $staticIdx = $i - $StaticStartIndex
            # Adjust for separators
            for ($j = $StaticStartIndex; $j -lt $i; $j++) {
                if ($Options[$j] -eq "---") { $staticIdx-- }
            }
            if ($Shortcuts -and $staticIdx -ge 0 -and $staticIdx -lt $Shortcuts.Count) {
                $char = $Shortcuts[$staticIdx].ToUpper()
            } else {
                $char = $Options[$i].Substring(0,1).ToUpper()
            }
            $shortcutDisplay[$i] = $char
            $shortcutMap[$char] = $i
        }

        # Pre-compute display text with embedded shortcut
        $shortcut = $shortcutDisplay[$i]
        $optText = $Options[$i]
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
            $displayTexts[$i] = $optText.Substring(0, $foundPos) + "[$actualChar]" + $optText.Substring($foundPos + 1)
        } else {
            $displayTexts[$i] = "[$shortcut] $optText"
        }
    }

    # Layout: Row 1=title, Row 2=separator, Rows 3..N+2=options, Row N+3=separator, Row N+4=info, Row N+5=hints
    $menuStartRow = 3
    $infoRow = $menuStartRow + $Options.Count + 1
    $hintsRow = $infoRow + 1

    # Hide cursor (cross-platform)
    Hide-Cursor

    # Helper: draw a single menu item
    function DrawItem($itemIdx, $selected) {
        $row = $menuStartRow + $itemIdx
        MoveTo $row
        [Console]::Write($clearLine)

        # Handle separators
        if ($separatorIndices.ContainsKey($itemIdx)) {
            Write-Host "    --------------------------------" -ForegroundColor DarkGray
            return
        }

        # Check if this is an unauthorized item
        $isUnauthorized = $Options[$itemIdx] -match "UNAUTHORIZED"

        if ($selected) {
            Write-Host "  ► " -NoNewline -ForegroundColor Cyan
            if ($isUnauthorized) {
                Write-Host $displayTexts[$itemIdx] -NoNewline -ForegroundColor Red
                Write-Host " ◄" -ForegroundColor Cyan
            } else {
                Write-OptionWithHighlight -Text $displayTexts[$itemIdx] -Selected $true -WithClosingArrow $true
            }
        } else {
            Write-Host "    " -NoNewline
            if ($isUnauthorized) {
                Write-Host $displayTexts[$itemIdx] -ForegroundColor Red
            } else {
                Write-OptionWithHighlight -Text $displayTexts[$itemIdx] -Selected $false
            }
        }
    }

    # Helper: draw info line
    function DrawInfo($itemIdx) {
        MoveTo $infoRow
        [Console]::Write($clearLine)
        Write-Host " Info: " -NoNewline -ForegroundColor Yellow
        if ($Descriptions[$itemIdx]) {
            Write-Host "$($Descriptions[$itemIdx])".PadRight(60) -ForegroundColor White
        } else {
            Write-Host "Select an option.".PadRight(60) -ForegroundColor DarkGray
        }
    }

    # Initial full draw
    Clear-Host
    # Make title red if unauthorized
    if ($Title -match "UNAUTHORIZED") {
        Write-Host " $Title" -ForegroundColor Red
    } else {
        Write-Host " $Title" -ForegroundColor Cyan
    }
    Write-Host " ================================================" -ForegroundColor DarkCyan
    for ($i = 0; $i -lt $Options.Count; $i++) {
        DrawItem $i ($i -eq $idx)
    }
    Write-Host " ================================================" -ForegroundColor DarkCyan
    DrawInfo $idx
    Write-Host " [" -NoNewline -ForegroundColor DarkGray
    Write-Host "Arrows" -NoNewline -ForegroundColor DarkCyan
    Write-Host ": Move] [" -NoNewline -ForegroundColor DarkGray
    Write-Host "Keys" -NoNewline -ForegroundColor Yellow
    Write-Host ": Select] [" -NoNewline -ForegroundColor DarkGray
    Write-Host "Enter" -NoNewline -ForegroundColor Green
    Write-Host ": OK] [" -NoNewline -ForegroundColor DarkGray
    Write-Host "ESC" -NoNewline -ForegroundColor Red
    Write-Host ": Back]" -ForegroundColor DarkGray

    $prevIdx = $idx

    while ($true) {
        $keyInput = Get-KeyInput
        $newIdx = $idx

        switch ($keyInput.Key) {
            "Escape" {
                Show-Cursor
                MoveTo ($hintsRow + 1)
                return -1
            }
            "Up" {
                $newIdx--
                if ($newIdx -lt 0) { $newIdx = $max }
                # Skip separators
                while ($separatorIndices.ContainsKey($newIdx) -and $newIdx -ge 0) {
                    $newIdx--
                    if ($newIdx -lt 0) { $newIdx = $max }
                }
            }
            "Down" {
                $newIdx++
                if ($newIdx -gt $max) { $newIdx = 0 }
                # Skip separators
                while ($separatorIndices.ContainsKey($newIdx) -and $newIdx -le $max) {
                    $newIdx++
                    if ($newIdx -gt $max) { $newIdx = 0 }
                }
            }
            "Enter" {
                # Don't select separators
                if (-not $separatorIndices.ContainsKey($idx)) {
                    Show-Cursor
                    MoveTo ($hintsRow + 1)
                    return $idx
                }
            }
            "Number" {
                $numIdx = [int][string]$keyInput.Char - 1
                if ($numIdx -ge 0 -and $numIdx -lt $StaticStartIndex -and $numIdx -le $max -and -not $separatorIndices.ContainsKey($numIdx)) {
                    Show-Cursor
                    MoveTo ($hintsRow + 1)
                    return $numIdx
                }
            }
            "Letter" {
                $pressedKey = $keyInput.Char
                if ($shortcutMap.ContainsKey($pressedKey)) {
                    Show-Cursor
                    MoveTo ($hintsRow + 1)
                    return $shortcutMap[$pressedKey]
                }
            }
        }

        # Only redraw if selection changed
        if ($newIdx -ne $idx) {
            DrawItem $idx $false      # Un-highlight old
            DrawItem $newIdx $true    # Highlight new
            DrawInfo $newIdx          # Update description
            $idx = $newIdx
        }
    }
}

# FIX #13: ESC key support for toggle
function Read-Toggle ($Prompt, $Options, $DefaultIndex=0) {
    # Horizontal toggle for [ YES ] NO using ANSI escape codes for flicker-free updates
    $idx = $DefaultIndex
    $max = $Options.Count - 1

    # ANSI color codes
    $esc = [char]27
    $cyan = "$esc[96m"
    $gray = "$esc[90m"
    $reset = "$esc[0m"
    $clearLine = "$esc[2K"
    $cursorBack = "$esc[G"  # Move cursor to column 1

    # Check if terminal supports ANSI (most modern terminals do)
    $useAnsi = $true
    $startPos = $null
    try { $startPos = $Host.UI.RawUI.CursorPosition } catch { $useAnsi = $true }

    while ($true) {
        # Build the entire line as a single string with embedded colors
        $line = "${gray}${Prompt}${reset} "
        for ($i=0; $i -lt $Options.Count; $i++) {
            if ($i -eq $idx) {
                $line += "${cyan} [ $($Options[$i]) ] ${reset}"
            } else {
                $line += "${gray}   $($Options[$i])   ${reset}"
            }
        }

        if ($useAnsi) {
            # Use ANSI: clear line, return to start, write in one shot
            [Console]::Write("$cursorBack$clearLine$line")
        } else {
            # Fallback: cursor positioning
            if ($startPos) {
                try { $Host.UI.RawUI.CursorPosition = $startPos } catch {}
            }
            [Console]::Write($line + "          ")
        }

        $keyInput = Get-KeyInput

        switch ($keyInput.Key) {
            "Escape" {
                Write-Host ""
                return -1
            }
            "Left" {
                $idx--; if ($idx -lt 0) { $idx = $max }
            }
            "Right" {
                $idx++; if ($idx -gt $max) { $idx = 0 }
            }
            "Enter" {
                Write-Host ""
                return $idx
            }
        }
    }
}

# --- REPORT GENERATOR ---
# Universal diagnostics for Shield, Google TV, and other Android TV devices
# Optimized: batches ADB commands to reduce round-trips
function Run-Report ($Target, $Name, $DeviceType = "Unknown") {
    $typeName = Get-DeviceTypeName $DeviceType
    Write-Header "Health Report: $Name ($typeName)"
    Write-Host " Gathering data..." -ForegroundColor Gray

    # --- BATCH ADB CALLS FOR SPEED ---
    # Combine multiple commands into fewer shell calls
    $batchCmd = @(
        "echo '::THERMAL::'; dumpsys thermalservice 2>/dev/null | head -50",
        "echo '::MEMINFO::'; dumpsys meminfo 2>/dev/null",
        "echo '::STORAGE::'; df -h /data 2>/dev/null",
        "echo '::PROPS::'; getprop ro.board.platform; getprop ro.build.version.release",
        "echo '::SETTINGS::'; settings get global window_animation_scale; settings get global background_process_limit",
        "echo '::PACKAGES::'; pm list packages -e 2>/dev/null"
    ) -join "; "

    $batchOutput = & $Script:AdbPath -s $Target shell $batchCmd 2>&1 | Out-String

    # Parse sections
    $sections = @{}
    $currentSection = ""
    foreach ($line in ($batchOutput -split "`n")) {
        if ($line -match "^::(\w+)::") {
            $currentSection = $matches[1]
            $sections[$currentSection] = ""
        } elseif ($currentSection) {
            $sections[$currentSection] += "$line`n"
        }
    }

    # --- TEMPERATURE ---
    $Temp = "N/A"
    $thermal = $sections["THERMAL"]
    if ($thermal) {
        if ($thermal -match "mValue=([\d\.]+).*mName=CPU\d*,") {
            $Temp = [math]::Round([float]$matches[1], 1)
        }
        elseif ($thermal -match "mValue=([\d\.]+).*mName=soc_thermal") {
            $Temp = [math]::Round([float]$matches[1], 1)
        }
        elseif ($thermal -match "Temperature\{mValue=([\d\.]+),.*mType=0") {
            $Temp = [math]::Round([float]$matches[1], 1)
        }
    }

    # --- RAM ---
    $Total = 0; $Free = 0; $Swap = 0
    $mem = $sections["MEMINFO"]
    if ($mem) {
        if ($mem -match "Total RAM:\s+([0-9,]+)\s*K") { $Total = [math]::Round(($matches[1] -replace ",","") / 1024, 0) }
        if ($mem -match "Free RAM:\s+([0-9,]+)\s*K") { $Free = [math]::Round(($matches[1] -replace ",","") / 1024, 0) }
        if ($mem -match "ZRAM:.*used for\s+([0-9,]+)\s*K") { $Swap = [math]::Round(($matches[1] -replace ",","") / 1024, 0) }
    }
    if ($Total -eq 0) { $Total = 2048 }
    $Used = $Total - $Free
    $Pct = if ($Total -gt 0) { [math]::Round(($Used / $Total) * 100, 0) } else { 0 }

    # --- STORAGE ---
    $StorageUsed = "N/A"; $StorageTotal = "N/A"; $StoragePct = 0
    $df = $sections["STORAGE"]
    if ($df) {
        foreach ($line in ($df -split "`n")) {
            if ($line -match "/data" -and $line -match "(\d+\.?\d*[GMKT]?)\s+(\d+\.?\d*[GMKT]?)\s+(\d+\.?\d*[GMKT]?)\s+(\d+)%") {
                $StorageTotal = $matches[1]
                $StorageUsed = $matches[2]
                $StoragePct = [int]$matches[4]
                break
            }
        }
    }

    # --- PROPS ---
    $Platform = "Unknown"; $AndroidVer = "Unknown"
    $props = $sections["PROPS"]
    if ($props) {
        $propLines = @($props -split "`n" | Where-Object { $_.Trim() })
        if ($propLines.Count -ge 1) { $Platform = $propLines[0].Trim() }
        if ($propLines.Count -ge 2) { $AndroidVer = $propLines[1].Trim() }
    }

    # --- SETTINGS ---
    $anim = "1.0"; $proc = "Standard"
    $settings = $sections["SETTINGS"]
    if ($settings) {
        $setLines = @($settings -split "`n" | Where-Object { $_.Trim() })
        if ($setLines.Count -ge 1 -and $setLines[0].Trim() -notmatch "null|Exception") { $anim = $setLines[0].Trim() }
        if ($setLines.Count -ge 2 -and $setLines[1].Trim() -notmatch "null|Exception" -and $setLines[1].Trim()) { $proc = $setLines[1].Trim() }
    }

    # --- DISPLAY RESULTS ---
    Write-Host "`r                      `r" -NoNewline  # Clear "Gathering data..."

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

    Write-SubHeader "Settings"
    Write-Host " Animation Speed: " -NoNewline -ForegroundColor Gray
    Write-Host "$anim" -ForegroundColor Cyan
    Write-Host " Process Limit:   " -NoNewline -ForegroundColor Gray
    Write-Host "$proc" -ForegroundColor Cyan

    # --- TOP MEMORY (from already-fetched meminfo) ---
    Write-SubHeader "Top Memory Users"
    $topApps = @()
    if ($mem) {
        foreach ($line in ($mem -split "`n")) {
            if ($line -match "^\s*([0-9,]+)K:\s*(\S+)\s*\(pid") {
                $kb = [int]($matches[1] -replace ",", "")
                $pkg = $matches[2]
                if ($pkg -match "^com\." -or $pkg -match "^tv\." -or $pkg -match "^me\.") {
                    $topApps += @{ Package = $pkg; MB = [math]::Round($kb / 1024, 1) }
                    if ($topApps.Count -ge 5) { break }
                }
            }
        }
    }
    if ($topApps.Count -gt 0) {
        foreach ($app in $topApps) {
            $memColor = if ($app.MB -gt 200) { "Red" } elseif ($app.MB -gt 100) { "Yellow" } else { "White" }
            Write-Host " $($app.MB.ToString('0.0').PadLeft(6)) MB  " -NoNewline -ForegroundColor $memColor
            Write-Host "$($app.Package)" -ForegroundColor Gray
        }
    } else {
        Write-Host " Unable to query memory info" -ForegroundColor DarkGray
    }

    # --- BLOAT CHECK (using already-fetched data) ---
    Write-SubHeader "Bloat Check"
    $clean = $true
    $bloatFound = @()

    $allBloatApps = @()
    $allBloatApps += $Script:CommonAppList
    $allBloatApps += $Script:ShieldAppList
    $allBloatApps += $Script:GoogleTVAppList

    $enabledPkgs = $sections["PACKAGES"]

    foreach ($app in $allBloatApps) {
        $pkg = $app[0]; $appName = $app[1]; $method = $app[2]; $risk = $app[3]; $defOpt = $app[6]
        if ($risk -match "Safe" -or $risk -match "Medium") {
            if ($enabledPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)") {
                # Get memory from already-fetched meminfo
                $bloatMem = 0
                if ($mem -match "([0-9,]+)K:\s*$([regex]::Escape($pkg))\s") {
                    $bloatMem = [math]::Round(([int]($matches[1] -replace ",", "")) / 1024, 1)
                }
                $bloatFound += @{
                    Package = $pkg
                    Name = $appName
                    Method = $method
                    Default = $defOpt
                    Memory = $bloatMem
                }
                $clean = $false
            }
        }
    }

    if ($bloatFound.Count -gt 0) {
        Write-Host ""
        Write-Host " " -NoNewline
        Write-Host "App Name".PadRight(28) -NoNewline -ForegroundColor White
        Write-Host "RAM".PadRight(8) -NoNewline -ForegroundColor White
        Write-Host "Action".PadRight(10) -NoNewline -ForegroundColor White
        Write-Host "Default" -ForegroundColor White
        Write-Host " $("-" * 55)" -ForegroundColor DarkGray

        foreach ($bloat in $bloatFound) {
            $memStr = if ($bloat.Memory -gt 0) { "$($bloat.Memory) MB" } else { "-- MB" }
            $defStr = if ($bloat.Default -eq "Y") { "YES" } else { "no" }
            $defColor = if ($bloat.Default -eq "Y") { "Green" } else { "DarkGray" }

            Write-Host " " -NoNewline
            Write-Host $bloat.Name.PadRight(28).Substring(0, [Math]::Min(28, $bloat.Name.Length + 10)).PadRight(28) -NoNewline -ForegroundColor Yellow
            Write-Host $memStr.PadRight(8) -NoNewline -ForegroundColor Cyan
            Write-Host $bloat.Method.PadRight(10) -NoNewline -ForegroundColor White
            Write-Host $defStr -ForegroundColor $defColor
        }

        $totalMem = ($bloatFound | Measure-Object -Property Memory -Sum).Sum
        if ($totalMem -gt 0) {
            Write-Host ""
            Write-Host " Total bloat memory: " -NoNewline -ForegroundColor Gray
            Write-Host "$totalMem MB" -ForegroundColor Cyan
        }
    }

    if ($clean) { Write-Success "System is clean - no bloat detected." }
    Write-Host "`n"
}

# --- LIVE WATCH MODE ---
# Real-time monitoring of device vitals
function Watch-Vitals ($Target, $Name) {
    $esc = [char]27
    $refreshInterval = 3  # seconds

    Clear-Host
    Write-Host " LIVE MONITOR: $Name" -ForegroundColor Cyan
    Write-Host " ================================================" -ForegroundColor DarkCyan
    Write-Host " Refreshing every ${refreshInterval}s. Press any key to stop." -ForegroundColor Gray
    Write-Host ""

    $headerRow = 5
    Hide-Cursor

    # Draw static labels
    Write-Host " Temp:      " -ForegroundColor Gray
    Write-Host " RAM:       " -ForegroundColor Gray
    Write-Host " Swap:      " -ForegroundColor Gray
    Write-Host ""
    Write-Host " Top Memory Users:" -ForegroundColor White
    Write-Host " $("-" * 45)" -ForegroundColor DarkGray

    $topAppsStartRow = 12

    try {
        while (-not [Console]::KeyAvailable) {
            # Batch fetch vitals
            $batchCmd = "dumpsys meminfo 2>/dev/null; echo '::SEP::'; dumpsys thermalservice 2>/dev/null | head -30"
            $output = & $Script:AdbPath -s $Target shell $batchCmd 2>&1 | Out-String

            $parts = $output -split "::SEP::"
            $mem = if ($parts.Count -ge 1) { $parts[0] } else { "" }
            $thermal = if ($parts.Count -ge 2) { $parts[1] } else { "" }

            # Parse temperature
            $Temp = "N/A"
            if ($thermal -match "mValue=([\d\.]+).*mName=CPU\d*,") {
                $Temp = [math]::Round([float]$matches[1], 1)
            } elseif ($thermal -match "mValue=([\d\.]+).*mName=soc_thermal") {
                $Temp = [math]::Round([float]$matches[1], 1)
            } elseif ($thermal -match "Temperature\{mValue=([\d\.]+),.*mType=0") {
                $Temp = [math]::Round([float]$matches[1], 1)
            }

            # Parse RAM
            $Total = 0; $Free = 0; $Swap = 0
            if ($mem -match "Total RAM:\s+([0-9,]+)\s*K") { $Total = [math]::Round(($matches[1] -replace ",","") / 1024, 0) }
            if ($mem -match "Free RAM:\s+([0-9,]+)\s*K") { $Free = [math]::Round(($matches[1] -replace ",","") / 1024, 0) }
            if ($mem -match "ZRAM:.*used for\s+([0-9,]+)\s*K") { $Swap = [math]::Round(($matches[1] -replace ",","") / 1024, 0) }
            if ($Total -eq 0) { $Total = 2048 }
            $Used = $Total - $Free
            $Pct = if ($Total -gt 0) { [math]::Round(($Used / $Total) * 100, 0) } else { 0 }

            # Parse top apps
            $topApps = @()
            foreach ($line in ($mem -split "`n")) {
                if ($line -match "^\s*([0-9,]+)K:\s*(\S+)\s*\(pid") {
                    $kb = [int]($matches[1] -replace ",", "")
                    $pkg = $matches[2]
                    if ($pkg -match "^com\." -or $pkg -match "^tv\." -or $pkg -match "^me\.") {
                        $topApps += @{ Package = $pkg; MB = [math]::Round($kb / 1024, 1) }
                        if ($topApps.Count -ge 10) { break }
                    }
                }
            }

            # Update display (move cursor and overwrite)
            # Temp
            [Console]::Write("$esc[$headerRow;13H")
            $tempColor = if ($Temp -ne "N/A" -and [float]$Temp -gt 70) { "91" } elseif ($Temp -ne "N/A" -and [float]$Temp -gt 50) { "93" } else { "92" }
            $tempStr = if ($Temp -ne "N/A") { "${Temp}C   " } else { "N/A    " }
            [Console]::Write("$esc[${tempColor}m$tempStr$esc[0m")

            # RAM
            [Console]::Write("$esc[$($headerRow+1);13H")
            $ramColor = if ($Pct -gt 85) { "91" } elseif ($Pct -gt 70) { "93" } else { "92" }
            $ramStr = "$Pct% ($Used / $Total MB)".PadRight(25)
            [Console]::Write("$esc[${ramColor}m$ramStr$esc[0m")

            # Swap
            [Console]::Write("$esc[$($headerRow+2);13H")
            [Console]::Write("$esc[97m$Swap MB     $esc[0m")

            # Top apps
            for ($i = 0; $i -lt 10; $i++) {
                [Console]::Write("$esc[$($topAppsStartRow + $i);1H$esc[2K")
                if ($i -lt $topApps.Count) {
                    $app = $topApps[$i]
                    $memColor = if ($app.MB -gt 200) { "91" } elseif ($app.MB -gt 100) { "93" } else { "97" }
                    $line = " $($app.MB.ToString('0.0').PadLeft(6)) MB  $($app.Package)"
                    [Console]::Write("$esc[${memColor}m$($line.Substring(0, [Math]::Min(50, $line.Length)))$esc[0m")
                }
            }

            # Timestamp
            [Console]::Write("$esc[$($topAppsStartRow + 11);1H$esc[2K")
            [Console]::Write("$esc[90m Updated: $(Get-Date -Format 'HH:mm:ss')$esc[0m")

            # Wait for interval or keypress
            for ($w = 0; $w -lt ($refreshInterval * 10); $w++) {
                if ([Console]::KeyAvailable) { break }
                Start-Sleep -Milliseconds 100
            }
        }

        # Consume the keypress
        $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
    }
    finally {
        Show-Cursor
        [Console]::Write("$esc[$($topAppsStartRow + 13);1H")
        Write-Host ""
        Write-Info "Watch mode stopped."
    }
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
            Write-Host " [$risk]" -ForegroundColor $c -NoNewline

            # Show memory usage if app is currently running (Optimize mode only)
            if ($Mode -eq "Optimize") {
                $appMem = Get-AppMemoryUsage -Target $Target -Package $pkg
                if ($appMem) {
                    $memColor = if ($appMem -gt 100) { "Yellow" } else { "DarkGray" }
                    Write-Host " ($appMem MB)" -ForegroundColor $memColor
                } else {
                    Write-Host ""  # Newline if not running
                }
            } else {
                Write-Host ""  # Newline for Restore mode
            }
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
                    if ($isInstalledForUser) {
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
            if ($status -eq "device") {
                $txt = $d.Name
                $mDescs += "$typeName | $($d.Model) | $($d.Serial)"
            } elseif ($status -eq "unauthorized") {
                # Show IP and clear UNAUTHORIZED warning
                $txt = "$($d.Serial) !! UNAUTHORIZED !!"
                $mDescs += "Check your TV screen and accept the connection prompt!"
            } else {
                # Show IP for other non-connected states
                $txt = "$($d.Serial) [$status]"
                $mDescs += "$typeName | $($d.Model)"
            }
            $mOpts += $txt
        }
        # Add separator after devices
        $mOpts += "---"
        $mDescs += ""
    }

    # Static options start after devices + separator
    $staticStart = if ($devs.Count -gt 0) { $devs.Count + 1 } else { 0 }

    $mOpts += "Scan Network"; $mDescs += "Auto-discover Android TV devices on local network."
    $mOpts += "Connect IP"; $mDescs += "Manually connect to a specific IP address."
    $mOpts += "Report All"; $mDescs += "Run Health Check on ALL connected devices."
    $mOpts += "Refresh"; $mDescs += "Reload device list."
    # UX #E: ADB Server Restart option
    $mOpts += "Restart ADB"; $mDescs += "Kill and restart ADB server (fixes connection issues)."
    $mOpts += "Help"; $mDescs += "View instructions and troubleshooting."
    $mOpts += "Quit"; $mDescs += "Exit Optimizer."

    # Menu title - keep it simple
    $menuTitle = "Android TV Optimizer $Script:Version - Main Menu"

    # Pass StaticStartIndex so devices use numbers, options use letters
    # Shortcuts: S=Scan, C=Connect, R=Report, F=reFresh, A=ADB, H=Help, Q=Quit
    $mainShortcuts = @("S", "C", "R", "F", "A", "H", "Q")
    $sel = Read-Menu -Title $menuTitle -Options $mOpts -Descriptions $mDescs -StaticStartIndex $staticStart -Shortcuts $mainShortcuts

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
            if ($target.Status -eq "unauthorized") {
                Write-Header "Device Authorization Required"
                Write-Warn "This computer is not authorized to connect to $($target.Name)."
                Write-Host ""
                Write-Host " To authorize this connection:" -ForegroundColor Cyan
                Write-Host "   1. Look at your TV screen now" -ForegroundColor White
                Write-Host "   2. You should see an 'Allow USB debugging?' prompt" -ForegroundColor White
                Write-Host "   3. Check 'Always allow from this computer'" -ForegroundColor White
                Write-Host "   4. Select 'Allow' or 'OK'" -ForegroundColor White
                Write-Host ""
                Write-Host " If you don't see the prompt:" -ForegroundColor Yellow
                Write-Host "   - Wake up the TV (press any button on remote)" -ForegroundColor Gray
                Write-Host "   - Try: Settings > Developer Options > Revoke USB debugging" -ForegroundColor Gray
                Write-Host "   - Then reconnect using Scan Network or Connect IP" -ForegroundColor Gray
                Write-Host ""
            } else {
                Write-Warn "Cannot manage device in state: $($target.Status)"
            }
            Pause; continue
        }

        # Get device type name for display
        $deviceTypeName = Get-DeviceTypeName $target.Type

        # Inner loop: stay on this device's action menu until Back/Reboot/Disconnect/ESC
        while ($true) {
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

            # Handle ESC - return to main menu
            if ($aSel -eq -1) { break }

            $act = $aOpts[$aSel]

            # Actions that return to main menu
            if ($act -eq "Back") { break }
            if ($act -eq "Reboot") { Show-RebootMenu -Target $target.Serial; Pause; break }
            if ($act -eq "Disconnect") { Disconnect-Device -Serial $target.Serial; Pause; break }

            # Actions that stay on this device's menu
            if ($act -eq "Optimize") { Run-Task -Target $target.Serial -Mode "Optimize" -DeviceType $target.Type }
            if ($act -eq "Restore") { Run-Task -Target $target.Serial -Mode "Restore" -DeviceType $target.Type }
            if ($act -eq "Report") {
                Run-Report -Target $target.Serial -Name $target.Name -DeviceType $target.Type
                Write-Host ""
                $watchChoice = Read-Toggle -Prompt "Enter Live Monitor mode?" -Options @("YES", "NO") -DefaultIndex 1
                if ($watchChoice -eq 0) {
                    Watch-Vitals -Target $target.Serial -Name $target.Name
                }
                Pause
            }
            if ($act -eq "Launcher Setup") { Setup-Launcher -Target $target.Serial; Pause }
            if ($act -eq "Profile") { Show-DeviceProfile -Target $target.Serial -DeviceInfo $target; Pause }
            if ($act -eq "Recovery") { Run-PanicRecovery -Target $target.Serial; Pause }
        }
    }
}
