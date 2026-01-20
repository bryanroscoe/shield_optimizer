# Shield Optimizer

A powerful, interactive PowerShell tool designed to debloat, optimize, and manage Nvidia Shield TV and other Android TV devices (Onn 4K, Chromecast, etc.). Features a colorful menu system with keyboard shortcuts, safe defaults, and a smart launcher wizard.

> **Note:** This tool is "vibe coded" - built with AI assistance. Tested and verified on real devices.

> **Verified:** Shield 2015 Pro, Shield 2019 Pro, Onn 4K

---

## Features

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
* **Device Report:** CPU temperature, RAM usage, swap status
* **Bloat Check:** Scan for active bloatware
* **Model Detection:** Identifies Shield 2015/2017/2019 Pro/Tube variants

### Connection Tools
* **Network Scan:** Auto-discover devices on local network (200ms timeout)
* **IP Validation:** Validates IP format before connecting
* **Restart ADB:** Fix connection issues with one click
* **Disconnect Device:** Cleanly disconnect without quitting

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
 Shield Optimizer v61 - Main Menu
 ================================================
  >  [1] Shield TV Pro
     [S]can Network
     [C]onnect IP
     [R]eport All
     re[F]resh
     restart [A]DB
     [H]elp
     [Q]uit
 ================================================
 Info: Model: Shield TV Pro (2019) | Serial: 192.168.1.50:5555
 [Arrows: Move] [Keys: Select] [Enter: OK] [ESC: Back]
```

### Action Menu (Per Device)
* **[O]ptimize** - Disable/uninstall bloatware, tune performance
* **[R]estore** - Re-enable apps, reset settings
* **Re[P]ort** - View device health and bloat status
* **[L]auncher Setup** - Install custom launcher, manage stock launcher
* **[D]isconnect** - Disconnect this device from ADB
* **[B]ack** - Return to main menu

### Optimize/Restore Flow
* Choose **Apply All Defaults** to skip individual prompts
* For each app: `DISABLE | UNINSTALL | SKIP | ABORT`
* **ABORT** shows partial summary of changes made
* Final summary shows counts: Disabled, Uninstalled, Skipped, Failed
* Option to reboot device when finished

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

**Launcher won't switch?**
1. The wizard auto-detects your stock launcher package
2. If detection fails, package name shown in Info line
3. Press Home button after disabling stock launcher
4. Some devices require selecting default in Android settings

**Command failed?**
* Errors now show actual ADB output
* Check if app package exists on your device
* Some packages vary by device model/region

---

## Credits & Disclaimer

* **Debloat Research:** Community guides including [florisse.nl/shield-debloat](https://florisse.nl/shield-debloat/)
* **Disclaimer:** Use at your own risk. This tool prioritizes "Disable" over "Uninstall" for safety, but modifying system settings always carries some risk. Changes can be reversed using Restore mode.
