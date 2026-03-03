"""Tests for Batch 12+13: FormData, Headers, Blob, getComputedStyle,
window.dispatchEvent, lenient selectors."""

import blazeweb


class TestFormData:
    def test_append_and_get(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("name", "John");
            document.getElementById('r').textContent = fd.get("name");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">John<" in result

    def test_get_all(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("tags", "admin");
            fd.append("tags", "user");
            document.getElementById('r').textContent = fd.getAll("tags").length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_has_and_delete(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("key", "val");
            var had = fd.has("key");
            fd.delete("key");
            var hasAfter = fd.has("key");
            document.getElementById('r').textContent = had + ',' + hasAfter;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,false<" in result

    def test_set_replaces(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("key", "v1");
            fd.append("key", "v2");
            fd.set("key", "v3");
            document.getElementById('r').textContent =
                fd.get("key") + ',' + fd.getAll("key").length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">v3,1<" in result

    def test_entries_keys_values(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("a", "1");
            fd.append("b", "2");
            var keys = fd.keys();
            var vals = fd.values();
            var entries = fd.entries();
            document.getElementById('r').textContent =
                keys.length + ',' + vals.join(';') + ',' + entries.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2,1;2,2<" in result

    def test_foreach(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("x", "10");
            fd.append("y", "20");
            var pairs = [];
            fd.forEach(function(value, key) { pairs.push(key + '=' + value); });
            document.getElementById('r').textContent = pairs.join(';');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">x=10;y=20<" in result

    def test_get_missing_returns_null(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            document.getElementById('r').textContent = String(fd.get("nope"));
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result


class TestHeaders:
    def test_constructor_with_init(self):
        html = """<html><body><div id="r"></div><script>
            var h = new Headers({"Content-Type": "application/json"});
            document.getElementById('r').textContent = h.get("Content-Type");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">application/json<" in result

    def test_case_insensitive(self):
        html = """<html><body><div id="r"></div><script>
            var h = new Headers({"Content-Type": "text/html"});
            document.getElementById('r').textContent = h.get("content-type");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">text/html<" in result

    def test_append(self):
        html = """<html><body><div id="r"></div><script>
            var h = new Headers();
            h.set("Accept", "text/html");
            h.append("Accept", "application/json");
            document.getElementById('r').textContent = h.get("Accept");
        </script></body></html>"""
        result = blazeweb.render(html)
        # Append should combine values
        assert "text/html" in result
        assert "application/json" in result

    def test_has_delete(self):
        html = """<html><body><div id="r"></div><script>
            var h = new Headers({"X-Custom": "val"});
            var had = h.has("x-custom");
            h.delete("X-Custom");
            var hasAfter = h.has("X-Custom");
            document.getElementById('r').textContent = had + ',' + hasAfter;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,false<" in result

    def test_entries_keys_values(self):
        html = """<html><body><div id="r"></div><script>
            var h = new Headers({"A": "1", "B": "2"});
            document.getElementById('r').textContent =
                h.keys().length + ',' + h.values().length + ',' + h.entries().length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2,2,2<" in result


class TestBlob:
    def test_size_and_type(self):
        html = """<html><body><div id="r"></div><script>
            var b = new Blob(["hello"], {type: "text/plain"});
            document.getElementById('r').textContent = b.size + ',' + b.type;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">5,text/plain<" in result

    def test_text(self):
        html = """<html><body><div id="r"></div><script>
            var b = new Blob(["Hello", " ", "World"]);
            b.text().then(function(t) {
                document.getElementById('r').textContent = t;
            });
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">Hello World<" in result

    def test_empty(self):
        html = """<html><body><div id="r"></div><script>
            var b = new Blob();
            document.getElementById('r').textContent = b.size + ',' + b.type;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0,<" in result

    def test_array_buffer(self):
        html = """<html><body><div id="r"></div><script>
            var b = new Blob(["abc"]);
            b.arrayBuffer().then(function(ab) {
                document.getElementById('r').textContent = ab.byteLength;
            });
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">3<" in result


class TestGetComputedStyleUpgrade:
    def test_inline_styles_parsed(self):
        html = """<html><body>
            <div id="el" style="color: red; font-size: 14px"></div>
            <div id="r"></div>
            <script>
                var cs = getComputedStyle(document.getElementById('el'));
                document.getElementById('r').textContent =
                    cs.color + ',' + cs.fontSize;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">red,14px<" in result

    def test_default_empty(self):
        html = """<html><body>
            <div id="el"></div>
            <div id="r"></div>
            <script>
                var cs = getComputedStyle(document.getElementById('el'));
                document.getElementById('r').textContent =
                    cs.display + '|' + cs.visibility;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">|<" in result

    def test_kebab_and_camel(self):
        html = """<html><body>
            <div id="el" style="margin-top: 10px; background-color: blue"></div>
            <div id="r"></div>
            <script>
                var cs = getComputedStyle(document.getElementById('el'));
                document.getElementById('r').textContent =
                    cs.marginTop + ',' + cs.backgroundColor;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">10px,blue<" in result


class TestWindowDispatchEvent:
    def test_fires_listener(self):
        html = """<html><body><div id="r"></div><script>
            var fired = false;
            window.addEventListener("custom-evt", function(e) { fired = true; });
            window.dispatchEvent(new Event("custom-evt"));
            document.getElementById('r').textContent = fired;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_returns_true_when_not_prevented(self):
        html = """<html><body><div id="r"></div><script>
            window.addEventListener("test", function(e) {});
            var result = window.dispatchEvent(new Event("test"));
            document.getElementById('r').textContent = result;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result


class TestLenientSelectors:
    def test_invalid_selector_returns_null(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.querySelector(":has(.foo)");
            document.getElementById('r').textContent = String(el);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result

    def test_invalid_selector_all_returns_empty(self):
        html = """<html><body><div id="r"></div><script>
            var els = document.querySelectorAll(":has(.foo)");
            document.getElementById('r').textContent = els.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0<" in result

    def test_element_level_lenient(self):
        html = """<html><body><div id="c"><p>hi</p></div><div id="r"></div><script>
            var c = document.getElementById('c');
            var el = c.querySelector(":has(p)");
            var els = c.querySelectorAll(":has(p)");
            document.getElementById('r').textContent =
                String(el) + ',' + els.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null,0<" in result
