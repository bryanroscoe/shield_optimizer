# Nvidia Shield Ultimate Optimizer

A powerful, self-contained PowerShell tool to debloat, optimize, and monitor Nvidia Shield TV devices. Designed for performance enthusiasts who want a snappier UI, more free RAM, and zero bloatware.

> **Compatibility Note:** While this tool is specifically tailored for the **Nvidia Shield TV**, the core optimization and reporting features may work on other Android TV / Google TV devices. However, the "Debloat" lists are specific to the Shield's operating system. Use on other devices at your own risk.

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

## 🚀 How to Run

1.  **Download the Script**
    * Download the `Optimize-Shield.ps1` file from this repository.
    * Save it to a folder on your computer (e.g., your Desktop).

2.  **Run with PowerShell**
    * Open the folder where you saved the file.
    * Right-click empty space in the folder and select **"Open in Terminal"** or **"Open PowerShell window here"**.
    * Run the following command:

    ```powershell
    Set-ExecutionPolicy Bypass -Scope Process -Force; .\Optimize-Shield.ps1
    ```

    * *Note: If `adb.exe` is missing, the script will automatically download it from Google and place it in the same folder.*

---

## 🛠 Features

### 1. **Auto-ADB Deployment**
* No need to install Android Platform Tools manually.
* The script checks for ADB. If missing, it downloads the latest version from Google, installs it to a portable temporary folder inside the script directory, and runs instantly.

### 2. **Three Operation Modes**
* **[1] OPTIMIZE:** Removes bloatware (Netflix, Google Assistant, Analytics), speeds up UI animations (0.5x), and cleans up background processes.
* **[2] RESTORE:** Made a mistake? Selling the device? This mode re-installs all removed apps, resets animations, and returns the device to stock factory configuration in seconds.
* **[3] REPORT:** Runs a non-destructive "Physical" on your device. Checks temperature, PSS Memory usage, Swap thrashing, and uptime.

### 3. **Intelligent "Self-Healing"**
* **Crash Proof:** Defaults to safety values if sensors or API calls fail.
* **Connectivity:** Capable of connecting to devices via IP address (WiFi) or USB.
* **Portable:** Can be run from a USB stick on any Windows PC.

### 4. **Precision Reporting**
* Calculates **PSS (Proportional Set Size)** for accurate memory reporting (avoiding the "over 100% usage" bug common in standard tools).
* Scans multiple thermal zones to find the active temperature sensor.
* Waits for the OS to settle after reboots to provide accurate "Cold Boot" benchmarks.

---

## 🛡️ Disclaimers
* **Use at your own risk.** While the "Restore" mode is robust, modifying system apps always carries a small risk.
* This tool is not affiliated with Nvidia or Google.
