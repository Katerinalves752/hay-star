"""
Hay Star - Frida Injection & RPC Bridge
Connects to Frida injector on Android, spawns Hay Day, injects Gadget,
loads hook.js / java_guard / quago_probe, and provides a simple JSON-RPC
pipe on stdin/stdout for the Rust orchestrator.
"""

import sys
import os
import time
import json
import threading
import argparse
import subprocess
import shlex
import re
from pathlib import Path

HELPER_RESIDUE_PATTERN = re.compile(
    r"^/data/local/tmp/(?:frida-[0-9a-fA-F]{32}|"
    r"frida-helper-[A-Za-z0-9][A-Za-z0-9._-]*|"
    r"\.frida-[A-Za-z0-9][A-Za-z0-9._-]*)$"
)
INJECTOR_OAT_DIR = "/data/local/tmp/oat"
SERVER_LOG = "/data/local/tmp/.mstar-server.log"

def log(msg):
    sys.stderr.write(f"[*] [bridge] {msg}\n")
    sys.stderr.flush()

def adb_cmd(adb, device_id, *args, timeout=10):
    if not adb or not device_id:
        return None
    try:
        return subprocess.run(
            [adb, "-s", device_id] + list(args),
            capture_output=True, text=True, timeout=timeout
        )
    except Exception:
        return None

def su_cmd(adb, device_id, cmd_str, timeout=10):
    args = ("shell", f"su -c {shlex.quote(cmd_str)}")
    return adb_cmd(adb, device_id, *args, timeout=timeout)

def process_exe(adb, device_id, pid):
    res = su_cmd(adb, device_id, f"readlink /proc/{int(pid)}/exe 2>/dev/null")
    if res is None or res.returncode != 0:
        return None
    return res.stdout.strip().removesuffix(" (deleted)") or None

def remote_realpath(adb, device_id, path):
    res = su_cmd(adb, device_id, f"readlink -f {shlex.quote(path)}")
    if res is None or res.returncode != 0:
        return path
    return res.stdout.strip() or path

def matching_server_pids(adb, device_id, server_exe):
    if not adb or not device_id or not server_exe:
        return set()
    process_name = server_exe.rsplit("/", 1)[-1]
    res = adb_cmd(adb, device_id, "shell", f"pidof {process_name}")
    if res is None or res.returncode not in (0, 1):
        return set()
    real_exe = remote_realpath(adb, device_id, server_exe)
    matches = set()
    for val in res.stdout.split():
        try:
            pid = int(val)
        except ValueError:
            continue
        exe = process_exe(adb, device_id, pid)
        if exe and (exe == server_exe or exe == real_exe):
            matches.add(pid)
    return matches

def stop_server_pids(adb, device_id, server_exe, pids, force=True):
    if not adb or not device_id or not server_exe or not pids:
        return
    real_exe = remote_realpath(adb, device_id, server_exe)
    owned = {pid for pid in pids if process_exe(adb, device_id, pid) in (server_exe, real_exe)}
    for pid in sorted(owned):
        sig = "kill -9" if force else "kill"
        su_cmd(adb, device_id, f"{sig} {pid}")
    deadline = time.monotonic() + 2.0
    while owned and time.monotonic() < deadline:
        owned = {pid for pid in owned if process_exe(adb, device_id, pid) in (server_exe, real_exe)}
        if owned:
            time.sleep(0.05)
    for pid in sorted(owned):
        su_cmd(adb, device_id, f"kill -9 {pid}")

def list_helper_residues(adb, device_id):
    if not adb or not device_id:
        return set()
    res = su_cmd(adb, device_id, "find /data/local/tmp -mindepth 1 -maxdepth 1 -print 2>/dev/null")
    if res is None or res.returncode != 0:
        return set()
    return {
        line.strip().rstrip("/")
        for line in res.stdout.splitlines()
        if HELPER_RESIDUE_PATTERN.fullmatch(line.strip().rstrip("/"))
    }

def release_injector(adb, device_id, server_exe, manager, frida_port, residues_before):
    # 1. Kill injector server immediately with SIGKILL on Android before dropping connection
    if adb and device_id and server_exe:
        pids = matching_server_pids(adb, device_id, server_exe)
        if pids:
            log(f"Terminating injector server PID(s): {', '.join(map(str, sorted(pids)))}...")
            stop_server_pids(adb, device_id, server_exe, pids, force=True)

    # 2. Remove remote device from Frida device manager
    if manager is not None:
        try:
            manager.remove_remote_device(f"127.0.0.1:{frida_port}")
        except Exception:
            pass

    # 3. Remove ADB port forward and clean residues
    if adb and device_id:
        try:
            adb_cmd(adb, device_id, "forward", "--remove", f"tcp:{frida_port}")
        except Exception:
            pass
        su_cmd(adb, device_id, f"rm -f {shlex.quote(SERVER_LOG)}")
        current = list_helper_residues(adb, device_id)
        if residues_before is not None:
            for path in (current - residues_before):
                su_cmd(adb, device_id, f"rm -rf {shlex.quote(path)}")
        su_cmd(adb, device_id, f"rm -rf {shlex.quote(INJECTOR_OAT_DIR)}")
    log("Injector server released cleanly.")

def main():
    parser = argparse.ArgumentParser(description="Hay Star Frida Bridge")
    parser.add_argument("--frida-port", type=int, default=31337)
    parser.add_argument("--gadget-port", type=int, default=31338)
    parser.add_argument("--project-dir", type=str, default=".")
    parser.add_argument("--package-name", type=str, default="com.supercell.hayday")
    parser.add_argument("--gadget-bin", type=str, default="/data/user/0/com.supercell.hayday/files/metrics/libmetrics.so")
    parser.add_argument("--adb", type=str, default="")
    parser.add_argument("--device-id", type=str, default="")
    parser.add_argument("--server-exe", type=str, default="")
    args = parser.parse_args()

    project_dir = Path(args.project_dir).resolve()
    import frida

    manager = frida.get_device_manager()
    residues_before = list_helper_residues(args.adb, args.device_id)

    # 1. Connect to Injector Server
    log(f"Connecting to injector server on 127.0.0.1:{args.frida_port}...")
    injector_addr = f"127.0.0.1:{args.frida_port}"
    injector = None
    deadline = time.monotonic() + 10.0
    while time.monotonic() < deadline:
        try:
            injector = manager.add_remote_device(injector_addr)
            _ = injector.enumerate_processes()
            break
        except Exception:
            time.sleep(0.25)
    if injector is None:
        raise RuntimeError(f"Could not connect to injector at {injector_addr}")
    log("Injector connected.")

    # 2. Spawn Game (suspended)
    log(f"Spawning {args.package_name} (suspended)...")
    pid = injector.spawn(args.package_name)
    log(f"Spawned PID: {pid}")

    # 3. Inject Gadget
    log(f"Injecting Gadget: {args.gadget_bin}...")
    injector.inject_library_file(pid, args.gadget_bin, "pthread_exit", "")
    log("Gadget injected.")

    # 4. Connect to Gadget
    gadget_addr = f"127.0.0.1:{args.gadget_port}"
    log(f"Connecting to Gadget on {gadget_addr}...")
    gadget_dev = None
    session = None
    deadline = time.monotonic() + 10.0
    while time.monotonic() < deadline:
        try:
            gadget_dev = manager.add_remote_device(gadget_addr)
            session = gadget_dev.attach("Gadget")
            break
        except Exception:
            time.sleep(0.2)
    if session is None:
        raise RuntimeError("Failed to attach to Gadget")
    log("Attached to Gadget.")

    detached_event = threading.Event()
    detach_reason = [""]
    def on_detached(reason, *a, **kw):
        detach_reason[0] = str(reason)
        sys.stderr.write(f"[!] [bridge] Gadget session detached: {reason}\n")
        sys.stderr.flush()
        detached_event.set()
    session.on("detached", on_detached)

    def on_message(message, data):
        mtype = message.get("type")
        if mtype == "send":
            sys.stderr.write(f"  {message.get('payload')}\n")
            sys.stderr.flush()
        elif mtype == "error":
            detail = message.get("stack") or message.get("description") or str(message)
            sys.stderr.write(f"  [HOOK ERROR] {detail}\n")
            sys.stderr.flush()

    def on_guard_message(message, data):
        mtype = message.get("type")
        if mtype == "send":
            sys.stderr.write(f"  {message.get('payload')}\n")
            sys.stderr.flush()
        elif mtype == "error":
            detail = message.get("stack") or message.get("description") or str(message)
            sys.stderr.write(f"  [JAVA-GUARD ERROR] {detail}\n")
            sys.stderr.flush()

    # 5. Load hook.js
    hook_file = project_dir / "hook.js"
    log(f"Loading hook script from {hook_file}...")
    hook_code = hook_file.read_text(encoding="utf-8")
    script = session.create_script(hook_code)
    script.on("message", on_message)
    script.load()
    log("hook.js loaded.")

    # 6. Load java_guard.bundle.js (if present)
    guard_script = None
    guard_file = project_dir / "java_guard.bundle.js"
    if guard_file.is_file():
        try:
            guard_code = guard_file.read_text(encoding="utf-8")
            guard_script = session.create_script(guard_code)
            guard_script.on("message", on_guard_message)
            guard_script.load()
            log("java_guard loaded.")
        except Exception as e:
            log(f"java_guard load failed (continuing): {e}")

    # 7. Load quago_probe.bundle.js (if NX_QUAGO=1)
    probe_script = None
    probe_file = project_dir / "quago_probe.bundle.js"
    if os.environ.get("NX_QUAGO") == "1" and probe_file.is_file():
        try:
            probe_code = probe_file.read_text(encoding="utf-8")
            probe_script = session.create_script(probe_code)
            probe_script.on("message", on_guard_message)
            probe_script.load()
            log("quago_probe loaded.")
        except Exception as e:
            log(f"quago_probe load failed: {e}")

    # 8. Verify ping and init
    log("Verifying agent ping...")
    if script.exports_sync.ping() != "pong":
        raise RuntimeError("Agent ping failed")

    status = script.exports_sync.status()
    if status.get("pid") != pid:
        raise RuntimeError(f"Gadget PID mismatch: expected {pid}, got {status.get('pid')}")
    if status.get("resumeComplete") is not False:
        raise RuntimeError("Resume barrier was already complete before external resume")

    log("Initializing agent...")
    if script.exports_sync.init() is not True:
        raise RuntimeError("Agent init failed")

    # 9. Resume process
    log("Resuming process...")
    injector.resume(pid)

    # 10. Wait for resume barrier
    log("Waiting for resume barrier...")
    deadline = time.monotonic() + 5.0
    barrier_ok = False
    while time.monotonic() < deadline:
        if detached_event.is_set():
            raise RuntimeError(f"Game process detached during resume: {detach_reason[0]}")
        try:
            st = script.exports_sync.status()
            if st.get("resumeComplete") is True:
                log("Resume barrier completed.")
                barrier_ok = True
                break
        except Exception:
            pass
        time.sleep(0.02)

    if not barrier_ok:
        raise RuntimeError("Spawn resume barrier did not complete within 5s")

    # 11. Release injector (kills frida-server with kill -9 FIRST, preventing SIGSEGV)
    log("Releasing injector server...")
    release_injector(args.adb, args.device_id, args.server_exe, manager, args.frida_port, residues_before)

    # 12. Wait for engine (libg.so)
    log("Waiting for libg.so base address...")
    engine_base = None
    stable_identity = None
    stable_since = None
    deadline = time.monotonic() + 90.0

    while time.monotonic() < deadline:
        if detached_event.is_set():
            raise RuntimeError(f"Game process detached/crashed: {detach_reason[0]}")
        try:
            info = script.exports_sync.info()
            base = info.get("base")
            size = info.get("size", 0)
            if base and size > 0 and int(str(base), 0) > 0:
                identity = (str(base), size)
                now = time.monotonic()
                if identity != stable_identity:
                    stable_identity = identity
                    stable_since = now
                    log(f"Engine heartbeat at {base} ({size} bytes); verifying 5s stability...")
                elif stable_since is not None and now - stable_since >= 5.0:
                    engine_base = base
                    log(f"libg.so stabilized at {base}.")
                    break
            else:
                stable_identity = None
                stable_since = None
        except Exception:
            pass
        time.sleep(0.5)

    if not engine_base:
        raise RuntimeError("libg.so did not become ready in time")

    # 13. Send ready signal to Rust host on stdout
    ready_msg = json.dumps({"status": "ready", "pid": pid, "base": engine_base})
    sys.stdout.write(ready_msg + "\n")
    sys.stdout.flush()

    # 14. Command processing loop
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            req_id = req.get("id", 0)
            method = req.get("method", "")
            args_list = req.get("args", [])

            if method.startswith("probe_") and probe_script:
                probe_method = method[6:]
                fn = getattr(probe_script.exports_sync, probe_method)
            else:
                fn = getattr(script.exports_sync, method)

            res = fn(*args_list)
            resp = json.dumps({"id": req_id, "result": res})
        except Exception as err:
            resp = json.dumps({"id": req_id if 'req_id' in locals() else 0, "error": str(err)})

        sys.stdout.write(resp + "\n")
        sys.stdout.flush()

if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        sys.stderr.write(f"[!] [bridge error] {exc}\n")
        sys.stderr.flush()
        sys.stdout.write(json.dumps({"status": "error", "error": str(exc)}) + "\n")
        sys.stdout.flush()
        sys.exit(1)
