# Android TV Optimizer

A powerful, interactive PowerShell tool designed to debloat, optimize, and manage Android TV devices. Supports Nvidia Shield TV, Onn 4K Pro, Chromecast with Google TV, Google TV Streamer, and more. Features a colorful menu system with keyboard shortcuts, device-specific optimizations, and safe defaults.

> **Note:** This tool is "vibe coded" - built with AI assistance. Tested and verified on real devices.

**Verified Devices:**
- Nvidia Shield 2015 Pro, Shield 2019 Pro
- Onn 4K Pro (Walmart)
- Chromecast with Google TV

---

## Features

### Multi-Device Support (v62)
* **Auto-Detection:** Automatically identifies device type (Shield, Google TV, etc.)
* **Device-Specific Apps:** Different bloatware lists for Shield vs Google TV devices
* **Universal Diagnostics:** Temperature, RAM, storage work across all device types
* **Smart Launcher Detection:** Finds the correct stock launcher package for your device

### Keyboard-Driven Interface
* **Arrow Keys:** Navigate menus up/down
* **Number Keys (1-9):** Quick-select connected devices
* **Letter Keys:** Quick-select menu options (shown as `[S]can`, `[Q]uit`, etc.)
* **ESC:** Go back / Cancel current operation
* **Enter:** Confirm selection
* **Left/Right Arrows:** Toggle between options like `[ YES ]  NO  ABORT`

### Smart Optimization
* **"Golden Set" Defaults:** Safe bloatware removal - disables rather than uninstalls
* **Apply All Defaults:** Skip prompts and apply recommended settings in one go
* **Abort Anytime:** Press ESC or select ABORT to stop mid-process
* **Session Summary:** See what was disabled, uninstalled, restored, or skipped
* **Granular Control:** Choose the action for every app individually

### Launcher Wizard
* **Auto-Detection:** Detects current launcher and shows `[ACTIVE]` status
* **Multi-Device Support:** Works with Shield, Onn 4K, Chromecast with Google TV
* **One-Click Setup:** Install Projectivy, FLauncher, ATV Launcher, or Wolf Launcher
* **Smart Stock Disable:** Automatically detects and disables the correct stock launcher package
* **Easy Restore:** Re-enable stock launcher when needed

### Performance Tuning
* **Animation Speed:** Set to 0.5x for snappier UI
* **Background Process Limit:** Restrict to 1-4 processes to free RAM
* **One-Click Reset:** Restore all settings to defaults

### Health & Diagnostics
* **Device Report:** Temperature, RAM, storage, Android version, platform info
* **Bloat Check:** Scan for active bloatware (device-specific)
* **Color-Coded Vitals:** Green/Yellow/Red indicators for system health
* **Model Detection:** Identifies Shield, Onn 4K, Chromecast, and other devices

### Safety Features
* **Panic Recovery:** Emergency button to re-enable ALL disabled packages
* **Reboot Options:** Normal, Recovery, or Bootloader modes
* **Device Profile:** View detected settings before making changes

### Connection Tools
* **Network Scan:** Auto-discover devices on local network (200ms timeout)
* **IP Validation:** Validates IP format before connecting
* **Restart ADB:** Fix connection issues with one click
* **Disconnect Device:** Cleanly disconnect without quitting

---

## Supported Devices

| Device | Status | Notes |
|--------|--------|-------|
| Nvidia Shield TV (2015/2017/2019) | Fully Supported | All models tested |
| Onn 4K Pro (Walmart) | Supported | Device-specific optimizations |
| Chromecast with Google TV | Supported | Uses Google TV app list |
| Google TV Streamer (2024) | Supported | Uses Google TV app list |
| Other Android TV | Basic Support | Common apps only |

---

## Requirements

1. **Android TV Device** - Shield TV, Onn 4K, Chromecast with Google TV, etc.
2. **Enable Developer Options:**
   * Settings > Device Preferences > About
   * Click **Build** 7 times
3. **Enable Debugging:**
   * Settings > Device Preferences > Developer Options
   * Enable **USB Debugging**
   * Enable **Network Debugging** (for WiFi connection)
4. **Windows PC** with PowerShell 5.1+ (included in Windows 10/11)

---

## How to Run

1. **Download** `Shield-Optimizer.ps1`
2. **Right-click** and select **Run with PowerShell**

   Or from terminal:
   ```powershell
   Set-ExecutionPolicy Bypass -Scope Process -Force; .\Shield-Optimizer.ps1
   ```
3. **First Run:** ADB tools download automatically from Google
4. **Authorize:** Accept the debugging prompt on your TV screen

---

## Menu Overview

### Main Menu
```
 Android TV Optimizer v62 - Main Menu
 ================================================
  >  [1] Living Room Shield
     [2] Bedroom Onn 4K
     [S]can Network
     [C]onnect IP
     [R]eport All
     re[F]resh
     restart [A]DB
     [H]elp
     [Q]uit
 ================================================
 Info: Nvidia Shield | Shield TV Pro (2019) | 192.168.1.50:5555
 [Arrows: Move] [Keys: Select] [Enter: OK] [ESC: Back]
```

### Action Menu (Per Device)
* **[O]ptimize** - Disable/uninstall bloatware (device-specific), tune performance
* **[R]estore** - Re-enable apps, reset settings
* **r[E]port** - View device health, storage, and bloat status
* **[L]auncher Setup** - Install custom launcher, manage stock launcher
* **[P]rofile** - View device type and detected settings
* **re[C]overy** - Emergency: Re-enable ALL disabled packages
* **re[B]oot** - Restart device (normal, recovery, or bootloader)
* **[D]isconnect** - Disconnect this device from ADB
* **bac[K]** - Return to main menu

### Optimize/Restore Flow
* Choose **Apply All Defaults** to skip individual prompts
* For each app: `DISABLE | UNINSTALL | SKIP | ABORT`
* **ABORT** shows partial summary of changes made
* Final summary shows counts: Disabled, Uninstalled, Skipped, Failed
* Option to reboot device when finished

### Health Report
```
=== Health Report: Living Room Shield (Nvidia Shield) ===

--- System Info ---
 Platform:  tegra
 Android:   11

--- Vitals ---
 Temp:    42.5°C
 RAM:     65% (1980 / 3048 MB)
 Swap:    128 MB
 Storage: 8.2G / 13G (63%)

--- Settings Check ---
 Animation Speed: 0.5
 Process Limit:   2

--- Bloat Check ---
 [OK] System is clean.
```

### Launcher Wizard
```
 Select Launcher
 ================================================
  >  [P]rojectivy Launcher [ACTIVE]
     [F]Launcher [MISSING]
     [A]TV Launcher [INSTALLED]
     [W]olf Launcher [MISSING]
     [S]tock Launcher (Google TV) [DISABLED]
     [B]ack
 ================================================
```

Status colors:
* **[ACTIVE]** - Green (currently in use)
* **[INSTALLED]** - Cyan
* **[MISSING]** - Red
* **[DISABLED]** - Yellow

---

## Keyboard Shortcuts Reference

| Key | Action |
|-----|--------|
| `↑` `↓` | Navigate menu |
| `←` `→` | Toggle options |
| `1-9` | Select device by number |
| `A-Z` | Select option by letter |
| `Enter` | Confirm |
| `ESC` | Back / Cancel / Abort |

---

## Troubleshooting

**Device not found?**
1. Ensure Network Debugging is enabled on TV
2. Try **Scan Network** or enter IP manually
3. Use **Restart ADB** to fix connection issues
4. Check TV screen for authorization prompt

**Device detected as wrong type?**
1. Use **Profile** to see detected device info
2. Detection uses brand, model, and installed packages
3. Unknown devices get a generic app list

**Launcher won't switch?**
1. The wizard auto-detects your stock launcher package
2. If detection fails, package name shown in Info line
3. Press Home button after disabling stock launcher
4. Some devices require selecting default in Android settings

**Command failed?**
* Errors show actual ADB output
* Check if app package exists on your device
* Some packages vary by device model/region

**Something broke?**
1. Use **Recovery** mode to re-enable all disabled packages
2. Use **Reboot** > **Recovery Mode** if device won't boot normally
3. Most changes can be reversed through Restore mode

---

## What's New in v62

* **Multi-device support** - Automatically detects Shield vs Google TV
* **Device-specific app lists** - Shield gets NVIDIA apps, Google TV gets Google/Walmart apps
* **Universal diagnostics** - Temperature/RAM/Storage work on all devices
* **Panic Recovery** - Emergency re-enable all disabled packages
* **Reboot Options** - Normal, Recovery, or Bootloader
* **Device Profile** - View detected device info before optimizing
* **Improved Help** - Updated for multi-device support

---

## Credits & Disclaimer

* **Debloat Research:** Community guides including [florisse.nl/shield-debloat](https://florisse.nl/shield-debloat/)
* **Disclaimer:** Use at your own risk. This tool prioritizes "Disable" over "Uninstall" for safety, but modifying system settings always carries some risk. Changes can be reversed using Restore mode or the new Recovery feature.
