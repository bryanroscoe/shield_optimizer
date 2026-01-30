# Android TV Optimizer

A cross-platform PowerShell tool to debloat and optimize Android TV devices. Runs on **Windows, macOS, and Linux**. Supports Nvidia Shield TV, Onn 4K Pro, Chromecast with Google TV, and other Android TV devices.

![Screen Recording 2026-01-20 at 5 51 56 PM](https://github.com/user-attachments/assets/4f8777ad-025d-4f64-b8ae-1c533b30abbf)

<img width="572" height="476" alt="image" src="https://github.com/user-attachments/assets/1badb90c-a1c2-4a8d-89fa-89cc4b5fbe3d" />
<img width="572" height="476" alt="image" src="https://github.com/user-attachments/assets/1a217695-e6de-4c38-be2f-012add6f8692" />
<img width="572" height="476" alt="image" src="https://github.com/user-attachments/assets/5f15a6c3-2108-46b5-b25b-50e8cea4fd5a" />



## Features

- **Cross-Platform** - Runs on Windows, macOS, and Linux (requires PowerShell 7+)
- **Multi-Device Support** - Auto-detects Shield, Onn 4K, Chromecast, Google TV Streamer
- **Device-Specific Debloat** - Different app lists for Shield vs Google TV devices
- **Launcher Wizard** - Install Projectivy/FLauncher/ATV/Wolf Launcher, safely disable stock
- **APK Sideloading** - Install APK files from local folder or custom path
- **Health Reports** - Temperature, RAM, storage, bloat scan with memory usage and recommended actions
- **Live Monitor** - Real-time vitals dashboard with auto-refresh (temp, RAM, top memory apps)
- **Fast Network Scan** - Parallel subnet scanning finds devices in seconds
- **Performance Tuning** - Animation speed, background process limits
- **Safe Defaults** - Disables rather than uninstalls, easy restore/recovery
- **Keyboard Navigation** - Arrow keys, letter shortcuts, ESC to cancel

## Experimental Features (Untested)

These features have been implemented but not yet tested on real devices:

| Feature | Description | Notes |
|---------|-------------|-------|
| **USB Device Support** | Connect to Android devices via USB cable | Displays `[USB]` tag in device list. Note: Shield TV doesn't support USB debugging (host ports only). Works with phones/tablets. |
| **PIN Pairing** | Pair with Android 11+ / Google TV devices using network debugging | **Experimental.** For newer Chromecasts with Google TV and Android TV devices that require pairing codes. Not needed for Shield TV (use standard Connect IP instead). |

## Tested Devices

| Device | Status |
|--------|--------|
| Nvidia Shield TV (2015/2017/2019) | Fully tested |
| Onn 4K Pro (Walmart) | Fully tested |
| Chromecast with Google TV | Should work (Google TV profile) |
| Google TV Streamer (2024) | Should work (Google TV profile) |

## Tested Platforms

| Platform | Status |
|----------|--------|
| Windows 10/11 (PowerShell 7+) | Fully tested |
| macOS (PowerShell 7+) | Fully tested |
| Linux/Ubuntu (PowerShell 7+) | Should work |

## Requirements

1. **PowerShell 7+** on all platforms (see installation below)
2. **Android TV** with Developer Options enabled:
   - Settings > Device Preferences > About > Click **Build** 7 times
   - Settings > Developer Options > Enable **Network Debugging**

## Installation

### Windows

PowerShell 7 is required. Windows Terminal is also recommended for the best experience.

**Option 1: winget (recommended)**
```powershell
winget install --id Microsoft.PowerShell; winget install --id Microsoft.WindowsTerminal
```

**Option 2: Download installers**
- PowerShell 7: [GitHub releases](https://github.com/PowerShell/PowerShell/releases/latest) - get the `.msi` file
- Windows Terminal: [Microsoft Store](https://aka.ms/terminal)

After installing, open Windows Terminal, select PowerShell 7, and run:
```powershell
pwsh .\Shield-Optimizer.ps1
```

### macOS

The easiest way to install PowerShell is via [Homebrew](https://brew.sh/):

```bash
# Install Homebrew if you don't have it
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install PowerShell
brew install powershell/tap/powershell
```

Alternative: Download the `.pkg` installer from [PowerShell releases](https://github.com/PowerShell/PowerShell/releases/latest).

### Linux (Debian/Ubuntu)

```bash
# Install PowerShell
sudo apt-get update
sudo apt-get install -y wget apt-transport-https software-properties-common
wget -q "https://packages.microsoft.com/config/ubuntu/$(lsb_release -rs)/packages-microsoft-prod.deb"
sudo dpkg -i packages-microsoft-prod.deb
sudo apt-get update
sudo apt-get install -y powershell

# Run the script
pwsh ./Shield-Optimizer.ps1
```

## Download

**Option 1: Download from Releases (Recommended)**
1. Go to the [Releases page](https://github.com/bryanroscoe/shield_optimizer/releases/latest)
2. Download `Source code (zip)` or `Source code (tar.gz)`
3. Extract to a folder you'll remember (e.g., `Downloads` or `Documents`)

**Option 2: Clone with git**
```bash
git clone https://github.com/bryanroscoe/shield_optimizer.git
cd shield_optimizer
```

**Important:** You must run the script from the folder where it's located. The commands below assume you're in that folder.

## Quick Start

1. Open Terminal (macOS/Linux) or PowerShell 7 (Windows)
2. Navigate to the folder containing the script:
   ```bash
   cd /path/to/shield_optimizer
   ```
3. Run the script:
   ```powershell
   pwsh ./Shield-Optimizer.ps1
   ```

**Force re-download ADB tools:**
```powershell
pwsh ./Shield-Optimizer.ps1 -ForceAdbDownload
```

**Theme auto-detection:**
The script automatically detects your system's light/dark mode setting. Override with `-LightMode` or `-DarkMode` if needed:
```powershell
pwsh ./Shield-Optimizer.ps1 -LightMode   # Force light theme
pwsh ./Shield-Optimizer.ps1 -DarkMode    # Force dark theme
```

ADB tools download automatically on first run (platform-appropriate version). Accept the debugging prompt on your TV.

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑` `↓` | Navigate menu |
| `←` `→` | Toggle options (YES/NO) |
| `1-9` | Select device by number |
| `A-Z` | Select option by highlighted letter |
| `Enter` | Confirm |
| `ESC` | Back / Cancel / Abort |

## Troubleshooting

### Windows - "Running scripts is disabled on this system"

This happens because PowerShell blocks unsigned scripts by default. Fix it with one of these options:

**Option 1: Change execution policy (recommended)**
1. Open PowerShell 7 as Administrator
2. Run: `Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser`

**Option 2: Bypass for a single run**
```powershell
pwsh -ExecutionPolicy Bypass -File .\Shield-Optimizer.ps1
```

### Windows - "Script errors or won't run properly"

Make sure you're using **PowerShell 7**, not Windows PowerShell:

| App | Version | Icon Color |
|-----|---------|------------|
| PowerShell 7 (correct) | 7.x | Black/Dark |
| Windows PowerShell (wrong) | 5.x | Blue |

**How to check:** Run `$PSVersionTable.PSVersion` - major version should be 7+.

**How to launch PowerShell 7:**
1. Open Windows Terminal
2. Click the dropdown arrow next to the tab
3. Select "PowerShell" (not "Windows PowerShell")

### Other Issues

| Problem | Solution |
|---------|----------|
| Device not found | Enable Network Debugging, try Scan Network, check TV for auth prompt |
| Device shows UNAUTHORIZED | Look at your TV screen and accept the "Allow USB debugging?" prompt |
| Launcher won't switch | Use Launcher Wizard, press Home after disabling stock |
| Something broke | Use Recovery mode to re-enable all disabled packages |
| Wrong device type | Check Profile view, detection uses brand/model/packages |
| Scan finds nothing (macOS/Linux) | Make sure devices have Network Debugging enabled; try Connect IP |
| Colors hard to read | Theme auto-detects, but try `-LightMode` or `-DarkMode` to override |

## Safety

- **Disable over Uninstall** - All changes reversible via Restore mode
- **Recovery Mode** - Emergency re-enable of all disabled packages
- **Abort Anytime** - ESC cancels mid-operation with partial summary

## Credits

- Debloat research from community guides including [florisse.nl/shield-debloat](https://florisse.nl/shield-debloat/)
- Built with AI assistance Gemini Pro and Claude Opus ( by an actual software engineer ) , tested on real devices

**Use at your own risk.** Modifying system apps carries some risk, but this tool prioritizes safe, reversible changes.
