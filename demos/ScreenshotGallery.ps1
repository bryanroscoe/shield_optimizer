# ScreenshotGallery.ps1 - Render UI screens for screenshots
# Usage: pwsh ./demos/ScreenshotGallery.ps1
#        pwsh ./demos/ScreenshotGallery.ps1 -Screen 1   # Render single screen and exit

param(
    [int]$Screen = 0        # 0 = interactive gallery, 1-10 = render single screen and exit
)

# Import function extraction helper
Import-Module (Join-Path $PSScriptRoot "../tests/TestHelpers.psm1") -Force
$ScriptPath = Join-Path $PSScriptRoot "../Shield-Optimizer.ps1"

# ============================================================================
# MOCK DATA
# ============================================================================

$MockDevices = @(
    @{
        Serial = "192.168.42.143:5555"
        Name = "Shield TV Pro"
        Model = "SHIELD Android TV"
        Type = "Shield"
        Status = "device"
    },
    @{
        Serial = "192.168.42.25:5555"
        Name = "Onn 4K Streaming Box"
        Model = "onn. 4K Streaming Box"
        Type = "GoogleTV"
        Status = "device"
    }
)

$MockVitals = @{
    Temperature = 42.5
    RAMPercent = 67
    RAMTotal = 3072
    RAMUsed = 2058
    Swap = 256
    StoragePercent = 45
    StorageTotal = "32G"
    StorageUsed = "14.4G"
    Platform = "tegra"
    AndroidVer = "11"
    AnimSpeed = "1.0"
    ProcessLimit = "Standard"
}

$MockTopApps = @(
    @{ Package = "com.google.android.youtube.tv"; MB = 245.3 },
    @{ Package = "com.netflix.ninja"; MB = 189.7 },
    @{ Package = "com.amazon.amazonvideo.livingroom"; MB = 156.2 },
    @{ Package = "com.disney.disneyplus"; MB = 134.8 },
    @{ Package = "com.hulu.livingroomplus"; MB = 98.4 },
    @{ Package = "com.plexapp.android"; MB = 87.6 },
    @{ Package = "com.google.android.tvlauncher"; MB = 72.1 },
    @{ Package = "com.nvidia.shield.remote"; MB = 45.3 },
    @{ Package = "com.google.android.backdrop"; MB = 38.9 },
    @{ Package = "com.android.providers.tv"; MB = 24.5 }
)

$MockBloatApps = @(
    @{ Name = "Google Feedback"; Package = "com.google.android.feedback"; Method = "DISABLE"; Default = "Y"; Memory = 12.3 },
    @{ Name = "Print Spooler"; Package = "com.android.printspooler"; Method = "DISABLE"; Default = "Y"; Memory = 0 },
    @{ Name = "Android Gallery"; Package = "com.android.gallery3d"; Method = "DISABLE"; Default = "Y"; Memory = 8.4 },
    @{ Name = "Device Usage"; Package = "com.nvidia.shield.deviceusage"; Method = "DISABLE"; Default = "Y"; Memory = 15.7 },
    @{ Name = "Nvidia Games"; Package = "com.nvidia.tegrazone3"; Method = "UNINSTALL"; Default = "Y"; Memory = 42.1 }
)

$MockLaunchers = @(
    @{ Name = "Projectivy Launcher"; Status = "ACTIVE" },
    @{ Name = "FLauncher"; Status = "INSTALLED" },
    @{ Name = "ATV Launcher"; Status = "MISSING" },
    @{ Name = "Wolf Launcher"; Status = "MISSING" },
    @{ Name = "AT4K Launcher"; Status = "MISSING" },
    @{ Name = "Stock Launcher (Google TV)"; Status = "DISABLED" }
)

$MockTaskSummary = @{
    Mode = "Optimize"
    Disabled = 8
    Uninstalled = 3
    Skipped = 5
    Failed = 0
}

# ============================================================================
# EXTRACT REQUIRED FUNCTIONS
# ============================================================================

$functionsToExtract = @(
    "Write-Header",
    "Write-SubHeader",
    "Write-Success",
    "Write-Warn",
    "Write-Info",
    "Write-Dim",
    "Write-Separator",
    "Get-VitalColor",
    "Get-DeviceTypeName",
    "Write-OptionWithHighlight"
)

foreach ($fn in $functionsToExtract) {
    try {
        $def = Get-FunctionDefinition -ScriptPath $ScriptPath -FunctionName $fn
        . ([ScriptBlock]::Create($def))
    } catch {
        # Silently skip if function not found
    }
}

# ============================================================================
# INITIALIZE COLOR SCHEME (Dark Mode for screenshots)
# ============================================================================

$Script:Colors = @{
    # Semantic colors
    Header      = "Cyan"
    SubHeader   = "Blue"
    Success     = "Green"
    Warning     = "Yellow"
    Error       = "Red"
    Info        = "White"

    # Text colors
    Text        = "White"
    TextDim     = "DarkCyan"
    TextBright  = "Cyan"

    # UI element colors
    Label       = "Cyan"
    Value       = "White"
    Separator   = "Blue"

    # Menu colors
    Selected    = "Cyan"
    Unselected  = "White"
    Shortcut    = "Yellow"
    Bracket     = "Gray"

    # Status colors
    Active      = "Green"
    Disabled    = "Magenta"
    Missing     = "Red"
    Installed   = "Cyan"
    NotFound    = "Gray"
}

$Script:AnsiColors = @{
    Text     = "97"   # Bright White
    TextDim  = "36"   # Cyan
    Selected = "96"   # Bright Cyan
    Gray     = "37"   # Gray
}

$Script:VitalThresholds = @{
    Temperature = @{ Warning = 50; Critical = 70; Normal = "Green"; NormalAnsi = "92" }
    RAM         = @{ Warning = 70; Critical = 85; Normal = "Green"; NormalAnsi = "92" }
    Storage     = @{ Warning = 75; Critical = 90; Normal = "Green"; NormalAnsi = "92" }
    AppMemory   = @{ Warning = 100; Critical = 200; Normal = "White"; NormalAnsi = "97" }
}

# ============================================================================
# STATIC MENU RENDERER (renders menu without interaction)
# ============================================================================

function Show-StaticMenu {
    param(
        [string]$Title,
        [array]$Options,
        [array]$Descriptions,
        [int]$SelectedIndex = 0,
        [array]$Shortcuts = @()
    )

    # Build display texts with embedded shortcuts
    $displayTexts = @{}
    for ($i = 0; $i -lt $Options.Count; $i++) {
        if ($Options[$i] -eq "---") {
            $displayTexts[$i] = "---"
            continue
        }

        $shortcut = if ($Shortcuts.Count -gt $i) { $Shortcuts[$i] } else { $Options[$i].Substring(0,1).ToUpper() }
        $optText = $Options[$i]

        # Find first occurrence of shortcut letter (not in brackets)
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

    # Draw menu
    Write-Host " $Title" -ForegroundColor $Script:Colors.Header
    Write-Host " ================================================" -ForegroundColor $Script:Colors.Separator

    for ($i = 0; $i -lt $Options.Count; $i++) {
        if ($Options[$i] -eq "---") {
            Write-Host "    --------------------------------" -ForegroundColor $Script:Colors.Separator
            continue
        }

        $selected = ($i -eq $SelectedIndex)
        if ($selected) {
            Write-Host "  > " -NoNewline -ForegroundColor $Script:Colors.Selected
            Write-OptionWithHighlight -Text $displayTexts[$i] -Selected $true -WithClosingArrow $true
        } else {
            Write-Host "    " -NoNewline
            Write-OptionWithHighlight -Text $displayTexts[$i] -Selected $false
        }
    }

    Write-Host " ================================================" -ForegroundColor $Script:Colors.Separator
    Write-Host " Info: " -NoNewline -ForegroundColor $Script:Colors.Warning
    if ($Descriptions.Count -gt $SelectedIndex -and $Descriptions[$SelectedIndex]) {
        Write-Host "$($Descriptions[$SelectedIndex])" -ForegroundColor $Script:Colors.Text
    } else {
        Write-Host "Select an option." -ForegroundColor $Script:Colors.TextDim
    }
    Write-Host " [" -NoNewline -ForegroundColor $Script:Colors.Text
    Write-Host "Arrows" -NoNewline -ForegroundColor $Script:Colors.Selected
    Write-Host ": Move] [" -NoNewline -ForegroundColor $Script:Colors.Text
    Write-Host "Keys" -NoNewline -ForegroundColor $Script:Colors.Shortcut
    Write-Host ": Select] [" -NoNewline -ForegroundColor $Script:Colors.Text
    Write-Host "Enter" -NoNewline -ForegroundColor $Script:Colors.Success
    Write-Host ": OK] [" -NoNewline -ForegroundColor $Script:Colors.Text
    Write-Host "ESC" -NoNewline -ForegroundColor $Script:Colors.Error
    Write-Host ": Back]" -ForegroundColor $Script:Colors.Text
}

# ============================================================================
# STATIC TOGGLE RENDERER
# ============================================================================

function Show-StaticToggle {
    param(
        [string]$Prompt,
        [array]$Options,
        [int]$SelectedIndex = 0
    )

    $esc = [char]27
    $selected = "$esc[96m"  # Bright Cyan
    $gray = "$esc[37m"
    $reset = "$esc[0m"

    $line = "${gray}${Prompt}${reset} "
    for ($i = 0; $i -lt $Options.Count; $i++) {
        if ($i -eq $SelectedIndex) {
            $line += "${selected} [ $($Options[$i]) ] ${reset}"
        } else {
            $line += "${gray}   $($Options[$i])   ${reset}"
        }
    }
    Write-Host $line
}

# ============================================================================
# SCREEN RENDERERS
# ============================================================================

function Show-Screen1-MainMenu {
    # Main Menu with 2 devices
    $options = @(
        "Shield TV Pro",
        "Onn 4K Streaming Box",
        "---",
        "Scan Network",
        "Connect IP",
        "Pair Device",
        "---",
        "Help",
        "Quit"
    )
    $descriptions = @(
        "Nvidia Shield - Connected via 192.168.42.143:5555",
        "Google TV - Connected via 192.168.42.25:5555",
        "",
        "Auto-discover Android TV devices on your network",
        "Enter IP address manually",
        "Pair new device with PIN code",
        "",
        "Help & troubleshooting guide",
        "Exit the application"
    )
    $shortcuts = @("1", "2", "", "S", "C", "P", "", "H", "Q")

    Clear-Host
    Show-StaticMenu -Title "Shield Optimizer - Select Device" -Options $options -Descriptions $descriptions -SelectedIndex 0 -Shortcuts $shortcuts
}

function Show-Screen2-ActionMenu {
    # Device Action Menu
    $options = @(
        "Optimize",
        "Restore",
        "Report",
        "Launcher Setup",
        "Install APK",
        "Profile",
        "---",
        "Recovery",
        "Back"
    )
    $descriptions = @(
        "Disable or uninstall bloatware apps",
        "Re-enable or reinstall disabled apps",
        "Generate health report (temp, RAM, storage, bloat)",
        "Install custom launcher, disable stock launcher",
        "Sideload APK files to device",
        "View device info and app optimization list",
        "",
        "Emergency restore - re-enable ALL disabled packages",
        "Return to device selection"
    )
    $shortcuts = @("O", "R", "E", "L", "I", "P", "", "C", "B")

    Clear-Host
    Show-StaticMenu -Title "Shield TV Pro - Actions" -Options $options -Descriptions $descriptions -SelectedIndex 0 -Shortcuts $shortcuts
}

function Show-Screen3-DeviceProfile {
    Clear-Host
    Write-Header "Device Profile"

    Write-Host " Device:  " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "Shield TV Pro" -ForegroundColor $Script:Colors.Value

    Write-Host " Model:   " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "SHIELD Android TV" -ForegroundColor $Script:Colors.Value

    Write-Host " Profile: " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "Nvidia Shield" -ForegroundColor $Script:Colors.Warning

    Write-Host " Serial:  " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "192.168.42.143:5555" -ForegroundColor $Script:Colors.TextDim

    Write-Host " Android: " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "11" -ForegroundColor $Script:Colors.Value

    Write-Host " SDK:     " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "30" -ForegroundColor $Script:Colors.Value

    Write-Host " Build:   " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "RQ1A.210105.003" -ForegroundColor $Script:Colors.Value

    Write-Host " Apps:    " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "42 apps in optimization list" -ForegroundColor $Script:Colors.Success

    Write-Host ""
    Show-StaticToggle -Prompt "Show full app profile?" -Options @("YES", "NO") -SelectedIndex 1
}

function Show-Screen4-HealthReport {
    Clear-Host
    Write-Header "Health Report: Shield TV Pro (Nvidia Shield)"

    Write-SubHeader "System Info"
    Write-Host " Platform:  " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "tegra" -ForegroundColor $Script:Colors.Value
    Write-Host " Android:   " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "11" -ForegroundColor $Script:Colors.Value

    Write-SubHeader "Vitals"
    Write-Host " Temp:    " -NoNewline -ForegroundColor $Script:Colors.Label
    $tempColor = Get-VitalColor -Type "Temperature" -Value $MockVitals.Temperature
    Write-Host "$($MockVitals.Temperature)`u{00B0}C" -ForegroundColor $tempColor

    Write-Host " RAM:     " -NoNewline -ForegroundColor $Script:Colors.Label
    $ramColor = Get-VitalColor -Type "RAM" -Value $MockVitals.RAMPercent
    Write-Host "$($MockVitals.RAMPercent)% ($($MockVitals.RAMUsed) / $($MockVitals.RAMTotal) MB)" -ForegroundColor $ramColor

    Write-Host " Swap:    " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "$($MockVitals.Swap) MB" -ForegroundColor $Script:Colors.Value

    Write-Host " Storage: " -NoNewline -ForegroundColor $Script:Colors.Label
    $storColor = Get-VitalColor -Type "Storage" -Value $MockVitals.StoragePercent
    Write-Host "$($MockVitals.StorageUsed) / $($MockVitals.StorageTotal) ($($MockVitals.StoragePercent)%)" -ForegroundColor $storColor

    Write-SubHeader "Settings"
    Write-Host " Animation Speed: " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "$($MockVitals.AnimSpeed)" -ForegroundColor $Script:Colors.Value
    Write-Host " Process Limit:   " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "$($MockVitals.ProcessLimit)" -ForegroundColor $Script:Colors.Value
}

function Show-Screen5-TopMemory {
    Clear-Host
    Write-Header "Health Report: Shield TV Pro"

    Write-SubHeader "Top Memory Users"
    foreach ($app in $MockTopApps) {
        $memColor = Get-VitalColor -Type "AppMemory" -Value $app.MB
        Write-Host " $($app.MB.ToString('0.0').PadLeft(6)) MB  " -NoNewline -ForegroundColor $memColor
        Write-Host "$($app.Package)" -ForegroundColor $Script:Colors.Value
    }
}

function Show-Screen6-BloatCheck {
    Clear-Host
    Write-Header "Health Report: Shield TV Pro"

    Write-SubHeader "Bloat Check"
    Write-Host ""
    Write-Host " " -NoNewline
    Write-Host "App Name".PadRight(28) -NoNewline -ForegroundColor $Script:Colors.Text
    Write-Host "RAM".PadRight(10) -NoNewline -ForegroundColor $Script:Colors.Text
    Write-Host "Action".PadRight(10) -NoNewline -ForegroundColor $Script:Colors.Text
    Write-Host "Default" -ForegroundColor $Script:Colors.Text
    Write-Host " $("-" * 55)" -ForegroundColor $Script:Colors.Separator

    foreach ($bloat in $MockBloatApps) {
        $memStr = if ($bloat.Memory -gt 0) { "$($bloat.Memory) MB" } else { "-- MB" }
        $defStr = if ($bloat.Default -eq "Y") { "YES" } else { "no" }
        $defColor = if ($bloat.Default -eq "Y") { $Script:Colors.Success } else { $Script:Colors.TextDim }

        Write-Host " " -NoNewline
        $nameDisplay = $bloat.Name.PadRight(28)
        if ($nameDisplay.Length -gt 28) { $nameDisplay = $nameDisplay.Substring(0, 28) }
        Write-Host $nameDisplay -NoNewline -ForegroundColor $Script:Colors.Warning
        Write-Host $memStr.PadRight(10) -NoNewline -ForegroundColor $Script:Colors.Label
        Write-Host $bloat.Method.PadRight(10) -NoNewline -ForegroundColor $Script:Colors.Value
        Write-Host $defStr -ForegroundColor $defColor
    }

    $totalMem = ($MockBloatApps | Measure-Object -Property Memory -Sum).Sum
    Write-Host ""
    Write-Host " Total bloat memory: " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "$totalMem MB" -ForegroundColor $Script:Colors.Label
}

function Show-Screen7-LauncherWizard {
    # Launcher Setup Menu
    $options = @()
    $descriptions = @()
    foreach ($l in $MockLaunchers) {
        $options += "$($l.Name) [$($l.Status)]"
        $descriptions += "Install or Enable $($l.Name)"
    }
    $options += "Back"
    $descriptions += "Return to Action Menu"

    $shortcuts = @("P", "F", "A", "W", "4", "S", "B")

    Clear-Host
    Write-Header "Custom Launcher Wizard"
    Write-Info "Current launcher: com.spocky.projengmenu"
    Write-Host ""
    Show-StaticMenu -Title "Select Launcher" -Options $options -Descriptions $descriptions -SelectedIndex 0 -Shortcuts $shortcuts
}

function Show-Screen8-HelpScreen {
    Clear-Host
    Write-Header "HELP & TROUBLESHOOTING"

    Write-Host "1. SETUP (on your TV)" -ForegroundColor $Script:Colors.Header
    Write-Host "   Enable Developer Options:" -ForegroundColor $Script:Colors.Text
    Write-Host "     Settings > Device Preferences > About > Build " -NoNewline -ForegroundColor $Script:Colors.TextDim
    Write-Host "(tap 7 times)" -ForegroundColor $Script:Colors.Warning
    Write-Host "   Enable Network Debugging:" -ForegroundColor $Script:Colors.Text
    Write-Host "     Settings > Device Preferences > Developer Options > Network Debugging" -ForegroundColor $Script:Colors.TextDim

    Write-Host "`n2. CONNECTING" -ForegroundColor $Script:Colors.Header
    Write-Host "   Scan Network    " -NoNewline -ForegroundColor $Script:Colors.Success
    Write-Host "Auto-discover devices on your local network" -ForegroundColor $Script:Colors.Text
    Write-Host "   Connect IP      " -NoNewline -ForegroundColor $Script:Colors.Success
    Write-Host "Enter IP address manually (standard port 5555)" -ForegroundColor $Script:Colors.Text
    Write-Host "   Pair Device     " -NoNewline -ForegroundColor $Script:Colors.Warning
    Write-Host "[EXPERIMENTAL] For newer Chromecasts/Google TV" -ForegroundColor $Script:Colors.Text
    Write-Host "                   " -NoNewline
    Write-Host "Uses PIN pairing - not needed for Shield TV" -ForegroundColor $Script:Colors.TextDim
    Write-Host "   UNAUTHORIZED?   " -NoNewline -ForegroundColor $Script:Colors.Error
    Write-Host "Check your TV screen to accept the connection" -ForegroundColor $Script:Colors.Text

    Write-Host "`n3. SUPPORTED DEVICES" -ForegroundColor $Script:Colors.Header
    Write-Host "   Nvidia Shield TV    " -NoNewline -ForegroundColor $Script:Colors.Success
    Write-Host "2015, 2017, 2019 - Fully tested" -ForegroundColor $Script:Colors.Text
    Write-Host "   Onn 4K Pro          " -NoNewline -ForegroundColor $Script:Colors.Success
    Write-Host "Walmart - Fully tested" -ForegroundColor $Script:Colors.Text
    Write-Host "   Chromecast w/ GTV   " -NoNewline -ForegroundColor $Script:Colors.Warning
    Write-Host "Should work (Google TV profile)" -ForegroundColor $Script:Colors.Text
    Write-Host "   Google TV Streamer  " -NoNewline -ForegroundColor $Script:Colors.Warning
    Write-Host "2024 model - Should work" -ForegroundColor $Script:Colors.Text

    Write-Host "`n4. KEYBOARD SHORTCUTS" -ForegroundColor $Script:Colors.Header
    Write-Host "   Up/Down         " -NoNewline -ForegroundColor $Script:Colors.Shortcut
    Write-Host "Navigate menus" -ForegroundColor $Script:Colors.Text
    Write-Host "   Left/Right      " -NoNewline -ForegroundColor $Script:Colors.Shortcut
    Write-Host "Toggle YES/NO options" -ForegroundColor $Script:Colors.Text
    Write-Host "   1-9             " -NoNewline -ForegroundColor $Script:Colors.Shortcut
    Write-Host "Quick select devices by number" -ForegroundColor $Script:Colors.Text
    Write-Host "   A-Z             " -NoNewline -ForegroundColor $Script:Colors.Shortcut
    Write-Host "Quick select by letter (e.g., [S]can, [Q]uit)" -ForegroundColor $Script:Colors.Text
    Write-Host "   Enter           " -NoNewline -ForegroundColor $Script:Colors.Success
    Write-Host "Confirm selection" -ForegroundColor $Script:Colors.Text
    Write-Host "   ESC             " -NoNewline -ForegroundColor $Script:Colors.Error
    Write-Host "Cancel / Go back / Abort operation" -ForegroundColor $Script:Colors.Text
}

function Show-Screen9-TogglePrompt {
    Clear-Host
    Write-Header "Application Management (Optimize)"

    Write-Host ""
    Show-StaticToggle -Prompt "Apply all default actions without prompting?" -Options @("NO (Review Each)", "YES (Use Defaults)") -SelectedIndex 0

    Write-Host ""
    Write-Host ""
    Write-Host "Remove: " -NoNewline
    Write-Host "Google Feedback" -ForegroundColor $Script:Colors.Label -NoNewline
    Write-Host " [Safe]" -ForegroundColor "Green" -NoNewline
    Write-Host " (using 12.3 MB RAM)" -ForegroundColor "DarkCyan"
    Write-Dim "    Stops feedback data collection."

    Write-Host ""
    Show-StaticToggle -Prompt "Action?" -Options @("DISABLE", "SKIP", "UNINSTALL", "ABORT") -SelectedIndex 0
}

function Show-Screen10-TaskSummary {
    Clear-Host
    Write-Header "Summary"

    Write-Host " Disabled:    $($MockTaskSummary.Disabled) apps" -ForegroundColor $Script:Colors.Success
    Write-Host " Uninstalled: $($MockTaskSummary.Uninstalled) apps" -ForegroundColor $Script:Colors.Success
    Write-Host " Skipped:     $($MockTaskSummary.Skipped) apps" -ForegroundColor $Script:Colors.TextDim
    if ($MockTaskSummary.Failed -gt 0) {
        Write-Host " Failed:      $($MockTaskSummary.Failed) apps" -ForegroundColor $Script:Colors.Error
    }

    Write-Host ""
    Write-Success "Optimization complete! Reboot your device for best results."
}

# ============================================================================
# SCREEN GALLERY
# ============================================================================

$screens = @(
    @{ Name = "Main Menu (Device Selection)"; Render = { Show-Screen1-MainMenu } },
    @{ Name = "Device Action Menu"; Render = { Show-Screen2-ActionMenu } },
    @{ Name = "Device Profile"; Render = { Show-Screen3-DeviceProfile } },
    @{ Name = "Health Report - Vitals"; Render = { Show-Screen4-HealthReport } },
    @{ Name = "Health Report - Top Memory"; Render = { Show-Screen5-TopMemory } },
    @{ Name = "Health Report - Bloat Check"; Render = { Show-Screen6-BloatCheck } },
    @{ Name = "Launcher Wizard"; Render = { Show-Screen7-LauncherWizard } },
    @{ Name = "Help Screen"; Render = { Show-Screen8-HelpScreen } },
    @{ Name = "Toggle Prompt (Optimize)"; Render = { Show-Screen9-TogglePrompt } },
    @{ Name = "Task Summary"; Render = { Show-Screen10-TaskSummary } }
)

# ============================================================================
# MAIN
# ============================================================================

if ($Screen -gt 0) {
    # Render single screen and exit (for automated capture)
    if ($Screen -le $screens.Count) {
        & $screens[$Screen - 1].Render
    } else {
        Write-Host "Invalid screen number. Valid range: 1-$($screens.Count)" -ForegroundColor Red
        exit 1
    }
    exit 0
}

# Interactive gallery mode
function Show-Gallery {
    $index = 0
    while ($index -lt $screens.Count) {
        Clear-Host
        Write-Host "=== SCREENSHOT GALLERY ===" -ForegroundColor Cyan
        Write-Host "Screen $($index + 1)/$($screens.Count): $($screens[$index].Name)" -ForegroundColor Yellow
        Write-Host ""

        # Render the screen
        & $screens[$index].Render

        Write-Host ""
        Write-Host "--- Press [N]ext, [P]rev, [Q]uit ---" -ForegroundColor DarkGray

        $key = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
        switch ($key.Character) {
            'n' { $index++; if ($index -ge $screens.Count) { $index = 0 } }
            'N' { $index++; if ($index -ge $screens.Count) { $index = 0 } }
            'p' { $index--; if ($index -lt 0) { $index = $screens.Count - 1 } }
            'P' { $index--; if ($index -lt 0) { $index = $screens.Count - 1 } }
            'q' { Clear-Host; return }
            'Q' { Clear-Host; return }
        }
        # Also handle arrow keys
        if ($key.VirtualKeyCode -eq 39) { $index++; if ($index -ge $screens.Count) { $index = 0 } }  # Right arrow
        if ($key.VirtualKeyCode -eq 37) { $index--; if ($index -lt 0) { $index = $screens.Count - 1 } }  # Left arrow
    }
}

Show-Gallery
