# Android TV Optimizer

A cross-platform PowerShell tool to debloat and optimize Android TV devices. Runs on **Windows, macOS, and Linux**. Supports Nvidia Shield TV, Onn 4K Pro, Chromecast with Google TV, and other Android TV devices.

## Screenshots![Animation](https://github.com/user-attachments/assets/0db3cb04-109f-4785-9acd-728bf9ff646f)

<img width="716" height="341" alt="image" src="https://github.com/user-attachments/assets/37f31233-8286-4b4b-a490-899735c95133" />

<img width="625" height="294" alt="image" src="https://github.com/user-attachments/assets/7ee836ea-1506-4ac5-9feb-aaca580d38a3" />

<img width="638" height="391" alt="image" src="https://github.com/user-attachments/assets/fb40e710-ad39-4d71-b6e7-6c74f0586f27" />

<img width="539" height="255" alt="image" src="https://github.com/user-attachments/assets/855d73b6-d79c-4023-b8bd-32e91af252a6" />

<img width="666" height="293" alt="image" src="https://github.com/user-attachments/assets/67423cb2-c09f-4ef1-b25a-583d870effd0" />

<img width="411" height="197" alt="image" src="https://github.com/user-attachments/assets/857717fb-c9db-4a22-b428-fb3d82bc067e" />

## Features

- **Cross-Platform** - Runs on Windows, macOS, and Linux with PowerShell 7+
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
| Windows 10/11 (PowerShell 5.1+) | Fully tested |
| macOS (PowerShell 7+) | Fully tested |
| Linux/Ubuntu (PowerShell 7+) | Should work |

## Requirements

1. **Computer** with PowerShell:
   - **Windows**: PowerShell 5.1+ (built-in) or PowerShell 7+
   - **macOS**: PowerShell 7+ (see installation below)
   - **Linux**: PowerShell 7+ (see installation below)
2. **Android TV** with Developer Options enabled:
   - Settings > Device Preferences > About > Click **Build** 7 times
   - Settings > Developer Options > Enable **Network Debugging**

## Installation

### Windows

PowerShell is built-in. Just download and run the script.

```powershell
# Download and run
.\Shield-Optimizer.ps1

# Or bypass execution policy
Set-ExecutionPolicy Bypass -Scope Process -Force; .\Shield-Optimizer.ps1
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
