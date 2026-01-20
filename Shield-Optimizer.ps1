<#
.SYNOPSIS
    Nvidia Shield Ultimate Optimizer (v60 - Robustness Update)
.DESCRIPTION
    - Fixed socket leak in Scan-Network
    - Fixed ADB output trimming issues
    - Added IP validation before connecting
    - Fixed cursor size error handling
    - Fixed window resize crash in Windows Terminal
    - Fixed variable shadowing issues
    - Fixed PowerShell error syntax (2>$null -> try/catch)
    - Fixed $PSScriptRoot empty when dot-sourced
    - Fixed string interpolation bug
    - Added Invoke-AdbCommand helper with error checking
    - Fixed inefficient package queries (cached)
    - Fixed false positive package matching
    - Added ESC key to cancel menus
    - Added Scan-Network results feedback
    - Added reboot confirmation output
    - Added "Apply All Defaults" option
    - Added summary after Optimize/Restore
    - Added number key shortcuts (1-9) in menus
    - Check stock launcher status before offering disable
    - Added ADB Server Restart option
    - Show version in menu title
    - Added Disconnect Device option
    - Improved Network Scan timeout (200ms)
#>

$Script:Version = "v60"
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# --- CONFIGURATION & DATA MODELS ---

$Script:AppList = @(
    # [GOLDEN SET]
    @("com.google.android.tvrecommendations", "Google TV Recommendations", "DISABLE", "Safe", "Removes 'Sponsored' rows.", "Restores 'Sponsored' rows.", "Y", "Y"),
    @("com.nvidia.stats", "Nvidia Telemetry", "DISABLE", "Safe", "Stops background data.", "Restores Nvidia data collection.", "Y", "Y"),
    @("com.google.android.feedback", "Google Feedback", "DISABLE", "Safe", "Stops background data.", "Restores Google feedback services.", "Y", "Y"),
    @("com.nvidia.diagtools", "Nvidia Diag Tools", "DISABLE", "Safe", "Stops diagnostic logging.", "Restores diagnostic tools.", "Y", "Y"),
    @("com.android.printspooler", "Print Spooler", "DISABLE", "Safe", "Disables print service.", "Restores print service.", "Y", "Y"),
    @("com.android.gallery3d", "Android Gallery", "DISABLE", "Safe", "Removes legacy viewer.", "Restores legacy photo viewer.", "Y", "Y"),

    # [DEAD APPS]
    @("com.google.android.videos", "Google Play Movies", "UNINSTALL", "Safe", "Removes defunct app.", "Restores defunct app.", "Y", "Y"),
    @("com.google.android.music", "Google Play Music", "UNINSTALL", "Safe", "Removes defunct app.", "Restores defunct app.", "Y", "Y"),

    # [COMMUNITY]
    @("com.nvidia.ocs", "Nvidia Content Opt.", "DISABLE", "Medium (Community)", "[Source: florisse.nl] Untested.", "[Source: florisse.nl] Restores content optimization.", "N", "Y"),
    @("com.nvidia.shieldtech.hooks", "Nvidia Hooks", "DISABLE", "Medium (Community)", "[Source: florisse.nl] Untested.", "[Source: florisse.nl] Restores system hooks.", "N", "Y"),
    @("com.android.dreams.basic", "Basic Daydream", "DISABLE", "Medium (Community)", "[Source: florisse.nl] Screensaver.", "Restores basic screensaver.", "N", "Y"),
    @("com.android.providers.tv", "Live Channels Prov.", "DISABLE", "Medium (Community)", "[Source: florisse.nl] TV Provider.", "Restores Live Channels support.", "N", "Y"),

    # [STREAMING APPS]
    @("com.netflix.ninja", "Netflix", "UNINSTALL", "Safe", "Streaming App.", "Restores Netflix.", "N", "N"),
    @("com.amazon.amazonvideo.livingroom", "Amazon Prime Video", "UNINSTALL", "Safe", "Streaming App.", "Restores Prime Video.", "N", "N"),
    @("com.wbd.stream", "HBO Max / Discovery", "UNINSTALL", "Safe", "Streaming App.", "Restores HBO/Max.", "N", "N"),
    @("com.hulu.livingroomplus", "Hulu", "UNINSTALL", "Safe", "Streaming App.", "Restores Hulu.", "N", "N"),
    @("tv.twitch.android.app", "Twitch", "UNINSTALL", "Safe", "Streaming App.", "Restores Twitch.", "N", "N"),
    @("com.disney.disneyplus.prod", "Disney+", "UNINSTALL", "Safe", "Streaming App.", "Restores Disney+.", "N", "N"),
    @("com.spotify.tv.android", "Spotify", "UNINSTALL", "Safe", "Streaming App.", "Restores Spotify.", "N", "N"),
    @("com.google.android.youtube.tvmusic", "YouTube Music", "UNINSTALL", "Safe", "Streaming App.", "Restores YouTube Music.", "N", "N"),

    # [HIGH RISK]
    @("com.google.android.katniss", "Google Assistant", "DISABLE", "High Risk", "Breaks Voice Search.", "Restores Voice Search.", "N", "Y"),
    @("com.google.android.speech.pumpkin", "Google Speech Svcs", "DISABLE", "High Risk", "Breaks Dictation.", "Restores Speech Services.", "N", "Y"),
    @("com.google.android.apps.mediashell", "Chromecast Built-in", "DISABLE", "High Risk", "Breaks Casting.", "Restores Chromecast.", "N", "Y"),
    @("com.nvidia.ota", "Nvidia System Updater", "DISABLE", "High Risk", "Stops OS Updates.", "Restores Updates.", "N", "Y"),
    @("com.google.android.tvlauncher", "Stock Android Launcher", "DISABLE", "High Risk", "Requires Custom Launcher.", "Restores Stock Home Screen.", "N", "Y"),
    @("com.plexapp.mediaserver.smb", "Plex Media Server", "DISABLE", "Advanced", "Breaks Plex Hosting.", "Restores Plex Server.", "N", "Y"),
    @("com.google.android.tts", "Google Text-to-Speech", "DISABLE", "Medium Risk", "Breaks Accessibility.", "Restores Text-to-Speech.", "N", "Y"),
    @("com.nvidia.tegrazone3", "Nvidia Games", "DISABLE", "Medium Risk", "Breaks Cloud Gaming.", "Restores Nvidia Games.", "N", "Y"),
    @("com.google.android.play.games", "Google Play Games", "DISABLE", "Medium Risk", "Breaks Game Cloud Saves.", "Restores Play Games.", "N", "Y"),
    @("com.google.android.backdrop", "Google Screensaver", "DISABLE", "Medium Risk", "Disables Screensaver.", "Restores Screensaver.", "N", "Y")
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

# --- UTILITY FUNCTIONS ---

function Write-Header ($Text)   { Write-Host "`n=== $Text ===" -ForegroundColor Cyan }
function Write-SubHeader ($Text){ Write-Host "`n--- $Text ---" -ForegroundColor DarkCyan }
function Write-Success ($Text)  { Write-Host " [OK] $Text" -ForegroundColor Green }
function Write-Warn ($Text)     { Write-Host " [!!] $Text" -ForegroundColor Yellow }
function Write-ErrorMsg ($Text) { Write-Host " [ERROR] $Text" -ForegroundColor Red }
function Write-Info ($Text)     { Write-Host " [INFO] $Text" -ForegroundColor Gray }
function Write-Dim ($Text)      { Write-Host " $Text" -ForegroundColor DarkGray }

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

    if (Test-Path $AdbExe) {
        $Script:AdbPath = $AdbExe
    }
    elseif (Get-Command "adb.exe" -ErrorAction SilentlyContinue) {
        $Script:AdbPath = (Get-Command "adb.exe").Source
    }

    if ($Script:AdbPath -eq "") {
        Write-Warn "ADB missing. Downloading..."
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
    Start-Sleep -Milliseconds 500
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

            $n = "Unknown Shield"
            $mod = "Unknown Model"

            if ($st -eq "device") {
                # FIX #2: Get Name with .Trim()
                try {
                    $n = (& $Script:AdbPath -s $s shell settings get global device_name 2>&1 | Out-String).Trim()
                    if (-not $n -or $n -match "Exception|Error") { $n = "Shield TV" }
                }
                catch { $n = "Shield TV" }

                # FIX #2: Get Model with .Trim()
                try {
                    $mCode = (& $Script:AdbPath -s $s shell getprop ro.product.model 2>&1 | Out-String).Trim()
                    switch ($mCode) {
                        "mdarcy" { $mod = "Shield TV Pro (2019)" }
                        "sif"    { $mod = "Shield TV (2019 Tube)" }
                        "darcy"  { $mod = "Shield TV (2017)" }
                        "foster" { $mod = "Shield TV (2015)" }
                        default  { $mod = "Shield Device ($mCode)" }
                    }
                }
                catch { $mod = "Shield Device (Unknown)" }
            } elseif ($st -eq "offline") {
                $n = "Offline"; $mod = "Rebooting..."
            } elseif ($st -eq "unauthorized") {
                $n = "Unauthorized"; $mod = "Check TV Screen"
            }

            $devs += [PSCustomObject]@{ ID = $devs.Count + 1; Serial = $s; Name = $n; Status = $st; Model = $mod }
        }
    }
    return $devs
}

# FIX #1: Socket leak fix and #14: Results feedback and #H: Timeout improvement
function Scan-Network {
    Write-Info "Scanning local subnet for port 5555 (timeout: 200ms)..."
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
        Write-Warn "No devices found on network. Ensure Network Debugging is enabled on Shield."
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
    Write-Host "   - Use [Scan Network] to auto-discover Shields."
    Write-Host "   - If scan fails, use [Connect IP] to enter the address manually."
    Write-Host "   - If a device shows 'UNAUTHORIZED', check your TV screen to accept the connection."

    Write-Host "`n3. MODES" -ForegroundColor Cyan
    Write-Host "   - OPTIMIZE: Disables bloatware. You can choose Disable or Uninstall."
    Write-Host "   - RESTORE: Re-enables or Re-installs apps."
    Write-Host "   - LAUNCHER: Install Projectivy or disable Stock launcher."

    Write-Host "`n4. KEYBOARD SHORTCUTS" -ForegroundColor Cyan
    Write-Host "   - Arrow Keys: Navigate menus"
    Write-Host "   - 1-9: Quick select menu items"
    Write-Host "   - Enter: Confirm selection"
    Write-Host "   - ESC: Cancel / Go back"

    Read-Host "`nPress Enter to return..."
}

# --- NEW VERTICAL MENU SYSTEM ---
# FIX #13: ESC key support, FIX #3: Cursor error handling, UX #C: Number/letter key shortcuts
# StaticStartIndex: Items before this index use numbers (1-9), items from this index use letters (A-Z)
function Read-Menu ($Title, $Options, $Descriptions, $DefaultIndex=0, $StaticStartIndex=-1) {
    $idx = $DefaultIndex
    $max = $Options.Count - 1

    # If StaticStartIndex not specified, all items use letters
    if ($StaticStartIndex -lt 0) { $StaticStartIndex = 0 }

    # Letter shortcuts A-Z
    $letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"

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
        Write-Host "$Title" -ForegroundColor White
        Write-Host "------------------------------------------------" -ForegroundColor DarkGray

        $letterCount = 0
        for ($i=0; $i -lt $Options.Count; $i++) {
            # Dynamic items (devices) use numbers, static items use letters
            if ($i -lt $StaticStartIndex) {
                # Device - use number
                $shortcut = "$($i + 1)."
            } else {
                # Static option - use letter
                $shortcut = "$($letters[$letterCount])."
                $letterCount++
            }

            if ($i -eq $idx) {
                # Selected Item (Black on Cyan)
                Write-Host " $shortcut> " -NoNewline -ForegroundColor Cyan
                Write-Host " $($Options[$i]) " -ForegroundColor Black -BackgroundColor Cyan
            } else {
                # Normal Item
                Write-Host " $shortcut  $($Options[$i])" -ForegroundColor Gray
            }
        }

        Write-Host "------------------------------------------------" -ForegroundColor DarkGray
        Write-Host " Info: " -NoNewline -ForegroundColor Yellow
        if ($Descriptions[$idx]) {
            Write-Host "$($Descriptions[$idx])" -ForegroundColor Gray
        } else {
            Write-Host "Select an option." -ForegroundColor DarkGray
        }

        # Show appropriate hint based on menu type
        if ($StaticStartIndex -gt 0) {
            Write-Host " [Arrows: Navigate] [1-$StaticStartIndex: Devices] [A-Z: Options] [ESC: Back]" -ForegroundColor DarkGray
        } else {
            Write-Host " [Arrows: Navigate] [A-Z: Quick Select] [Enter: Confirm] [ESC: Back]" -ForegroundColor DarkGray
        }

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
        # Letter keys A-Z for static options (VirtualKeyCode 65-90)
        elseif ($key.VirtualKeyCode -ge 65 -and $key.VirtualKeyCode -le 90) {
            $letterIdx = $key.VirtualKeyCode - 65  # 65 = 'A', so index 0
            $actualIdx = $StaticStartIndex + $letterIdx
            if ($actualIdx -le $max) {
                try { $Host.UI.RawUI.CursorSize = $origCursor } catch {}
                return $actualIdx
            }
        }
    }
}

# FIX #13: ESC key support for toggle
function Read-Toggle ($Prompt, $Options, $DefaultIndex=0) {
    # Horizontal toggle for [ YES ] NO using `r carriage return for overwrite
    $idx = $DefaultIndex
    $max = $Options.Count - 1

    while ($true) {
        Write-Host "`r$Prompt " -NoNewline -ForegroundColor Gray

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
            Write-Host "`n"
            return -1  # Return -1 to indicate cancellation
        }
        elseif ($key.VirtualKeyCode -eq 37) { # Left
            $idx--; if ($idx -lt 0) { $idx = $max }
        }
        elseif ($key.VirtualKeyCode -eq 39) { # Right
            $idx++; if ($idx -gt $max) { $idx = 0 }
        }
        elseif ($key.VirtualKeyCode -eq 13) { # Enter
            Write-Host "`n"
            return $idx
        }
    }
}

# --- REPORT GENERATOR ---
function Run-Report ($Target, $Name) {
    Write-Header "Health Report: $Name"

    # Sensors
    $Temp = "N/A"
    try {
        $dump = & $Script:AdbPath -s $Target shell dumpsys thermalservice 2>&1
        foreach ($l in $dump) {
            if ($l -match "mValue=([\d\.]+).*mName=CPU") {
                if ($matches.Count -ge 2) { $Temp = [math]::Round([float]$matches[1], 1); break }
            }
        }
    } catch {}

    # RAM
    $mem = & $Script:AdbPath -s $Target shell dumpsys meminfo
    $Total=3072; $Free=0; $Swap=0
    foreach ($l in $mem) {
        if ($l -match "Total RAM:\s+([0-9,]+)K") { $Total=[math]::Round(($matches[1]-replace",","")/1024,0) }
        if ($l -match "Free RAM:\s+([0-9,]+)K") { $Free=[math]::Round(($matches[1]-replace",","")/1024,0) }
        if ($l -match "ZRAM:.*used for\s+([0-9,]+)K") { $Swap=[math]::Round(($matches[1]-replace",","")/1024,0) }
    }
    $Used = $Total - $Free
    $Pct = [math]::Round(($Used/$Total)*100, 0)

    Write-SubHeader "Vitals"
    Write-Host " Temp: $Temp C"
    Write-Host " RAM:  $Pct% ($Used / $Total MB)"
    Write-Host " Swap: $Swap MB"

    Write-SubHeader "Settings Check"
    $anim = (& $Script:AdbPath -s $Target shell settings get global window_animation_scale | Out-String).Trim()
    $proc = (& $Script:AdbPath -s $Target shell settings get global background_process_limit | Out-String).Trim()
    if ($proc -eq "null" -or $proc -eq "") { $proc = "Standard" }

    Write-Host " Animation Speed: " -NoNewline; Write-Host "$anim" -ForegroundColor Cyan
    Write-Host " Process Limit:   " -NoNewline; Write-Host "$proc" -ForegroundColor Cyan

    Write-SubHeader "Bloat Check"
    $clean = $true

    # FIX #11: Query packages once instead of per-app
    $enabledPkgs = (& $Script:AdbPath -s $Target shell pm list packages -e | Out-String)

    foreach ($app in $Script:AppList) {
        $pkg = $app[0]; $name = $app[1]; $risk = $app[3]
        if ($risk -match "Safe" -or $risk -match "Medium") {
            # FIX #12: Use exact match with word boundary
            if ($enabledPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)") {
                Write-Host " [ACTIVE BLOAT] $name" -ForegroundColor Yellow
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
    "com.google.android.apps.tv.launcherx"     # Newer Google TV Launcher
)

function Setup-Launcher ($Target) {
    Write-Header "Custom Launcher Wizard"
    Write-Info "Checking installed launchers..."

    $installedPkgs = (& $Script:AdbPath -s $Target shell pm list packages | Out-String)
    $disabledPkgs = (& $Script:AdbPath -s $Target shell pm list packages -d | Out-String)

    # Find which stock launcher is installed on this device
    $stockLauncherPkg = $null
    $stockLauncherInstalled = $false
    $stockLauncherDisabled = $false

    foreach ($stockPkg in $Script:StockLaunchers) {
        if ($installedPkgs -match "package:$([regex]::Escape($stockPkg))(\r|\n|$)") {
            $stockLauncherPkg = $stockPkg
            $stockLauncherInstalled = $true
            $stockLauncherDisabled = $disabledPkgs -match "package:$([regex]::Escape($stockPkg))(\r|\n|$)"
            break
        }
    }

    $lOpts = @(); $lDescs = @(); $launchers = @()

    foreach ($l in $Script:Launchers) {
        $status = "MISSING"
        # FIX #12: Use exact match
        if ($installedPkgs -match "package:$([regex]::Escape($l.Pkg))(\r|\n|$)") { $status = "INSTALLED" }
        $lOpts += "$($l.Name) [$status]"
        $lDescs += "Install or Enable $($l.Name)"
        $launchers += $l
    }

    # UX #D: Show stock launcher status (only if installed)
    if ($stockLauncherInstalled) {
        if ($stockLauncherDisabled) {
            $lOpts += "Restore Stock Launcher [DISABLED]"
            $lDescs += "Re-enable default Android TV Home (currently disabled)"
        } else {
            $lOpts += "Restore Stock Launcher [ENABLED]"
            $lDescs += "Stock launcher is already enabled"
        }
    } else {
        $lOpts += "Restore Stock Launcher [NOT FOUND]"
        $lDescs += "No standard stock launcher detected on this device"
    }
    $lOpts += "Back"; $lDescs += "Return to Action Menu"

    $sel = Read-Menu -Title "Select Launcher" -Options $lOpts -Descriptions $lDescs

    # Handle ESC or Back
    if ($sel -eq -1 -or $lOpts[$sel] -eq "Back") { return }

    if ($lOpts[$sel] -match "Restore Stock") {
        if (-not $stockLauncherInstalled) {
            Write-Warn "No stock launcher found on this device."
            Write-Info "This Shield may use a custom launcher or different package name."
            return
        }
        if (-not $stockLauncherDisabled) {
            Write-Info "Stock Launcher is already enabled."
            return
        }
        Write-Header "Restoring Stock Launcher"
        $result = Invoke-AdbCommand -Target $Target -Command "pm enable $stockLauncherPkg"
        if ($result.Success) {
            Write-Success "Stock Launcher Enabled."
        } else {
            Write-ErrorMsg "Failed to enable stock launcher: $($result.Error)"
        }
        return
    }

    $choice = $launchers[$sel]
    # FIX #12: Use exact match
    if (-not ($installedPkgs -match "package:$([regex]::Escape($choice.Pkg))(\r|\n|$)")) {
        $idx = Read-Toggle -Prompt "Not Installed. Open Play Store?" -Options @("YES", "NO") -DefaultIndex 0
        if ($idx -eq 0) {
            Open-PlayStore -Target $Target -PkgId $choice.Pkg
        }
    } else {
        # UX #D: Only offer to disable stock if it exists and is not already disabled
        if (-not $stockLauncherInstalled) {
            Write-Success "$($choice.Name) is installed."
            Write-Info "No stock launcher to disable. Press Home button to switch launchers."
        } elseif ($stockLauncherDisabled) {
            Write-Success "$($choice.Name) is installed and Stock Launcher is already disabled."
            Write-Info "Press Home button on your remote to switch launchers."
        } else {
            $idx = Read-Toggle -Prompt "Disable Stock Launcher to activate?" -Options @("YES", "NO") -DefaultIndex 0
            if ($idx -eq 0) {
                # FIX #10: Use helper with error checking
                $result = Invoke-AdbCommand -Target $Target -Command "pm disable-user --user 0 $stockLauncherPkg"
                if ($result.Success) {
                    Write-Success "Stock Launcher Disabled."
                } else {
                    Write-ErrorMsg "Failed to disable stock launcher: $($result.Error)"
                }
            }
        }
    }
}

# --- ENGINE: UNIFIED TASK RUNNER ---
# UX #A: Apply All Defaults, UX #B: Summary tracking
function Run-Task ($Target, $Mode) {
    Write-Header "Application Management ($Mode)"

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
    $allPkgs = (& $Script:AdbPath -s $Target shell pm list packages -u | Out-String)
    $disabledPkgs = (& $Script:AdbPath -s $Target shell pm list packages -d | Out-String)

    foreach ($app in $Script:AppList) {
        $pkg = $app[0]; $name = $app[1]; $defMethod = $app[2]; $risk = $app[3]
        $optDesc = $app[4]; $restDesc = $app[5]
        $defOpt = $app[6]; $defRest = $app[7]

        # FIX #12: Use exact match with word boundary
        $isInstalled = $allPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)"
        $isDisabled = $disabledPkgs -match "package:$([regex]::Escape($pkg))(\r|\n|$)"

        $skip = $false

        if ($Mode -eq "Optimize") {
            if (-not $isInstalled) { Write-Dim "$name ... [NOT INSTALLED]"; $skip=$true }
            elseif ($isDisabled) { Write-Dim "$name ... [ALREADY DISABLED]"; $skip=$true }
            $desc = $optDesc
        } else {
            if ($isInstalled -and -not $isDisabled) { Write-Dim "$name ... [ALREADY ACTIVE]"; $skip=$true }
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
                if ($isInstalled) { Write-Host "    [Status: Disabled]" -ForegroundColor DarkGray }
                else { Write-Host "    [Status: Missing]" -ForegroundColor Yellow }
            }

            if ($Mode -eq "Optimize") {
                if ($defMethod -eq "DISABLE") {
                    $opts = @("DISABLE", "UNINSTALL", "SKIP", "ABORT")
                    if ($defOpt -eq "Y") { $defIdx = 0 } else { $defIdx = 2 }
                } else {
                    $opts = @("UNINSTALL", "SKIP", "ABORT")
                    if ($defOpt -eq "Y") { $defIdx = 0 } else { $defIdx = 1 }
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
                    return
                }

                if ($selStr -ne "SKIP") {
                    if ($selStr -eq "UNINSTALL") {
                        # FIX #10: Use helper with error checking
                        $result = Invoke-AdbCommand -Target $Target -Command "pm uninstall --user 0 $pkg"
                        if ($result.Success -and $result.Output -notmatch "Failure") {
                            Write-Success "Uninstalled."
                            $Script:Summary.Uninstalled++
                        } else {
                            Write-ErrorMsg "Uninstall failed: $($result.Output)"
                            $Script:Summary.Failed++
                        }
                    } else {
                        # FIX #10: Use helper with error checking
                        $result = Invoke-AdbCommand -Target $Target -Command "pm disable-user --user 0 $pkg"
                        if ($result.Success -and $result.Output -notmatch "Failure") {
                            Write-Success "Disabled."
                            $Script:Summary.Disabled++
                        } else {
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
            $idx = Read-Toggle -Prompt "    >> Set to $($tAnim)?" -Options @("YES", "NO") -DefaultIndex 0
            if ($idx -eq -1) { $idx = 1 }  # ESC = NO
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
            $procOpts = @("Standard", "At Most 1", "At Most 2", "At Most 3", "At Most 4", "Skip")
            $procDescs = @("Unlimited background apps.", "Aggressive RAM saving.", "Balanced RAM saving.", "Moderate.", "Light limit.", "Do not change.")

            $sel = Read-Menu -Title "Select Process Limit" -Options $procOpts -Descriptions $procDescs -DefaultIndex 2
            if ($sel -eq -1) { $sel = 5 }  # ESC = Skip
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
                $idx = Read-Toggle -Prompt "    >> Reset to Standard?" -Options @("YES", "NO") -DefaultIndex 0
                if ($idx -eq -1) { $idx = 1 }  # ESC = NO
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

    # UX #B: Show Summary
    Write-Header "Summary"
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

# --- MAIN MENU ---
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
Write-Header "NVIDIA SHIELD OPTIMIZER $Script:Version"

while ($true) {
    $devs = @(Get-Devices)
    $mOpts = @(); $mDescs = @()

    if ($devs.Count -gt 0) {
        foreach ($d in $devs) {
            $status = $d.Status
            if ($status -eq "device") { $txt = $d.Name } else { $txt = "$($d.Name) [$status]" }
            $mOpts += $txt
            $mDescs += "Model: $($d.Model) | Serial: $($d.Serial)"
        }
    }

    # Static options start after devices (devices use numbers, everything else uses letters)
    $staticStart = $devs.Count

    $mOpts += "Scan Network"; $mDescs += "Auto-discover Shield TVs on local network."
    $mOpts += "Connect IP"; $mDescs += "Manually connect to a specific IP address."
    $mOpts += "Report All"; $mDescs += "Run Health Check on ALL connected devices."
    $mOpts += "Refresh"; $mDescs += "Reload device list."
    # UX #E: ADB Server Restart option
    $mOpts += "Restart ADB"; $mDescs += "Kill and restart ADB server (fixes connection issues)."
    $mOpts += "Help"; $mDescs += "View instructions and troubleshooting."
    $mOpts += "Quit"; $mDescs += "Exit Optimizer."

    # UX #F: Show version in menu title
    # Pass StaticStartIndex so devices use numbers, options use letters
    $sel = Read-Menu -Title "Shield Optimizer $Script:Version - Main Menu" -Options $mOpts -Descriptions $mDescs -StaticStartIndex $staticStart

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
        foreach ($d in $devs) { if ($d.Status -eq "device") { Run-Report -Target $d.Serial -Name $d.Name } }
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

        # UX #G: Add Disconnect option to action menu
        $aOpts = @("Optimize", "Restore", "Report", "Launcher Setup", "Disconnect", "Back")
        $aDescs = @(
            "Debloat apps and tune performance.",
            "Undo optimizations and fix missing apps.",
            "Check Temp, RAM, and Storage health.",
            "Install Projectivy or Switch Launchers.",
            "Disconnect this device from ADB.",
            "Return to Main Menu."
        )

        $aSel = Read-Menu -Title "Action Menu: $($target.Name)" -Options $aOpts -Descriptions $aDescs

        # Handle ESC
        if ($aSel -eq -1) { continue }

        $act = $aOpts[$aSel]

        if ($act -eq "Optimize") { Run-Task -Target $target.Serial -Mode "Optimize" }
        if ($act -eq "Restore") { Run-Task -Target $target.Serial -Mode "Restore" }
        if ($act -eq "Report") { Run-Report -Target $target.Serial -Name $target.Name; Pause }
        if ($act -eq "Launcher Setup") { Setup-Launcher -Target $target.Serial; Pause }
        # UX #G: Handle disconnect
        if ($act -eq "Disconnect") { Disconnect-Device -Serial $target.Serial; Pause }
    }
}
