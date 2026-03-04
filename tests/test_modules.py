"""Comprehensive tests for ES module support.

Tests cover:
- <script type="module"> execution (inline + external)
- Static imports (import/export between modules)
- Dynamic import() expressions
- Module scope isolation (no global leaking)
- Module execution order (after classic scripts, before DOMContentLoaded)
- import.meta.url
- Module deduplication (same URL imported twice → single fetch+eval)
- nomodule fallback handling
- Error cases (syntax errors, missing modules, circular imports)
- Real-world patterns (default exports, named exports, re-exports, namespace imports)
"""

import re

import pytest

import blazeweb


def text_of(html: str, element_id: str) -> str:
    """Extract the text content of an element by id from rendered HTML.

    Looks for id="<element_id>">...< and returns the text between > and <.
    This avoids false positives from matching text inside <script> tags.
    """
    pattern = rf'id="{re.escape(element_id)}"[^>]*>([^<]*)<'
    m = re.search(pattern, html)
    return m.group(1) if m else ""


@pytest.fixture
def httpserver():
    pytest.importorskip("pytest_httpserver")
    from pytest_httpserver import HTTPServer

    server = HTTPServer(host="127.0.0.1")
    server.start()
    yield server
    server.clear()
    if server.is_running():
        server.stop()


# ─── Basic <script type="module"> execution ──────────────────────────────────


class TestInlineModuleExecution:
    """<script type="module"> with inline code."""

    def test_inline_module_executes(self):
        """Inline module script should execute and modify the DOM."""
        html = b"""<html><body>
            <p id="out">original</p>
            <script type="module">
                document.getElementById('out').textContent = 'module-executed';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "module-executed"

    def test_multiple_inline_modules(self):
        """Multiple inline modules execute in document order."""
        html = b"""<html><body>
            <div id="log"></div>
            <script type="module">
                document.getElementById('log').textContent += 'A';
            </script>
            <script type="module">
                document.getElementById('log').textContent += 'B';
            </script>
            <script type="module">
                document.getElementById('log').textContent += 'C';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "log") == "ABC"

    def test_module_scope_isolation(self):
        """Variables in one module should NOT leak to another module."""
        html = b"""<html><body>
            <div id="out"></div>
            <script type="module">
                const secret = 42;
            </script>
            <script type="module">
                try {
                    document.getElementById('out').textContent = 'leaked:' + secret;
                } catch(e) {
                    document.getElementById('out').textContent = 'isolated';
                }
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "isolated"

    def test_module_strict_mode(self):
        """Modules always execute in strict mode."""
        html = b"""<html><body>
            <div id="out"></div>
            <script type="module">
                try {
                    // In strict mode, assigning to undeclared variable throws
                    undeclaredVar = 123;
                    document.getElementById('out').textContent = 'sloppy';
                } catch(e) {
                    document.getElementById('out').textContent = 'strict';
                }
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "strict"

    def test_module_this_is_undefined(self):
        """Top-level `this` in a module should be undefined (not window)."""
        html = b"""<html><body>
            <div id="out"></div>
            <script type="module">
                document.getElementById('out').textContent =
                    'this=' + String(this);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "this=undefined"

    def test_module_export_syntax_parses(self):
        """export syntax should parse without error in a module."""
        html = b"""<html><body>
            <div id="out"></div>
            <script type="module">
                export const VERSION = '1.0';
                document.getElementById('out').textContent = 'v=' + VERSION;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "v=1.0"
        assert not result.errors


# ─── External module scripts ─────────────────────────────────────────────────


class TestExternalModuleScripts:
    """<script type="module" src="..."> fetching and execution."""

    def test_external_module_executes(self, httpserver):
        """External module script should be fetched and executed."""
        httpserver.expect_request("/app.mjs").respond_with_data(
            "document.getElementById('out').textContent = 'external-module';",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <p id="out">original</p>
            <script type="module" src="{httpserver.url_for('/app.mjs')}"></script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "external-module"

    def test_external_module_with_exports(self, httpserver):
        """External module with export syntax should parse without error."""
        httpserver.expect_request("/lib.mjs").respond_with_data(
            """
            export const VERSION = '2.0';
            document.getElementById('out').textContent = 'loaded-' + VERSION;
            """,
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <p id="out">original</p>
            <script type="module" src="{httpserver.url_for('/lib.mjs')}"></script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "loaded-2.0"


# ─── Static imports ──────────────────────────────────────────────────────────


class TestStaticImports:
    """Static import/export between modules."""

    def test_named_import(self, httpserver):
        """Import a named export from another module."""
        httpserver.expect_request("/math.mjs").respond_with_data(
            "export const PI = 3.14159;",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                import {{ PI }} from '{httpserver.url_for('/math.mjs')}';
                document.getElementById('out').textContent = 'PI=' + PI;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "PI=3.14159"

    def test_default_import(self, httpserver):
        """Import a default export from another module."""
        httpserver.expect_request("/greet.mjs").respond_with_data(
            "export default function() { return 'hello'; }",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                import greet from '{httpserver.url_for('/greet.mjs')}';
                document.getElementById('out').textContent = greet();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "hello"

    def test_namespace_import(self, httpserver):
        """Import all exports as a namespace object."""
        httpserver.expect_request("/utils.mjs").respond_with_data(
            "export const a = 1; export const b = 2; export const c = 3;",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                import * as utils from '{httpserver.url_for('/utils.mjs')}';
                document.getElementById('out').textContent =
                    'sum=' + (utils.a + utils.b + utils.c);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "sum=6"

    def test_chained_imports(self, httpserver):
        """Module A imports from B which imports from C."""
        httpserver.expect_request("/c.mjs").respond_with_data(
            "export const BASE = 100;",
            content_type="application/javascript",
        )
        httpserver.expect_request("/b.mjs").respond_with_data(
            f"import {{ BASE }} from '{httpserver.url_for('/c.mjs')}'; export const DOUBLED = BASE * 2;",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                import {{ DOUBLED }} from '{httpserver.url_for('/b.mjs')}';
                document.getElementById('out').textContent = 'val=' + DOUBLED;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "val=200"

    def test_re_export(self, httpserver):
        """Module re-exports bindings from another module."""
        httpserver.expect_request("/impl.mjs").respond_with_data(
            "export const X = 'from-impl';",
            content_type="application/javascript",
        )
        httpserver.expect_request("/facade.mjs").respond_with_data(
            f"export {{ X }} from '{httpserver.url_for('/impl.mjs')}';",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                import {{ X }} from '{httpserver.url_for('/facade.mjs')}';
                document.getElementById('out').textContent = X;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "from-impl"

    def test_module_deduplication(self, httpserver):
        """Same module imported by two scripts should only be evaluated once."""
        # The counter module increments a global on each evaluation
        httpserver.expect_request("/counter.mjs").respond_with_data(
            """
            if (!globalThis.__counter) globalThis.__counter = 0;
            globalThis.__counter++;
            export const count = globalThis.__counter;
            """,
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                import {{ count }} from '{httpserver.url_for('/counter.mjs')}';
                document.getElementById('out').textContent = 'first=' + count;
            </script>
            <script type="module">
                import {{ count }} from '{httpserver.url_for('/counter.mjs')}';
                document.getElementById('out').textContent += ',second=' + count;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        # Both should see count=1 because the module is only evaluated once
        assert "first=1" in text_of(result.html, "out")
        assert "second=1" in text_of(result.html, "out")


# ─── Dynamic import() ────────────────────────────────────────────────────────


class TestDynamicImport:
    """Dynamic import() expressions."""

    def test_dynamic_import_basic(self, httpserver):
        """Dynamic import() should resolve and return the module namespace."""
        httpserver.expect_request("/dyn.mjs").respond_with_data(
            "export const msg = 'dynamic-loaded';",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                const mod = await import('{httpserver.url_for('/dyn.mjs')}');
                document.getElementById('out').textContent = mod.msg;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "dynamic-loaded"

    def test_dynamic_import_from_classic_script(self, httpserver):
        """Dynamic import() should work in classic (non-module) scripts too."""
        httpserver.expect_request("/helper.mjs").respond_with_data(
            "export function add(a, b) { return a + b; }",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out">pending</div>
            <script>
                import('{httpserver.url_for('/helper.mjs')}').then(function(mod) {{
                    document.getElementById('out').textContent = 'sum=' + mod.add(3, 4);
                }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "sum=7"

    def test_dynamic_import_default_export(self, httpserver):
        """Dynamic import of a module with a default export."""
        httpserver.expect_request("/widget.mjs").respond_with_data(
            "export default class Widget { name() { return 'MyWidget'; } }",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                const {{ default: Widget }} = await import('{httpserver.url_for('/widget.mjs')}');
                const w = new Widget();
                document.getElementById('out').textContent = w.name();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "MyWidget"

    def test_dynamic_import_missing_module_rejects(self, httpserver):
        """Dynamic import of a non-existent module should reject the promise."""
        httpserver.expect_request("/404.mjs").respond_with_data(
            "Not found", status=404,
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                try {{
                    await import('{httpserver.url_for('/404.mjs')}');
                    document.getElementById('out').textContent = 'no-error';
                }} catch(e) {{
                    document.getElementById('out').textContent = 'caught';
                }}
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "caught"


# ─── Module execution order ──────────────────────────────────────────────────


class TestModuleExecutionOrder:
    """Module scripts execute after classic scripts per HTML spec."""

    def test_classic_before_module(self):
        """Classic scripts should execute before module scripts."""
        html = b"""<html><body>
            <div id="log"></div>
            <script type="module">
                document.getElementById('log').textContent += 'M';
            </script>
            <script>
                document.getElementById('log').textContent += 'C';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        # Per spec: classic scripts first, then modules
        assert text_of(result.html, "log") == "CM"

    def test_modules_maintain_relative_order(self):
        """Multiple module scripts execute in their document order."""
        html = b"""<html><body>
            <div id="log"></div>
            <script>document.getElementById('log').textContent += '1';</script>
            <script type="module">
                document.getElementById('log').textContent += '2';
            </script>
            <script>document.getElementById('log').textContent += '3';</script>
            <script type="module">
                document.getElementById('log').textContent += '4';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        # Classic scripts (1,3) first, then modules (2,4) in order
        assert text_of(result.html, "log") == "1324"


# ─── nomodule ────────────────────────────────────────────────────────────────


class TestNoModule:
    """<script nomodule> fallback handling."""

    def test_nomodule_skipped(self):
        """<script nomodule> should be skipped when modules are supported."""
        html = b"""<html><body>
            <div id="out">initial</div>
            <script nomodule>
                document.getElementById('out').textContent = 'nomodule-ran';
            </script>
            <script type="module">
                document.getElementById('out').textContent = 'module-ran';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "module-ran"

    def test_nomodule_with_type_also_skipped(self):
        """<script nomodule type="text/javascript"> should still be skipped."""
        html = b"""<html><body>
            <div id="out">initial</div>
            <script nomodule type="text/javascript">
                document.getElementById('out').textContent = 'nomodule-ran';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "initial"


# ─── import.meta ─────────────────────────────────────────────────────────────


class TestImportMeta:
    """import.meta properties."""

    def test_import_meta_url_inline(self):
        """import.meta.url should be available in inline modules."""
        html = b"""<html><body>
            <div id="out"></div>
            <script type="module">
                document.getElementById('out').textContent =
                    'meta=' + (typeof import.meta.url);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "meta=string"

    def test_import_meta_url_external(self, httpserver):
        """import.meta.url should reflect the module's URL."""
        httpserver.expect_request("/meta-test.mjs").respond_with_data(
            "document.getElementById('out').textContent = import.meta.url;",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module" src="{httpserver.url_for('/meta-test.mjs')}"></script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "/meta-test.mjs" in text_of(result.html, "out")


# ─── Error handling ──────────────────────────────────────────────────────────


class TestModuleErrors:
    """Error handling in module scripts."""

    def test_module_syntax_error_non_fatal(self):
        """Syntax error in one module should not prevent subsequent modules."""
        html = b"""<html><body>
            <div id="out">initial</div>
            <script type="module">
                this is not valid javascript !!!
            </script>
            <script type="module">
                document.getElementById('out').textContent = 'recovered';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "recovered"
        assert len(result.errors) >= 1  # syntax error captured

    def test_module_runtime_error_non_fatal(self):
        """Runtime error in module should not prevent other modules."""
        html = b"""<html><body>
            <div id="out">initial</div>
            <script type="module">
                null.crash();
            </script>
            <script type="module">
                document.getElementById('out').textContent = 'survived';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "survived"
        assert len(result.errors) >= 1

    def test_import_nonexistent_binding_is_undefined(self, httpserver):
        """Accessing a non-exported name via dynamic import returns undefined."""
        httpserver.expect_request("/partial.mjs").respond_with_data(
            "export const A = 1;",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                const mod = await import('{httpserver.url_for('/partial.mjs')}');
                document.getElementById('out').textContent =
                    'A=' + mod.A + ',Z=' + mod.Z;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        # Z is not exported so mod.Z is undefined
        assert text_of(result.html, "out") == "A=1,Z=undefined"

    def test_static_import_network_error(self, httpserver):
        """Static import of unreachable module should produce an error."""
        httpserver.expect_request("/gone.mjs").respond_with_data(
            "gone", status=404,
        )
        html = f"""<html><body>
            <div id="out">initial</div>
            <script type="module">
                import {{ x }} from '{httpserver.url_for('/gone.mjs')}';
                document.getElementById('out').textContent = 'loaded';
            </script>
            <script type="module">
                document.getElementById('out').textContent = 'second-ok';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        # First module should fail, second should still execute
        assert text_of(result.html, "out") == "second-ok"
        assert len(result.errors) >= 1


# ─── Top-level await ─────────────────────────────────────────────────────────


class TestTopLevelAwait:
    """Top-level await in modules (ES2022)."""

    def test_top_level_await_promise(self):
        """Top-level await should work with resolved promises."""
        html = b"""<html><body>
            <div id="out"></div>
            <script type="module">
                const value = await Promise.resolve(42);
                document.getElementById('out').textContent = 'val=' + value;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "val=42"

    def test_top_level_await_with_import(self, httpserver):
        """Top-level await combined with dynamic import."""
        httpserver.expect_request("/async-data.mjs").respond_with_data(
            "export const data = await Promise.resolve([1,2,3]);",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                const mod = await import('{httpserver.url_for('/async-data.mjs')}');
                document.getElementById('out').textContent = 'len=' + mod.data.length;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "len=3"


# ─── Mixed classic + module interaction ──────────────────────────────────────


class TestClassicModuleInteraction:
    """Interaction between classic scripts and modules."""

    def test_module_reads_global_from_classic(self):
        """Module should be able to read globals set by classic scripts."""
        html = b"""<html><body>
            <div id="out"></div>
            <script>
                window.GLOBAL_CONFIG = { version: '1.0' };
            </script>
            <script type="module">
                document.getElementById('out').textContent =
                    'v=' + window.GLOBAL_CONFIG.version;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "v=1.0"

    def test_module_sets_global_for_classic(self):
        """Module can set globals — but classic scripts run first."""
        html = b"""<html><body>
            <div id="out"></div>
            <script type="module">
                window.MODULE_DATA = 'from-module';
            </script>
            <script>
                // This runs before the module, so MODULE_DATA won't be set yet
                document.getElementById('out').textContent =
                    'data=' + (window.MODULE_DATA || 'not-set');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        # Classic runs first → MODULE_DATA not yet set
        assert text_of(result.html, "out") == "data=not-set"

    def test_module_sets_global_visible_to_later_module(self):
        """First module sets a global, second module reads it."""
        html = b"""<html><body>
            <div id="out"></div>
            <script type="module">
                window.SHARED = 'module-shared';
            </script>
            <script type="module">
                document.getElementById('out').textContent = window.SHARED;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "module-shared"


# ─── Real-world patterns ─────────────────────────────────────────────────────


class TestRealWorldPatterns:
    """Patterns commonly used by modern JS frameworks."""

    def test_class_export_and_import(self, httpserver):
        """Export/import a class with methods."""
        httpserver.expect_request("/component.mjs").respond_with_data(
            """
            export class Component {
                constructor(name) { this.name = name; }
                render() { return '<div>' + this.name + '</div>'; }
            }
            """,
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                import {{ Component }} from '{httpserver.url_for('/component.mjs')}';
                const c = new Component('MyApp');
                document.getElementById('out').innerHTML = c.render();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "MyApp" in result.html

    def test_multiple_exports(self, httpserver):
        """Module exporting multiple named values."""
        httpserver.expect_request("/config.mjs").respond_with_data(
            """
            export const API_URL = 'https://api.example.com';
            export const TIMEOUT = 5000;
            export function createClient(url) { return { url, timeout: TIMEOUT }; }
            """,
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                import {{ API_URL, TIMEOUT, createClient }} from '{httpserver.url_for('/config.mjs')}';
                const client = createClient(API_URL);
                document.getElementById('out').textContent =
                    client.url + ',' + client.timeout;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "https://api.example.com,5000"

    def test_conditional_dynamic_import(self, httpserver):
        """Dynamic import based on runtime condition (common in code splitting)."""
        httpserver.expect_request("/feature-a.mjs").respond_with_data(
            "export const name = 'A';",
            content_type="application/javascript",
        )
        httpserver.expect_request("/feature-b.mjs").respond_with_data(
            "export const name = 'B';",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script type="module">
                const useA = true;
                const mod = useA
                    ? await import('{httpserver.url_for('/feature-a.mjs')}')
                    : await import('{httpserver.url_for('/feature-b.mjs')}');
                document.getElementById('out').textContent = 'feature=' + mod.name;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert text_of(result.html, "out") == "feature=A"
