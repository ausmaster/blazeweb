"""E2E tests: HTML element APIs (media, forms, canvas, attributes)"""

from .conftest import text_of, render
import blazeweb

class TestLegacyDOM:
    def test_childnodes_item_method(self):
        """childNodes.item(0) should return the first child node."""
        html = render("""<html><body>
        <div id="parent"><span id="a">A</span><span id="b">B</span></div>
        <div id="result"></div>
        <script>
            var parent = document.getElementById('parent');
            var first = parent.childNodes.item(0);
            document.getElementById('result').textContent = first.textContent;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "A"

    def test_childnodes_item_out_of_bounds(self):
        """childNodes.item() with out-of-bounds index should return null."""
        html = render("""<html><body>
        <div id="parent"><span>A</span></div>
        <div id="result"></div>
        <script>
            var parent = document.getElementById('parent');
            var item = parent.childNodes.item(99);
            document.getElementById('result').textContent = String(item);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "null"

    def test_getelementsbytagname_item(self):
        """getElementsByTagName result should have item() method."""
        html = render("""<html><body>
        <ul><li>First</li><li>Second</li><li>Third</li></ul>
        <div id="result"></div>
        <script>
            var items = document.getElementsByTagName('li');
            document.getElementById('result').textContent = items.item(1).textContent;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "Second"

    def test_getelementsbyclassname_item(self):
        """getElementsByClassName result should have item() method."""
        html = render("""<html><body>
        <div class="x">Alpha</div><div class="x">Beta</div>
        <div id="result"></div>
        <script>
            var items = document.getElementsByClassName('x');
            document.getElementById('result').textContent = items.item(0).textContent;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "Alpha"

    def test_init_custom_event(self):
        """document.createEvent('CustomEvent').initCustomEvent() should work."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var evt = document.createEvent('CustomEvent');
            evt.initCustomEvent('myevent', true, true, {key: 'val'});
            document.getElementById('result').textContent = evt.type + ':' + evt.detail.key;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "myevent:val"

    def test_init_custom_event_dispatch(self):
        """initCustomEvent events should be dispatchable and receivable."""
        html = render("""<html><body>
        <div id="target"></div>
        <div id="result"></div>
        <script>
            var target = document.getElementById('target');
            target.addEventListener('myevent', function(e) {
                document.getElementById('result').textContent = e.detail.data;
            });
            var evt = document.createEvent('CustomEvent');
            evt.initCustomEvent('myevent', true, true, {data: 'hello'});
            target.dispatchEvent(evt);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "hello"


# ─── Streams API (Phase 2) ───────────────────────────────────────────────────


class TestCanvasAPI:
    def test_get_context_2d(self):
        """canvas.getContext('2d') should return non-null."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var c = document.createElement('canvas');
            var ctx = c.getContext('2d');
            document.getElementById('result').textContent = (ctx !== null).toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true"

    def test_get_context_webgl_null(self):
        """canvas.getContext('webgl') should return null (no GPU in SSR)."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var c = document.createElement('canvas');
            document.getElementById('result').textContent =
                (c.getContext('webgl') === null).toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true"

    def test_canvas_2d_methods_exist(self):
        """2D context should have standard draw methods."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var ctx = document.createElement('canvas').getContext('2d');
            var methods = ['fillRect','strokeRect','clearRect','beginPath',
                'fill','stroke','moveTo','lineTo','arc','fillText',
                'measureText','drawImage','save','restore'];
            var ok = methods.every(function(m) { return typeof ctx[m] === 'function'; });
            document.getElementById('result').textContent = ok.toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true"

    def test_path2d_constructor(self):
        """Path2D should be constructable."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = (typeof Path2D);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "function"

    def test_measure_text_returns_object(self):
        """ctx.measureText() should return object with width property."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var ctx = document.createElement('canvas').getContext('2d');
            var m = ctx.measureText('hello');
            document.getElementById('result').textContent =
                (typeof m.width === 'number').toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true"

    def test_canvas_width_height(self):
        """canvas.width and canvas.height should be readable."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var c = document.createElement('canvas');
            document.getElementById('result').textContent =
                (typeof c.width === 'number' && typeof c.height === 'number').toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true"


# ─── CSSOM (Phase 5) ─────────────────────────────────────────────────────────


class TestHTMLMediaElement:
    def test_video_play_returns_promise(self):
        html = render("""<html><body><div id="result"></div><script>
            var v = document.createElement("video");
            v.play().then(function() { document.getElementById('result').textContent = "ok"; });
        </script></body></html>""")
        assert text_of(html, "result") == "ok"

    def test_video_paused_default(self):
        html = render("""<html><body><div id="result"></div><script>
            var v = document.createElement("video");
            document.getElementById('result').textContent = v.paused.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_video_can_play_type(self):
        html = render("""<html><body><div id="result"></div><script>
            var v = document.createElement("video");
            document.getElementById('result').textContent =
                (typeof v.canPlayType === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_audio_methods(self):
        html = render("""<html><body><div id="result"></div><script>
            var a = document.createElement("audio");
            var methods = ["play","pause","load","canPlayType"];
            document.getElementById('result').textContent =
                methods.every(function(m){return typeof a[m]==="function"}).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_video_duration_nan(self):
        html = render("""<html><body><div id="result"></div><script>
            var v = document.createElement("video");
            document.getElementById('result').textContent = isNaN(v.duration).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_video_current_time_zero(self):
        html = render("""<html><body><div id="result"></div><script>
            var v = document.createElement("video");
            document.getElementById('result').textContent = v.currentTime.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "0"


class TestFormValidation:
    def test_input_check_validity(self):
        html = render("""<html><body><div id="result"></div><script>
            var i = document.createElement("input");
            document.getElementById('result').textContent = i.checkValidity().toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_input_validity_state(self):
        html = render("""<html><body><div id="result"></div><script>
            var i = document.createElement("input");
            document.getElementById('result').textContent = i.validity.valid.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_form_elements(self):
        html = render("""<html><body><div id="result"></div><script>
            var f = document.createElement("form");
            document.getElementById('result').textContent =
                (typeof f.elements === "object").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_form_submit_no_throw(self):
        html = render("""<html><body><div id="result"></div><script>
            var f = document.createElement("form");
            f.submit();
            document.getElementById('result').textContent = "ok";
        </script></body></html>""")
        assert text_of(html, "result") == "ok"

    def test_select_selected_index(self):
        html = render("""<html><body><div id="result"></div><script>
            var s = document.createElement("select");
            document.getElementById('result').textContent = s.selectedIndex.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "-1"

    def test_input_set_custom_validity(self):
        html = render("""<html><body><div id="result"></div><script>
            var i = document.createElement("input");
            i.setCustomValidity("err");
            document.getElementById('result').textContent = "ok";
        </script></body></html>""")
        assert text_of(html, "result") == "ok"


# ─── Round 2 Phase 6: History State Tracking ─────────────────────────────────


class TestHTMLElementAttributes:
    def test_element_title(self):
        html = render("""<html><body><div id="result"></div><script>
            var d = document.createElement("div");
            d.title = "tooltip";
            document.getElementById('result').textContent = d.title;
        </script></body></html>""")
        assert text_of(html, "result") == "tooltip"

    def test_element_title_reflects_attribute(self):
        html = render("""<html><body><div id="result"></div><script>
            var d = document.createElement("div");
            d.title = "tip";
            document.getElementById('result').textContent = d.getAttribute("title");
        </script></body></html>""")
        assert text_of(html, "result") == "tip"

    def test_element_lang(self):
        html = render("""<html><body><div id="result"></div><script>
            var d = document.createElement("div");
            d.lang = "en";
            document.getElementById('result').textContent = d.lang;
        </script></body></html>""")
        assert text_of(html, "result") == "en"

    def test_element_dir(self):
        html = render("""<html><body><div id="result"></div><script>
            var d = document.createElement("div");
            d.dir = "rtl";
            document.getElementById('result').textContent = d.dir;
        </script></body></html>""")
        assert text_of(html, "result") == "rtl"

    def test_element_draggable(self):
        html = render("""<html><body><div id="result"></div><script>
            var d = document.createElement("div");
            d.draggable = true;
            document.getElementById('result').textContent = d.draggable.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_element_content_editable(self):
        html = render("""<html><body><div id="result"></div><script>
            var d = document.createElement("div");
            d.contentEditable = "true";
            document.getElementById('result').textContent = d.contentEditable;
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_element_is_content_editable(self):
        html = render("""<html><body><div id="result"></div><script>
            var d = document.createElement("div");
            d.contentEditable = "true";
            document.getElementById('result').textContent = d.isContentEditable.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"



