# ScreenshotGallery.ps1 - Render UI screens for screenshots
# Usage: pwsh ./demos/ScreenshotGallery.ps1
#        pwsh ./demos/ScreenshotGallery.ps1 -Screen 1   # Render single screen and exit

param(
    [int]$Screen = 0        # 0 = interactive gallery, 1-N = render single screen and exit
)

# Import function extraction helper
Import-Module (Join-Path $PSScriptRoot "../tests/TestHelpers.psm1") -Force
$ScriptPath = Join-Path $PSScriptRoot "../Shield-Optimizer.ps1"

# ============================================================================
# FIXED SIZE OUTPUT - All screens render exactly 20 lines x 60 chars
# ============================================================================

$Script:TargetLines = 20
$Script:TargetWidth = 60
$Script:CurrentLine = 0

function Reset-LineCount { $Script:CurrentLine = 0 }

function Add-Line {
    param([string]$Text = "")
    # Pad line to fixed width
    if ($Text.Length -lt $Script:TargetWidth) {
        $Text = $Text + (" " * ($Script:TargetWidth - $Text.Length))
    }
    Write-Host $Text
    $Script:CurrentLine++
}

function Add-LinePadded {
    # Write an empty line padded to target width
    Write-Host (" " * $Script:TargetWidth)
    $Script:CurrentLine++
}

function Pad-ToTarget {
    while ($Script:CurrentLine -lt $Script:TargetLines) {
        Write-Host (" " * $Script:TargetWidth)
        $Script:CurrentLine++
    }
}

# ============================================================================
# MOCK DATA
# ============================================================================

$MockVitals = @{
    Temperature = 42.5; RAMPercent = 67; RAMTotal = 3072; RAMUsed = 2058
    Swap = 256; StoragePercent = 45; StorageTotal = "32G"; StorageUsed = "14.4G"
}

$MockTopApps = @(
    @{ Package = "com.google.android.youtube.tv"; MB = 245.3 },
    @{ Package = "com.netflix.ninja"; MB = 189.7 },
    @{ Package = "com.amazon.amazonvideo.livingroom"; MB = 156.2 },
    @{ Package = "com.disney.disneyplus"; MB = 134.8 },
    @{ Package = "com.plexapp.android"; MB = 87.6 }
)

$MockBloatApps = @(
    @{ Name = "Google Feedback"; Method = "DISABLE"; Memory = 12.3 },
    @{ Name = "Print Spooler"; Method = "DISABLE"; Memory = 0 },
    @{ Name = "Android Gallery"; Method = "DISABLE"; Memory = 8.4 },
    @{ Name = "Device Usage Stats"; Method = "DISABLE"; Memory = 15.7 },
    @{ Name = "Nvidia Games"; Method = "UNINSTALL"; Memory = 42.1 }
)

$MockLaunchers = @(
    @{ Name = "Projectivy Launcher"; Status = "INSTALLED" },
    @{ Name = "FLauncher"; Status = "MISSING" },
    @{ Name = "Wolf Launcher"; Status = "MISSING" },
    @{ Name = "Stock Launcher"; Status = "ACTIVE" }
)

# ============================================================================
# EXTRACT REQUIRED FUNCTIONS
# ============================================================================

$functionsToExtract = @(
    "Write-Header", "Write-SubHeader", "Write-Success", "Write-Warn",
    "Write-Info", "Write-Dim", "Get-VitalColor", "Write-OptionWithHighlight"
)

foreach ($fn in $functionsToExtract) {
    try {
        $def = Get-FunctionDefinition -ScriptPath $ScriptPath -FunctionName $fn
        . ([ScriptBlock]::Create($def))
    } catch { }
}

# ============================================================================
# COLOR SCHEME (Dark Mode)
# ============================================================================

$Script:Colors = @{
    Header = "Cyan"; SubHeader = "Blue"; Success = "Green"; Warning = "Yellow"
    Error = "Red"; Info = "White"; Text = "White"; TextDim = "DarkCyan"
    TextBright = "Cyan"; Label = "Cyan"; Value = "White"; Separator = "Blue"
    Selected = "Cyan"; Unselected = "White"; Shortcut = "Yellow"; Bracket = "Gray"
    # Status colors for launcher tags
    Active = "Green"; Disabled = "Magenta"; Missing = "Red"; Installed = "Cyan"; NotFound = "Gray"
}

$Script:VitalThresholds = @{
    Temperature = @{ Warning = 50; Critical = 70; Normal = "Green" }
    RAM = @{ Warning = 70; Critical = 85; Normal = "Green" }
    Storage = @{ Warning = 75; Critical = 90; Normal = "Green" }
    AppMemory = @{ Warning = 100; Critical = 200; Normal = "White" }
}

# ============================================================================
# MENU RENDERER
# ============================================================================

function Show-StaticMenu {
    param([string]$Title, [array]$Options, [array]$Descriptions, [int]$SelectedIndex = 0, [array]$Shortcuts = @())

    $displayTexts = @{}
    for ($i = 0; $i -lt $Options.Count; $i++) {
        if ($Options[$i] -eq "---") { $displayTexts[$i] = "---"; continue }
        $shortcut = if ($Shortcuts.Count -gt $i -and $Shortcuts[$i]) { $Shortcuts[$i] } else { $Options[$i].Substring(0,1).ToUpper() }
        $optText = $Options[$i]
        $foundPos = -1; $inBracket = $false
        for ($c = 0; $c -lt $optText.Length; $c++) {
            if ($optText[$c] -eq '[') { $inBracket = $true }
            elseif ($optText[$c] -eq ']') { $inBracket = $false }
            elseif (-not $inBracket -and $optText[$c].ToString().ToUpper() -eq $shortcut.ToUpper()) { $foundPos = $c; break }
        }
        if ($foundPos -ge 0) { $displayTexts[$i] = $optText.Substring(0, $foundPos) + "[$($optText[$foundPos])]" + $optText.Substring($foundPos + 1) }
        else { $displayTexts[$i] = "[$shortcut] $optText" }
    }

    Add-Line " $Title"
    Add-Line " ================================================"
    for ($i = 0; $i -lt $Options.Count; $i++) {
        if ($Options[$i] -eq "---") { Add-Line "    --------------------------------"; continue }
        if ($i -eq $SelectedIndex) {
            Write-Host "  > " -NoNewline -ForegroundColor $Script:Colors.Selected
            Write-OptionWithHighlight -Text $displayTexts[$i] -Selected $true -WithClosingArrow $true
        } else {
            Write-Host "    " -NoNewline
            Write-OptionWithHighlight -Text $displayTexts[$i] -Selected $false
        }
        $Script:CurrentLine++
    }
    Add-Line " ================================================"
    Write-Host " Info: " -NoNewline -ForegroundColor $Script:Colors.Warning
    if ($Descriptions.Count -gt $SelectedIndex) { Write-Host "$($Descriptions[$SelectedIndex])" -ForegroundColor $Script:Colors.Text }
    else { Write-Host "Select an option." -ForegroundColor $Script:Colors.TextDim }
    $Script:CurrentLine++
    Add-Line " [Arrows: Move] [Keys: Select] [Enter: OK] [ESC: Back]"
}

function Show-StaticToggle {
    param([string]$Prompt, [array]$Options, [int]$SelectedIndex = 0)
    $esc = [char]27; $sel = "$esc[96m"; $gray = "$esc[37m"; $reset = "$esc[0m"
    $line = "${gray}${Prompt}${reset} "
    for ($i = 0; $i -lt $Options.Count; $i++) {
        if ($i -eq $SelectedIndex) { $line += "${sel} [ $($Options[$i]) ] ${reset}" }
        else { $line += "${gray}   $($Options[$i])   ${reset}" }
    }
    Add-Line $line
}

# ============================================================================
# SCREENS
# ============================================================================

function Show-Screen-MainMenu {
    Reset-LineCount
    $options = @("Shield TV Pro", "Onn 4K Box", "---", "Scan Network", "Connect IP", "---", "Help", "Quit")
    $descs = @("Nvidia Shield - 192.168.42.143:5555", "Google TV - 192.168.42.25:5555", "", "Auto-discover devices on your network", "Enter device IP manually", "", "Help & troubleshooting", "Exit application")
    $shortcuts = @("1", "2", "", "S", "C", "", "H", "Q")
    Show-StaticMenu -Title "Shield Optimizer - Select Device" -Options $options -Descriptions $descs -SelectedIndex 0 -Shortcuts $shortcuts
    Pad-ToTarget
}

function Show-Screen-ActionMenu {
    Reset-LineCount
    $options = @("Optimize", "Restore", "Report", "Launcher Setup", "---", "Back")
    $descs = @("Disable/uninstall bloatware apps", "Re-enable previously disabled apps", "Health check: temp, RAM, storage, bloat", "Install custom launcher", "", "Return to device selection")
    $shortcuts = @("O", "R", "E", "L", "", "B")
    Show-StaticMenu -Title "Shield TV Pro - Actions" -Options $options -Descriptions $descs -SelectedIndex 0 -Shortcuts $shortcuts
    Pad-ToTarget
}

function Show-Screen-Scanning {
    Reset-LineCount
    Add-Line ""
    Write-Host "=== Scanning Network ===" -ForegroundColor $Script:Colors.Header; $Script:CurrentLine++
    Add-Line ""
    Write-Host " Subnet: " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "192.168.42.x" -ForegroundColor $Script:Colors.Value; $Script:CurrentLine++
    Add-Line ""
    Write-Host " [" -NoNewline -ForegroundColor $Script:Colors.TextDim
    Write-Host "################" -NoNewline -ForegroundColor $Script:Colors.Success
    Write-Host "--------" -NoNewline -ForegroundColor $Script:Colors.TextDim
    Write-Host "] 67%" -ForegroundColor $Script:Colors.TextDim; $Script:CurrentLine++
    Add-Line ""
    Write-Host " Found:" -ForegroundColor $Script:Colors.Success; $Script:CurrentLine++
    Write-Host "   192.168.42.143 " -NoNewline -ForegroundColor $Script:Colors.Value
    Write-Host "Shield TV Pro" -ForegroundColor $Script:Colors.Label; $Script:CurrentLine++
    Write-Host "   192.168.42.25  " -NoNewline -ForegroundColor $Script:Colors.Value
    Write-Host "Onn 4K Box" -ForegroundColor $Script:Colors.Label; $Script:CurrentLine++
    Add-Line ""
    Write-Host " Scanning 192.168.42.171..." -ForegroundColor $Script:Colors.TextDim; $Script:CurrentLine++
    Pad-ToTarget
}

function Show-Screen-Report {
    Reset-LineCount
    Add-Line ""
    Write-Host "=== Health Report: Shield TV Pro ===" -ForegroundColor $Script:Colors.Header; $Script:CurrentLine++
    Add-Line ""
    Write-Host "--- Vitals ---" -ForegroundColor $Script:Colors.SubHeader; $Script:CurrentLine++
    Write-Host " Temp:    " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "$($MockVitals.Temperature)°C" -ForegroundColor (Get-VitalColor -Type "Temperature" -Value $MockVitals.Temperature); $Script:CurrentLine++
    Write-Host " RAM:     " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "$($MockVitals.RAMPercent)% ($($MockVitals.RAMUsed) / $($MockVitals.RAMTotal) MB)" -ForegroundColor (Get-VitalColor -Type "RAM" -Value $MockVitals.RAMPercent); $Script:CurrentLine++
    Write-Host " Storage: " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "$($MockVitals.StorageUsed) / $($MockVitals.StorageTotal) ($($MockVitals.StoragePercent)%)" -ForegroundColor (Get-VitalColor -Type "Storage" -Value $MockVitals.StoragePercent); $Script:CurrentLine++
    Add-Line ""
    Write-Host "--- Top Memory ---" -ForegroundColor $Script:Colors.SubHeader; $Script:CurrentLine++
    foreach ($app in $MockTopApps) {
        $memColor = Get-VitalColor -Type "AppMemory" -Value $app.MB
        Write-Host " $($app.MB.ToString('0.0').PadLeft(6)) MB  " -NoNewline -ForegroundColor $memColor
        Write-Host "$($app.Package)" -ForegroundColor $Script:Colors.Value; $Script:CurrentLine++
    }
    Pad-ToTarget
}

function Show-Screen-BloatCheck {
    Reset-LineCount
    Add-Line ""
    Write-Host "=== Bloat Check ===" -ForegroundColor $Script:Colors.Header; $Script:CurrentLine++
    Add-Line ""
    Write-Host " App Name                RAM        Action" -ForegroundColor $Script:Colors.Text; $Script:CurrentLine++
    Write-Host " $("-" * 50)" -ForegroundColor $Script:Colors.Separator; $Script:CurrentLine++
    foreach ($bloat in $MockBloatApps) {
        $memStr = if ($bloat.Memory -gt 0) { "$($bloat.Memory) MB" } else { "-- MB" }
        Write-Host " " -NoNewline
        Write-Host $bloat.Name.PadRight(20) -NoNewline -ForegroundColor $Script:Colors.Warning
        Write-Host $memStr.PadRight(11) -NoNewline -ForegroundColor $Script:Colors.Label
        Write-Host $bloat.Method -ForegroundColor $Script:Colors.Value; $Script:CurrentLine++
    }
    Add-Line ""
    $totalMem = ($MockBloatApps | Measure-Object -Property Memory -Sum).Sum
    Write-Host " Total bloat: " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "$totalMem MB - Run Optimize to clean" -ForegroundColor $Script:Colors.Warning; $Script:CurrentLine++
    Pad-ToTarget
}

function Show-Screen-Optimize {
    Reset-LineCount
    Add-Line ""
    Write-Host "=== Optimize: Shield TV Pro ===" -ForegroundColor $Script:Colors.Header; $Script:CurrentLine++
    Add-Line ""
    Show-StaticToggle -Prompt "Apply defaults?" -Options @("NO (Review)", "YES (Auto)") -SelectedIndex 0
    Add-Line ""
    Write-Host " [INFO] Processing 42 apps..." -ForegroundColor $Script:Colors.Info; $Script:CurrentLine++
    Add-Line ""
    Write-Host "Remove: " -NoNewline
    Write-Host "Google Feedback" -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host " [Safe]" -NoNewline -ForegroundColor "Green"
    Write-Host " (12 MB)" -ForegroundColor "DarkCyan"; $Script:CurrentLine++
    Write-Host "     Stops feedback data collection." -ForegroundColor $Script:Colors.TextDim; $Script:CurrentLine++
    Add-Line ""
    Show-StaticToggle -Prompt "Action?" -Options @("DISABLE", "SKIP", "UNINSTALL") -SelectedIndex 0
    Pad-ToTarget
}

function Show-Screen-OptimizeProgress {
    Reset-LineCount
    Add-Line ""
    Write-Host "=== Optimize: Shield TV Pro ===" -ForegroundColor $Script:Colors.Header; $Script:CurrentLine++
    Add-Line ""
    Write-Host " [OK] Google Feedback ... DISABLED" -ForegroundColor $Script:Colors.Success; $Script:CurrentLine++
    Write-Host " [OK] Print Spooler ... DISABLED" -ForegroundColor $Script:Colors.Success; $Script:CurrentLine++
    Write-Host " Android Gallery ... [ALREADY DISABLED]" -ForegroundColor $Script:Colors.TextDim; $Script:CurrentLine++
    Write-Host " [OK] Device Usage ... DISABLED" -ForegroundColor $Script:Colors.Success; $Script:CurrentLine++
    Add-Line ""
    Write-Host "Remove: " -NoNewline
    Write-Host "Nvidia Games" -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host " [Medium]" -NoNewline -ForegroundColor "Yellow"
    Write-Host " (42 MB)" -ForegroundColor "DarkCyan"; $Script:CurrentLine++
    Write-Host "     GeForce NOW game streaming." -ForegroundColor $Script:Colors.TextDim; $Script:CurrentLine++
    Add-Line ""
    Show-StaticToggle -Prompt "Action?" -Options @("DISABLE", "SKIP", "UNINSTALL") -SelectedIndex 2
    Pad-ToTarget
}

function Show-Screen-Summary {
    Reset-LineCount
    Add-Line ""
    Write-Host "=== Optimization Complete ===" -ForegroundColor $Script:Colors.Header; $Script:CurrentLine++
    Add-Line ""
    Write-Host " Disabled:    " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "8 apps" -ForegroundColor $Script:Colors.Success; $Script:CurrentLine++
    Write-Host " Uninstalled: " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "3 apps" -ForegroundColor $Script:Colors.Success; $Script:CurrentLine++
    Write-Host " Skipped:     " -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host "5 apps" -ForegroundColor $Script:Colors.TextDim; $Script:CurrentLine++
    Add-Line ""
    Write-Host " [OK] Done! Reboot device for best results." -ForegroundColor $Script:Colors.Success; $Script:CurrentLine++
    Add-Line ""
    Write-Host " Freed approximately 156 MB of RAM" -ForegroundColor $Script:Colors.TextDim; $Script:CurrentLine++
    Write-Host " Removed 3 background services" -ForegroundColor $Script:Colors.TextDim; $Script:CurrentLine++
    Pad-ToTarget
}

function Show-Screen-Launcher {
    Reset-LineCount
    Add-Line ""
    Write-Host "=== Launcher Setup ===" -ForegroundColor $Script:Colors.Header; $Script:CurrentLine++
    Write-Host " [INFO] Current: Stock Launcher (Google TV)" -ForegroundColor $Script:Colors.Info; $Script:CurrentLine++
    Add-Line ""
    $options = @()
    foreach ($l in $MockLaunchers) { $options += "$($l.Name) [$($l.Status)]" }
    $options += "Back"
    $descs = @("Minimal, fast, customizable", "Open source launcher", "Feature-rich with widgets", "Default Android TV launcher", "Return to menu")
    $shortcuts = @("P", "F", "W", "S", "B")
    Show-StaticMenu -Title "Select Launcher" -Options $options -Descriptions $descs -SelectedIndex 0 -Shortcuts $shortcuts
    Pad-ToTarget
}

function Show-Screen-Restore {
    Reset-LineCount
    Add-Line ""
    Write-Host "=== Restore: Shield TV Pro ===" -ForegroundColor $Script:Colors.Header; $Script:CurrentLine++
    Add-Line ""
    Write-Host " [OK] Google Feedback ... RESTORED" -ForegroundColor $Script:Colors.Success; $Script:CurrentLine++
    Write-Host " [OK] Print Spooler ... RESTORED" -ForegroundColor $Script:Colors.Success; $Script:CurrentLine++
    Write-Host " Nvidia Games ... [REINSTALLING]" -ForegroundColor $Script:Colors.Warning; $Script:CurrentLine++
    Add-Line ""
    Write-Host "Restore: " -NoNewline
    Write-Host "Android Gallery" -NoNewline -ForegroundColor $Script:Colors.Label
    Write-Host " [Safe]" -ForegroundColor "Green"; $Script:CurrentLine++
    Write-Host "     Legacy photo viewer app." -ForegroundColor $Script:Colors.TextDim; $Script:CurrentLine++
    Add-Line ""
    Show-StaticToggle -Prompt "Action?" -Options @("RESTORE", "SKIP") -SelectedIndex 0
    Pad-ToTarget
}

# ============================================================================
# SCREEN GALLERY
# ============================================================================

$screens = @(
    @{ Name = "main-menu"; Render = { Show-Screen-MainMenu } },
    @{ Name = "scanning"; Render = { Show-Screen-Scanning } },
    @{ Name = "action-menu"; Render = { Show-Screen-ActionMenu } },
    @{ Name = "report"; Render = { Show-Screen-Report } },
    @{ Name = "bloat-check"; Render = { Show-Screen-BloatCheck } },
    @{ Name = "optimize"; Render = { Show-Screen-Optimize } },
    @{ Name = "optimize-progress"; Render = { Show-Screen-OptimizeProgress } },
    @{ Name = "summary"; Render = { Show-Screen-Summary } },
    @{ Name = "launcher"; Render = { Show-Screen-Launcher } },
    @{ Name = "restore"; Render = { Show-Screen-Restore } }
)

# ============================================================================
# MAIN
# ============================================================================

if ($Screen -gt 0) {
    if ($Screen -le $screens.Count) { & $screens[$Screen - 1].Render }
    else { Write-Host "Invalid screen. Valid: 1-$($screens.Count)" -ForegroundColor Red; exit 1 }
    exit 0
}

function Show-Gallery {
    $index = 0
    while ($true) {
        Clear-Host
        Write-Host "=== SCREENSHOT GALLERY ($($index + 1)/$($screens.Count)) ===" -ForegroundColor Cyan
        Write-Host "Screen: $($screens[$index].Name)" -ForegroundColor Yellow
        & $screens[$index].Render
        Write-Host "`n--- [N]ext [P]rev [Q]uit ---" -ForegroundColor DarkGray
        $key = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
        switch -Regex ($key.Character) {
            '[nN]' { $index = ($index + 1) % $screens.Count }
            '[pP]' { $index = if ($index -eq 0) { $screens.Count - 1 } else { $index - 1 } }
            '[qQ]' { Clear-Host; return }
        }
        if ($key.VirtualKeyCode -eq 39) { $index = ($index + 1) % $screens.Count }
        if ($key.VirtualKeyCode -eq 37) { $index = if ($index -eq 0) { $screens.Count - 1 } else { $index - 1 } }
    }
}

Show-Gallery
