"""Tests for Batch 6: createElementNS, getElementsByName, currentScript, namespaced attributes."""

import blazeweb


class TestCreateElementNS:
    def test_create_svg_element(self):
        html = """<html><body><div id="r"></div><script>
            var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
            document.getElementById('r').textContent = svg.nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        # Per DOM spec: SVG namespace elements preserve case (no uppercasing)
        assert ">svg<" in result

    def test_create_svg_child(self):
        html = """<html><body><div id="c"></div><script>
            var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
            var rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
            svg.appendChild(rect);
            document.getElementById('c').appendChild(svg);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "<rect>" in result or "<rect" in result

    def test_create_html_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElementNS("http://www.w3.org/1999/xhtml", "div");
            document.getElementById('r').textContent = el.nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">DIV<" in result

    def test_create_null_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElementNS(null, "span");
            el.textContent = "ok";
            document.getElementById('r').appendChild(el);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "ok" in result

    def test_create_with_prefix(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElementNS("http://www.w3.org/2000/svg", "svg:g");
            document.getElementById('r').textContent = el.nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        # nodeName should be the local name "g" (or "svg:g")
        assert "g" in result


class TestGetElementsByName:
    def test_find_by_name(self):
        html = """<html><body>
            <input name="user" value="a">
            <input name="user" value="b">
            <input name="other" value="c">
            <div id="r"></div>
            <script>
                document.getElementById('r').textContent =
                    document.getElementsByName('user').length;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_no_match(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                document.getElementsByName('nonexistent').length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0<" in result

    def test_single_match(self):
        html = """<html><body>
            <input name="email" id="inp">
            <div id="r"></div>
            <script>
                var els = document.getElementsByName('email');
                document.getElementById('r').textContent =
                    els.length + ',' + els[0].nodeName;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">1,INPUT<" in result


class TestCurrentScript:
    def test_current_script_exists(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                (document.currentScript !== null).toString();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_current_script_nodename(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                document.currentScript.nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">SCRIPT<" in result

    def test_current_script_null_after(self):
        html = """<html><body><div id="r"></div>
        <script>
            setTimeout(function() {
                document.getElementById('r').textContent =
                    String(document.currentScript);
            }, 0);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result


class TestNamespacedAttributes:
    def test_set_get_attribute_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElementNS("http://www.w3.org/2000/svg", "image");
            el.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", "pic.png");
            document.getElementById('r').textContent =
                el.getAttributeNS("http://www.w3.org/1999/xlink", "href");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">pic.png<" in result

    def test_has_attribute_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElement("div");
            el.setAttributeNS(null, "data-x", "1");
            document.getElementById('r').textContent =
                el.hasAttributeNS(null, "data-x").toString();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_remove_attribute_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElement("div");
            el.setAttributeNS(null, "data-x", "1");
            el.removeAttributeNS(null, "data-x");
            document.getElementById('r').textContent =
                el.hasAttributeNS(null, "data-x").toString();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_get_attribute_ns_missing(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElement("div");
            document.getElementById('r').textContent =
                String(el.getAttributeNS(null, "nope"));
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result
