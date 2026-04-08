"""E2E tests: Web Components (custom elements, streams, CSSOM, Shadow DOM)"""

from .conftest import text_of, render
import blazeweb

class TestStreamsAPI:
    def test_readable_stream_exists(self):
        """ReadableStream should be a defined constructor."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = typeof ReadableStream;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "function"

    def test_readable_stream_constructable(self):
        """new ReadableStream() should not throw."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            try {
                var rs = new ReadableStream();
                document.getElementById('result').textContent = 'ok';
            } catch(e) {
                document.getElementById('result').textContent = 'error:' + e.message;
            }
        </script>
        </body></html>""")
        assert text_of(html, "result") == "ok"

    def test_readable_stream_locked(self):
        """ReadableStream.locked should be false initially, true after getReader()."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var rs = new ReadableStream({
                start: function(controller) {
                    controller.enqueue('data');
                    controller.close();
                }
            });
            var before = rs.locked;
            var reader = rs.getReader();
            var after = rs.locked;
            document.getElementById('result').textContent = before + ',' + after;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "false,true"

    def test_readable_stream_reader_read(self):
        """reader.read() should return chunks then done."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var rs = new ReadableStream({
                start: function(controller) {
                    controller.enqueue('hello');
                    controller.close();
                }
            });
            var reader = rs.getReader();
            reader.read().then(function(r) {
                document.getElementById('result').textContent = r.value + ':' + r.done;
            });
        </script>
        </body></html>""")
        assert text_of(html, "result") == "hello:false"


# ─── Custom Elements Lifecycle (Phase 3) ─────────────────────────────────────


class TestCustomElementsLifecycle:
    def test_connected_callback_fires(self):
        """connectedCallback should fire when custom element is added to DOM."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            class MyEl extends HTMLElement {
                connectedCallback() {
                    document.getElementById('result').textContent = 'connected';
                }
            }
            customElements.define('my-el', MyEl);
            var el = document.createElement('my-el');
            document.body.appendChild(el);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "connected"

    def test_disconnected_callback_fires(self):
        """disconnectedCallback should fire when removed from DOM."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var log = [];
            class MyEl extends HTMLElement {
                disconnectedCallback() { log.push('disconnected'); }
            }
            customElements.define('my-el', MyEl);
            var el = document.createElement('my-el');
            document.body.appendChild(el);
            document.body.removeChild(el);
            document.getElementById('result').textContent = log.join(',');
        </script>
        </body></html>""")
        assert text_of(html, "result") == "disconnected"

    def test_attribute_changed_callback(self):
        """attributeChangedCallback should fire on observed attribute changes."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var log = [];
            class MyEl extends HTMLElement {
                static get observedAttributes() { return ['color']; }
                attributeChangedCallback(name, oldVal, newVal) {
                    log.push(name + '=' + newVal);
                }
            }
            customElements.define('my-el', MyEl);
            var el = document.createElement('my-el');
            document.body.appendChild(el);
            el.setAttribute('color', 'red');
            document.getElementById('result').textContent = log.join(',');
        </script>
        </body></html>""")
        assert text_of(html, "result") == "color=red"

    def test_non_observed_attribute_ignored(self):
        """Attributes not in observedAttributes should not trigger callback."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var count = 0;
            class MyEl extends HTMLElement {
                static get observedAttributes() { return ['color']; }
                attributeChangedCallback() { count++; }
            }
            customElements.define('my-el', MyEl);
            var el = document.createElement('my-el');
            document.body.appendChild(el);
            el.setAttribute('size', 'large');
            el.setAttribute('name', 'test');
            document.getElementById('result').textContent = String(count);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "0"

    def test_define_and_get(self):
        """customElements.get() should return the constructor after define()."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            class MyEl extends HTMLElement {}
            customElements.define('my-el', MyEl);
            document.getElementById('result').textContent =
                (customElements.get('my-el') === MyEl).toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true"


# ─── Canvas API (Phase 4) ────────────────────────────────────────────────────


class TestCSSOM:
    def test_computed_style_display_div(self):
        """getComputedStyle on div should return display value."""
        html = render("""<html><body>
        <div id="target"></div>
        <div id="result"></div>
        <script>
            var div = document.getElementById('target');
            var style = getComputedStyle(div);
            document.getElementById('result').textContent = style.display;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "block"

    def test_computed_style_display_span(self):
        """getComputedStyle on span should return inline."""
        html = render("""<html><body>
        <span id="target">text</span>
        <div id="result"></div>
        <script>
            var span = document.getElementById('target');
            document.getElementById('result').textContent = getComputedStyle(span).display;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "inline"

    def test_cssstylesheet_constructable(self):
        """new CSSStyleSheet() should not throw."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            try {
                var sheet = new CSSStyleSheet();
                document.getElementById('result').textContent = 'ok';
            } catch(e) {
                document.getElementById('result').textContent = 'error';
            }
        </script>
        </body></html>""")
        assert text_of(html, "result") == "ok"

    def test_cssstylesheet_replace_sync(self):
        """CSSStyleSheet.replaceSync should accept CSS text without throwing."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            try {
                var sheet = new CSSStyleSheet();
                sheet.replaceSync('body { color: red; }');
                document.getElementById('result').textContent = 'ok';
            } catch(e) {
                document.getElementById('result').textContent = 'error:' + e.message;
            }
        </script>
        </body></html>""")
        assert text_of(html, "result") == "ok"

    def test_adopted_stylesheets_is_array(self):
        """document.adoptedStyleSheets should be an array."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent =
                Array.isArray(document.adoptedStyleSheets).toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true"


# ─── Workers (Phase 6) ───────────────────────────────────────────────────────


class TestWorkerStubs:
    def test_service_worker_in_navigator(self):
        """'serviceWorker' should exist in navigator."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent =
                ('serviceWorker' in navigator).toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true"

    def test_service_worker_register_returns_promise(self):
        """navigator.serviceWorker.register() should return a Promise."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var p = navigator.serviceWorker.register('/sw.js');
            p.then(function() {
                document.getElementById('result').textContent = 'resolved';
            }).catch(function() {
                document.getElementById('result').textContent = 'rejected';
            });
        </script>
        </body></html>""")
        result = text_of(html, "result")
        assert result in ("resolved", "rejected")

    def test_shared_worker_constructor(self):
        """SharedWorker should be a function."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = typeof SharedWorker;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "function"


# ─── Round 2 Phase 1: Critical Bug Fixes ────────────────────────────────────


class TestShadowDOM:
    def test_attach_shadow_returns_object(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            var sr = div.attachShadow({mode: "open"});
            document.getElementById('result').textContent =
                (sr !== null && typeof sr === "object").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_shadow_root_mode(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            var sr = div.attachShadow({mode: "open"});
            document.getElementById('result').textContent = sr.mode;
        </script></body></html>""")
        assert text_of(html, "result") == "open"

    def test_shadow_root_host(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            var sr = div.attachShadow({mode: "open"});
            document.getElementById('result').textContent = (sr.host === div).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_element_shadow_root_getter(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            var sr = div.attachShadow({mode: "open"});
            document.getElementById('result').textContent = (div.shadowRoot === sr).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_closed_shadow_root(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            div.attachShadow({mode: "closed"});
            document.getElementById('result').textContent = (div.shadowRoot === null).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_shadow_root_innerhtml(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            var sr = div.attachShadow({mode: "open"});
            sr.innerHTML = "<span>shadow content</span>";
            document.getElementById('result').textContent = sr.querySelector("span").textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "shadow content"

    def test_shadow_root_append_child(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            var sr = div.attachShadow({mode: "open"});
            var span = document.createElement("span");
            span.textContent = "hello";
            sr.appendChild(span);
            document.getElementById('result').textContent = sr.firstChild.textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "hello"

    def test_shadow_root_query_selector(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            var sr = div.attachShadow({mode: "open"});
            sr.innerHTML = '<p class="x">found</p>';
            document.getElementById('result').textContent = sr.querySelector(".x").textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "found"

    def test_double_attach_throws(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            div.attachShadow({mode: "open"});
            try { div.attachShadow({mode: "open"}); document.getElementById('result').textContent = "no-throw"; }
            catch(e) { document.getElementById('result').textContent = "threw"; }
        </script></body></html>""")
        assert text_of(html, "result") == "threw"

    def test_invalid_host_throws(self):
        html = render("""<html><body><div id="result"></div><script>
            var input = document.createElement("input");
            try { input.attachShadow({mode: "open"}); document.getElementById('result').textContent = "no-throw"; }
            catch(e) { document.getElementById('result').textContent = "threw"; }
        </script></body></html>""")
        assert text_of(html, "result") == "threw"

    def test_shadow_root_instanceof(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            var sr = div.attachShadow({mode: "open"});
            document.getElementById('result').textContent = (sr instanceof ShadowRoot).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_custom_element_shadow(self):
        html = render("""<html><body><div id="result"></div><script>
            var el = document.createElement("my-widget");
            var sr = el.attachShadow({mode: "open"});
            sr.innerHTML = "<b>works</b>";
            document.getElementById('result').textContent = sr.querySelector("b").textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "works"

    def test_shadow_root_children_count(self):
        html = render("""<html><body><div id="result"></div><script>
            var div = document.createElement("div");
            var sr = div.attachShadow({mode: "open"});
            sr.innerHTML = "<p>1</p><p>2</p>";
            document.getElementById('result').textContent =
                sr.childNodes.length + ":" + sr.children.length;
        </script></body></html>""")
        assert text_of(html, "result") == "2:2"

    def test_event_composed_property(self):
        """Events should support composed property."""
        html = render("""<html><body><div id="result"></div><script>
            var e = new Event("test", {composed: true});
            document.getElementById('result').textContent = e.composed.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_event_composed_default_false(self):
        """Event.composed should default to false."""
        html = render("""<html><body><div id="result"></div><script>
            var e = new Event("test");
            document.getElementById('result').textContent = e.composed.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "false"

    def test_shadow_content_in_output(self):
        """Shadow DOM content should appear in serialized HTML output."""
        html = render("""<html><body>
        <div id="host"></div>
        <script>
            var host = document.getElementById("host");
            var sr = host.attachShadow({mode: "open"});
            sr.innerHTML = '<span class="shadow">shadow text</span>';
        </script>
        </body></html>""")
        assert "shadow text" in str(html)

    def test_slot_assigned_nodes(self):
        """slot.assignedNodes() should return slotted light DOM children."""
        html = render("""<html><body><div id="result"></div><script>
            var host = document.createElement("div");
            var child = document.createElement("span");
            child.textContent = "slotted";
            host.appendChild(child);
            var sr = host.attachShadow({mode: "open"});
            sr.innerHTML = "<slot></slot>";
            var slot = sr.querySelector("slot");
            var assigned = slot.assignedNodes();
            document.getElementById('result').textContent = assigned.length + ":" +
                (assigned[0] ? assigned[0].textContent : "none");
        </script></body></html>""")
        assert text_of(html, "result") == "1:slotted"

    def test_named_slot(self):
        """Named slots should match children with matching slot attribute."""
        html = render("""<html><body><div id="result"></div><script>
            var host = document.createElement("div");
            var c1 = document.createElement("span");
            c1.setAttribute("slot", "header");
            c1.textContent = "H";
            host.appendChild(c1);
            var c2 = document.createElement("span");
            c2.textContent = "D";
            host.appendChild(c2);
            var sr = host.attachShadow({mode: "open"});
            sr.innerHTML = '<slot name="header"></slot><slot></slot>';
            var slots = sr.querySelectorAll("slot");
            var h = slots.item(0).assignedNodes();
            var d = slots.item(1).assignedNodes();
            document.getElementById('result').textContent = h[0].textContent + ":" + d[0].textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "H:D"

    def test_slot_assigned_elements(self):
        """slot.assignedElements() should return only element children."""
        html = render("""<html><body><div id="result"></div><script>
            var host = document.createElement("div");
            host.appendChild(document.createTextNode("text"));
            host.appendChild(document.createElement("span"));
            var sr = host.attachShadow({mode: "open"});
            sr.innerHTML = "<slot></slot>";
            var slot = sr.querySelector("slot");
            document.getElementById('result').textContent =
                slot.assignedNodes().length + ":" + slot.assignedElements().length;
        </script></body></html>""")
        assert text_of(html, "result") == "2:1"


# ─── DOM Type Hierarchy ─────────────────────────────────────────────────────



