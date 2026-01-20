# Nvidia Shield Ultimate Optimizer

A powerful, interactive PowerShell tool designed to debloat, optimize, and manage Nvidia Shield TV devices. It features a modern vertical menu system, safe "Golden Set" defaults, and a wizard for setting up custom launchers like Projectivy.

> **👨‍💻 A Note from the Author:**
> This tool is "vibe coded". I am a professional developer but mostly just vibed this with gemini in the background. I did my best to test it.

> **✅ Verified:** Tested on Shield 2015 Pro and 2019 Pro

---

## 🚀 Features

### 🎮 **1. Vertical Command Interface**
* **Arrow Key Navigation:** No more typing numbers. Navigate menus and options using your Up/Down arrow keys.
* **Smart Toggles:** Switch between actions (e.g., `[ DISABLE ]  UNINSTALL  SKIP`) using Left/Right arrows.
* **Dynamic Help:** Helpful descriptions update in real-time as you scroll through options.

### 🛡️ **2. "Golden Set" Optimization**
* **Smart Defaults:** Defaults to **Disabling** bloatware rather than uninstalling, making it safer and easier to reverse.
* **Granular Control:** You choose the action for every app.
* **Safe List:** Removes Telemetry, Sponsored Rows, and defunct Google apps while preserving core functionality.

### 🏠 **3. Launcher Wizard**
* **One-Click Setup:** Easily install **Projectivy**, **FLauncher**, or **Wolf Launcher**.
* **Stock Disabler:** Automatically handles the tricky process of disabling the stock Android TV launcher to make your custom launcher the permanent default.
* **Restore Stock:** A dedicated option to revert back to the original Shield interface instantly.

### ⚡ **4. Performance Tuning**
* **Animation Speed:** Instantly set UI animations to **0.5x** for a snappier feel.
* **Background Process Limiter:** strictly limit background processes (1, 2, 3, or 4) to free up RAM for gaming, or reset to Standard.

### 🩺 **5. Health & Restore**
* **Dashboard:** View CPU Temperature, RAM usage, and Swap thrashing.
* **Smart Restore:** The "Restore" mode detects if an app is Disabled or Missing. It attempts to re-enable it first, then tries to re-install from the system image, and finally offers to open the **Play Store** page if the file is gone.

---

## ⚠️ Requirements

1.  **Nvidia Shield TV** (Any model) or any android TV.
2.  **Enable Developer Options**:
    * Go to *Settings > Device Preferences > About*.
    * Scroll to **Build** and click it **7 times** until it says "You are a developer".
3.  **Enable USB Debugging**:
    * Go to *Settings > Device Preferences > Developer Options*.
    * Turn **ON** `USB Debugging`.
    * Turn **ON** `Network Debugging` (if connecting via WiFi).
4.  **PC with Windows**: PowerShell 5.1 or later (Pre-installed on Windows 10/11).

---

## 📥 How to Run

1.  **Download** the `Optimize-Shield.ps1` file.
2.  **Right-click** the file and select **Run with PowerShell**.
    * *Alternatively, open a terminal in the folder and run:*
    ```powershell
    Set-ExecutionPolicy Bypass -Scope Process -Force; .\Optimize-Shield.ps1
    ```
3.  **First Run:** The script will automatically download the necessary ADB tools from Google.
4.  **Authorize:** Look at your TV screen and select **"Allow"** when the debugging prompt appears.

---

## 📸 Menu Overview

<img width="526" height="282" alt="image" src="https://github.com/user-attachments/assets/5ea056d2-bf33-43a8-8432-0937c98b5d34" />

### **Main Menu**
* **Scan Network:** Auto-discovers Shields on your local network (ARP Scan).
* **Connect IP:** Manually enter an IP address.
* **Report All:** Runs a health check on every connected device in sequence.

<img width="511" height="191" alt="image" src="https://github.com/user-attachments/assets/85e86f2b-56e5-491c-8b15-d21d0ee596fd" />
### **Action Menu (Per Device)**
* **[1] Optimize:** Walk through the app list to Disable/Uninstall bloat.
* <img width="566" height="514" alt="image" src="https://github.com/user-attachments/assets/e57c4e66-8f47-4cd1-81e8-837eca04b7be" />

* **[2] Restore:** Undo changes. Re-enable stock apps and reset performance tweaks.
<img width="494" height="325" alt="image" src="https://github.com/user-attachments/assets/80b3cff0-5efa-4e3a-83ea-01de065ced69" />

* **[3] Report:** View device vitals (Temp/RAM) and check for active bloatware.
<img width="461" height="409" alt="image" src="https://github.com/user-attachments/assets/23bb29ef-07c3-4647-bc42-11ac9cf4ebf6" />

* **[4] Launcher Setup:** Install a custom launcher and set it as default.

---

## 📝 Credits & Disclaimer
* **Credits:** Debloat lists curated based on community research, including the extensive guide by [florisse.nl](https://florisse.nl/shield-debloat/).
* **Disclaimer:** Use at your own risk. While this tool prioritizes "Disabling" over "Uninstalling" for safety, modifying system settings always carries a small risk.
