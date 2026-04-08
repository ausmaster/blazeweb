"""Debug slow blazeweb.fetch() by running with verbose logging + hard timeout."""
import sys
import os
import signal
import time
import threading

def watchdog(pid, timeout):
    """Kill process after timeout seconds."""
    time.sleep(timeout)
    print(f"\n{'='*60}")
    print(f"WATCHDOG: {timeout}s elapsed, killing process")
    print(f"{'='*60}")
    os.kill(pid, signal.SIGKILL)

url = sys.argv[1] if len(sys.argv) > 1 else "https://www.tesla.com"
timeout = int(sys.argv[2]) if len(sys.argv) > 2 else 12

# Set RUST_LOG for detailed tracing
os.environ["RUST_LOG"] = "_blazeweb=debug"
os.environ["BLAZEWEB_NETWORK_TIMEOUT"] = "8"

# Start watchdog thread
t = threading.Thread(target=watchdog, args=(os.getpid(), timeout), daemon=True)
t.start()

import blazeweb

print(f"Fetching {url} (watchdog: {timeout}s)...")
t0 = time.perf_counter()
try:
    result = blazeweb.fetch(url)
    elapsed = (time.perf_counter() - t0) * 1000
    print(f"SUCCESS: {elapsed:.0f}ms, {len(str(result))} bytes, {str(result).count('<')} tags")
except Exception as e:
    elapsed = (time.perf_counter() - t0) * 1000
    print(f"ERROR after {elapsed:.0f}ms: {e}")
