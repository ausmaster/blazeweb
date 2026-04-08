"""E2E tests: Web APIs (atob/btoa, CSS, window methods, navigator, history, geometry, WebSocket)"""

from .conftest import text_of, render
import blazeweb

class TestWebAPIs:
    def test_local_storage(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            localStorage.setItem('key', 'value');
            document.getElementById('result').textContent =
                localStorage.getItem('key');
        </script></body></html>""")
        assert text_of(html, "result") == "value"

    def test_atob_btoa(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var encoded = btoa('hello world');
            var decoded = atob(encoded);
            document.getElementById('result').textContent =
                encoded + '|' + decoded;
        </script></body></html>""")
        assert text_of(html, "result") == "aGVsbG8gd29ybGQ=|hello world"

    def test_url_constructor(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var u = new URL('https://example.com/path?q=1#hash');
            document.getElementById('result').textContent =
                u.hostname + '|' + u.pathname + '|' + u.search + '|' + u.hash;
        </script></body></html>""")
        assert text_of(html, "result") == "example.com|/path|?q=1|#hash"

    def test_dataset(self):
        html = render("""<html><body>
        <div id="target" data-foo="bar" data-baz-qux="hello"></div>
        <div id="result"></div>
        <script>
            var ds = document.getElementById('target').dataset;
            document.getElementById('result').textContent =
                ds.foo + '|' + ds.bazQux;
        </script></body></html>""")
        assert text_of(html, "result") == "bar|hello"


# ─── Module edge cases (missing from test_modules.py) ────────────────────────


class TestAtobBtoa:
    def test_atob_unpadded_input(self):
        """atob() should handle base64 without padding (auto-pad with =)."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = atob("SGVsbG8");
        </script>
        </body></html>""")
        assert text_of(html, "result") == "Hello"

    def test_atob_standard_padded(self):
        """atob() should handle properly padded base64."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = atob("SGVsbG8=");
        </script>
        </body></html>""")
        assert text_of(html, "result") == "Hello"

    def test_atob_binary_data(self):
        """atob() should return Latin-1 string for bytes > 127."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = atob("gA==").charCodeAt(0).toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "128"

    def test_atob_full_binary_range(self):
        """atob() should handle bytes across the full 0-255 range."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var s = atob("/w==");
            document.getElementById('result').textContent = s.charCodeAt(0).toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "255"

    def test_atob_empty_string(self):
        """atob('') should return empty string."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = atob("") === "" ? "ok" : "fail";
        </script>
        </body></html>""")
        assert text_of(html, "result") == "ok"


class TestCSSGlobal:
    def test_css_supports_exists(self):
        """CSS.supports() should be a function."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent =
                (typeof CSS.supports === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_css_escape_exists(self):
        """CSS.escape() should be a function."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent =
                (typeof CSS.escape === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"


class TestWindowMethods:
    def test_postmessage_exists(self):
        """window.postMessage should be a function."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = typeof postMessage;
        </script></body></html>""")
        assert text_of(html, "result") == "function"

    def test_alert_exists(self):
        """window.alert should be a function."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = typeof alert;
        </script></body></html>""")
        assert text_of(html, "result") == "function"

    def test_confirm_returns_false(self):
        """window.confirm() should return false in SSR."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = confirm("test").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "false"

    def test_prompt_returns_null(self):
        """window.prompt() should return null in SSR."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = (prompt("test") === null).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_globalthis_equals_window(self):
        """globalThis should equal window."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = (globalThis === window).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_filelist_defined(self):
        """FileList should be defined."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = (typeof FileList !== "undefined").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_dom_exception_constructor(self):
        """DOMException should be constructable with name and message."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var e = new DOMException("msg", "NotFoundError");
            document.getElementById('result').textContent = e.name + ":" + e.message;
        </script></body></html>""")
        assert text_of(html, "result") == "NotFoundError:msg"


# ─── Round 2 Phase 3: Event Constructor Expansion ───────────────────────────


class TestNavigatorStubs:
    def test_navigator_clipboard(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof navigator.clipboard === "object").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_navigator_clipboard_write_text(self):
        html = render("""<html><body><div id="result"></div><script>
            navigator.clipboard.writeText("test").then(function() {
                document.getElementById('result').textContent = "ok";
            });
        </script></body></html>""")
        assert text_of(html, "result") == "ok"

    def test_navigator_permissions_query(self):
        html = render("""<html><body><div id="result"></div><script>
            navigator.permissions.query({name:"notifications"}).then(function(r) {
                document.getElementById('result').textContent = r.state;
            });
        </script></body></html>""")
        assert text_of(html, "result") == "prompt"

    def test_navigator_media_devices(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof navigator.mediaDevices === "object").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_navigator_connection(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = navigator.connection.effectiveType;
        </script></body></html>""")
        assert text_of(html, "result") == "4g"

    def test_navigator_geolocation(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof navigator.geolocation === "object").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_navigator_max_touch_points(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = navigator.maxTouchPoints.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "0"

    def test_navigator_storage_estimate(self):
        html = render("""<html><body><div id="result"></div><script>
            navigator.storage.estimate().then(function(r) {
                document.getElementById('result').textContent =
                    (typeof r.quota === "number").toString();
            });
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_navigator_webdriver(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = navigator.webdriver.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "false"

    def test_navigator_device_memory(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof navigator.deviceMemory === "number").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"


class TestWindowOnHandlers:
    def test_onload_settable(self):
        html = render("""<html><body><div id="result"></div><script>
            window.onload = function(){};
            document.getElementById('result').textContent =
                (typeof window.onload === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_onerror_exists(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                ("onerror" in window).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_onpopstate_exists(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                ("onpopstate" in window).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"


# ─── Round 2 Phase 5: HTMLMediaElement + Form Validation ─────────────────────


class TestHistoryState:
    def test_push_state_stores_state(self):
        html = render("""<html><body><div id="result"></div><script>
            history.pushState({page: 1}, "", "/foo");
            document.getElementById('result').textContent = history.state.page.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "1"

    def test_push_state_increments_length(self):
        html = render("""<html><body><div id="result"></div><script>
            var initial = history.length;
            history.pushState(null, "", "/a");
            history.pushState(null, "", "/b");
            document.getElementById('result').textContent =
                (history.length - initial).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "2"

    def test_replace_state_no_length_change(self):
        html = render("""<html><body><div id="result"></div><script>
            var initial = history.length;
            history.replaceState({x: 2}, "", "/bar");
            document.getElementById('result').textContent =
                history.state.x + ":" + (history.length === initial);
        </script></body></html>""")
        assert text_of(html, "result") == "2:true"

    def test_push_state_updates_location(self):
        html = render("""<html><body><div id="result"></div><script>
            history.pushState(null, "", "/test/path?q=1#hash");
            document.getElementById('result').textContent =
                location.pathname + location.search + location.hash;
        </script></body></html>""")
        assert text_of(html, "result") == "/test/path?q=1#hash"

    def test_scroll_restoration(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = history.scrollRestoration;
        </script></body></html>""")
        assert text_of(html, "result") == "auto"


# ─── Round 2 Phase 7: Constructor Stubs + Missing Interfaces ────────────────


class TestGeometryConstructors:
    def test_domrect_constructor(self):
        html = render("""<html><body><div id="result"></div><script>
            var r = new DOMRect(10, 20, 100, 50);
            document.getElementById('result').textContent =
                r.x + "," + r.y + "," + r.width + "," + r.height;
        </script></body></html>""")
        assert text_of(html, "result") == "10,20,100,50"

    def test_domrect_computed_props(self):
        html = render("""<html><body><div id="result"></div><script>
            var r = new DOMRect(10, 20, 100, 50);
            document.getElementById('result').textContent =
                r.top + "," + r.right + "," + r.bottom + "," + r.left;
        </script></body></html>""")
        assert text_of(html, "result") == "20,110,70,10"

    def test_dompoint_constructor(self):
        html = render("""<html><body><div id="result"></div><script>
            var p = new DOMPoint(1, 2, 3, 4);
            document.getElementById('result').textContent = p.x + "," + p.y;
        </script></body></html>""")
        assert text_of(html, "result") == "1,2"

    def test_dommatrix_identity(self):
        html = render("""<html><body><div id="result"></div><script>
            var m = new DOMMatrix();
            document.getElementById('result').textContent =
                m.is2D.toString() + "," + m.isIdentity.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true,true"


class TestWebSocketBroadcastChannel:
    def test_websocket_constants(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                WebSocket.CONNECTING + "," + WebSocket.OPEN + "," + WebSocket.CLOSED;
        </script></body></html>""")
        assert text_of(html, "result") == "0,1,3"

    def test_websocket_constructor(self):
        html = render("""<html><body><div id="result"></div><script>
            var ws = new WebSocket("ws://localhost");
            document.getElementById('result').textContent = ws.readyState.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "3"

    def test_broadcast_channel_name(self):
        html = render("""<html><body><div id="result"></div><script>
            var bc = new BroadcastChannel("test");
            document.getElementById('result').textContent = bc.name;
        </script></body></html>""")
        assert text_of(html, "result") == "test"


class TestDocumentFontsState:
    def test_document_fonts_ready(self):
        html = render("""<html><body><div id="result"></div><script>
            document.fonts.ready.then(function() {
                document.getElementById('result').textContent = "ok";
            });
        </script></body></html>""")
        assert text_of(html, "result") == "ok"

    def test_document_visibility_state(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = document.visibilityState;
        </script></body></html>""")
        assert text_of(html, "result") == "visible"

    def test_document_hidden(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = document.hidden.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "false"

    def test_performance_time_origin(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof performance.timeOrigin === "number").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"


# ─── Round 2 Phase 8: HTMLElement Global Attrs + Document Hardening ──────────


class TestDocumentHardening:
    def test_computed_style_get_property_value(self):
        html = render("""<html><body><div id="result"></div><script>
            var d = document.createElement("div");
            document.body.appendChild(d);
            document.getElementById('result').textContent =
                getComputedStyle(d).getPropertyValue("display");
        </script></body></html>""")
        assert text_of(html, "result") == "block"

    def test_document_ready_state(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = document.readyState;
        </script></body></html>""")
        assert text_of(html, "result") == "complete"

    def test_document_has_focus(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = document.hasFocus().toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"


# ─── Shadow DOM ──────────────────────────────────────────────────────────────



