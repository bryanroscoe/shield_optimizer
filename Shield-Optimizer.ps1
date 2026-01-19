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
            else
