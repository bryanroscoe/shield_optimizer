<#
.SYNOPSIS
    Nvidia Shield Ultimate Optimizer (v56 - Syntax Perfect)
.DESCRIPTION
    - Fixed ParserError on line 233 (Variable expansion with colon).
    - Full Arrow-Key Navigation for Menus and Prompts.
    - Restored Help [H] and auto-help on scan failure.
#>

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# --- CONFIGURATION & DATA MODELS ---

# Format: Package, Name, Method, Risk, OptDesc, RestDesc, OptDefault(Y/N), RestDefault(Y/N)
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

function Show-Help {
    Clear-Host
    Write-Header "HELP & TROUBLESHOOTING"
    Write-Host "1. PRE-REQUISITES" -ForegroundColor Cyan
    Write-Host "   - Enable Developer Options: Settings > Device Preferences > About > Build (Click 7 times)"
    Write-Host "   - Enable USB Debugging: Settings > Device Preferences > Developer Options > USB Debugging"
    Write-Host "   - Network Debugging: Enable this if connecting via WiFi."
    
    Write-Host "`n2. CONNECTING" -ForegroundColor Cyan
    Write-Host "   - Use [S]can to auto-discover Shields on your local network."
    Write-Host "   - If scan fails, use [C]onnect and enter the IP address manually (found in Network settings)."
    Write-Host "   - On first connection, check your TV screen to AUTHORIZE the computer."

    Write-Host "`n3. MODES" -ForegroundColor Cyan
    Write-Host "   - OPTIMIZE: Disables bloatware. You can choose Disable or Uninstall."
    Write-Host "   - RESTORE: Re-enables or Re-installs apps."
    Write-Host "   - LAUNCHER: Install Projectivy or disable Stock launcher."
    
    Read-Host "`nPress Enter to return..."
}

# --- INPUT SYSTEM (ARROW KEYS) ---
function Read-Choice ($Prompt, $Options, $DefaultIndex=0) {
    $idx = $DefaultIndex
    $max = $Options.Count - 1
    $cursorPos = $Host.UI.RawUI.CursorPosition
    
    while ($true) {
        $Host.UI.RawUI.CursorPosition = $cursorPos
        Write-Host "$Prompt " -NoNewline -ForegroundColor Gray
        
        for ($i=0; $i -lt $Options.Count; $i++) {
            if ($i -eq $idx) {
                Write-Host " [ $($Options[$i]) ] " -NoNewline -ForegroundColor Cyan -BackgroundColor DarkGray
            } else {
                Write-Host "   $($Options[$i])   " -NoNewline -ForegroundColor Gray
            }
        }
        Write-Host "                               " -NoNewline 
        $Host.UI.RawUI.CursorPosition = $cursorPos
        
        $key = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
        if ($key.VirtualKeyCode -eq 37 -or $key.VirtualKeyCode -eq 38) { # Left/Up
            $idx--; if ($idx -lt 0) { $idx = $max }
        }
        elseif ($key.VirtualKeyCode -eq 39 -or $key.VirtualKeyCode -eq 40) { # Right/Down
            $idx++; if ($idx -gt $max) { $idx = 0 }
        }
        elseif ($key.VirtualKeyCode -eq 13) { # Enter
            Write-Host "`n"
            return $idx
        }
    }
}

function Check-Adb {
    $ScriptDir = $PSScriptRoot; $AdbExe = "$ScriptDir\adb.exe"; $Script:AdbPath = ""
    if (Test-Path $AdbExe) { $Script:AdbPath = $AdbExe }
    elseif (Get-Command "adb.exe" -ErrorAction SilentlyContinue) { $Script:AdbPath = (Get-Command "adb.exe").Source }
    
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

function Get-Devices {
    $raw = & $Script:AdbPath devices
    $devs = @()
    foreach ($line in $raw) {
        if ($line -match "^(\S+)\s+device") {
            $s = $matches[1]
            $n = & $Script:AdbPath -s $s shell settings get global device_name 2>$null
            if (-not $n) { $n = "Unknown Shield" }
            $devs += [PSCustomObject]@{ ID = $devs.Count + 1; Serial = $s; Name = $n; Status = "Ready" }
        }
    }
    return $devs
}

function Scan-Network {
    Write-Info "Scanning local subnet for port 5555..."
    $arp = arp -a | Select-String "dynamic"
    foreach ($entry in $arp) {
        if ($entry -match "(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})") {
            $ip = $matches[1]
            try {
                $sock = New-Object System.Net.Sockets.TcpClient
                if ($sock.BeginConnect($ip, 5555, $null, $null).AsyncWaitHandle.WaitOne(100, $false)) {
                    Write-Success "Found $ip"
                    & $Script:AdbPath connect $ip | Out-Null
                }
            } catch {}
        }
    }
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

# --- ENGINE: UNIFIED TASK RUNNER ---
function Run-Task ($Target, $Mode) {
    Write-Header "Application Management ($Mode)"
    
    foreach ($app in $Script:AppList) {
        $pkg = $app[0]; $name = $app[1]; $defMethod = $app[2]; $risk = $app[3]
        $optDesc = $app[4]; $restDesc = $app[5]
        $defOpt = $app[6]; $defRest = $app[7]

        $isInstalled = & $Script:AdbPath -s $Target shell pm list packages -u | Select-String "package:$pkg"
        $isDisabled = & $Script:AdbPath -s $Target shell pm list packages -d | Select-String "package:$pkg"
        
        $skip = $false
        
        # --- SKIP LOGIC & COLORING ---
        if ($Mode -eq "Optimize") {
            if (-not $isInstalled) { Write-Dim "$name ... [NOT INSTALLED]"; $skip=$true }
            elseif ($isDisabled) { Write-Dim "$name ... [ALREADY DISABLED]"; $skip=$true }
            $promptDefault = $defOpt
            $desc = $optDesc
        } else {
            if ($isInstalled -and -not $isDisabled) { Write-Dim "$name ... [ALREADY ACTIVE]"; $skip=$true }
            $promptDefault = $defRest
            $desc = $restDesc
        }

        if (-not $skip) {
            # Display Header
            if ($Mode -eq "Optimize") { $verb = "Remove" } else { $verb = "Restore" }
            # FIX: Use curly braces for variable expansion
            Write-Host -NoNewline "${verb}: "
            Write-Host "$name" -ForegroundColor Cyan -NoNewline
            if ($risk -match "Safe") { $c="Green" } elseif ($risk -match "Medium") { $c="Yellow" } else { $c="Red" }
            Write-Host " [$risk]" -ForegroundColor $c
            Write-Dim "    $desc"
            
            if ($Mode -eq "Restore") {
                if ($isInstalled) { Write-Host "    [Status: Disabled]" -ForegroundColor DarkGray }
                else { Write-Host "    [Status: Missing]" -ForegroundColor Yellow }
            }

            # --- ARROW KEY SELECTION ---
            if ($Mode -eq "Optimize") {
                if ($defMethod -eq "DISABLE") {
                    # Choices: Disable (Default), Uninstall, Skip
                    $opts = @("DISABLE", "UNINSTALL", "SKIP")
                    if ($defOpt -eq "Y") { $defIdx = 0 } else { $defIdx = 2 }
                } else {
                    # Choices: Uninstall (Default), Skip
                    $opts = @("UNINSTALL", "SKIP")
                    if ($defOpt -eq "Y") { $defIdx = 0 } else { $defIdx = 1 }
                }
                
                $res = Read-Choice -Prompt "    >> Action:" -Options $opts -DefaultIndex $defIdx
                $selStr = $opts[$res]
                
                if ($selStr -ne "SKIP") {
                    if ($selStr -eq "UNINSTALL") { 
                        & $Script:AdbPath -s $Target shell pm uninstall --user 0 $pkg | Out-Null; Write-Success "Uninstalled." 
                    } else { 
                        & $Script:AdbPath -s $Target shell pm disable-user --user 0 $pkg | Out-Null; Write-Success "Disabled." 
                    }
                }
            } else {
                # Restore Mode
                $opts = @("RESTORE", "SKIP")
                if ($defRest -eq "Y") { $defIdx = 0 } else { $defIdx = 1 }
                
                $res = Read-Choice -Prompt "    >> Action:" -Options $opts -DefaultIndex $defIdx
                
                if ($opts[$res] -eq "RESTORE") {
                    Write-Host "    Attempting Recovery..." -NoNewline -ForegroundColor Gray
                    if ($isInstalled) {
                        & $Script:AdbPath -s $Target shell pm enable $pkg 2>$null | Out-Null
                        Write-Success "Re-enabled."
                    } else {
                        $res = (& $Script:AdbPath -s $Target shell cmd package install-existing $pkg 2>&1)
                        if ($res -match "Package .* doesn't exist") {
                            Write-Host " [FILE MISSING]" -ForegroundColor Red
                            $playIdx = Read-Choice -Prompt "    >> Open Play Store?" -Options @("YES", "NO") -DefaultIndex 0
                            if ($playIdx -eq 0) { Open-PlayStore -Target $Target -PkgId $pkg }
                        } else {
                            & $Script:AdbPath -s $Target shell pm enable $pkg 2>$null | Out-Null
                            Write-Success "Restored."
                        }
                    }
                }
            }
        }
    }

    Write-Header "Performance Settings ($Mode)"
    foreach ($perf in $Script:PerfList) {
        $key = $perf.Key; $name = $perf.Name
        $curr = (& $Script:AdbPath -s $Target shell settings get global $key | Out-String).Trim()
        if ($curr -eq "null" -or $curr -eq "") { $curr = "Standard" }
        
        if ($Mode -eq "Optimize") { $targetVal = $perf.OptVal } else { $targetVal = $perf.RestVal }
        
        if ($key -eq "window_animation_scale") {
            Write-Host "$name" -NoNewline
            if ($curr -eq $targetVal) { Write-Host " [ALREADY OPTIMIZED]" -ForegroundColor DarkGray }
            else {
                Write-Host " [Current: ${curr}x]" -ForegroundColor Yellow
                $idx = Read-Choice -Prompt "    >> Set to ${targetVal}x?" -Options @("YES", "NO") -DefaultIndex 0
                if ($idx -eq 0) {
                    & $Script:AdbPath -s $Target shell settings put global window_animation_scale $targetVal
                    & $Script:AdbPath -s $Target shell settings put global transition_animation_scale $targetVal
                    & $Script:AdbPath -s $Target shell settings put global animator_duration_scale $targetVal
                    Write-Success "Applied."
                }
            }
        }
        elseif ($key -eq "background_process_limit") {
            Write-Host "$name" -NoNewline
            Write-Host " [Current: $curr]" -ForegroundColor Gray
            if ($Mode -eq "Optimize") {
                $idx = Read-Choice -Prompt "    >> Set Limit to ${targetVal}?" -Options @("YES", "NO") -DefaultIndex 1
                if ($idx -eq 0) {
                    & $Script:AdbPath -s $Target shell settings put global background_process_limit $targetVal
                    Write-Success "Limit Set."
                }
            } else {
                $idx = Read-Choice -Prompt "    >> Reset to Standard?" -Options @("YES", "NO") -DefaultIndex 0
                if ($idx -eq 0) {
                    & $Script:AdbPath -s $Target shell settings delete global background_process_limit
                    Write-Success "Reset."
                }
            }
        }
    }
    
    Write-Header "Finished"
    $r = Read-Choice -Prompt "Reboot Device Now?" -Options @("YES", "NO") -DefaultIndex 0
    if ($r -eq 0) { & $Script:AdbPath -s $Target reboot }
}

# --- LAUNCHER WIZARD ---
function Setup-Launcher ($Target) {
    Write-Header "Custom Launcher Wizard"
    
    $installedPkgs = & $Script:AdbPath -s $Target shell pm list packages
    $lOpts = @()
    $lData = @()
    
    foreach ($l in $Script:Launchers) {
        $lOpts += $l.Name
        $lData += $l
    }
    $lOpts += "Restore Stock Launcher"
    
    $selIdx = Read-Choice -Prompt "Select Action:" -Options $lOpts
    
    if ($selIdx -eq $lOpts.Count - 1) {
        # Restore Stock
        & $Script:AdbPath -s $Target shell pm enable com.google.android.tvlauncher 2>$null | Out-Null
        Write-Success "Stock Launcher Enabled."
        return
    }
    
    $choice = $lData[$selIdx]
    if ($installedPkgs -notmatch $choice.Pkg) {
        $idx = Read-Choice -Prompt "Not Installed. Open Play Store?" -Options @("YES", "NO") -DefaultIndex 0
        if ($idx -eq 0) {
            Open-PlayStore -Target $Target -PkgId $choice.Pkg
        }
    } else {
        $idx = Read-Choice -Prompt "Disable Stock Launcher to activate?" -Options @("YES", "NO") -DefaultIndex 0
        if ($idx -eq 0) {
            & $Script:AdbPath -s $Target shell pm disable-user --user 0 com.google.android.tvlauncher | Out-Null
            Write-Success "Stock Launcher Disabled."
        }
    }
}

# --- MAIN MENU ---
Clear-Host; Check-Adb
if ((Get-Host).UI.RawUI.WindowSize.Width -lt 80) { $Host.UI.RawUI.WindowSize = New-Object Management.Automation.Host.Size(100, 30) }

Write-Header "NVIDIA SHIELD OPTIMIZER v55"
Show-Help # Show help on start
Clear-Host
Write-Header "NVIDIA SHIELD OPTIMIZER v55"

while ($true) {
    $devs = @(Get-Devices)
    
    Write-Host "`nConnected Devices:"
    if ($devs.Count -eq 0) { 
        Write-Warn "None." 
        $h = Read-Choice -Prompt "No devices found. View Help?" -Options @("YES", "NO") -DefaultIndex 0
        if ($h -eq 0) { Show-Help; continue }
    }
    else { $devs | ForEach-Object { Write-Host " [$($_.ID)] $($_.Name) ($($_.Serial))" -ForegroundColor Green } }

    Write-SubHeader "Main Menu"
    $menuOpts = @("Scan Network", "Connect IP", "Refresh", "Help", "Quit")
    $menuDevs = @()
    foreach ($d in $devs) { $menuDevs += $d.Name }
    
    if ($devs.Count -gt 0) { $menuOpts = $menuDevs + $menuOpts }
    
    $sel = Read-Choice -Prompt "Select:" -Options $menuOpts -DefaultIndex 0
    $selStr = $menuOpts[$sel]
    
    if ($selStr -eq "Scan Network") { Scan-Network; continue }
    if ($selStr -eq "Connect IP") { $i = Read-Host "IP"; & $Script:AdbPath connect $i; continue }
    if ($selStr -eq "Refresh") { continue }
    if ($selStr -eq "Help") { Show-Help; continue }
    if ($selStr -eq "Quit") { Exit }
    
    # Device Selected
    $target = $devs[$sel]
    Write-Header "Action Menu: $($target.Name)"
    $actOpts = @("Optimize", "Restore", "Report", "Launcher Setup", "Report All", "Back")
    $aSel = Read-Choice -Prompt "Action:" -Options $actOpts
    
    if ($actOpts[$aSel] -eq "Optimize") { Run-Task -Target $target.Serial -Mode "Optimize" }
    if ($actOpts[$aSel] -eq "Restore") { Run-Task -Target $target.Serial -Mode "Restore" }
    if ($actOpts[$aSel] -eq "Report") { Run-Report -Target $target.Serial -Name $target.Name; Pause }
    if ($actOpts[$aSel] -eq "Launcher Setup") { Setup-Launcher -Target $target.Serial; Pause }
    if ($actOpts[$aSel] -eq "Report All") { 
        foreach ($d in $devs) { Run-Report -Target $d.Serial -Name $d.Name }
        Pause 
    }
}
