# Nvidia Shield Ultimate Optimizer

A powerful, self-contained PowerShell tool to debloat, optimize, and monitor Nvidia Shield TV devices. Designed for performance enthusiasts who want a snappier UI, more free RAM, and zero bloatware.

## 🚀 Quick Start (Run from Command Line)
You can run this tool directly from PowerShell without downloading anything manually. It handles ADB setup automatically.

**1. On your PC, open PowerShell.**
**2. Paste and run this command:**

```powershell
Set-ExecutionPolicy Bypass -Scope Process -Force; [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('[https://raw.githubusercontent.com/bryanroscoe/shield_optimizer/main/Optimize-Shield.ps1](https://raw.githubusercontent.com/bryanroscoe/shield_optimizer/main/Optimize-Shield.ps1)'))
```

---

## ⚠️ Requirements
1.  **Nvidia Shield TV** (Any model: 2015, 2017, 2019 Pro/Tube).
2.  **USB Debugging Enabled**:
    * *Settings > Device Preferences > About > Build* (Click 7 times).
    * *Settings > Device Preferences > Developer Options > USB Debugging* (Turn ON).
    * *Network Debugging* (Turn ON if connecting via WiFi).
3.  **[CRITICAL] Custom Launcher Installed**:
    * You **MUST** install a third-party launcher (like **Projectivy Launcher**) before running this tool.
    * This script disables the stock Android TV launcher to save RAM. If no replacement is installed, you will boot to a black screen.

---

## 🛠 Features

### 1. **Auto-ADB Deployment**
* No need to install Android Platform Tools manually.
* The script checks for ADB. If missing, it downloads the latest version from Google, installs it to a portable temporary folder, and runs instantly.

### 2. **Three Operation Modes**
* **[1] OPTIMIZE:** Removes bloatware (Netflix, Google Assistant, Analytics), speeds up UI animations (0.5x), and cleans up background processes.
* **[2] RESTORE:** Made a mistake? Selling the device? This mode re-installs all removed apps, resets animations, and returns the device to stock factory configuration in seconds.
* **[3] REPORT:** Runs a non-destructive "Physical" on your device. Checks temperature, PSS Memory usage, Swap thrashing, and uptime.

### 3. **Intelligent "Self-Healing"**
* **Crash Proof:** Defaults to safety values if sensors or API calls fail.
* **Device Memory:** Remembers your device's IP address so you don't have to re-enter it on every run.
* **Portable:** Can be run from a USB stick on any Windows PC.

### 4. **Precision Reporting**
* Calculates **PSS (Proportional Set Size)** for accurate memory reporting (avoiding the "over 100% usage" bug common in standard tools).
* Scans multiple thermal zones to find the active temperature sensor.
* Waits for the OS to settle after reboots to provide accurate "Cold Boot" benchmarks.

---

## 📦 Manual Installation
If you prefer not to use the one-liner, you can run it manually:

1.  Download the `Optimize-Shield.ps1` file from this repository.
2.  Right-click the file > **Run with PowerShell**.
    * *Note: If restricted, run PowerShell as Administrator and execute `Set-ExecutionPolicy Bypass -Scope Process` first.*

---

## 🛡️ Disclaimers
* **Use at your own risk.** While the "Restore" mode is robust, modifying system apps always carries a small risk.
* This tool is not affiliated with Nvidia or Google.
