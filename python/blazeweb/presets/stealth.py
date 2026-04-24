"""Stealth preset — opt-in, transparent anti-anti-bot.

The default chrome-headless-shell UA contains the literal substring
``HeadlessChrome``, and Akamai Bot Manager (among others) first-byte-matches
on it and returns a stub response in ~60 ms. Beyond UA, anti-bot scripts
also probe ``navigator.webdriver``, ``window.chrome``, ``navigator.plugins``,
``navigator.permissions.query``, WebGL vendor/renderer, canvas fingerprint
noise, and CPU/memory heuristics. This module ships the canonical patch
set, each with a comment naming the detection vector it counters.

Philosophy — opt-in and transparent, modeled on
`rebrowser-patches <https://github.com/rebrowser/rebrowser-patches>`_. Stealth
is OFF by default; every modification is named and auditable.

What it does NOT fix (out of scope):

* TLS ClientHello fingerprint — already matches real Chrome because
  chrome-headless-shell and full Chrome use the same BoringSSL build.
* Cross-origin iframe script propagation — CDP limitation; Cloudflare
  Turnstile in particular runs in a cross-origin iframe.
* Service-worker / shared-worker scope — CDP does not inject into workers.
* Behavioral simulation — mouse/scroll/keyboard timing is not simulated.

Usage::

    from blazeweb import Client
    from blazeweb.presets import stealth

    client = Client(**stealth.BASIC)             # 6-patch Phase 1
    client = Client(**stealth.FINGERPRINT)       # + WebGL vendor + canvas noise
"""

from __future__ import annotations

# Linux x86_64, Chrome 131 — matches the OS on which chrome-headless-shell
# runs locally, and so stays consistent with the TLS fingerprint. Bump this
# string alongside CHROME_VERSION in _download_chrome.py when the bundled
# Chrome major version moves.
BASIC_UA: str = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
)

BASIC_UA_METADATA: dict = {
    "brands": [
        {"brand": "Google Chrome", "version": "131"},
        {"brand": "Chromium", "version": "131"},
        {"brand": "Not_A Brand", "version": "24"},
    ],
    "full_version_list": [
        {"brand": "Google Chrome", "version": "131.0.6778.69"},
        {"brand": "Chromium", "version": "131.0.6778.69"},
        {"brand": "Not_A Brand", "version": "24.0.0.0"},
    ],
    "platform": "Linux",
    "platform_version": "",
    "architecture": "x86",
    "model": "",
    "mobile": False,
    "bitness": "64",
    "wow64": False,
}


# -- JS patches ---------------------------------------------------------------
# Each patch is a self-contained statement / IIFE. All are idempotent and
# guard with feature checks so double-registration or running on shapes
# other than chrome-headless-shell is harmless.

_PATCH_WEBDRIVER = """\
/* navigator.webdriver — Akamai/PerimeterX/DataDome/Cloudflare read this
   property and treat true as 'automation'. Real browsers never expose it.
   We redefine via the prototype so the getter returns undefined. */
Object.defineProperty(Navigator.prototype, 'webdriver', {
  get: () => undefined,
  configurable: true,
});
"""

_PATCH_WINDOW_CHROME = """\
/* window.chrome — real desktop Chrome exposes a populated chrome.runtime
   object (plus loadTimes, csi, app). chrome-headless-shell leaves most of
   it unset; anti-bot scripts check for chrome.runtime.OnInstalledReason
   or window.chrome.loadTimes as a tell. */
if (!window.chrome || !window.chrome.runtime) {
  window.chrome = window.chrome || {};
  window.chrome.runtime = window.chrome.runtime || {
    OnInstalledReason: { CHROME_UPDATE: 'chrome_update', INSTALL: 'install',
      SHARED_MODULE_UPDATE: 'shared_module_update', UPDATE: 'update' },
    OnRestartRequiredReason: { APP_UPDATE: 'app_update',
      OS_UPDATE: 'os_update', PERIODIC: 'periodic' },
    PlatformArch: { ARM: 'arm', ARM64: 'arm64', MIPS: 'mips',
      MIPS64: 'mips64', X86_32: 'x86-32', X86_64: 'x86-64' },
    PlatformNaclArch: { ARM: 'arm', MIPS: 'mips', MIPS64: 'mips64',
      X86_32: 'x86-32', X86_64: 'x86-64' },
    PlatformOs: { ANDROID: 'android', CROS: 'cros', LINUX: 'linux',
      MAC: 'mac', OPENBSD: 'openbsd', WIN: 'win' },
    RequestUpdateCheckStatus: { NO_UPDATE: 'no_update',
      THROTTLED: 'throttled', UPDATE_AVAILABLE: 'update_available' },
  };
  window.chrome.loadTimes = function() { return {}; };
  window.chrome.csi = function() { return {}; };
  window.chrome.app = { isInstalled: false };
}
"""

_PATCH_PLUGINS = """\
/* navigator.plugins — headless Chrome exposes an empty PluginArray; real
   desktop Chrome exposes ~5 built-in PDF-viewer entries. Anti-bot scripts
   flag navigator.plugins.length === 0 as a headless indicator. */
(() => {
  const makePlugin = (name, filename, desc) => {
    const mt = { type: 'application/pdf', suffixes: 'pdf', description: '' };
    const plugin = { name, filename, description: desc, length: 1, 0: mt };
    return plugin;
  };
  const plugins = [
    makePlugin('PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format'),
    makePlugin('Chrome PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format'),
    makePlugin('Chromium PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format'),
    makePlugin('Microsoft Edge PDF Viewer', 'internal-pdf-viewer', 'Portable Document Format'),
    makePlugin('WebKit built-in PDF', 'internal-pdf-viewer', 'Portable Document Format'),
  ];
  Object.defineProperty(plugins, 'length', { get: () => 5 });
  Object.defineProperty(navigator, 'plugins', { get: () => plugins, configurable: true });
})();
"""

_PATCH_PERMISSIONS = """\
/* navigator.permissions.query — headless Chrome returns state='denied' for
   'notifications'; real Chrome returns state='default' unless the user has
   explicitly granted or blocked. Anti-bot scripts compare the two. */
(() => {
  if (!window.navigator.permissions || !window.navigator.permissions.query) return;
  const origQuery = window.navigator.permissions.query.bind(window.navigator.permissions);
  window.navigator.permissions.query = (parameters) => (
    parameters && parameters.name === 'notifications'
      ? Promise.resolve({
          state: typeof Notification !== 'undefined' ? Notification.permission : 'default',
          name: 'notifications',
          onchange: null,
        })
      : origQuery(parameters)
  );
})();
"""

_PATCH_HARDWARE = """\
/* navigator.hardwareConcurrency / navigator.deviceMemory — CI/container
   environments commonly show 2/4 (suspiciously low). We report 8/8 to
   match typical desktop hardware. */
Object.defineProperty(navigator, 'hardwareConcurrency', {
  get: () => 8, configurable: true,
});
Object.defineProperty(navigator, 'deviceMemory', {
  get: () => 8, configurable: true,
});
"""

_PATCH_WEBGL = """\
/* WebGL UNMASKED_VENDOR_WEBGL / UNMASKED_RENDERER_WEBGL — headless Chrome
   returns 'Google Inc. (Google)' / 'ANGLE (...)' referencing SwiftShader
   (the software renderer used when no GPU is available). Anti-bot scripts
   read these via getParameter(37445/37446). Override to a common integrated
   GPU identity. */
(() => {
  const spoof = (p) => {
    if (p === 37445) return 'Intel Inc.';
    if (p === 37446) return 'Intel(R) UHD Graphics 620';
    return null;
  };
  const orig1 = WebGLRenderingContext.prototype.getParameter;
  WebGLRenderingContext.prototype.getParameter = function(p) {
    const s = spoof(p);
    return s !== null ? s : orig1.call(this, p);
  };
  if (typeof WebGL2RenderingContext !== 'undefined') {
    const orig2 = WebGL2RenderingContext.prototype.getParameter;
    WebGL2RenderingContext.prototype.getParameter = function(p) {
      const s = spoof(p);
      return s !== null ? s : orig2.call(this, p);
    };
  }
})();
"""

_PATCH_CANVAS_NOISE = """\
/* Canvas fingerprint — headless Chrome's canvas renders are bit-identical
   across sessions (deterministic software rasterizer); real hardware varies
   subtly due to GPU drivers. Anti-bot scripts hash toDataURL() output. We
   perturb a small fraction of pixels' alpha channel by +/-1 so the hash
   differs per session while the visual is preserved. */
(() => {
  const sessionSeed = Math.floor(Math.random() * 1e9);
  const origToDataURL = HTMLCanvasElement.prototype.toDataURL;
  HTMLCanvasElement.prototype.toDataURL = function() {
    try {
      const ctx = this.getContext('2d');
      const w = this.width, h = this.height;
      if (ctx && w > 0 && h > 0) {
        const imageData = ctx.getImageData(0, 0, w, h);
        const d = imageData.data;
        const step = Math.max(4, Math.floor(d.length / 400) * 4);
        for (let i = 3; i < d.length; i += step) {
          d[i] = Math.max(0, Math.min(255, d[i] + ((sessionSeed + i) % 3) - 1));
        }
        ctx.putImageData(imageData, 0, 0);
      }
    } catch (e) { /* non-2d canvas, CORS-tainted, etc. — fall through */ }
    return origToDataURL.apply(this, arguments);
  };
})();
"""


BASIC_PATCHES: list[str] = [
    _PATCH_WEBDRIVER,
    _PATCH_WINDOW_CHROME,
    _PATCH_PLUGINS,
    _PATCH_PERMISSIONS,
    _PATCH_HARDWARE,
]

FINGERPRINT_PATCHES: list[str] = [
    *BASIC_PATCHES,
    _PATCH_WEBGL,
    _PATCH_CANVAS_NOISE,
]


BASIC: dict = {
    "user_agent": BASIC_UA,
    "user_agent_metadata": BASIC_UA_METADATA,
    "scripts": {"on_new_document": BASIC_PATCHES},
}

FINGERPRINT: dict = {
    "user_agent": BASIC_UA,
    "user_agent_metadata": BASIC_UA_METADATA,
    "scripts": {"on_new_document": FINGERPRINT_PATCHES},
}


__all__ = [
    "BASIC",
    "BASIC_PATCHES",
    "BASIC_UA",
    "BASIC_UA_METADATA",
    "FINGERPRINT",
    "FINGERPRINT_PATCHES",
]
