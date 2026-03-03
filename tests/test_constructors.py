"""Tests for Batch 7+10: MessageChannel, Worker, customElements,
IntersectionObserver, matchMedia, Image, Audio, Option constructors."""

import blazeweb


class TestMessageChannel:
    def test_ports_exist(self):
        html = """<html><body><div id="r"></div><script>
            var mc = new MessageChannel();
            document.getElementById('r').textContent =
                (mc.port1 !== undefined) + ',' + (mc.port2 !== undefined);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,true<" in result

    def test_port_methods(self):
        html = """<html><body><div id="r"></div><script>
            var mc = new MessageChannel();
            document.getElementById('r').textContent =
                typeof mc.port1.postMessage + ',' +
                typeof mc.port1.close + ',' +
                typeof mc.port1.start;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">function,function,function<" in result

    def test_port_onmessage_settable(self):
        html = """<html><body><div id="r"></div><script>
            var mc = new MessageChannel();
            mc.port1.onmessage = function(e) {};
            document.getElementById('r').textContent = 'ok';
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestWorkerStub:
    def test_worker_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var w = new Worker("script.js");
            document.getElementById('r').textContent =
                typeof w.postMessage + ',' + typeof w.terminate;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">function,function<" in result

    def test_worker_onmessage_settable(self):
        html = """<html><body><div id="r"></div><script>
            var w = new Worker("script.js");
            w.onmessage = function(e) {};
            w.onerror = function(e) {};
            document.getElementById('r').textContent = 'ok';
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestCustomElements:
    def test_define_and_get(self):
        html = """<html><body><div id="r"></div><script>
            function MyCtor() {}
            window.customElements.define("my-el", MyCtor);
            var got = window.customElements.get("my-el");
            document.getElementById('r').textContent = (got !== undefined).toString();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_get_undefined(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                String(window.customElements.get("nonexistent") === undefined);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_when_defined(self):
        html = """<html><body><div id="r"></div><script>
            var p = window.customElements.whenDefined("x-foo");
            document.getElementById('r').textContent = typeof p.then;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">function<" in result


class TestIntersectionObserverUpgrade:
    def test_thresholds_array(self):
        html = """<html><body><div id="r"></div><script>
            var io = new IntersectionObserver(function(){}, {threshold: [0, 0.5, 1]});
            document.getElementById('r').textContent = io.thresholds.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">3<" in result

    def test_thresholds_single(self):
        html = """<html><body><div id="r"></div><script>
            var io = new IntersectionObserver(function(){}, {threshold: 0.5});
            document.getElementById('r').textContent = io.thresholds[0];
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0.5<" in result

    def test_root_margin(self):
        html = """<html><body><div id="r"></div><script>
            var io = new IntersectionObserver(function(){});
            document.getElementById('r').textContent = io.rootMargin;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0px 0px 0px 0px<" in result

    def test_take_records_empty(self):
        html = """<html><body><div id="r"></div><script>
            var io = new IntersectionObserver(function(){});
            document.getElementById('r').textContent = io.takeRecords().length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0<" in result


class TestMatchMediaUpgrade:
    def test_screen_matches(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = matchMedia("screen").matches;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_print_no_match(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = matchMedia("print").matches;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_min_width(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                matchMedia("(min-width: 1200px)").matches;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_max_width_narrow(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                matchMedia("(max-width: 500px)").matches;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_prefers_color_scheme_light(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                matchMedia("(prefers-color-scheme: light)").matches;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_media_property(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                matchMedia("(min-width: 768px)").media;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">(min-width: 768px)<" in result


class TestImageConstructor:
    def test_basic(self):
        html = """<html><body><div id="r"></div><script>
            var img = new Image();
            document.getElementById('r').textContent = img.nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">IMG<" in result

    def test_with_dimensions(self):
        html = """<html><body><div id="r"></div><script>
            var img = new Image(100, 50);
            document.getElementById('r').textContent =
                img.getAttribute('width') + ',' + img.getAttribute('height');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">100,50<" in result

    def test_appendable(self):
        html = """<html><body><div id="c"></div><script>
            var img = new Image();
            img.setAttribute('src', 'test.png');
            document.getElementById('c').appendChild(img);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert '<img' in result
        assert 'src="test.png"' in result


class TestAudioConstructor:
    def test_basic(self):
        html = """<html><body><div id="r"></div><script>
            var a = new Audio();
            document.getElementById('r').textContent = a.nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">AUDIO<" in result

    def test_with_src(self):
        html = """<html><body><div id="r"></div><script>
            var a = new Audio("song.mp3");
            document.getElementById('r').textContent = a.getAttribute('src');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">song.mp3<" in result


class TestOptionConstructor:
    def test_basic(self):
        html = """<html><body><div id="r"></div><script>
            var opt = new Option("Label", "val1");
            document.getElementById('r').textContent =
                opt.nodeName + ',' + opt.textContent + ',' + opt.getAttribute('value');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">OPTION,Label,val1<" in result

    def test_selected(self):
        html = """<html><body><select id="s"></select><div id="r"></div><script>
            var opt = new Option("L", "v", false, true);
            document.getElementById('s').appendChild(opt);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "selected" in result
