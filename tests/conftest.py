"""Shared test fixtures.

These tests require a usable Chromium binary. blazeweb auto-resolves from:
  1. explicit chrome_path= on Client (not used here)
  2. bundled python/blazeweb/_binaries/<platform>/chrome-headless-shell
  3. system chromium (apt install chromium-browser etc.)

If neither bundled nor system chromium is available, tests that spin a Client
will fail at Client() construction with a clear "chrome binary not found"
error — intended. Install chromium to run the suite.
"""
