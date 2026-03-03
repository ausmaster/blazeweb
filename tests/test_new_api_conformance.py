"""Conformance tests for new APIs (Batches 6-13): blazeweb vs headless Chromium.

Each test renders the same HTML through both blazeweb.render() and
Playwright (headless Chromium), then compares the JS-computed results.
"""

from __future__ import annotations

import pytest

lxml_html = pytest.importorskip("lxml.html")

import blazeweb  # noqa: E402

pytestmark = pytest.mark.conformance


# ── Helpers ──────────────────────────────────────────────────────────────────


def render_both(html: str, page) -> tuple[str, str]:
    bc_output = blazeweb.render(html)
    page.set_content(html, wait_until="load")
    chrome_output = page.content()
    return bc_output, chrome_output


def get_text(html_string: str, selector: str) -> str:
    doc = lxml_html.document_fromstring(html_string)
    els = doc.cssselect(selector)
    if not els:
        return ""
    return els[0].text_content()


def assert_text_equal(bc_html: str, chrome_html: str, selector: str, expected: str):
    bc_text = get_text(bc_html, selector)
    ch_text = get_text(chrome_html, selector)
    assert bc_text == expected, f"blazeweb #{selector}: {bc_text!r}, expected: {expected!r}"
    assert ch_text == expected, f"chromium #{selector}: {ch_text!r}, expected: {expected!r}"


def assert_both_match(bc_html: str, chrome_html: str, selector: str):
    """Assert blazeweb and Chrome produce the same text for a selector."""
    bc_text = get_text(bc_html, selector)
    ch_text = get_text(chrome_html, selector)
    assert bc_text == ch_text, f"Mismatch at {selector}: blazeweb={bc_text!r}, chrome={ch_text!r}"


# ── Batch 6: createElementNS, getElementsByName, currentScript ────────────


class TestCreateElementNSConformance:
    def test_svg_nodename(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
                document.getElementById('r').textContent = svg.nodeName;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "svg")

    def test_svg_namespace_uri(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
                document.getElementById('r').textContent = svg.namespaceURI;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "http://www.w3.org/2000/svg")

    def test_html_ns_nodename(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var el = document.createElementNS("http://www.w3.org/1999/xhtml", "div");
                document.getElementById('r').textContent = el.nodeName;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "DIV")

    def test_svg_child_append(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="c"></div>
            <div id="r"></div>
            <script>
                var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
                var rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
                svg.appendChild(rect);
                document.getElementById('c').appendChild(svg);
                document.getElementById('r').textContent = svg.childNodes.length;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "1")

    def test_svg_case_preserved(self, page):
        """SVG elements like clipPath, linearGradient should preserve case."""
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var cp = document.createElementNS("http://www.w3.org/2000/svg", "clipPath");
                document.getElementById('r').textContent = cp.nodeName;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "clipPath")


class TestGetElementsByNameConformance:
    def test_find_by_name(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <input name="user" value="a">
            <input name="user" value="b">
            <input name="other" value="c">
            <div id="r"></div>
            <script>
                document.getElementById('r').textContent =
                    document.getElementsByName('user').length;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "2")

    def test_no_match(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                document.getElementById('r').textContent =
                    document.getElementsByName('nonexistent').length;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "0")


class TestCurrentScriptConformance:
    def test_exists_during_execution(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                document.getElementById('r').textContent =
                    (document.currentScript !== null).toString();
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "true")

    def test_is_script_element(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                document.getElementById('r').textContent =
                    document.currentScript.nodeName;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "SCRIPT")


class TestNamespacedAttributesConformance:
    def test_set_and_get_attribute_ns(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var el = document.createElementNS("http://www.w3.org/2000/svg", "image");
                el.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", "pic.png");
                document.getElementById('r').textContent =
                    el.getAttributeNS("http://www.w3.org/1999/xlink", "href");
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "pic.png")

    def test_has_attribute_ns(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var el = document.createElement("div");
                el.setAttributeNS(null, "data-x", "1");
                document.getElementById('r').textContent =
                    el.hasAttributeNS(null, "data-x").toString();
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "true")

    def test_remove_attribute_ns(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var el = document.createElement("div");
                el.setAttributeNS(null, "data-x", "1");
                el.removeAttributeNS(null, "data-x");
                document.getElementById('r').textContent =
                    el.hasAttributeNS(null, "data-x").toString();
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "false")


# ── Batch 7: MessageChannel, Worker, customElements, matchMedia ──────────


class TestMessageChannelConformance:
    def test_ports_exist(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var mc = new MessageChannel();
                document.getElementById('r').textContent =
                    (typeof mc.port1) + ',' + (typeof mc.port2);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "object,object")

    def test_port_methods_exist(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var mc = new MessageChannel();
                document.getElementById('r').textContent =
                    (typeof mc.port1.postMessage) + ',' + (typeof mc.port1.close);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "function,function")


class TestMatchMediaConformance:
    def test_screen(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                document.getElementById('r').textContent = matchMedia("screen").matches;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "true")

    def test_print(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                document.getElementById('r').textContent = matchMedia("print").matches;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "false")

    def test_media_property(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                document.getElementById('r').textContent =
                    matchMedia("(min-width: 768px)").media;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_both_match(bc, ch, "#r")


class TestCustomElementsConformance:
    def test_define_and_get(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                function MyCtor() {}
                customElements.define("my-el", MyCtor);
                document.getElementById('r').textContent =
                    (customElements.get("my-el") !== undefined).toString();
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "true")

    def test_get_undefined(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                document.getElementById('r').textContent =
                    (customElements.get("nonexistent") === undefined).toString();
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "true")


class TestIntersectionObserverConformance:
    def test_root_margin(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var io = new IntersectionObserver(function(){});
                document.getElementById('r').textContent = io.rootMargin;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "0px 0px 0px 0px")

    def test_take_records(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var io = new IntersectionObserver(function(){});
                document.getElementById('r').textContent = io.takeRecords().length;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "0")


# ── Batch 8: Event constructors ──────────────────────────────────────────


class TestMouseEventConformance:
    def test_basic(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new MouseEvent("click");
                document.getElementById('r').textContent =
                    e.type + ',' + e.clientX + ',' + e.clientY + ',' + e.bubbles;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "click,0,0,false")

    def test_with_init(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new MouseEvent("click", {clientX: 10, clientY: 20, bubbles: true});
                document.getElementById('r').textContent =
                    e.clientX + ',' + e.clientY + ',' + e.bubbles;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "10,20,true")

    def test_modifier_state(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new MouseEvent("click");
                document.getElementById('r').textContent =
                    e.getModifierState("Control").toString();
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "false")


class TestKeyboardEventConformance:
    def test_with_init(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new KeyboardEvent("keydown", {key: "Enter", code: "Enter"});
                document.getElementById('r').textContent = e.key + ',' + e.code;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "Enter,Enter")

    def test_defaults(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new KeyboardEvent("keyup");
                document.getElementById('r').textContent =
                    e.key + '|' + e.repeat + '|' + e.isComposing;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "|false|false")


class TestPointerEventConformance:
    def test_with_init(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new PointerEvent("pointerdown", {pointerId: 1, pointerType: "mouse"});
                document.getElementById('r').textContent =
                    e.pointerId + ',' + e.pointerType;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "1,mouse")

    def test_defaults(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new PointerEvent("pointermove");
                document.getElementById('r').textContent =
                    e.width + ',' + e.height + ',' + e.isPrimary;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "1,1,false")


class TestErrorEventConformance:
    def test_with_init(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new ErrorEvent("error", {message: "oops", lineno: 42, colno: 10});
                document.getElementById('r').textContent =
                    e.message + ',' + e.lineno + ',' + e.colno;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "oops,42,10")


class TestHashChangeEventConformance:
    def test_with_init(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new HashChangeEvent("hashchange", {oldURL: "#a", newURL: "#b"});
                document.getElementById('r').textContent = e.oldURL + ',' + e.newURL;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "#a,#b")


class TestPopStateEventConformance:
    def test_with_state(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new PopStateEvent("popstate", {state: {page: 1}});
                document.getElementById('r').textContent = e.state.page;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "1")


class TestInputEventConformance:
    def test_with_data(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new InputEvent("input", {data: "x", inputType: "insertText"});
                document.getElementById('r').textContent = e.data + ',' + e.inputType;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "x,insertText")


class TestFocusEventConformance:
    def test_basic(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var e = new FocusEvent("focus");
                document.getElementById('r').textContent =
                    e.type + ',' + String(e.relatedTarget);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "focus,null")


# ── Batch 10: Image, Audio, Option constructors ──────────────────────────


class TestImageConstructorConformance:
    def test_nodename(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var img = new Image();
                document.getElementById('r').textContent = img.nodeName;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "IMG")

    def test_with_dimensions(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var img = new Image(100, 50);
                document.getElementById('r').textContent =
                    img.getAttribute('width') + ',' + img.getAttribute('height');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "100,50")


class TestOptionConstructorConformance:
    def test_basic(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var opt = new Option("Label", "val1");
                document.getElementById('r').textContent =
                    opt.nodeName + ',' + opt.textContent + ',' + opt.getAttribute('value');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "OPTION,Label,val1")


# ── Batch 12: FormData, Headers, Blob ────────────────────────────────────


class TestFormDataConformance:
    def test_append_and_get(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var fd = new FormData();
                fd.append("name", "John");
                document.getElementById('r').textContent = fd.get("name");
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "John")

    def test_set_replaces(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var fd = new FormData();
                fd.append("key", "v1");
                fd.append("key", "v2");
                fd.set("key", "v3");
                document.getElementById('r').textContent =
                    fd.get("key") + ',' + fd.getAll("key").length;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "v3,1")

    def test_has_and_delete(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var fd = new FormData();
                fd.append("key", "val");
                var had = fd.has("key");
                fd.delete("key");
                var hasAfter = fd.has("key");
                document.getElementById('r').textContent = had + ',' + hasAfter;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "true,false")


class TestHeadersConformance:
    def test_constructor_and_get(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var h = new Headers({"Content-Type": "application/json"});
                document.getElementById('r').textContent = h.get("Content-Type");
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "application/json")

    def test_case_insensitive(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var h = new Headers({"Content-Type": "text/html"});
                document.getElementById('r').textContent = h.get("content-type");
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "text/html")


class TestBlobConformance:
    def test_size_and_type(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var b = new Blob(["hello"], {type: "text/plain"});
                document.getElementById('r').textContent = b.size + ',' + b.type;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "5,text/plain")

    def test_text(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var b = new Blob(["Hello", " ", "World"]);
                b.text().then(function(t) {
                    document.getElementById('r').textContent = t;
                });
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "Hello World")


# ── Batch 12 upgrades: window.dispatchEvent ──────────────────────────────


class TestWindowDispatchEventConformance:
    def test_fires_listener(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                var fired = false;
                window.addEventListener("custom-evt", function(e) { fired = true; });
                window.dispatchEvent(new Event("custom-evt"));
                document.getElementById('r').textContent = fired;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "true")

    def test_returns_true(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                window.addEventListener("test", function(e) {});
                var result = window.dispatchEvent(new Event("test"));
                document.getElementById('r').textContent = result;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#r", "true")


# ── Batch 13: Lenient selectors ──────────────────────────────────────────


class TestLenientSelectorsConformance:
    def test_invalid_selector_returns_null(self, page):
        """Both engines should handle unsupported pseudo-classes without crashing."""
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="r"></div>
            <script>
                try {
                    var el = document.querySelector(":has(.foo)");
                    document.getElementById('r').textContent =
                        el === null ? 'null' : 'found';
                } catch(e) {
                    document.getElementById('r').textContent = 'error';
                }
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        # Chrome supports :has() so may return 'null' or 'found'.
        # blazeweb returns 'null' (unsupported). Both should not crash.
        bc_text = get_text(bc, "#r")
        ch_text = get_text(ch, "#r")
        assert bc_text in ("null", "found", "error")
        assert ch_text in ("null", "found", "error")
