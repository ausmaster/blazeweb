"""Pre-packaged ``ClientConfig`` bundles — spread into ``Client(**preset)``.

Presets are plain ``dict`` s. Everything they set lives in the existing
config hierarchy — no new API surface; no ``StealthConfig`` class. Compose
via standard Python dict spread::

    from blazeweb import Client
    from blazeweb.presets import stealth, recon

    # Spread a preset directly
    client = Client(**stealth.BASIC)

    # Tweak a preset field — pre-merge the dict, Python forbids duplicate
    # keyword args across ``**`` spreads or between ``**`` and an explicit kwarg
    client = Client(**{**stealth.BASIC, "user_agent": "MyBot/1.0"})

    # Compose two presets — same pre-merge idiom
    client = Client(**{**stealth.BASIC, **recon.FAST})

See individual modules for specifics:

* :mod:`blazeweb.presets.stealth` — UA brand swap + ``Sec-CH-UA`` metadata
  + canonical JS runtime patches. Avoids first-byte anti-bot tripwires
  (Akamai's ``HeadlessChrome`` substring check, etc.) and the commonly
  detected ``navigator.webdriver`` / ``window.chrome`` / canvas / WebGL
  tells. Documents what each patch counters.

* :mod:`blazeweb.presets.recon` — fast-scan config (JS off, short nav
  timeout, ad/tracker blocking) for high-throughput URL sweeps.

* :mod:`blazeweb.presets.archival` — long nav timeout + extra settle time
  for change-detection / snapshot workflows.

A note on **list-valued fields**: when two presets both set
``scripts.on_new_document`` (or other list fields), naive dict spread
replaces rather than concatenates. To cumulate, spread manually::

    Client(**stealth.BASIC, scripts={"on_new_document": [
        *stealth.BASIC["scripts"]["on_new_document"],
        my_custom_script,
    ]})
"""

from blazeweb.presets import archival, recon, stealth

__all__ = ["archival", "recon", "stealth"]
