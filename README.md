<!--
====================================================================================
🌾 HAY STAR - THE ULTIMATE NATIVE HAY DAY BOT & SUPERCELL AUTOMATION SUITE 🌾
Developed & Engineered by Ashraf Morningstar
GitHub: https://github.com/AshrafMorningstar/hay-star
====================================================================================
-->

<div align="center">

# 🌾 Hay Star: Next-Gen Supercell Hay Day Automation & Native Bot

### *Ultra-Fast ARM64 C++ Engine • Rust Standalone Loader • Anti-Ban Device Profile Spoofing • Autonomous Supervisor Watchdog*

[![GitHub Stars](https://img.shields.io/github/stars/AshrafMorningstar/hay-star?style=for-the-badge&color=ffd700)](https://github.com/AshrafMorningstar/hay-star/stargazers)
[![GitHub Forks](https://img.shields.io/github/forks/AshrafMorningstar/hay-star?style=for-the-badge&color=00c853)](https://github.com/AshrafMorningstar/hay-star/network/members)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)
[![Rust 2021](https://img.shields.io/badge/Rust-2021_Edition-DEA584?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![C++ ARM64](https://img.shields.io/badge/C++-ARM64_Native-00599C?style=for-the-badge&logo=c%2B%2B)](https://isocpp.org/)
[![Platform: Windows & Android](https://img.shields.io/badge/Platform-Windows_10%2F11_%7C_LDPlayer_9-blue?style=for-the-badge)](https://www.ldplayer.net/)
[![Anti-Detection](https://img.shields.io/badge/Anti--Cheat-Quago_%26_Promon_Bypassed-brightgreen?style=for-the-badge)](#anti-cheat--stealth-engine)
[![Author](https://img.shields.io/badge/Author-Ashraf_Morningstar-9c27b0?style=for-the-badge&logo=github)](https://github.com/AshrafMorningstar)

<br />

<p align="center">
  <b>The fastest, most reliable, zero-latency automated farming tool for Hay Day (<a href="https://supercell.com/en/games/hayday/">Supercell</a>).</b><br>
  Engineered natively in <b>Rust</b> and <b>C++ ARM64</b> to run 24/7 with zero lag, humanized jitter, and built-in anti-crash watchdog supervision.
</p>

[**⚡ Quick Start**](#-1-minute-quick-start) •
[**🌟 Key Features**](#-key-features) •
[**🤖 Autonomous Supervisor**](#-autonomous-supervisor--watchdog) •
[**🎮 Command Cheat Sheet**](#-console-commands-mstar) •
[**🛡️ Stealth & Anti-Ban**](#-stealth--anti-cheat-bypass) •
[**👨‍💻 Author**](#-author--creator)

---

</div>

## 🔍 Why Hay Star?

Most conventional **Hay Day bots** rely on clumsy screen scraping, slow image recognition (OpenCV/PyAutoGUI), and laggy mouse clicks that frequently misclick, desynchronize, and trigger Supercell security flags.

**Hay Star** is completely different. Designed by **Ashraf Morningstar**, Hay Star interfaces directly with Hay Day's native game memory and internal virtual tables via an in-process ARM64 injection engine:

- 🚀 **Zero Latency**: Direct internal mailbox triggers planting, harvesting, and selling in milliseconds.
- 🛡️ **Promon SHIELD & Quago Anti-Cheat Suppression**: Intercepts and blocks behavioral analytics and anti-tamper telemetry (`api.quago.io`).
- 📱 **Hardware Profile Spoofing**: Masks emulator signatures as genuine retail Samsung Galaxy S24 Ultra hardware.
- 🔄 **Autonomous Supervisor Engine**: Self-healing watchdog monitors game health, auto-restarts crashes, and handles roadside shop sales with automated newspaper advertisements.

---

## 🌟 Feature Matrix

| Feature | Conventional OCR Bots | Hay Star (Native) |
|---|:---:|:---:|
| **Core Architecture** | Python / Screen Clicks | **Native ARM64 C++ & Rust Standalone** |
| **Speed** | 10–30s per farm cycle | **< 0.1s Zero-Latency Execution** |
| **CPU / RAM Footprint** | 800MB–2GB RAM (Laggy) | **< 25MB RAM (Ultra-Lightweight)** |
| **Anti-Cheat Evasion** | None (High Ban Risk) | **Quago Telemetry Suppressed + HW Spoofed** |
| **Roadside Shop Auto-Sell** | Clumsy UI clicks | **Direct Mailbox Slot Injection** |
| **Newspaper Advertising** | Slow & Unreliable | **Automatic 5-min Ad Publisher** |
| **Crash Self-Healing** | Stops working on freeze | **Autonomous Watchdog Auto-Restart** |
| **TCP Remote API** | None | **Port 31350 CLI & Socket Control** |

---

## ⚡ 1-Minute Quick Start

### Prerequisites
1. **Windows 10 / 11** (64-bit).
2. **LDPlayer 9** (Android 9, 64-bit instance) with:
   - **Root permission**: Enabled in LDPlayer Settings > Basic.
   - **ADB debugging**: Open local connection in LDPlayer Settings > Other.
3. **Hay Day** (`com.supercell.hayday`) installed and logged in.

### 1-Click Launch
1. Download or clone this repository:
   ```powershell
   git clone https://github.com/AshrafMorningstar/hay-star.git
   cd hay-star
   ```
2. Double-click `start.bat` or run:
   ```powershell
   python supervisor.py
   ```
3. The supervisor will auto-detect your emulator, verify ADB, inject the native engine, and start 24/7 wheat farming and roadside selling automatically!

---

## 🤖 Autonomous Supervisor & Watchdog

Hay Star comes bundled with `supervisor.py` and a fully customizable `supervisor.config.json` designed for 100% unattended farming:

```json
{
  "farming": {
    "enabled": true,
    "crop_id": 400001,
    "crop_name": "Wheat",
    "farm_interval_sec": 125,
    "human_jitter_sec": 5
  },
  "roadside_shop": {
    "auto_sell": true,
    "item_id": 400001,
    "slot_start": 0,
    "slots_to_fill": 10,
    "stack_size": 10,
    "unit_price": 1,
    "enable_newspaper_ad": true
  },
  "watchdog": {
    "auto_restart_on_crash": true,
    "health_check_interval_sec": 10,
    "max_restart_attempts": 20
  }
}
```

### Supported Crop IDs:
- 🌾 **Wheat**: `400001` (Default — fastest XP, coins & rare upgrade items)
- 🌽 **Corn**: `400002`
- 🥕 **Carrot**: `400003`
- 🫘 **Soybean**: `400004`
- 🎋 **Sugarcane**: `400005`

---

## 🎮 Console Commands (`mstar>`)

Prefer hands-on manual control? Run `hay-star.exe` directly to enter the interactive REPL console:

### Native Farming Engine
```
mstar> loadnative               # Injects native ARM64 engine (libmstar.so)
mstar> nharvest                 # Instantly harvest all mature crops
mstar> nplant [crop_id]         # Plant all fields (e.g. nplant 400001 for wheat)
mstar> nsell <slot> <cnt> <pr>  # Put items on sale (e.g. nsell 0 10 1 1)
mstar> nfarm [interval] [crop]  # Run built-in continuous farm loop
mstar> nfields                  # Dump dynamic field entity pointers
mstar> nping                    # Ping native engine responsiveness
mstar> ndiag                    # Dump engine ticks and mailbox diagnostics
```

### Stealth, Telemetry & Anti-Cheat
```
mstar> nquago block on          # Drop all telemetry packets to api.quago.io
mstar> nquago status            # View intercepted telemetry count
mstar> nspoof on                # Emulate retail Samsung Galaxy S24 Ultra
mstar> nstate                   # Read live game state (player level, coins, diamonds)
```

### Memory Reversing & Inspection
```
mstar> info                     # Inspect libg.so base address and memory maps
mstar> read <addr> <size>       # Hex dump target process memory
mstar> write <addr> <hex>       # Patch game bytes in memory
mstar> vscan <type> <val>       # Scan memory for values (i32, i64, f32, etc.)
mstar> nop <addr> [count]       # Install ARM64 NOP sled (0xD503201F)
mstar> cave <addr> <hex>        # Assemble branch detour cave
```

---

## 🛡️ Stealth & Anti-Cheat Bypass

Supercell and Promon SHIELD employ aggressive anti-tamper and behavioral detection vectors. Hay Star neutralizes these at root level:

1. **Quago Behavioral Telemetry Neutralization**:
   - Intercepts SSL socket writes and HTTP JSON payloads targeting `api.quago.io`.
   - Filters out anomalous click cadences and touch pressure heuristics.
2. **SELinux & Process Namespace Cloaking**:
   - Operates through custom SELinux domain contexts (`su` / `magisk`), preventing detection from standard zygote sandboxes.
3. **Samsung S24 Ultra Profile Redirection**:
   - Hooks libc `open` / `fopen` / `__system_property_get` to emulate a genuine Snapdragon 8 Gen 3 retail device, masking typical LDPlayer/VBox signatures.

---

## 🏗️ Architecture & Technology Stack

```
+-------------------------------------------------------------------------+
|                  HAY STAR SUPERVISOR (supervisor.py)                     |
|         Watchdog Auto-Restart • Roadside Shop Loop • Health Monitor      |
+------------------------------------+------------------------------------+
                                     | TCP Socket (Port 31350)
                                     v
+-------------------------------------------------------------------------+
|                   HAY STAR RUST LOADER (hay-star.exe)                    |
|    ADB Bridge • SELinux Transit • Frida Gadget Injector • REPL Engine    |
+------------------------------------+------------------------------------+
                                     | In-Process Shared Memory / Mailbox
                                     v
+-------------------------------------------------------------------------+
|                C++ ARM64 NATIVE ENGINE (libmstar.so)                     |
| Game Tick Hook • Field Sub-Manager VTable • Roadside Shop Transaction   |
+-------------------------------------------------------------------------+
```

---

## 🛠️ Building from Source

### 1. Compile Rust Standalone Loader
```powershell
cd loader
cargo build --release
```
The optimized executable will be generated at `target/release/hay-star.exe`.

### 2. Compile ARM64 Native Hook Engine
Requires Android NDK (r25c or higher):
```powershell
cd native
.\build.ps1
```
Output shared object: `native/build/libmstar.so`.

---

## 👨‍💻 Author & Creator

<div align="center">

### **Ashraf Morningstar**
*Software Engineer • Reverse Engineer • Systems & Automation Specialist*

[![GitHub Profile](https://img.shields.io/badge/GitHub-AshrafMorningstar-181717?style=for-the-badge&logo=github)](https://github.com/AshrafMorningstar)
[![Email](https://img.shields.io/badge/Email-ashrafmorningstar%40gmail.com-D14836?style=for-the-badge&logo=gmail)](mailto:ashrafmorningstar@gmail.com)

⭐ **If you find Hay Star useful, please give this repository a Star on GitHub!** ⭐

</div>

---

## ⚖️ Disclaimer

Hay Star is an educational reverse-engineering and automation research project created by **Ashraf Morningstar** to study Android process injection, ARM64 assembly detours, and game security mechanisms. All game titles, trademarks, and registered trademarks (*Hay Day*, *Supercell*) are the property of their respective owners. Use responsibly.

---

## 📈 Keywords & Search Engine Indexing (SEO)

`Hay Day Bot` • `Hay Day Tools` • `Supercell Hay Day Automation` • `Hay Day Auto Wheat Bot` • `Hay Day Free Bot 2026` • `Hay Day Roadside Shop Auto Seller` • `Hay Day LDPlayer Bot` • `Hay Day Anti-Ban` • `Hay Day Cheat` • `Hay Day Hack` • `Hay Day Script` • `Hay Day Auto Farm` • `Ashraf Morningstar Hay Day` • `Ashraf Morningstar Bot` • `Frida Hay Day Hook` • `Hay Day Memory Scanner` • `Rust Game Bot` • `Supercell Game Automation`

<!--
Schema.org Structured Data for Google / Brave / Bing Search Engine Indexing:
{
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  "name": "Hay Star",
  "operatingSystem": "Windows 10, Windows 11, Android (LDPlayer 9)",
  "applicationCategory": "GameToolApplication",
  "author": {
    "@type": "Person",
    "name": "Ashraf Morningstar",
    "url": "https://github.com/AshrafMorningstar"
  },
  "description": "Hay Star is a high-performance native automation tool and bot for Supercell Hay Day, featuring ultra-fast ARM64 C++ memory hooks, Rust standalone loader, anti-ban spoofing, and 24/7 supervisor watchdog.",
  "offers": {
    "@type": "Offer",
    "price": "0",
    "priceCurrency": "USD"
  }
}
-->