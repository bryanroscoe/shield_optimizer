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
- **Health Reports** - Temperature, RAM, storage, bloat scan with memory usage and recommended actions
- **Live Monitor** - Real-time vitals dashboard with auto-refresh (temp, RAM, top memory apps)
- **Fast Network Scan** - Parallel subnet scanning finds devices in seconds
- **Performance Tuning** - Animation speed, background process limits
- **Safe Defaults** - Disables rather than uninstalls, easy restore/recovery
- **Keyboard Navigation** - Arrow keys, letter shortcuts, ESC to cancel

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

```bash
# Install PowerShell via Homebrew
brew install powershell/tap/powershell

# Run the script
pwsh ./Shield-Optimizer.ps1
```

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

## Quick Start

```powershell
# Standard run
pwsh ./Shield-Optimizer.ps1

# Demo mode (for screenshots)
pwsh ./Shield-Optimizer.ps1 -Demo

# Force re-download ADB tools
pwsh ./Shield-Optimizer.ps1 -ForceAdbDownload
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

| Problem | Solution |
|---------|----------|
| Device not found | Enable Network Debugging, try Scan Network, check TV for auth prompt |
| Device shows UNAUTHORIZED | Look at your TV screen and accept the "Allow USB debugging?" prompt |
| Launcher won't switch | Use Launcher Wizard, press Home after disabling stock |
| Something broke | Use Recovery mode to re-enable all disabled packages |
| Wrong device type | Check Profile view, detection uses brand/model/packages |
| Scan finds nothing (macOS/Linux) | Make sure devices have Network Debugging enabled; try Connect IP |

## Safety

- **Disable over Uninstall** - All changes reversible via Restore mode
- **Recovery Mode** - Emergency re-enable of all disabled packages
- **Abort Anytime** - ESC cancels mid-operation with partial summary

## Credits

- Debloat research from community guides including [florisse.nl/shield-debloat](https://florisse.nl/shield-debloat/)
- Built with AI assistance, tested on real devices

**Use at your own risk.** Modifying system apps carries some risk, but this tool prioritizes safe, reversible changes.
