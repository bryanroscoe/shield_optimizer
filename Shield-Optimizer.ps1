<#
.SYNOPSIS
    Nvidia Shield Ultimate Optimizer (v58 - Flicker Free)
.DESCRIPTION
    - Fixed Menu Ghosting: Uses Clear-Host for robust rendering.
    - Fixed Run-Report scope error.
    - High-Contrast Selection (Black on Cyan).
    - Removed auto-help on startup.
#>

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
        if ($line -match "^(\S+)\s+(device|offline|unauthorized)") {
            $s = $matches[1]
            $st = $matches[2]
            
            $n = "Unknown Shield"
            if ($st -eq "device") {
                $n = & $Script:AdbPath -s $s shell settings get global device_name 2>$null
                if (-not $n) { $n = "Unknown Shield" }
            } elseif ($st -eq "offline") {
                $n = "Device Offline (Rebooting?)"
            } elseif ($st -eq "unauthorized") {
                $n = "UNAUTHORIZED (Check TV)"
            }
            
            $devs += [PSCustomObject]@{ ID = $devs.Count + 1; Serial = $s; Name = $n; Status = $st }
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
                if ($sock.BeginConnect($ip, 5555, $null, $null).AsyncWaitHandle.WaitOne(50, $false)) {
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
    
    Read-Host "`nPress Enter to return..."
}

# --- NEW VERTICAL MENU SYSTEM (Clean Redraw) ---
function Read-Menu ($Title, $Options, $Descriptions, $DefaultIndex=0) {
    $idx = $DefaultIndex
    $max = $Options.Count - 1
    
    # Hide Cursor
    $origCursor = $Host.UI.RawUI.CursorSize
    $Host.UI.RawUI.CursorSize = 0 2>$null

    while ($true) {
        Clear-Host
        Write-Host "$Title" -ForegroundColor White
        Write-Host "------------------------------------------------" -ForegroundColor DarkGray
        
        for ($i=0; $i -lt $Options.Count; $i++) {
            if ($i -eq $idx) {
                # Selected Item (Black on Cyan)
                Write-Host " > " -NoNewline -ForegroundColor Cyan
                Write-Host " $($Options[$i]) " -ForegroundColor Black -BackgroundColor Cyan
            } else {
                # Normal Item
                Write-Host "   $($Options[$i])" -ForegroundColor Gray
            }
        }
        
        Write-Host "------------------------------------------------" -ForegroundColor DarkGray
        Write-Host " Info: " -NoNewline -ForegroundColor Yellow
        if ($Descriptions[$idx]) {
            Write-Host "$($Descriptions[$idx])" -ForegroundColor Gray
        } else {
            Write-Host "Select an option." -ForegroundColor DarkGray
        }
        
        $key = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
        if ($key.VirtualKeyCode -eq 38) { # Up
            $idx--; if ($idx -lt 0) { $idx = $max }
        }
        elseif ($key.VirtualKeyCode -eq 40) { # Down
            $idx++; if ($idx -gt $max) { $idx = 0 }
        }
        elseif ($key.VirtualKeyCode -eq 13) { # Enter
            $Host.UI.RawUI.CursorSize = $origCursor 2>$null
            return $idx
        }
    }
}

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
        
        $key = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
        if ($key.VirtualKeyCode -eq 37) { # Left
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
    $dump = & $Script:AdbPath -s $Target shell dumpsys thermalservice 2>$null
    foreach ($l in $dump) {
        if ($l -match "mValue=([\d\.]+).*mName=CPU") { 
            if ($matches.Count -ge 2) { $Temp = [math]::Round([float]$matches[1], 1); break }
        }
    }
    
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
    foreach ($app in $Script:AppList) {
        $pkg = $app[0]; $name = $app[1]; $risk = $app[3]
        if ($risk -match "Safe" -or $risk -match "Medium") {
            $act = & $Script:AdbPath -s $Target shell pm list packages -e | Select-String $pkg
            if ($act) { 
                Write-Host " [ACTIVE BLOAT] $name" -ForegroundColor Yellow
                $clean = $false 
            }
        }
    }
    if ($clean) { Write-Success "System is clean." }
    Write-Host "`n"
}

# --- LAUNCHER WIZARD ---
function Setup-Launcher ($Target) {
    Write-Header "Custom Launcher Wizard"
    
    $installedPkgs = & $Script:AdbPath -s $Target shell pm list packages
    $lOpts = @(); $lDescs = @(); $launchers = @()
    
    foreach ($l in $Script:Launchers) {
        $status = "MISSING"
        if ($installedPkgs -match $l.Pkg) { $status = "INSTALLED" }
        $lOpts += "$($l.Name) [$status]"
        $lDescs += "Install or Enable $($l.Name)"
        $launchers += $l
    }
    $lOpts += "Restore Stock Launcher"; $lDescs += "Re-enable default Android TV Home"
    $lOpts += "Back"; $lDescs += "Return to Action Menu"
    
    $sel = Read-Menu -Title "Select Launcher" -Options $lOpts -Descriptions $lDescs
    
    if ($lOpts[$sel] -eq "Back") { return }
    
    if ($lOpts[$sel] -match "Restore Stock") {
        Write-Header "Restoring Stock Launcher"
        & $Script:AdbPath -s $Target shell pm enable com.google.android.tvlauncher 2>$null | Out-Null
        Write-Success "Stock Launcher Enabled."
        return
    }
    
    $choice = $launchers[$sel]
    if ($installedPkgs -notmatch $choice.Pkg) {
        $idx = Read-Toggle -Prompt "Not Installed. Open Play Store?" -Options @("YES", "NO") -DefaultIndex 0
        if ($idx -eq 0) {
            Open-PlayStore -Target $Target -PkgId $choice.Pkg
        }
    } else {
        $idx = Read-Toggle -Prompt "Disable Stock Launcher to activate?" -Options @("YES", "NO") -DefaultIndex 0
        if ($idx -eq 0) {
            & $Script:AdbPath -s $Target shell pm disable-user --user 0 com.google.android.tvlauncher | Out-Null
            Write-Success "Stock Launcher Disabled."
        }
    }
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

            # --- TOGGLE SELECTION ---
            if ($Mode -eq "Optimize") {
                if ($defMethod -eq "DISABLE") {
                    $opts = @("DISABLE", "UNINSTALL", "SKIP")
                    if ($defOpt -eq "Y") { $defIdx = 0 } else { $defIdx = 2 }
                } else {
                    $opts = @("UNINSTALL", "SKIP")
                    if ($defOpt -eq "Y") { $defIdx = 0 } else { $defIdx = 1 }
                }
                
                $res = Read-Toggle -Prompt "    >> Action:" -Options $opts -DefaultIndex $defIdx
                $selStr = $opts[$res]
                
                if ($selStr -ne "SKIP") {
                    if ($selStr -eq "UNINSTALL") { 
                        & $Script:AdbPath -s $Target shell pm uninstall --user 0 $pkg | Out-Null; Write-Success "Uninstalled." 
                    } else { 
                        & $Script:AdbPath -s $Target shell pm disable-user --user 0 $pkg | Out-Null; Write-Success "Disabled." 
                    }
                }
            } else {
                $opts = @("RESTORE", "SKIP")
                if ($defRest -eq "Y") { $defIdx = 0 } else { $defIdx = 1 }
                $res = Read-Toggle -Prompt "    >> Action:" -Options $opts -DefaultIndex $defIdx
                
                if ($opts[$res] -eq "RESTORE") {
                    Write-Host "    Attempting Recovery..." -NoNewline -ForegroundColor Gray
                    if ($isInstalled) {
                        & $Script:AdbPath -s $Target shell pm enable $pkg 2>$null | Out-Null
                        Write-Success "Re-enabled."
                    } else {
                        $res = (& $Script:AdbPath -s $Target shell cmd package install-existing $pkg 2>&1)
                        if ($res -match "Package .* doesn't exist") {
                            Write-Host " [FILE MISSING]" -ForegroundColor Red
                            $playIdx = Read-Toggle -Prompt "    >> Open Play Store?" -Options @("YES", "NO") -DefaultIndex 0
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
    
    # 1. Animation
    $currAnim = (& $Script:AdbPath -s $Target shell settings get global window_animation_scale | Out-String).Trim()
    if ($Mode -eq "Optimize") { $tAnim = "0.5" } else { $tAnim = "1.0" }
    
    Write-Host "Animation Speed" -NoNewline
    if ($currAnim -eq $tAnim) { Write-Host " [ALREADY $tAnim]" -ForegroundColor DarkGray }
    else {
        Write-Host " [Current: ${currAnim}]" -ForegroundColor Yellow
        $idx = Read-Toggle -Prompt "    >> Set to $tAnim?" -Options @("YES", "NO") -DefaultIndex 0
        if ($idx -eq 0) {
            & $Script:AdbPath -s $Target shell settings put global window_animation_scale $tAnim
            & $Script:AdbPath -s $Target shell settings put global transition_animation_scale $tAnim
            & $Script:AdbPath -s $Target shell settings put global animator_duration_scale $tAnim
            Write-Success "Applied."
        }
    }

    # 2. Process Limit
    $currProc = (& $Script:AdbPath -s $Target shell settings get global background_process_limit | Out-String).Trim()
    if ($currProc -eq "null" -or $currProc -eq "") { $currProc = "Standard" }
    
    Write-Host "`nBackground Process Limit" -NoNewline
    Write-Host " [Current: $currProc]" -ForegroundColor Gray
    
    if ($Mode -eq "Optimize") {
        $procOpts = @("Standard", "At Most 1", "At Most 2", "At Most 3", "At Most 4", "Skip")
        $procDescs = @("Unlimited background apps.", "Aggressive RAM saving.", "Balanced RAM saving.", "Moderate.", "Light limit.", "Do not change.")
        
        $sel = Read-Menu -Title "Select Process Limit" -Options $procOpts -Descriptions $procDescs -DefaultIndex 2
        
        $val = $null
        if ($sel -eq 1) { $val = 1 }
        elseif ($sel -eq 2) { $val = 2 }
        elseif ($sel -eq 3) { $val = 3 }
        elseif ($sel -eq 4) { $val = 4 }
        
        if ($sel -eq 0) { # Standard
             & $Script:AdbPath -s $Target shell settings delete global background_process_limit
             Write-Success "Reset to Standard."
        } elseif ($val) {
             & $Script:AdbPath -s $Target shell settings put global background_process_limit $val
             Write-Success "Set to $val processes."
        }
    } else {
        if ($currProc -ne "Standard") {
            $idx = Read-Toggle -Prompt "    >> Reset to Standard?" -Options @("YES", "NO") -DefaultIndex 0
            if ($idx -eq 0) {
                & $Script:AdbPath -s $Target shell settings delete global background_process_limit
                Write-Success "Reset."
            }
        }
    }
    
    Write-Header "Finished"
    $r = Read-Toggle -Prompt "Reboot Device Now?" -Options @("YES", "NO") -DefaultIndex 0
    if ($r -eq 0) { & $Script:AdbPath -s $Target reboot }
}

# --- MAIN MENU ---
Clear-Host; Check-Adb
if ((Get-Host).UI.RawUI.WindowSize.Width -lt 80) { $Host.UI.RawUI.WindowSize = New-Object Management.Automation.Host.Size(100, 35) }

Write-Header "NVIDIA SHIELD OPTIMIZER v58"

while ($true) {
    $devs = @(Get-Devices)
    $mOpts = @(); $mDescs = @()
    
    # 1. Devices
    if ($devs.Count -gt 0) {
        foreach ($d in $devs) {
            $status = $d.Status
            if ($status -eq "device") { $txt = $d.Name } else { $txt = "$($d.Name) [$status]" }
            $mOpts += $txt
            $mDescs += "Manage this device ($($d.Serial))"
        }
    } else {
        $mOpts += "No Devices Found"
        $mDescs += "Use Scan Network or Connect IP"
    }
    
    # 2. Tools
    $mOpts += "Scan Network"; $mDescs += "Auto-discover Shield TVs on local network."
    $mOpts += "Connect IP"; $mDescs += "Manually connect to a specific IP address."
    $mOpts += "Report All"; $mDescs += "Run Health Check on ALL connected devices."
    $mOpts += "Refresh"; $mDescs += "Reload device list."
    $mOpts += "Help"; $mDescs += "View instructions and troubleshooting."
    $mOpts += "Quit"; $mDescs += "Exit Optimizer."
    
    $sel = Read-Menu -Title "Main Menu" -Options $mOpts -Descriptions $mDescs
    $selText = $mOpts[$sel]
    
    if ($selText -eq "Scan Network") { Scan-Network; continue }
    if ($selText -eq "Connect IP") { $i = Read-Host "IP"; & $Script:AdbPath connect $i; continue }
    if ($selText -eq "Refresh") { continue }
    if ($selText -eq "Report All") { 
        foreach ($d in $devs) { if ($d.Status -eq "device") { Run-Report -Target $d.Serial -Name $d.Name } }
        Pause; continue 
    }
    if ($selText -eq "Help") { Show-Help; continue }
    if ($selText -eq "Quit") { Exit }
    if ($selText -eq "No Devices Found") { Show-Help; continue }
    
    # Device Selected (Index less than device count)
    if ($sel -lt $devs.Count) {
        $target = $devs[$sel]
        
        if ($target.Status -ne "device") {
            Write-Warn "Cannot manage device in state: $($target.Status)"
            Pause; continue
        }
        
        $aOpts = @("Optimize", "Restore", "Report", "Launcher Setup", "Back")
        $aDescs = @(
            "Debloat apps and tune performance.",
            "Undo optimizations and fix missing apps.",
            "Check Temp, RAM, and Storage health.",
            "Install Projectivy or Switch Launchers.",
            "Return to Main Menu."
        )
        
        $aSel = Read-Menu -Title "Action Menu: $($target.Name)" -Options $aOpts -Descriptions $aDescs
        $act = $aOpts[$aSel]
        
        if ($act -eq "Optimize") { Run-Task -Target $target.Serial -Mode "Optimize" }
        if ($act -eq "Restore") { Run-Task -Target $target.Serial -Mode "Restore" }
        if ($act -eq "Report") { Run-Report -Target $target.Serial -Name $target.Name; Pause }
        if ($act -eq "Launcher Setup") { Setup-Launcher -Target $target.Serial; Pause }
    }
}
