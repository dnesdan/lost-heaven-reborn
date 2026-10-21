"""Bounded isolated X11 render capture; subprocess ownership is recorded privately."""

import argparse
import ctypes
import ctypes.util
import fcntl
import json
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def identity(pid):
    """Linux process birth identity prevents acting on a recycled PID."""
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return {"pid": pid, "start_ticks": fields[19],
                "boot_id": Path("/proc/sys/kernel/random/boot_id").read_text().strip(),
                "command": Path(f"/proc/{pid}/cmdline").read_bytes().hex()}
    except (OSError, IndexError):
        return None


def recover(directory):
    """Recover only our records and verified process births using Linux pidfds."""
    if not directory.exists():
        return
    for record in directory.glob("*.json"):
        try:
            data = json.loads(record.read_text())
        except (ValueError, OSError):
            # Preserve malformed evidence and never guess process identity.
            record.rename(record.with_suffix(".unreadable"))
            continue
        if not isinstance(data, dict):
            record.rename(record.with_suffix(".unreadable"))
            continue
        if data.get("owner") != "scripts/capture.py":
            continue
        controller = data.get("controller")
        if isinstance(controller, dict) and identity(controller.get("pid")) == controller:
            continue
        for saved in reversed(data.get("processes", [])):
            if not isinstance(saved, dict) or not isinstance(saved.get("pid"), int):
                continue
            pid = saved["pid"]
            if identity(pid) != saved:
                continue
            if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
                raise RuntimeError("owned leftovers require Linux pidfd support or manual verified cleanup")
            try:
                descriptor = os.pidfd_open(pid)
            except ProcessLookupError:
                continue
            try:
                if identity(pid) != saved:
                    continue
                try:
                    signal.pidfd_send_signal(descriptor, signal.SIGTERM)
                    if not select.select([descriptor], [], [], 8)[0]:
                        signal.pidfd_send_signal(descriptor, signal.SIGKILL)
                        if not select.select([descriptor], [], [], 3)[0]:
                            raise RuntimeError("owned process did not exit")
                except ProcessLookupError:
                    pass
            finally:
                os.close(descriptor)
        record.unlink()


def close_window(pid, display):
    """Send WM_DELETE_WINDOW directly; isolated Xvfb has no window manager."""
    windows = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(pid)],
                             env=dict(os.environ, DISPLAY=display), capture_output=True,
                             text=True, timeout=3)
    if windows.returncode:
        return
    library = ctypes.util.find_library("X11")
    if not library:
        return
    x11 = ctypes.CDLL(library)
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x11.XInternAtom.restype = ctypes.c_ulong
    x11.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.c_void_p]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]

    class Message(ctypes.Structure):
        _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong),
                    ("send_event", ctypes.c_int), ("display", ctypes.c_void_p),
                    ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong),
                    ("format", ctypes.c_int), ("data", ctypes.c_long * 5)]

    class Event(ctypes.Union):
        _fields_ = [("message", Message), ("padding", ctypes.c_long * 24)]

    connection = x11.XOpenDisplay(display.encode())
    if not connection:
        return
    try:
        for window in windows.stdout.split():
            event = Event()
            event.message.type = 33
            event.message.send_event = 1
            event.message.display = connection
            event.message.window = int(window)
            event.message.message_type = x11.XInternAtom(connection, b"WM_PROTOCOLS", 0)
            event.message.format = 32
            event.message.data[0] = x11.XInternAtom(connection, b"WM_DELETE_WINDOW", 0)
            x11.XSendEvent(connection, int(window), 0, 0, ctypes.byref(event))
        x11.XFlush(connection)
    finally:
        x11.XCloseDisplay(connection)


def main():
    os.umask(0o077)
    signal.signal(signal.SIGTERM, lambda _signal, _frame: sys.exit(143))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--godot", required=True)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--wait", type=float, default=6)
    args = parser.parse_args()
    if not 1 <= args.wait <= 45:
        parser.error("--wait must be between 1 and 45 seconds")
    for tool in ("Xvfb", "import", "identify", "xdotool"):
        if not shutil.which(tool):
            parser.error(f"{tool} is required")
    base = Path(os.environ.get("LHR_PRIVATE_DIR", "/workspace/.local/share/lost-heaven-reborn/private")).resolve()
    if base == ROOT or ROOT in base.parents:
        parser.error("private registry must remain outside the repository")
    output = (args.output or base / "captures" / time.strftime("%Y%m%d-%H%M%S")).resolve()
    if output == ROOT or ROOT in output.parents:
        parser.error("captures must remain outside the repository")
    registry = base / "capture-owners"
    registry.mkdir(parents=True, mode=0o700, exist_ok=True)
    with (registry / ".lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError("another capture is active; its processes were preserved") from None
        recover(registry)
        return capture(args, output, registry)


def capture(args, output, registry):
    output.mkdir(parents=True, mode=0o700, exist_ok=False)
    descriptor, name = tempfile.mkstemp(prefix="capture-", suffix=".json", dir=registry)
    os.close(descriptor)
    record = Path(name)
    processes = []
    game = None
    display = None

    def save_record():
        descriptor, temporary = tempfile.mkstemp(prefix=".update-", dir=registry)
        try:
            with os.fdopen(descriptor, "w") as stream:
                json.dump({"owner": "scripts/capture.py", "controller": identity(os.getpid()),
                           "output": str(output), "processes": [identity(p.pid) for p in processes]}, stream)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, record)
        finally:
            Path(temporary).unlink(missing_ok=True)

    save_record()
    read_fd, write_fd = os.pipe()
    try:
        with (output / "xvfb.log").open("w") as xlog, (output / "game.log").open("w") as glog:
            xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(write_fd), "-screen", "0", "1280x720x24", "-nolisten", "tcp"],
                                    pass_fds=(write_fd,), stdout=xlog, stderr=xlog)
            processes.append(xvfb)
            os.close(write_fd)
            write_fd = None
            save_record()
            if not select.select([read_fd], [], [], 10)[0]:
                raise RuntimeError("Xvfb did not become ready")
            display = os.read(read_fd, 64).decode().strip()
            if not display.isdigit():
                raise RuntimeError("Xvfb returned an invalid display")
            environment = dict(os.environ, DISPLAY=":" + display)
            game = subprocess.Popen([args.godot, "--path", str(ROOT), "--rendering-method", "mobile", "--rendering-driver", "vulkan", "--audio-driver", "Dummy", "--resolution", "1280x720", "--", "--capture-check"],
                                    env=environment, stdout=glog, stderr=glog)
            processes.append(game)
            save_record()
            deadline = time.monotonic() + 30
            window = None
            while time.monotonic() < deadline:
                if game.poll() is not None:
                    raise RuntimeError("game exited before its window became ready")
                search = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(game.pid)],
                                        env=environment, capture_output=True, text=True, timeout=3)
                if search.returncode == 0 and search.stdout.strip():
                    window = search.stdout.split()[0]
                    break
                time.sleep(0.25)
            if window is None:
                raise RuntimeError("game window did not become ready within 30 seconds")
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                if game.poll() is not None:
                    raise RuntimeError("game exited before rendering the scene")
                log = (output / "game.log").read_text(errors="replace")
                if any(value in log for value in ("SCRIPT ERROR", "ERROR:")):
                    raise RuntimeError("game reported an error; inspect private game.log")
                if "LHR_RENDER_READY" in log:
                    break
                time.sleep(0.25)
            else:
                raise RuntimeError("game scene did not render within 90 seconds")
            time.sleep(args.wait)
            if game.poll() is not None:
                raise RuntimeError("game exited before capture; inspect private game.log")
            subprocess.run(["import", "-display", ":" + display, "-window", window, str(output / "render.png")],
                           check=True, timeout=15, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if not (output / "render.png").is_file():
                raise RuntimeError("capture file was not created")
            spread = subprocess.check_output(["identify", "-format", "%[fx:standard_deviation]", str(output / "render.png")],
                                             text=True, timeout=5)
            if float(spread) < 0.005:
                raise RuntimeError("capture is nearly blank; inspect private logs and retry after renderer initialization")
            print("Vulkan capture saved privately. Inspect game.log to identify the actual renderer.")
    finally:
        if game is not None and game.poll() is None and display is not None:
            try:
                close_window(game.pid, ":" + display)
                game.wait(timeout=8)
            except (OSError, subprocess.SubprocessError):
                pass
        os.close(read_fd)
        if write_fd is not None:
            os.close(write_fd)
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=3)
        record.unlink(missing_ok=True)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Capture failed: {error}", file=sys.stderr)
        sys.exit(1)
