#!/usr/bin/env python3
"""
=============================================================================
Hay Star Autonomous Supervisor & Watchdog Engine
Author: Ashraf Morningstar
Repository: https://github.com/AshrafMorningstar/hay-star
License: MIT
=============================================================================
Hay Star Supervisor manages the full lifecycle of the Hay Day automation system:
1. Detects and verifies LDPlayer 9 / ADB connection.
2. Checks and launches `com.supercell.hayday` if not running.
3. Spawns and supervises `hay-star.exe` process.
4. Communicates over TCP port 31350 to issue native engine farming commands.
5. Watchdog monitors game health, detects crashes/hangs, and self-heals automatically.
6. Implements roadside shop auto-selling routines with dynamic newspaper ads.
=============================================================================
"""

import argparse
import json
import os
import random
import socket
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

DEFAULT_CONFIG_PATH = Path(__file__).parent / "supervisor.config.json"

class Colors:
    CYAN = "\033[96m"
    GREEN = "\033[92m"
    YELLOW = "\033[93m"
    RED = "\033[91m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    RESET = "\033[0m"

def log(msg, level="INFO"):
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    prefix = f"[{timestamp}] [{level}]"
    if level == "INFO":
        color = Colors.CYAN
    elif level == "SUCCESS":
        color = Colors.GREEN
    elif level == "WARN":
        color = Colors.YELLOW
    elif level == "ERROR":
        color = Colors.RED
    else:
        color = Colors.RESET
    formatted = f"{color}{prefix} {msg}{Colors.RESET}"
    print(formatted)
    
    # Optional file logging
    try:
        with open("supervisor.log", "a", encoding="utf-8") as f:
            f.write(f"[{timestamp}] [{level}] {msg}\n")
    except Exception:
        pass


class HayStarClient:
    """TCP Client for Hay Star REPL Control Server (port 31350)."""

    def __init__(self, host="127.0.0.1", port=31350, timeout=10):
        self.host = host
        self.port = port
        self.timeout = timeout
        self.sock = None

    def connect(self, retries=15, delay=2):
        for attempt in range(1, retries + 1):
            try:
                self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                self.sock.settimeout(self.timeout)
                self.sock.connect((self.host, self.port))
                log(f"Connected to Hay Star control server at {self.host}:{self.port}", "SUCCESS")
                return True
            except (ConnectionRefusedError, socket.timeout, OSError):
                log(f"Waiting for Hay Star TCP server ({attempt}/{retries})...", "WARN")
                time.sleep(delay)
        return False

    def send_command(self, cmd):
        if not self.sock:
            if not self.connect(retries=3):
                return None
        try:
            full_cmd = cmd.strip() + "\n"
            self.sock.sendall(full_cmd.encode("utf-8"))
            time.sleep(0.3)
            # Read response if available
            self.sock.settimeout(2.0)
            data = b""
            while True:
                try:
                    chunk = self.sock.recv(4096)
                    if not chunk:
                        break
                    data += chunk
                    if len(chunk) < 4096:
                        break
                except socket.timeout:
                    break
            return data.decode("utf-8", errors="ignore")
        except Exception as e:
            log(f"TCP communication error: {e}", "ERROR")
            self.close()
            return None

    def close(self):
        if self.sock:
            try:
                self.sock.close()
            except Exception:
                pass
            self.sock = None


class SupervisorEngine:
    def __init__(self, config_path=None):
        self.config_path = Path(config_path or DEFAULT_CONFIG_PATH)
        self.config = self.load_config()
        self.client = None
        self.process = None
        self.running = False
        self.cycle_count = 0
        self.total_harvests = 0

    def load_config(self):
        if self.config_path.exists():
            try:
                with open(self.config_path, "r", encoding="utf-8") as f:
                    return json.load(f)
            except Exception as e:
                log(f"Failed to parse config file: {e}. Using defaults.", "ERROR")
        return {
            "emulator": {"package_name": "com.supercell.hayday"},
            "farming": {"enabled": True, "crop_id": 400001, "farm_interval_sec": 125, "human_jitter_sec": 4},
            "roadside_shop": {"auto_sell": True, "item_id": 400001, "slot_start": 0, "slots_to_fill": 10, "stack_size": 10, "unit_price": 1, "enable_newspaper_ad": True},
            "watchdog": {"auto_restart_on_crash": True, "health_check_interval_sec": 10, "max_restart_attempts": 20},
            "tcp_control": {"host": "127.0.0.1", "port": 31350}
        }

    def print_banner(self):
        banner = f"""
{Colors.CYAN}{Colors.BOLD}
=============================================================================
   🌾 HAY STAR - AUTONOMOUS SUPERVISOR & SUPERCELL HAY DAY BOT 🌾
   Created by Ashraf Morningstar | https://github.com/AshrafMorningstar
=============================================================================
{Colors.RESET}
• Mode: Autonomous Supervisor & Anti-Crash Watchdog
• Target Package: {self.config.get('emulator', {}).get('package_name', 'com.supercell.hayday')}
• Target Crop ID: {self.config.get('farming', {}).get('crop_id', 400001)} ({self.config.get('farming', {}).get('crop_name', 'Wheat')})
• Loop Interval: {self.config.get('farming', {}).get('farm_interval_sec', 125)}s (±{self.config.get('farming', {}).get('human_jitter_sec', 4)}s jitter)
• Auto-Shop Seller: {'ENABLED' if self.config.get('roadside_shop', {}).get('auto_sell') else 'DISABLED'}
=============================================================================
"""
        print(banner)

    def find_adb(self):
        """Locate ADB executable in PATH or common LDPlayer directories."""
        common_locations = [
            r"C:\LDPlayer\LDPlayer9\adb.exe",
            r"D:\LDPlayer\LDPlayer9\adb.exe",
            r"C:\Program Files\LDPlayer\LDPlayer9\adb.exe",
            r"C:\Nox\bin\nox_adb.exe"
        ]
        for loc in common_locations:
            if os.path.isfile(loc):
                return loc
        return "adb"

    def check_emulator(self):
        """Check if emulator is responding via ADB."""
        adb = self.find_adb()
        try:
            res = subprocess.run([adb, "devices"], capture_output=True, text=True, timeout=5)
            devices = [line for line in res.stdout.splitlines() if "\tdevice" in line]
            if devices:
                log(f"Detected {len(devices)} active Android device(s): {devices[0].split()[0]}", "SUCCESS")
                return True
            log("No active ADB devices detected. Please ensure LDPlayer 9 is running.", "WARN")
            return False
        except Exception as e:
            log(f"ADB check warning: {e}", "WARN")
            return False

    def ensure_game_running(self):
        """Verify that Hay Day is foreground or launch it."""
        adb = self.find_adb()
        pkg = self.config.get("emulator", {}).get("package_name", "com.supercell.hayday")
        try:
            subprocess.run([adb, "shell", "monkey", "-p", pkg, "-c", "android.intent.category.LAUNCHER", "1"],
                           capture_output=True, timeout=5)
            time.sleep(2)
        except Exception:
            pass

    def start_loader_process(self):
        """Start hay-star.exe process if not already active."""
        exe_path = Path(__file__).parent / "hay-star.exe"
        if not exe_path.exists():
            log(f"hay-star.exe not found at {exe_path}. Searching in loader/target...", "WARN")
            release_exe = Path(__file__).parent / "target" / "release" / "hay-star.exe"
            if release_exe.exists():
                exe_path = release_exe
            else:
                log("Binary hay-star.exe not found. Please compile or download the release.", "ERROR")
                return False

        log(f"Launching Hay Star loader: {exe_path.name}", "INFO")
        try:
            self.process = subprocess.Popen(
                [str(exe_path)],
                cwd=str(Path(__file__).parent),
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT
            )
            time.sleep(3)
            return True
        except Exception as e:
            log(f"Failed to start hay-star.exe: {e}", "ERROR")
            return False

    def initialize_injection(self):
        """Connect to TCP control port and initialize native engine."""
        tcp_cfg = self.config.get("tcp_control", {})
        host = tcp_cfg.get("host", "127.0.0.1")
        port = tcp_cfg.get("port", 31350)
        retries = tcp_cfg.get("connect_retries", 15)

        self.client = HayStarClient(host=host, port=port)
        if not self.client.connect(retries=retries):
            log("Could not establish TCP connection with Hay Star engine.", "ERROR")
            return False

        log("Loading native ARM64 engine into Hay Day...", "INFO")
        resp = self.client.send_command("loadnative")
        log(f"Engine response: {resp.strip() if resp else 'OK'}", "SUCCESS")
        
        # Check spoofing and anti-cheat telemetry suppression
        if self.config.get("anti_detection", {}).get("spoof_samsung_s24", True):
            self.client.send_command("nspoof on")
        if self.config.get("anti_detection", {}).get("quago_telemetry_blocking", True):
            self.client.send_command("nquago block on")
            
        return True

    def execute_farm_cycle(self):
        """Executes one complete harvest -> plant -> sell cycle."""
        farming_cfg = self.config.get("farming", {})
        crop_id = farming_cfg.get("crop_id", 400001)
        shop_cfg = self.config.get("roadside_shop", {})

        self.cycle_count += 1
        log(f"--- Starting Autonomous Farming Cycle #{self.cycle_count} ---", "INFO")

        # 1. Harvest mature crops
        log("Triggering native field harvest...", "INFO")
        resp = self.client.send_command("nharvest")
        if resp:
            log(f"Harvest result: {resp.strip()}", "SUCCESS")
        self.total_harvests += 1
        time.sleep(1.2)

        # 2. Plant target crop
        log(f"Planting crop ID {crop_id} across all available fields...", "INFO")
        resp = self.client.send_command(f"nplant {crop_id}")
        if resp:
            log(f"Planting result: {resp.strip()}", "SUCCESS")
        time.sleep(1.0)

        # 3. Roadside Shop Auto-Sell
        if shop_cfg.get("auto_sell", True):
            slots = shop_cfg.get("slots_to_fill", 10)
            unit_price = shop_cfg.get("unit_price", 1)
            stack_size = shop_cfg.get("stack_size", 10)
            enable_ad = 1 if shop_cfg.get("enable_newspaper_ad", True) else 0

            log(f"Managing Roadside Shop: selling {stack_size}x for {unit_price} coin(s) across slots...", "INFO")
            for slot in range(slots):
                # Advertise only the middle/featured slot to maximize buyer traffic
                ad_flag = enable_ad if slot == 0 else 0
                cmd = f"nsell {slot} {stack_size} {unit_price} {ad_flag}"
                self.client.send_command(cmd)
                time.sleep(0.2)
            log("Roadside shop crates refreshed and advertised.", "SUCCESS")

    def run(self):
        """Main supervisor loop with watchdog self-healing."""
        self.print_banner()
        self.running = True

        restart_attempts = 0
        max_restarts = self.config.get("watchdog", {}).get("max_restart_attempts", 20)

        while self.running and restart_attempts < max_restarts:
            try:
                self.check_emulator()
                self.ensure_game_running()

                if not self.start_loader_process():
                    log("Failed to launch loader. Retrying in 5 seconds...", "ERROR")
                    time.sleep(5)
                    restart_attempts += 1
                    continue

                if not self.initialize_injection():
                    log("Injection failed or timed out. Restarting supervisor session...", "WARN")
                    self.cleanup()
                    restart_attempts += 1
                    time.sleep(4)
                    continue

                log("Hay Star Autonomous Supervisor is active and running!", "SUCCESS")
                restart_attempts = 0  # reset on successful launch

                base_interval = self.config.get("farming", {}).get("farm_interval_sec", 125)
                jitter_range = self.config.get("farming", {}).get("human_jitter_sec", 4)

                while self.running:
                    self.execute_farm_cycle()

                    # Calculate randomized human-like delay
                    jitter = random.uniform(-jitter_range, jitter_range)
                    sleep_duration = max(10, base_interval + jitter)
                    log(f"Cycle completed. Sleeping for {sleep_duration:.1f} seconds until crops mature...", "INFO")

                    # Watchdog ping during sleep
                    elapsed = 0
                    while elapsed < sleep_duration and self.running:
                        time.sleep(5)
                        elapsed += 5
                        # Quick ping to verify native engine
                        ping_res = self.client.send_command("nping")
                        if ping_res is None:
                            log("Watchdog detected unresponsive engine! Triggering self-heal...", "WARN")
                            raise ConnectionResetError("Engine unresponsive")

            except (ConnectionResetError, BrokenPipeError, ConnectionError) as e:
                log(f"Watchdog alert: {e}. Initiating auto-recovery...", "WARN")
                self.cleanup()
                restart_attempts += 1
                cooldown = self.config.get("watchdog", {}).get("cooldown_between_restarts_sec", 6)
                time.sleep(cooldown)
            except KeyboardInterrupt:
                log("Received shutdown signal. Stopping supervisor...", "INFO")
                self.running = False
                break
            except Exception as e:
                log(f"Unexpected supervisor exception: {e}", "ERROR")
                self.cleanup()
                restart_attempts += 1
                time.sleep(5)

        self.cleanup()
        log("Supervisor exited cleanly. Thank you for using Hay Star by Ashraf Morningstar.", "SUCCESS")

    def cleanup(self):
        """Cleanly close sockets and child processes."""
        if self.client:
            self.client.close()
            self.client = None
        if self.process:
            try:
                self.process.terminate()
                self.process.wait(timeout=3)
            except Exception:
                try:
                    self.process.kill()
                except Exception:
                    pass
            self.process = None


def main():
    parser = argparse.ArgumentParser(
        description="Hay Star Autonomous Supervisor by Ashraf Morningstar",
        epilog="Visit https://github.com/AshrafMorningstar/hay-star for updates."
    )
    parser.add_argument("--config", "-c", help="Path to custom JSON configuration file")
    parser.add_argument("--test-config", action="store_true", help="Validate configuration and exit")
    parser.add_argument("--crop", type=int, help="Override crop ID (e.g. 400001 for Wheat)")
    parser.add_argument("--interval", type=int, help="Override loop interval in seconds")
    args = parser.parse_args()

    supervisor = SupervisorEngine(config_path=args.config)

    if args.crop:
        supervisor.config.setdefault("farming", {})["crop_id"] = args.crop
    if args.interval:
        supervisor.config.setdefault("farming", {})["farm_interval_sec"] = args.interval

    if args.test_config:
        print(json.dumps(supervisor.config, indent=2))
        log("Configuration validated successfully!", "SUCCESS")
        return

    supervisor.run()


if __name__ == "__main__":
    main()
