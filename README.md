# Nvidia Shield Ultimate Optimizer

A powerful, self-contained PowerShell tool to debloat, optimize, and monitor Nvidia Shield TV devices. Designed for performance enthusiasts who want a snappier UI, more free RAM, and zero bloatware.

> **✅ Verified:** The "Golden Set" defaults have been personally tested for maximum stability on multiple Shield models including 2015, 2017, and 2019 (Pro & Tube).

## ⚠️ Requirements
1.  **Nvidia Shield TV** (Any model).
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

### 1. **"Golden Set" Optimization**
* **Tested Stability:** The script defaults to a specific "Golden Set" of optimizations. These have been personally tested by the author to provide maximum performance without breaking daily usability.
* **Extended Bloat List:** Includes a more aggressive list of removable items sourced from community guides (see credits). These default to **NO** to prevent accidental loss of features (like Chromecast or Voice Search).

### 2. **Auto-ADB Deployment**
* No need to install Android Platform Tools manually. The script handles the entire ADB setup process automatically, making it fully portable.

### 3. **Fleet Management**
* **Multi-Device Support:** Connect via USB or WiFi. The tool remembers your device IP for easy reconnection.
* **"Report All":** Run a health check on every Shield in your house in one go.

### 4. **Precision Reporting**
* **PSS Memory Analysis:** Accurately reports RAM usage (solving the common "over 100% usage" reporting bug).
* **Thermal HAL Access:** Reads the internal thermal service to get accurate CPU temps, even on newer Shield Experience versions where file access is restricted.
* **Storage & Swap Monitor:** Alerts you if your storage is filling up or if the system is thrashing the swap file.

---

## 🛡️ Disclaimers & Credits
* **Credits:** The "Golden Set" defaults are based on personal stress testing. The extended list of debloat targets was curated from the comprehensive research by [florisse.nl](https://florisse.nl/shield-debloat/).
* **Restore Function:** This script includes a "Restore" mode to re-enable stock apps. **Note:** This feature has not been fully tested in all scenarios. Use with caution.
* **Disclaimer:** Use at your own risk. Modifying system apps always carries a small risk. This tool is not affiliated with Nvidia or Google.
