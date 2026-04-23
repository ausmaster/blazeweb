"""Concurrency tests for blazeweb.Client.

These tests guard against the V8 JSDispatchTable race in concurrent isolate
init that crashed real-world usage when calling Client.fetch() from multiple
threads (vault8 scrape regression).
"""
from __future__ import annotations

import concurrent.futures
import textwrap

import pytest

import blazeweb


# Heavy JS that forces V8 isolate creation, compilation, GC pressure, and
# enough runtime to ensure isolates from concurrent renders overlap in lifetime.
# The race is in V8's global JSDispatchTable freelist — it triggers when
# isolate-A is doing background work (GC/compilation worker threads) while
# isolate-B is being created on another thread. Short scripts that exit
# instantly don't reproduce because isolate lifetimes never overlap.
HTML_WITH_JS = textwrap.dedent(
    """\
    <!DOCTYPE html>
    <html><head>
    <script>
      // Force compilation of many functions (puts pressure on JSDispatchTable)
      function makeFn(seed) {
        return function(x) {
          var r = seed;
          for (var i = 0; i < 50; i++) {
            r = ((r * 31 + x + i) ^ (r >>> 7)) | 0;
          }
          return r;
        };
      }
      var fns = [];
      for (var k = 0; k < 200; k++) fns.push(makeFn(k));
      // Force GC + dispatch table churn
      var sum = 0;
      for (var j = 0; j < 1000; j++) {
        for (var f = 0; f < fns.length; f++) {
          sum = (sum + fns[f](j)) | 0;
        }
      }
      document.title = 'sum=' + sum;
    </script>
    </head><body><div id="d">hello</div></body></html>
    """
).encode("utf-8")


def _render_one(client: blazeweb.Client, idx: int) -> tuple[int, int, str | None]:
    """Render once. Returns (idx, output_len, error_or_none)."""
    try:
        result = client.render(HTML_WITH_JS)
        return idx, len(str(result)), None
    except Exception as exc:
        return idx, 0, f"{type(exc).__name__}: {exc}"


@pytest.mark.parametrize("workers", [2, 4, 8])
def test_client_render_concurrent_does_not_crash(workers: int) -> None:
    """Concurrent client.render() from multiple threads must not SIGSEGV.

    Regression: V8 135's JSDispatchTable freelist races during concurrent
    Isolate::Init when worker threads with live isolates are doing background
    work (GC, compilation) at the same time as another thread is creating a
    new isolate. Our fix serializes the entire isolate lifetime.
    """
    client = blazeweb.Client()
    n_renders = workers * 4  # enough to overlap

    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as ex:
        results = list(
            ex.map(lambda i: _render_one(client, i), range(n_renders))
        )

    failures = [(i, err) for i, _, err in results if err]
    assert not failures, f"concurrent renders failed: {failures}"

    # Sanity: every render produced output
    for i, length, _ in results:
        assert length > 0, f"render {i} returned empty output"
