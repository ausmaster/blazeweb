"""Integration tests for blazeweb.render()."""

import blazeweb
import pytest


class TestRenderBasic:
    """Basic HTML rendering without scripts."""

    def test_plain_html(self):
        result = blazeweb.render("<html><body><p>Hello</p></body></html>")
        assert "<p>Hello</p>" in result

    def test_bytes_input(self):
        result = blazeweb.render(b"<html><body>OK</body></html>")
        assert "OK" in result

    def test_str_input(self):
        result = blazeweb.render("<html><body>OK</body></html>")
        assert "OK" in result

    def test_no_scripts_passthrough(self):
        html = "<html><head><title>Test</title></head><body><div>Content</div></body></html>"
        result = blazeweb.render(html)
        assert "<div>Content</div>" in result
        assert "<title>Test</title>" in result


class TestInlineScripts:
    """Inline <script> execution."""

    def test_noop_script(self):
        result = blazeweb.render("<html><body><script>var x = 1;</script><p>Hi</p></body></html>")
        assert "<p>Hi</p>" in result

    def test_set_text_content(self):
        html = """<html><body>
            <p id="target">old</p>
            <script>document.getElementById('target').textContent = 'new';</script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "new" in result

    def test_create_and_append_element(self):
        html = """<html><body>
            <div id="container"></div>
            <script>
                var el = document.createElement('span');
                el.textContent = 'dynamic';
                document.getElementById('container').appendChild(el);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "<span>dynamic</span>" in result

    def test_set_attribute(self):
        html = """<html><body>
            <div id="target"></div>
            <script>document.getElementById('target').setAttribute('class', 'active');</script>
        </body></html>"""
        result = blazeweb.render(html)
        assert 'class="active"' in result

    def test_remove_child(self):
        html = """<html><body>
            <div id="parent"><span id="child">remove me</span></div>
            <script>
                var parent = document.getElementById('parent');
                var child = document.getElementById('child');
                parent.removeChild(child);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "remove me" not in result

    def test_inner_html_set(self):
        html = """<html><body>
            <div id="target">old</div>
            <script>document.getElementById('target').innerHTML = '<b>bold</b>';</script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "<b>bold</b>" in result
        assert ">old<" not in result

    def test_multiple_scripts_shared_state(self):
        html = """<html><body>
            <script>var shared = 42;</script>
            <script>
                if (shared !== 42) throw new Error('not shared');
                var el = document.createElement('p');
                el.textContent = 'value=' + shared;
                document.body.appendChild(el);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "value=42" in result

    def test_script_error_non_fatal(self):
        html = """<html><body>
            <script>throw new Error('boom');</script>
            <script>
                var el = document.createElement('p');
                el.textContent = 'survived';
                document.body.appendChild(el);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "survived" in result

    def test_script_type_json_ignored(self):
        html = """<html><body>
            <script type="application/json">{"not": "executed"}</script>
            <p>still here</p>
        </body></html>"""
        result = blazeweb.render(html)
        assert "still here" in result

    def test_script_type_module_ignored(self):
        html = """<html><body>
            <script type="module">import x from './x';</script>
            <p>still here</p>
        </body></html>"""
        result = blazeweb.render(html)
        assert "still here" in result

    def test_console_log_no_crash(self):
        html = "<html><body><script>console.log('hello'); console.warn('w'); console.error('e');</script></body></html>"
        result = blazeweb.render(html)
        assert "<body>" in result

    def test_dom_traversal(self):
        html = """<html><body>
            <div id="parent"><span>child</span></div>
            <script>
                var div = document.getElementById('parent');
                var span = div.firstChild;
                span.textContent = 'traversed';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "traversed" in result


class TestExternalScripts:
    """External <script src="..."> fetching via HTTP server."""

    @pytest.fixture
    def httpserver(self):
        pytest.importorskip("pytest_httpserver")
        from pytest_httpserver import HTTPServer
        server = HTTPServer(host="127.0.0.1")
        server.start()
        yield server
        server.clear()
        if server.is_running():
            server.stop()

    def test_fetch_and_execute(self, httpserver):
        httpserver.expect_request("/app.js").respond_with_data(
            "document.getElementById('target').textContent = 'fetched';",
            content_type="application/javascript",
        )
        html = f"""<html><body>
            <p id="target">original</p>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "fetched" in result
        assert "original" not in result

    def test_fetch_multiple_scripts(self, httpserver):
        httpserver.expect_request("/a.js").respond_with_data(
            "var counter = 1;", content_type="application/javascript",
        )
        httpserver.expect_request("/b.js").respond_with_data(
            "counter += 10;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <div id="out"></div>
            <script src="{httpserver.url_for('/a.js')}"></script>
            <script src="{httpserver.url_for('/b.js')}"></script>
            <script>document.getElementById('out').textContent = 'total=' + counter;</script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "total=11" in result

    def test_mixed_inline_and_external(self, httpserver):
        httpserver.expect_request("/lib.js").respond_with_data(
            "var LIB_VERSION = '1.0';", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script>var prefix = 'v';</script>
            <script src="{httpserver.url_for('/lib.js')}"></script>
            <script>
                var el = document.createElement('span');
                el.textContent = prefix + LIB_VERSION;
                document.body.appendChild(el);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "v1.0" in result

    def test_fetch_relative_url_with_base(self, httpserver):
        httpserver.expect_request("/scripts/app.js").respond_with_data(
            "document.getElementById('x').textContent = 'relative';",
            content_type="application/javascript",
        )
        html = """<html><body>
            <p id="x">orig</p>
            <script src="scripts/app.js"></script>
        </body></html>"""
        result = blazeweb.render(html, base_url=httpserver.url_for("/"))
        assert "relative" in result

    def test_fetch_404_non_fatal(self, httpserver):
        httpserver.expect_request("/missing.js").respond_with_data(
            "Not Found", status=404,
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/missing.js')}"></script>
            <script>
                var el = document.createElement('p');
                el.textContent = 'survived';
                document.body.appendChild(el);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "survived" in result

    def test_fetch_relative_url_no_base_non_fatal(self):
        html = """<html><body>
            <script src="relative.js"></script>
            <script>
                var el = document.createElement('p');
                el.textContent = 'survived';
                document.body.appendChild(el);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "survived" in result


class TestClient:
    """Client class with per-instance script cache."""

    @pytest.fixture
    def httpserver(self):
        pytest.importorskip("pytest_httpserver")
        from pytest_httpserver import HTTPServer
        server = HTTPServer(host="127.0.0.1")
        server.start()
        yield server
        server.clear()
        if server.is_running():
            server.stop()

    def test_basic_render(self):
        client = blazeweb.Client()
        result = client.render("<html><body><p>hi</p></body></html>")
        assert "<p>hi</p>" in result

    def test_render_with_inline_script(self):
        client = blazeweb.Client()
        html = """<html><body>
            <div id="out"></div>
            <script>document.getElementById('out').textContent = 'ok';</script>
        </body></html>"""
        result = client.render(html)
        assert "ok" in result

    def test_render_str_input(self):
        """Client.render accepts str (auto-encoded to UTF-8 by PyO3)."""
        client = blazeweb.Client()
        result = client.render("<html><body>OK</body></html>")
        assert "OK" in result

    def test_cache_populated_by_external_scripts(self, httpserver):
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        client = blazeweb.Client()
        assert client.cache_size == 0
        client.render(html)
        assert client.cache_size == 1

    def test_cache_hit_no_refetch(self, httpserver):
        """Second render uses cache — server only expects one request."""
        httpserver.expect_ordered_request("/lib.js").respond_with_data(
            "document.getElementById('out').textContent = 'cached';",
            content_type="application/javascript",
        )
        base = httpserver.url_for("")
        html = f"""<html><body>
            <p id="out">empty</p>
            <script src="{base}/lib.js"></script>
        </body></html>"""
        client = blazeweb.Client()
        result1 = client.render(html)
        assert "cached" in result1

        # Stop server — second render must use cache
        httpserver.stop()
        result2 = client.render(html)
        assert "cached" in result2

    def test_cache_false_bypasses_cache(self, httpserver):
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        client = blazeweb.Client()
        client.render(html, cache=False)
        assert client.cache_size == 0  # nothing written

    def test_cache_write_false(self, httpserver):
        """cache_write=False reads cache but doesn't write new entries."""
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        client = blazeweb.Client()
        client.render(html, cache_write=False)
        assert client.cache_size == 0  # read was enabled, but nothing to read; write disabled

    def test_cache_read_false(self, httpserver):
        """cache_read=False skips cache lookup but saves fetched results."""
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        client = blazeweb.Client()
        client.render(html, cache_read=False)
        assert client.cache_size == 1  # written even though read was disabled

    def test_class_level_cache_toggle(self, httpserver):
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        client = blazeweb.Client()
        client.cache = False
        client.render(html)
        assert client.cache_size == 0  # cache disabled at class level

    def test_per_render_overrides_class(self, httpserver):
        """Per-render cache=True overrides class-level cache=False."""
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        client = blazeweb.Client(cache=False)
        assert client.cache is False
        client.render(html, cache=True)
        assert client.cache_size == 1  # per-render override took effect

    def test_clear_cache(self, httpserver):
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        client = blazeweb.Client()
        client.render(html)
        assert client.cache_size == 1
        client.clear_cache()
        assert client.cache_size == 0

    def test_separate_clients_separate_caches(self, httpserver):
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        c1 = blazeweb.Client()
        c2 = blazeweb.Client()
        c1.render(html)
        assert c1.cache_size == 1
        assert c2.cache_size == 0  # c2 has its own empty cache

    def test_constructor_defaults(self):
        client = blazeweb.Client()
        assert client.cache is True
        assert client.cache_read is True
        assert client.cache_write is True

    def test_constructor_kwargs(self):
        client = blazeweb.Client(cache=False, cache_read=False, cache_write=False)
        assert client.cache is False
        assert client.cache_read is False
        assert client.cache_write is False


# ── Batch 1: querySelector / querySelectorAll ───────────────────────────────


class TestQuerySelector:
    """CSS selector matching via the selectors crate."""

    def test_query_selector_by_class(self):
        html = """<html><body>
            <p class="target">found</p>
            <div id="result"></div>
            <script>
                var el = document.querySelector('.target');
                document.getElementById('result').textContent = el ? el.textContent : 'null';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "found" in result

    def test_query_selector_by_id(self):
        html = """<html><body>
            <p id="myp">hello</p>
            <div id="result"></div>
            <script>
                var el = document.querySelector('#myp');
                document.getElementById('result').textContent = el.textContent;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "hello" in result

    def test_query_selector_by_tag(self):
        html = """<html><body>
            <span>first span</span>
            <div id="result"></div>
            <script>
                var el = document.querySelector('span');
                document.getElementById('result').textContent = el.textContent;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "first span" in result

    def test_query_selector_compound(self):
        html = """<html><body>
            <div class="a">no</div>
            <div class="a b">yes</div>
            <div id="result"></div>
            <script>
                var el = document.querySelector('div.a.b');
                document.getElementById('result').textContent = el.textContent;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "yes" in result

    def test_query_selector_attribute(self):
        html = """<html><body>
            <input type="text" value="hello">
            <input type="checkbox" value="nope">
            <div id="result"></div>
            <script>
                var el = document.querySelector('input[type="text"]');
                document.getElementById('result').textContent = el.getAttribute('value');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "hello" in result

    def test_query_selector_descendant(self):
        html = """<html><body>
            <div class="outer"><span class="inner">deep</span></div>
            <div id="result"></div>
            <script>
                var el = document.querySelector('.outer .inner');
                document.getElementById('result').textContent = el.textContent;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "deep" in result

    def test_query_selector_child_combinator(self):
        html = """<html><body>
            <div class="parent"><span>direct</span><div><span>nested</span></div></div>
            <div id="result"></div>
            <script>
                var el = document.querySelector('.parent > span');
                document.getElementById('result').textContent = el.textContent;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "direct" in result

    def test_query_selector_no_match(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var el = document.querySelector('.nonexistent');
                document.getElementById('result').textContent = el === null ? 'null' : 'found';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "null" in result

    def test_query_selector_all_count(self):
        html = """<html><body>
            <p class="item">a</p><p class="item">b</p><p class="item">c</p>
            <div id="result"></div>
            <script>
                var els = document.querySelectorAll('.item');
                document.getElementById('result').textContent = els.length.toString();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">3<" in result

    def test_query_selector_all_iterate(self):
        html = """<html><body>
            <li class="item">a</li><li class="item">b</li><li class="item">c</li>
            <div id="result"></div>
            <script>
                var els = document.querySelectorAll('.item');
                var texts = [];
                for (var i = 0; i < els.length; i++) texts.push(els[i].textContent);
                document.getElementById('result').textContent = texts.join(',');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "a,b,c" in result

    def test_element_query_selector(self):
        html = """<html><body>
            <div id="scope"><span class="a">inside</span></div>
            <span class="a">outside</span>
            <div id="result"></div>
            <script>
                var scope = document.getElementById('scope');
                var el = scope.querySelector('.a');
                document.getElementById('result').textContent = el.textContent;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "inside" in result

    def test_element_query_selector_all(self):
        html = """<html><body>
            <div id="scope"><span class="a">1</span><span class="a">2</span></div>
            <span class="a">3</span>
            <div id="result"></div>
            <script>
                var scope = document.getElementById('scope');
                var els = scope.querySelectorAll('.a');
                document.getElementById('result').textContent = els.length.toString();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_matches(self):
        html = """<html><body>
            <div id="target" class="foo bar"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var parts = [
                    el.matches('.foo'),
                    el.matches('.baz'),
                    el.matches('div.bar'),
                    el.matches('#target')
                ];
                document.getElementById('result').textContent = parts.join(',');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "true,false,true,true" in result

    def test_closest(self):
        html = """<html><body>
            <div class="outer" id="o"><div class="inner"><span id="target">text</span></div></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var c1 = el.closest('.outer');
                var c2 = el.closest('.nonexistent');
                document.getElementById('result').textContent = (c1 ? c1.id : 'null') + ',' + (c2 === null ? 'null' : 'found');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "o,null" in result

    def test_closest_self_match(self):
        html = """<html><body>
            <div id="target" class="self"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var c = el.closest('.self');
                document.getElementById('result').textContent = c ? c.id : 'null';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "target" in result

    def test_nth_child_selector(self):
        html = """<html><body>
            <ul><li>1</li><li>2</li><li>3</li></ul>
            <div id="result"></div>
            <script>
                var el = document.querySelector('li:nth-child(2)');
                document.getElementById('result').textContent = el ? el.textContent : 'null';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_not_pseudo_class(self):
        html = """<html><body>
            <div class="a">A</div><div class="b">B</div>
            <div id="result"></div>
            <script>
                var els = document.querySelectorAll('div:not(.a):not(#result)');
                var texts = [];
                for (var i = 0; i < els.length; i++) texts.push(els[i].textContent);
                document.getElementById('result').textContent = texts.join(',');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "B" in result

    def test_invalid_selector_no_crash(self):
        html = """<html><body>
            <div id="result">ok</div>
            <script>
                try { document.querySelector('[[[invalid'); } catch(e) {}
                document.getElementById('result').textContent = 'survived';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "survived" in result


# ── Batch 2: Timer APIs ─────────────────────────────────────────────────────


class TestTimers:
    """setTimeout, setInterval, clearTimeout, clearInterval, requestAnimationFrame."""

    def test_set_timeout_fires(self):
        html = """<html><body>
            <div id="result">before</div>
            <script>
                setTimeout(function() {
                    document.getElementById('result').textContent = 'after';
                }, 0);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "after" in result

    def test_set_timeout_with_delay(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var order = [];
                setTimeout(function() { order.push('b'); }, 100);
                setTimeout(function() { order.push('a'); }, 0);
                setTimeout(function() {
                    order.push('c');
                    document.getElementById('result').textContent = order.join(',');
                }, 200);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "a,b,c" in result

    def test_clear_timeout(self):
        html = """<html><body>
            <div id="result">unchanged</div>
            <script>
                var id = setTimeout(function() {
                    document.getElementById('result').textContent = 'changed';
                }, 0);
                clearTimeout(id);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "unchanged" in result

    def test_set_interval_fires_once(self):
        """In SSR mode, intervals fire exactly once."""
        html = """<html><body>
            <div id="result">0</div>
            <script>
                var count = 0;
                setInterval(function() {
                    count++;
                    document.getElementById('result').textContent = count.toString();
                }, 10);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">1<" in result

    def test_clear_interval(self):
        html = """<html><body>
            <div id="result">unchanged</div>
            <script>
                var id = setInterval(function() {
                    document.getElementById('result').textContent = 'changed';
                }, 0);
                clearInterval(id);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "unchanged" in result

    def test_request_animation_frame(self):
        html = """<html><body>
            <div id="result">before</div>
            <script>
                requestAnimationFrame(function() {
                    document.getElementById('result').textContent = 'after';
                });
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "after" in result

    def test_cancel_animation_frame(self):
        html = """<html><body>
            <div id="result">unchanged</div>
            <script>
                var id = requestAnimationFrame(function() {
                    document.getElementById('result').textContent = 'changed';
                });
                cancelAnimationFrame(id);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "unchanged" in result

    def test_set_timeout_returns_id(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var id = setTimeout(function(){}, 0);
                document.getElementById('result').textContent = typeof id === 'number' ? 'ok' : 'fail';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "ok" in result

    def test_timer_scheduled_by_timer(self):
        """Timer callbacks can schedule new timers (re-drain)."""
        html = """<html><body>
            <div id="result">0</div>
            <script>
                setTimeout(function() {
                    setTimeout(function() {
                        document.getElementById('result').textContent = '2';
                    }, 0);
                }, 0);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result


# ── Batch 3: Event System ───────────────────────────────────────────────────


class TestEvents:
    """addEventListener, removeEventListener, DOMContentLoaded."""

    def test_dom_content_loaded_fires(self):
        html = """<html><body>
            <div id="result">pending</div>
            <script>
                document.addEventListener('DOMContentLoaded', function() {
                    document.getElementById('result').textContent = 'loaded';
                });
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "loaded" in result

    def test_dom_content_loaded_on_window(self):
        html = """<html><body>
            <div id="result">pending</div>
            <script>
                window.addEventListener('DOMContentLoaded', function() {
                    document.getElementById('result').textContent = 'window-loaded';
                });
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "window-loaded" in result

    def test_dom_content_loaded_multiple_listeners(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var parts = [];
                document.addEventListener('DOMContentLoaded', function() { parts.push('a'); });
                document.addEventListener('DOMContentLoaded', function() { parts.push('b'); });
                document.addEventListener('DOMContentLoaded', function() {
                    parts.push('c');
                    document.getElementById('result').textContent = parts.join(',');
                });
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "a,b,c" in result

    def test_remove_event_listener(self):
        html = """<html><body>
            <div id="result">not called</div>
            <script>
                function handler() {
                    document.getElementById('result').textContent = 'called';
                }
                document.addEventListener('DOMContentLoaded', handler);
                document.removeEventListener('DOMContentLoaded', handler);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "not called" in result

    def test_add_event_listener_no_crash(self):
        html = """<html><body>
            <div id="target"></div>
            <div id="result">ok</div>
            <script>
                var el = document.getElementById('target');
                el.addEventListener('click', function() {});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "ok" in result

    def test_dcl_fires_after_scripts_before_timers(self):
        """DOMContentLoaded should fire after scripts, and its timers should be drained."""
        html = """<html><body>
            <div id="result"></div>
            <script>
                var order = [];
                order.push('script');
                document.addEventListener('DOMContentLoaded', function() {
                    order.push('dcl');
                    setTimeout(function() {
                        order.push('timer');
                        document.getElementById('result').textContent = order.join(',');
                    }, 0);
                });
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "script,dcl,timer" in result


# ── Batch 4: classList + style ───────────────────────────────────────────────


class TestClassList:
    """classList API on elements."""

    def test_classlist_add(self):
        html = """<html><body>
            <div id="target"></div>
            <script>
                document.getElementById('target').classList.add('foo', 'bar');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert 'class="foo bar"' in result

    def test_classlist_remove(self):
        html = """<html><body>
            <div id="target" class="foo bar baz"></div>
            <script>
                document.getElementById('target').classList.remove('bar');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert 'class="foo baz"' in result
        assert "bar" not in result.split('class="')[1].split('"')[0]

    def test_classlist_toggle_on(self):
        html = """<html><body>
            <div id="target" class="foo"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var added = el.classList.toggle('bar');
                document.getElementById('result').textContent = added.toString();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert 'class="foo bar"' in result
        assert "true" in result

    def test_classlist_toggle_off(self):
        html = """<html><body>
            <div id="target" class="foo bar"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var removed = el.classList.toggle('bar');
                document.getElementById('result').textContent = removed.toString();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert 'class="foo"' in result
        assert "false" in result

    def test_classlist_toggle_force(self):
        html = """<html><body>
            <div id="target" class="foo bar"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                el.classList.toggle('bar', true);
                document.getElementById('result').textContent = el.className;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "foo bar" in result

    def test_classlist_contains(self):
        html = """<html><body>
            <div id="target" class="foo bar"></div>
            <div id="result"></div>
            <script>
                var cl = document.getElementById('target').classList;
                document.getElementById('result').textContent =
                    cl.contains('foo') + ',' + cl.contains('baz');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "true,false" in result

    def test_classlist_replace(self):
        html = """<html><body>
            <div id="target" class="foo bar"></div>
            <script>
                document.getElementById('target').classList.replace('foo', 'baz');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert 'class="baz bar"' in result

    def test_classlist_item(self):
        html = """<html><body>
            <div id="target" class="foo bar baz"></div>
            <div id="result"></div>
            <script>
                var cl = document.getElementById('target').classList;
                document.getElementById('result').textContent =
                    cl.item(0) + ',' + cl.item(1) + ',' + cl.item(2);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "foo,bar,baz" in result

    def test_classlist_no_duplicates(self):
        html = """<html><body>
            <div id="target" class="foo"></div>
            <script>
                var el = document.getElementById('target');
                el.classList.add('foo');
                el.classList.add('foo');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert 'class="foo"' in result


class TestStyle:
    """element.style property — CSSStyleDeclaration-like."""

    def test_style_set(self):
        html = """<html><body>
            <div id="target"></div>
            <script>
                document.getElementById('target').style.display = 'none';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert 'style="display: none;"' in result or 'style="display:none"' in result

    def test_style_read_existing(self):
        html = """<html><body>
            <div id="target" style="color: red; font-size: 14px"></div>
            <div id="result"></div>
            <script>
                var s = document.getElementById('target').style;
                document.getElementById('result').textContent = s.color + '|' + s.fontSize;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "red|14px" in result

    def test_style_get_property_value(self):
        html = """<html><body>
            <div id="target" style="background-color: blue"></div>
            <div id="result"></div>
            <script>
                var s = document.getElementById('target').style;
                document.getElementById('result').textContent = s.getPropertyValue('background-color');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "blue" in result

    def test_style_set_property(self):
        html = """<html><body>
            <div id="target"></div>
            <script>
                document.getElementById('target').style.setProperty('margin-top', '10px');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "margin-top" in result
        assert "10px" in result

    def test_style_remove_property(self):
        html = """<html><body>
            <div id="target" style="color: red; display: block"></div>
            <script>
                document.getElementById('target').style.removeProperty('color');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "display" in result
        # color should be removed
        style_attr = result.split('style="')[1].split('"')[0] if 'style="' in result else ""
        assert "color" not in style_attr

    def test_style_css_text(self):
        html = """<html><body>
            <div id="target" style="color: red; font-size: 14px"></div>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent =
                    document.getElementById('target').style.cssText;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "color" in result
        assert "font-size" in result

    def test_style_length(self):
        html = """<html><body>
            <div id="target" style="color: red; font-size: 14px; display: block"></div>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent =
                    document.getElementById('target').style.length.toString();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">3<" in result

    def test_style_camel_case_access(self):
        html = """<html><body>
            <div id="target" style="background-color: green"></div>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent =
                    document.getElementById('target').style.backgroundColor;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "green" in result


# ── Batch 5: Window/Document stubs ──────────────────────────────────────────


class TestWindowStubs:
    """window.location, navigator, localStorage, sessionStorage."""

    def test_location_href(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent =
                    typeof location.href === 'string' ? 'ok' : 'fail';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "ok" in result

    def test_location_with_base_url(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = location.hostname;
            </script>
        </body></html>"""
        result = blazeweb.render(html, base_url="https://example.com/page")
        assert "example.com" in result

    def test_location_protocol(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = location.protocol;
            </script>
        </body></html>"""
        result = blazeweb.render(html, base_url="https://example.com")
        assert "https:" in result

    def test_navigator_user_agent(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent =
                    typeof navigator.userAgent === 'string' ? 'ok' : 'fail';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "ok" in result

    def test_navigator_language(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = navigator.language;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "en" in result

    def test_local_storage_roundtrip(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                localStorage.setItem('key', 'value');
                document.getElementById('result').textContent = localStorage.getItem('key');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "value" in result

    def test_local_storage_get_missing(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var val = localStorage.getItem('nonexistent');
                document.getElementById('result').textContent = val === null ? 'null' : val;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "null" in result

    def test_local_storage_remove(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                localStorage.setItem('key', 'value');
                localStorage.removeItem('key');
                var val = localStorage.getItem('key');
                document.getElementById('result').textContent = val === null ? 'removed' : val;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "removed" in result

    def test_local_storage_clear(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                localStorage.setItem('a', '1');
                localStorage.setItem('b', '2');
                localStorage.clear();
                var val = localStorage.getItem('a');
                document.getElementById('result').textContent = val === null ? 'cleared' : val;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "cleared" in result

    def test_session_storage_roundtrip(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                sessionStorage.setItem('key', 'value');
                document.getElementById('result').textContent = sessionStorage.getItem('key');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "value" in result

    def test_document_ready_state(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = document.readyState;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "complete" in result

    def test_document_cookie(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                document.cookie = 'a=1';
                document.getElementById('result').textContent = document.cookie;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "a=1" in result

    def test_get_computed_style_no_crash(self):
        html = """<html><body>
            <div id="target" style="color: red"></div>
            <div id="result"></div>
            <script>
                var cs = getComputedStyle(document.getElementById('target'));
                document.getElementById('result').textContent =
                    typeof cs.getPropertyValue === 'function' ? 'ok' : 'fail';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "ok" in result

    def test_atob_btoa(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var encoded = btoa('hello');
                var decoded = atob(encoded);
                document.getElementById('result').textContent = decoded;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "hello" in result

    def test_event_constructor(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var e = new Event('test', { bubbles: true });
                document.getElementById('result').textContent = e.type + ',' + e.bubbles;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "test,true" in result

    def test_custom_event_constructor(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var e = new CustomEvent('myevent', { detail: { foo: 'bar' } });
                document.getElementById('result').textContent = e.type + ',' + e.detail.foo;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "myevent,bar" in result

    def test_mutation_observer_no_crash(self):
        html = """<html><body>
            <div id="result">ok</div>
            <script>
                var obs = new MutationObserver(function() {});
                obs.observe(document.body, { childList: true });
                obs.disconnect();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "ok" in result

    def test_match_media_no_crash(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var mq = matchMedia('(min-width: 768px)');
                document.getElementById('result').textContent =
                    typeof mq.matches + ',' + typeof mq.addListener;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "boolean" in result

    def test_queue_microtask(self):
        html = """<html><body>
            <div id="result">before</div>
            <script>
                queueMicrotask(function() {
                    document.getElementById('result').textContent = 'after';
                });
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "after" in result

    def test_url_constructor(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var u = new URL('https://example.com/path?q=1#hash');
                document.getElementById('result').textContent =
                    u.hostname + ',' + u.pathname + ',' + u.search;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "example.com,/path,?q=1" in result

    def test_performance_now(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent =
                    typeof performance.now() === 'number' ? 'ok' : 'fail';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "ok" in result


# ── Batch 6: Misc DOM Methods ───────────────────────────────────────────────


class TestMiscDOM:
    """replaceChild, append, prepend, before, after, insertAdjacentHTML, etc."""

    def test_replace_child(self):
        html = """<html><body>
            <div id="parent"><span id="old">old</span></div>
            <script>
                var parent = document.getElementById('parent');
                var old = document.getElementById('old');
                var newEl = document.createElement('em');
                newEl.textContent = 'new';
                parent.replaceChild(newEl, old);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "<em>new</em>" in result
        assert "old" not in result.split("<em>")[0]  # old span should be gone

    def test_append(self):
        html = """<html><body>
            <div id="target"><span>existing</span></div>
            <script>
                var el = document.getElementById('target');
                var newEl = document.createElement('b');
                newEl.textContent = 'appended';
                el.append(newEl);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "<span>existing</span><b>appended</b>" in result

    def test_prepend(self):
        html = """<html><body>
            <div id="target"><span>existing</span></div>
            <script>
                var el = document.getElementById('target');
                var newEl = document.createElement('b');
                newEl.textContent = 'prepended';
                el.prepend(newEl);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "<b>prepended</b><span>existing</span>" in result

    def test_before(self):
        html = """<html><body>
            <div id="container"><span id="target">target</span></div>
            <script>
                var target = document.getElementById('target');
                var newEl = document.createElement('b');
                newEl.textContent = 'before';
                target.before(newEl);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "<b>before</b><span" in result

    def test_after(self):
        html = """<html><body>
            <div id="container"><span id="target">target</span></div>
            <script>
                var target = document.getElementById('target');
                var newEl = document.createElement('b');
                newEl.textContent = 'after';
                target.after(newEl);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "</span><b>after</b>" in result

    def test_replace_with(self):
        html = """<html><body>
            <div id="container"><span id="target">old</span></div>
            <script>
                var target = document.getElementById('target');
                var newEl = document.createElement('em');
                newEl.textContent = 'replacement';
                target.replaceWith(newEl);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "<em>replacement</em>" in result
        assert "<span" not in result.split("<div")[1].split("</div>")[0]

    def test_insert_adjacent_html_beforeend(self):
        html = """<html><body>
            <div id="target">existing</div>
            <script>
                document.getElementById('target').insertAdjacentHTML('beforeend', '<b>added</b>');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "existing<b>added</b>" in result

    def test_insert_adjacent_html_afterbegin(self):
        html = """<html><body>
            <div id="target">existing</div>
            <script>
                document.getElementById('target').insertAdjacentHTML('afterbegin', '<b>first</b>');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "<b>first</b>existing" in result

    def test_insert_adjacent_html_beforebegin(self):
        html = """<html><body>
            <div id="container"><span id="target">target</span></div>
            <script>
                document.getElementById('target').insertAdjacentHTML('beforebegin', '<b>before</b>');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "<b>before</b><span" in result

    def test_insert_adjacent_html_afterend(self):
        html = """<html><body>
            <div id="container"><span id="target">target</span></div>
            <script>
                document.getElementById('target').insertAdjacentHTML('afterend', '<b>after</b>');
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "</span><b>after</b>" in result

    def test_get_elements_by_tag_name_on_element(self):
        html = """<html><body>
            <div id="scope"><span>1</span><span>2</span></div>
            <span>3</span>
            <div id="result"></div>
            <script>
                var scope = document.getElementById('scope');
                var els = scope.getElementsByTagName('span');
                document.getElementById('result').textContent = els.length.toString();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_get_elements_by_class_name_on_element(self):
        html = """<html><body>
            <div id="scope"><span class="a">1</span><span class="a">2</span></div>
            <span class="a">3</span>
            <div id="result"></div>
            <script>
                var scope = document.getElementById('scope');
                var els = scope.getElementsByClassName('a');
                document.getElementById('result').textContent = els.length.toString();
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_get_bounding_client_rect(self):
        html = """<html><body>
            <div id="target">box</div>
            <div id="result"></div>
            <script>
                var rect = document.getElementById('target').getBoundingClientRect();
                document.getElementById('result').textContent =
                    typeof rect.width + ',' + typeof rect.height + ',' +
                    typeof rect.top + ',' + typeof rect.left;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "number,number,number,number" in result

    def test_offset_dimensions(self):
        html = """<html><body>
            <div id="target">box</div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                document.getElementById('result').textContent =
                    typeof el.offsetWidth + ',' + typeof el.offsetHeight;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "number,number" in result

    def test_dataset_read(self):
        html = """<html><body>
            <div id="target" data-user-id="42" data-name="test"></div>
            <div id="result"></div>
            <script>
                var ds = document.getElementById('target').dataset;
                document.getElementById('result').textContent = ds.userId + ',' + ds.name;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "42,test" in result

    def test_dataset_write(self):
        html = """<html><body>
            <div id="target"></div>
            <script>
                document.getElementById('target').dataset.userId = '99';
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert 'data-user-id="99"' in result

    def test_is_equal_node(self):
        html = """<html><body>
            <div id="result"></div>
            <script>
                var a = document.createElement('span');
                a.textContent = 'test';
                var b = document.createElement('span');
                b.textContent = 'test';
                var c = document.createElement('span');
                c.textContent = 'other';
                document.getElementById('result').textContent =
                    a.isEqualNode(b) + ',' + a.isEqualNode(c);
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "true,false" in result

    def test_html_element_constructors_no_crash(self):
        """Common constructor stubs shouldn't crash."""
        html = """<html><body>
            <div id="result">ok</div>
            <script>
                // These should all be defined (as constructor stubs)
                var names = ['HTMLElement', 'HTMLDivElement', 'HTMLSpanElement',
                             'HTMLInputElement', 'HTMLFormElement', 'HTMLAnchorElement',
                             'Text', 'Comment', 'DocumentFragment', 'NodeList'];
                for (var i = 0; i < names.length; i++) {
                    if (typeof window[names[i]] === 'undefined') {
                        document.getElementById('result').textContent = 'missing: ' + names[i];
                        break;
                    }
                }
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "ok" in result


class TestFetch:
    """fetch() API integration tests using a local HTTP server."""

    @pytest.fixture(autouse=True)
    def _server(self):
        """Start a local HTTP server for every test in this class."""
        from pytest_httpserver import HTTPServer

        self.server = HTTPServer(host="127.0.0.1")
        self.server.start()
        yield
        self.server.clear()
        if self.server.is_running():
            self.server.stop()

    @property
    def url(self):
        # url_for("/") gives "http://host:port/", strip trailing slash for base
        return self.server.url_for("").rstrip("/")

    def test_fetch_basic_get(self):
        """Basic GET fetch writes response text to DOM."""
        self.server.expect_request("/data").respond_with_data(
            "hello from server", content_type="text/plain"
        )
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/data')
                    .then(function(r) {{ return r.text(); }})
                    .then(function(text) {{
                        document.getElementById('result').textContent = text;
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "hello from server" in result

    def test_fetch_json(self):
        """fetch().then(r => r.json()) parses JSON response."""
        self.server.expect_request("/api").respond_with_json(
            {"name": "blazeweb", "version": 1}
        )
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/api')
                    .then(function(r) {{ return r.json(); }})
                    .then(function(data) {{
                        document.getElementById('result').textContent =
                            data.name + '-' + data.version;
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "blazeweb-1" in result

    def test_fetch_response_properties(self):
        """Response object has correct status, ok, statusText, url."""
        self.server.expect_request("/props").respond_with_data(
            "ok", status=200, content_type="text/plain"
        )
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/props')
                    .then(function(r) {{
                        document.getElementById('result').textContent =
                            r.status + ',' + r.ok + ',' + r.statusText + ',' + r.type;
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "200,true,OK,basic" in result

    def test_fetch_404_status(self):
        """404 response has ok=false."""
        self.server.expect_request("/missing").respond_with_data(
            "not found", status=404
        )
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/missing')
                    .then(function(r) {{
                        document.getElementById('result').textContent =
                            r.status + ',' + r.ok;
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "404,false" in result

    def test_fetch_post_with_body(self):
        """POST fetch sends method, headers, and body."""
        self.server.expect_request(
            "/submit", method="POST"
        ).respond_with_data("accepted", content_type="text/plain")
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/submit', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ key: 'value' }})
                }})
                .then(function(r) {{ return r.text(); }})
                .then(function(text) {{
                    document.getElementById('result').textContent = text;
                }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "accepted" in result

    def test_fetch_response_headers(self):
        """Response headers are accessible via headers.get()."""
        self.server.expect_request("/headers").respond_with_data(
            "ok",
            content_type="text/plain",
            headers={"X-Custom-Header": "test-value"},
        )
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/headers')
                    .then(function(r) {{
                        var custom = r.headers.get('x-custom-header');
                        var missing = r.headers.get('x-nonexistent');
                        var has = r.headers.has('x-custom-header');
                        document.getElementById('result').textContent =
                            custom + ',' + missing + ',' + has;
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "test-value,null,true" in result

    def test_fetch_concurrent_promise_all(self):
        """Promise.all with multiple concurrent fetches."""
        self.server.expect_request("/a").respond_with_data("alpha")
        self.server.expect_request("/b").respond_with_data("beta")
        self.server.expect_request("/c").respond_with_data("gamma")
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                Promise.all([
                    fetch('{self.url}/a').then(function(r) {{ return r.text(); }}),
                    fetch('{self.url}/b').then(function(r) {{ return r.text(); }}),
                    fetch('{self.url}/c').then(function(r) {{ return r.text(); }})
                ]).then(function(results) {{
                    document.getElementById('result').textContent = results.join(',');
                }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "alpha" in result
        assert "beta" in result
        assert "gamma" in result

    def test_fetch_chained(self):
        """Chained fetches: first fetch drives second fetch."""
        self.server.expect_request("/first").respond_with_json(
            {"next": "/second"}
        )
        self.server.expect_request("/second").respond_with_data("final-data")
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/first')
                    .then(function(r) {{ return r.json(); }})
                    .then(function(data) {{
                        return fetch('{self.url}' + data.next);
                    }})
                    .then(function(r) {{ return r.text(); }})
                    .then(function(text) {{
                        document.getElementById('result').textContent = text;
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "final-data" in result

    def test_fetch_error_rejection(self):
        """Fetch to unreachable server triggers catch handler."""
        html = """<html><body>
            <div id="result">pending</div>
            <script>
                fetch('http://127.0.0.1:1/nope')
                    .then(function(r) {
                        document.getElementById('result').textContent = 'should-not-reach';
                    })
                    .catch(function(err) {
                        document.getElementById('result').textContent = 'caught';
                    });
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "caught" in result

    def test_fetch_response_clone(self):
        """Response.clone() creates independent copy."""
        self.server.expect_request("/clone").respond_with_data(
            "clone-body", content_type="text/plain"
        )
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/clone')
                    .then(function(r) {{
                        var r2 = r.clone();
                        return Promise.all([r.text(), r2.text()]);
                    }})
                    .then(function(results) {{
                        document.getElementById('result').textContent =
                            results[0] + '|' + results[1];
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "clone-body|clone-body" in result

    def test_fetch_json_parse_error(self):
        """Parsing invalid JSON rejects the promise."""
        self.server.expect_request("/bad-json").respond_with_data(
            "not{json", content_type="application/json"
        )
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/bad-json')
                    .then(function(r) {{ return r.json(); }})
                    .then(function(data) {{
                        document.getElementById('result').textContent = 'should-not-reach';
                    }})
                    .catch(function(err) {{
                        document.getElementById('result').textContent = 'json-error';
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "json-error" in result

    def test_fetch_typeof(self):
        """fetch is a function on globalThis."""
        html = """<html><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = typeof fetch;
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "function" in result

    def test_fetch_with_base_url(self):
        """Relative URL resolves against base_url."""
        self.server.expect_request("/rel/data").respond_with_data("relative-ok")
        html = """<html><body>
            <div id="result">pending</div>
            <script>
                fetch('/rel/data')
                    .then(function(r) { return r.text(); })
                    .then(function(text) {
                        document.getElementById('result').textContent = text;
                    });
            </script>
        </body></html>"""
        result = blazeweb.render(html, base_url=self.url)
        assert "relative-ok" in result

    def test_fetch_headers_foreach(self):
        """headers.forEach iterates over response headers."""
        self.server.expect_request("/hdr").respond_with_data(
            "ok",
            headers={"X-Foo": "bar", "X-Baz": "qux"},
        )
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.url}/hdr')
                    .then(function(r) {{
                        var items = [];
                        r.headers.forEach(function(value, key) {{
                            if (key.startsWith('x-')) {{
                                items.push(key + '=' + value);
                            }}
                        }});
                        items.sort();
                        document.getElementById('result').textContent = items.join(';');
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "x-baz=qux" in result
        assert "x-foo=bar" in result

    def test_queue_microtask_integration(self):
        """queueMicrotask works with fetch promise chains."""
        self.server.expect_request("/mt").respond_with_data("micro")
        html = f"""<html><body>
            <div id="result">pending</div>
            <script>
                var order = [];
                fetch('{self.url}/mt')
                    .then(function(r) {{ return r.text(); }})
                    .then(function(text) {{
                        order.push(text);
                        queueMicrotask(function() {{
                            order.push('microtask');
                            document.getElementById('result').textContent = order.join(',');
                        }});
                    }});
            </script>
        </body></html>"""
        result = blazeweb.render(html)
        assert "micro,microtask" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 0: RenderResult diagnostics
# ══════════════════════════════════════════════════════════════════════════════


class TestRenderResult:
    """RenderResult is a str subclass with .html and .errors attributes."""

    def test_render_result_is_str(self):
        result = blazeweb.render("<html><body>OK</body></html>")
        assert isinstance(result, str)

    def test_render_result_html_attribute(self):
        result = blazeweb.render("<html><body>OK</body></html>")
        assert result.html == str(result)

    def test_render_result_errors_empty_on_success(self):
        result = blazeweb.render("<html><body><script>var x = 1;</script></body></html>")
        assert result.errors == []

    def test_render_result_errors_on_js_error(self):
        result = blazeweb.render("<html><body><script>undefined.foo</script></body></html>")
        assert len(result.errors) == 1
        assert "TypeError" in result.errors[0]

    def test_render_result_errors_multiple_scripts(self):
        html = """<html><body>
            <script>undefined.foo</script>
            <script>null.bar</script>
            <script>var ok = 1;</script>
        </body></html>"""
        result = blazeweb.render(html)
        assert len(result.errors) == 2

    def test_render_result_contains(self):
        result = blazeweb.render("<html><body><p>Hello</p></body></html>")
        assert "Hello" in result

    def test_render_result_len(self):
        result = blazeweb.render("<html><body>OK</body></html>")
        assert len(result) > 0

    def test_render_result_str_methods(self):
        """RenderResult supports str methods (find, replace, etc.)."""
        result = blazeweb.render("<html><body><p>Hello World</p></body></html>")
        assert result.find("Hello") >= 0
        assert result.upper() != result  # str method works
        assert result.count("Hello") == 1

    def test_render_result_repr(self):
        result = blazeweb.render("<html><body>OK</body></html>")
        r = repr(result)
        assert "RenderResult" in r

    def test_render_result_errors_no_scripts(self):
        result = blazeweb.render("<html><body>No JS</body></html>")
        assert result.errors == []

    def test_render_result_syntax_error(self):
        result = blazeweb.render("<html><body><script>function {</script></body></html>")
        assert len(result.errors) == 1
        assert "SyntaxError" in result.errors[0]

    def test_render_result_reference_error(self):
        result = blazeweb.render("<html><body><script>unknownVar.foo</script></body></html>")
        assert len(result.errors) == 1

    def test_client_render_result(self):
        client = blazeweb.Client()
        result = client.render("<html><body><script>undefined.x</script></body></html>")
        assert isinstance(result, blazeweb.RenderResult)
        assert len(result.errors) == 1


# ══════════════════════════════════════════════════════════════════════════════
# Batch 1: Window/Global stubs
# ══════════════════════════════════════════════════════════════════════════════


class TestWindowDimensions:
    """Window dimension and scroll properties."""

    def test_inner_width(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = window.innerWidth;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1920<" in result

    def test_inner_height(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = window.innerHeight;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1080<" in result

    def test_outer_width(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = window.outerWidth;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1920<" in result

    def test_outer_height(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = window.outerHeight;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1080<" in result

    def test_scroll_x_y(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = scrollX + ',' + scrollY;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0,0<" in result

    def test_page_offset(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = pageXOffset + ',' + pageYOffset;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0,0<" in result

    def test_device_pixel_ratio(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = devicePixelRatio;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1<" in result

    def test_is_secure_context(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = isSecureContext;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_scroll_to_no_crash(self):
        html = """<html><body><div id="r">ok</div><script>
            scrollTo(0, 100);
            scrollBy(10, 10);
            scroll(0, 0);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestWindowScreen:
    """window.screen object."""

    def test_screen_width(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = screen.width;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1920<" in result

    def test_screen_height(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = screen.height;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1080<" in result

    def test_screen_color_depth(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = screen.colorDepth;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">24<" in result

    def test_screen_avail_width(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = screen.availWidth;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1920<" in result


class TestWindowHistory:
    """window.history object."""

    def test_history_length(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = history.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1<" in result

    def test_history_state_null(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = String(history.state);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result

    def test_history_push_state_no_crash(self):
        html = """<html><body><div id="r">ok</div><script>
            history.pushState({}, '', '/new');
            history.replaceState({}, '', '/newer');
            history.back();
            history.forward();
            history.go(-1);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestRequestIdleCallback:
    """requestIdleCallback / cancelIdleCallback."""

    def test_request_idle_callback_fires(self):
        html = """<html><body><div id="r">before</div><script>
            requestIdleCallback(function() {
                document.getElementById('r').textContent = 'after';
            });
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">after<" in result

    def test_cancel_idle_callback(self):
        html = """<html><body><div id="r">unchanged</div><script>
            var id = requestIdleCallback(function() {
                document.getElementById('r').textContent = 'changed';
            });
            cancelIdleCallback(id);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">unchanged<" in result


class TestGetSelection:
    """window.getSelection()."""

    def test_get_selection_returns_object(self):
        html = """<html><body><div id="r"></div><script>
            var s = getSelection();
            document.getElementById('r').textContent = s.rangeCount + ',' + s.isCollapsed;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0,true<" in result

    def test_get_selection_to_string(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = getSelection().toString();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "><" in result  # empty string between tags


class TestVisualViewport:
    """window.visualViewport."""

    def test_visual_viewport_dimensions(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                visualViewport.width + ',' + visualViewport.height + ',' + visualViewport.scale;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1920,1080,1<" in result


class TestCrypto:
    """window.crypto."""

    def test_crypto_get_random_values(self):
        html = """<html><body><div id="r"></div><script>
            var arr = new Uint8Array(4);
            crypto.getRandomValues(arr);
            document.getElementById('r').textContent = arr.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">4<" in result

    def test_crypto_random_uuid(self):
        html = """<html><body><div id="r"></div><script>
            var uuid = crypto.randomUUID();
            document.getElementById('r').textContent = uuid.length + ',' + uuid.includes('-');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">36,true<" in result


class TestStructuredClone:
    """structuredClone()."""

    def test_structured_clone_object(self):
        html = """<html><body><div id="r"></div><script>
            var obj = {a: 1, b: 'hello'};
            var cloned = structuredClone(obj);
            cloned.a = 2;
            document.getElementById('r').textContent = obj.a + ',' + cloned.a;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1,2<" in result

    def test_structured_clone_array(self):
        html = """<html><body><div id="r"></div><script>
            var arr = [1, 2, 3];
            var cloned = structuredClone(arr);
            cloned.push(4);
            document.getElementById('r').textContent = arr.length + ',' + cloned.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">3,4<" in result


class TestWindowOrigin:
    """window.origin."""

    def test_origin_with_base_url(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = window.origin;
        </script></body></html>"""
        result = blazeweb.render(html, base_url="https://example.com/page")
        assert ">https://example.com<" in result

    def test_origin_without_base_url(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = window.origin;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result


class TestWindowSelfReferences:
    """Batch 5: window.parent, top, frames, opener, closed, name, etc."""

    def test_parent_is_window(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = (window.parent === window);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_top_is_window(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = (window.top === window);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_frames_is_window(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = (window.frames === window);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_opener_is_null(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = String(window.opener);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result

    def test_closed_is_false(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = window.closed;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_name_is_empty(self):
        html = """<html><body><div id="r"></div><script>
            var v = window.name;
            document.getElementById('r').textContent = typeof v + ':' + (v === '' ? 'empty' : v);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">string:empty<" in result

    def test_frame_element_is_null(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = String(window.frameElement);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result

    def test_length_is_zero(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = window.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0<" in result

    def test_statusbar_visible(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = statusbar.visible;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 2: Document API stubs
# ══════════════════════════════════════════════════════════════════════════════


class TestDocumentStringAccessors:
    """document.characterSet, compatMode, contentType, etc."""

    def test_character_set(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.characterSet;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">UTF-8<" in result

    def test_charset_alias(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.charset;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">UTF-8<" in result

    def test_input_encoding(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.inputEncoding;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">UTF-8<" in result

    def test_compat_mode(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.compatMode;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">CSS1Compat<" in result

    def test_content_type(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.contentType;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">text/html<" in result

    def test_referrer_empty(self):
        html = """<html><body><div id="r"></div><script>
            var v = document.referrer;
            document.getElementById('r').textContent = typeof v + ':' + (v === '' ? 'empty' : v);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">string:empty<" in result

    def test_domain_with_base_url(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.domain;
        </script></body></html>"""
        result = blazeweb.render(html, base_url="https://example.com/page")
        assert ">example.com<" in result

    def test_domain_without_base_url(self):
        html = """<html><body><div id="r"></div><script>
            var v = document.domain;
            document.getElementById('r').textContent = typeof v + ':' + (v === '' ? 'empty' : v);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">string:empty<" in result


class TestDocumentActiveElement:
    """document.activeElement."""

    def test_active_element_is_body(self):
        html = """<html><body><div id="r"></div><script>
            var ae = document.activeElement;
            document.getElementById('r').textContent = ae.tagName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">BODY<" in result


class TestDocumentHasFocus:
    """document.hasFocus()."""

    def test_has_focus_true(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.hasFocus();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result


class TestDocumentCollections:
    """document.forms, images, links, scripts, anchors."""

    def test_forms(self):
        html = """<html><body>
            <form id="f1"></form><form id="f2"></form>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.forms.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_images(self):
        html = """<html><body>
            <img src="a.png"><img src="b.png"><img src="c.png">
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.images.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">3<" in result

    def test_links(self):
        html = """<html><body>
            <a href="/a">A</a><a href="/b">B</a><a>no href</a>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.links.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_scripts_collection(self):
        html = """<html><body>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.scripts.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1<" in result

    def test_anchors(self):
        html = """<html><body>
            <a name="top">Top</a><a name="bottom">Bottom</a><a href="/x">Link</a>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.anchors.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result


class TestDocumentCreateRange:
    """document.createRange()."""

    def test_create_range_properties(self):
        html = """<html><body><div id="r"></div><script>
            var range = document.createRange();
            document.getElementById('r').textContent =
                range.collapsed + ',' + range.startOffset + ',' + range.endOffset;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,0,0<" in result

    def test_create_range_contextual_fragment(self):
        html = """<html><body><div id="target"></div><script>
            var range = document.createRange();
            var frag = range.createContextualFragment('<b>bold</b>');
            document.getElementById('target').appendChild(frag);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "<b>bold</b>" in result

    def test_create_range_no_crash_methods(self):
        html = """<html><body><div id="r">ok</div><script>
            var range = document.createRange();
            range.setStart(document.body, 0);
            range.setEnd(document.body, 0);
            range.collapse(true);
            range.detach();
            range.toString();
            range.getBoundingClientRect();
            range.getClientRects();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestDocumentCreateTreeWalker:
    """document.createTreeWalker()."""

    def test_create_tree_walker_basic(self):
        html = """<html><body>
            <div id="container"><p>A</p><p>B</p><p>C</p></div>
            <div id="r"></div><script>
            var container = document.getElementById('container');
            var walker = document.createTreeWalker(container, 1); // SHOW_ELEMENT
            var count = 0;
            while (walker.nextNode()) count++;
            document.getElementById('r').textContent = count;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">3<" in result

    def test_create_tree_walker_text_nodes(self):
        html = """<html><body>
            <div id="container">Hello <b>World</b></div>
            <div id="r"></div><script>
            var container = document.getElementById('container');
            var walker = document.createTreeWalker(container, 4); // SHOW_TEXT
            var texts = [];
            while (walker.nextNode()) texts.push(walker.currentNode.textContent);
            document.getElementById('r').textContent = texts.join('|');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "Hello " in result
        assert "World" in result

    def test_create_node_iterator_alias(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = typeof document.createNodeIterator;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">function<" in result


class TestDocumentElementFromPoint:
    """document.elementFromPoint / elementsFromPoint."""

    def test_element_from_point_returns_body(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.elementFromPoint(100, 100);
            document.getElementById('r').textContent = el.tagName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">BODY<" in result

    def test_elements_from_point_returns_array(self):
        html = """<html><body><div id="r"></div><script>
            var els = document.elementsFromPoint(100, 100);
            document.getElementById('r').textContent = Array.isArray(els) + ',' + els.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,1<" in result


class TestDocumentDoctype:
    """document.doctype."""

    def test_doctype_present(self):
        html = """<!DOCTYPE html><html><body><div id="r"></div><script>
            var dt = document.doctype;
            document.getElementById('r').textContent = dt ? dt.name : 'null';
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">html<" in result


class TestDocumentImplementation:
    """document.implementation."""

    def test_implementation_has_feature(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.implementation.hasFeature();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_implementation_create_html_document(self):
        html = """<html><body><div id="r"></div><script>
            var doc = document.implementation.createHTMLDocument('test');
            document.getElementById('r').textContent = typeof doc.body;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">object<" in result


class TestDocumentMiscMethods:
    """document.getSelection, write, execCommand, adoptNode, importNode."""

    def test_document_get_selection(self):
        html = """<html><body><div id="r"></div><script>
            var s = document.getSelection();
            document.getElementById('r').textContent = typeof s;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">object<" in result

    def test_document_write_no_crash(self):
        html = """<html><body><div id="r">ok</div><script>
            document.write('ignored');
            document.writeln('ignored');
            document.open();
            document.close();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result

    def test_document_exec_command_returns_false(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.execCommand('bold');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_adopt_node(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElement('div');
            var adopted = document.adoptNode(el);
            document.getElementById('r').textContent = (adopted === el);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 3: Functional constructors
# ══════════════════════════════════════════════════════════════════════════════


class TestTextEncoder:
    """TextEncoder constructor."""

    def test_text_encoder_encoding_property(self):
        html = """<html><body><div id="r"></div><script>
            var enc = new TextEncoder();
            document.getElementById('r').textContent = enc.encoding;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">utf-8<" in result

    def test_text_encoder_encode(self):
        html = """<html><body><div id="r"></div><script>
            var enc = new TextEncoder();
            var arr = enc.encode('hello');
            document.getElementById('r').textContent = arr.length + ',' + arr[0];
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">5,104<" in result  # 'h' = 104

    def test_text_encoder_encode_returns_uint8array(self):
        html = """<html><body><div id="r"></div><script>
            var enc = new TextEncoder();
            var arr = enc.encode('hi');
            document.getElementById('r').textContent = (arr instanceof Uint8Array);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_text_encoder_encode_into(self):
        html = """<html><body><div id="r"></div><script>
            var enc = new TextEncoder();
            var result = enc.encodeInto('hi', new Uint8Array(10));
            document.getElementById('r').textContent = typeof result.read + ',' + typeof result.written;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">number,number<" in result


class TestTextDecoder:
    """TextDecoder constructor."""

    def test_text_decoder_encoding_property(self):
        html = """<html><body><div id="r"></div><script>
            var dec = new TextDecoder();
            document.getElementById('r').textContent = dec.encoding;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">utf-8<" in result

    def test_text_decoder_decode(self):
        html = """<html><body><div id="r"></div><script>
            var enc = new TextEncoder();
            var dec = new TextDecoder();
            var encoded = enc.encode('hello');
            var decoded = dec.decode(encoded);
            document.getElementById('r').textContent = decoded;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">hello<" in result

    def test_text_decoder_custom_encoding(self):
        html = """<html><body><div id="r"></div><script>
            var dec = new TextDecoder('ascii');
            document.getElementById('r').textContent = dec.encoding;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ascii<" in result

    def test_text_decoder_empty_input(self):
        html = """<html><body><div id="r"></div><script>
            var dec = new TextDecoder();
            var v = dec.decode();
            document.getElementById('r').textContent = typeof v + ':' + (v === '' ? 'empty' : v);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">string:empty<" in result


class TestURLSearchParams:
    """URLSearchParams constructor."""

    def test_url_search_params_from_string(self):
        html = """<html><body><div id="r"></div><script>
            var params = new URLSearchParams('foo=bar&baz=qux');
            document.getElementById('r').textContent = params.get('foo') + ',' + params.get('baz');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">bar,qux<" in result

    def test_url_search_params_has(self):
        html = """<html><body><div id="r"></div><script>
            var params = new URLSearchParams('a=1');
            document.getElementById('r').textContent = params.has('a') + ',' + params.has('b');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,false<" in result

    def test_url_search_params_set(self):
        html = """<html><body><div id="r"></div><script>
            var params = new URLSearchParams('a=1');
            params.set('a', '2');
            document.getElementById('r').textContent = params.get('a');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_url_search_params_delete(self):
        html = """<html><body><div id="r"></div><script>
            var params = new URLSearchParams('a=1&b=2');
            params.delete('a');
            document.getElementById('r').textContent = params.has('a') + ',' + params.get('b');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false,2<" in result

    def test_url_search_params_append(self):
        html = """<html><body><div id="r"></div><script>
            var params = new URLSearchParams('a=1');
            params.append('a', '2');
            var all = params.getAll('a');
            document.getElementById('r').textContent = all.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_url_search_params_to_string(self):
        html = """<html><body><div id="r"></div><script>
            var params = new URLSearchParams('foo=bar&baz=qux');
            document.getElementById('r').textContent = params.toString();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "foo=bar" in result
        assert "baz=qux" in result

    def test_url_search_params_for_each(self):
        html = """<html><body><div id="r"></div><script>
            var params = new URLSearchParams('a=1&b=2');
            var pairs = [];
            params.forEach(function(v, k) { pairs.push(k + '=' + v); });
            document.getElementById('r').textContent = pairs.join(',');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "a=1" in result
        assert "b=2" in result

    def test_url_search_params_with_question_mark(self):
        html = """<html><body><div id="r"></div><script>
            var params = new URLSearchParams('?x=10');
            document.getElementById('r').textContent = params.get('x');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">10<" in result


class TestDOMParser:
    """DOMParser constructor."""

    def test_dom_parser_parse_from_string(self):
        html = """<html><body><div id="r"></div><script>
            var parser = new DOMParser();
            var doc = parser.parseFromString('<p>Hello</p>', 'text/html');
            document.getElementById('r').textContent = typeof doc.body;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">object<" in result

    def test_dom_parser_document_element(self):
        html = """<html><body><div id="r"></div><script>
            var parser = new DOMParser();
            var doc = parser.parseFromString('<p>Hello</p>', 'text/html');
            document.getElementById('r').textContent = typeof doc.documentElement;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">object<" in result


class TestAbortController:
    """AbortController constructor."""

    def test_abort_controller_signal(self):
        html = """<html><body><div id="r"></div><script>
            var ac = new AbortController();
            document.getElementById('r').textContent = ac.signal.aborted;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_abort_controller_abort(self):
        html = """<html><body><div id="r"></div><script>
            var ac = new AbortController();
            ac.abort();
            document.getElementById('r').textContent = ac.signal.aborted;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_abort_controller_signal_event_listeners(self):
        html = """<html><body><div id="r">ok</div><script>
            var ac = new AbortController();
            ac.signal.addEventListener('abort', function() {});
            ac.signal.removeEventListener('abort', function() {});
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 4: Element method stubs
# ══════════════════════════════════════════════════════════════════════════════


class TestElementFocusBlurClick:
    """element.focus(), blur(), click() — no-ops."""

    def test_focus_blur_click_no_crash(self):
        html = """<html><body>
            <input id="inp" type="text">
            <div id="r">ok</div><script>
            var el = document.getElementById('inp');
            el.focus();
            el.blur();
            el.click();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestScrollIntoView:
    """element.scrollIntoView() — no-op."""

    def test_scroll_into_view_no_crash(self):
        html = """<html><body>
            <div id="target">target</div>
            <div id="r">ok</div><script>
            document.getElementById('target').scrollIntoView();
            document.getElementById('target').scrollIntoView({behavior: 'smooth'});
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestGetClientRects:
    """element.getClientRects()."""

    def test_get_client_rects_returns_array(self):
        html = """<html><body><div id="r"></div><script>
            var rects = document.body.getClientRects();
            document.getElementById('r').textContent = rects.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1<" in result


class TestInnerText:
    """element.innerText getter/setter."""

    def test_inner_text_getter(self):
        html = """<html><body>
            <div id="src">Hello <b>World</b></div>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.getElementById('src').innerText;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "Hello World" in result

    def test_inner_text_setter(self):
        html = """<html><body>
            <div id="target"><b>old</b></div><script>
            document.getElementById('target').innerText = 'new text';
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "new text" in result
        assert "<b>" not in result.split("target")[1]  # old <b> removed

    def test_outer_text_getter(self):
        html = """<html><body>
            <div id="src">Hello</div>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.getElementById('src').outerText;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "Hello" in result


class TestHiddenProperty:
    """element.hidden getter/setter."""

    def test_hidden_getter_false(self):
        html = """<html><body>
            <div id="target">visible</div>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.getElementById('target').hidden;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_hidden_getter_true(self):
        html = """<html><body>
            <div id="target" hidden>hidden</div>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.getElementById('target').hidden;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_hidden_setter(self):
        html = """<html><body>
            <div id="target">content</div><script>
            document.getElementById('target').hidden = true;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert 'hidden' in result


class TestTabIndex:
    """element.tabIndex getter/setter."""

    def test_tab_index_default(self):
        html = """<html><body>
            <div id="target"></div>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.getElementById('target').tabIndex;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">-1<" in result

    def test_tab_index_setter(self):
        html = """<html><body>
            <div id="target"></div><script>
            document.getElementById('target').tabIndex = 5;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert 'tabindex="5"' in result


class TestGetAttributeNames:
    """element.getAttributeNames()."""

    def test_get_attribute_names(self):
        html = """<html><body>
            <div id="target" class="foo" data-x="1"></div>
            <div id="r"></div><script>
            var names = document.getElementById('target').getAttributeNames();
            document.getElementById('r').textContent = names.sort().join(',');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "class" in result
        assert "data-x" in result
        assert "id" in result


class TestHasAttributes:
    """element.hasAttributes()."""

    def test_has_attributes_true(self):
        html = """<html><body>
            <div id="target" class="foo"></div>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.getElementById('target').hasAttributes();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_has_attributes_false(self):
        html = """<html><body>
            <div id="r"></div><script>
            var el = document.createElement('div');
            document.getElementById('r').textContent = el.hasAttributes();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result


class TestToggleAttribute:
    """element.toggleAttribute()."""

    def test_toggle_attribute_add(self):
        html = """<html><body>
            <div id="target"></div>
            <div id="r"></div><script>
            var result = document.getElementById('target').toggleAttribute('disabled');
            document.getElementById('r').textContent = result;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result
        assert "disabled" in result

    def test_toggle_attribute_remove(self):
        html = """<html><body>
            <div id="target" disabled></div>
            <div id="r"></div><script>
            var result = document.getElementById('target').toggleAttribute('disabled');
            document.getElementById('r').textContent = result;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_toggle_attribute_force_true(self):
        html = """<html><body>
            <div id="target"></div>
            <div id="r"></div><script>
            document.getElementById('target').toggleAttribute('hidden', true);
            document.getElementById('r').textContent = document.getElementById('target').hasAttribute('hidden');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_toggle_attribute_force_false(self):
        html = """<html><body>
            <div id="target" hidden></div>
            <div id="r"></div><script>
            document.getElementById('target').toggleAttribute('hidden', false);
            document.getElementById('r').textContent = document.getElementById('target').hasAttribute('hidden');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result


class TestGetAttributeNode:
    """element.getAttributeNode()."""

    def test_get_attribute_node_exists(self):
        html = """<html><body>
            <div id="target" class="foo"></div>
            <div id="r"></div><script>
            var attr = document.getElementById('target').getAttributeNode('class');
            document.getElementById('r').textContent = attr.name + ',' + attr.value + ',' + attr.specified;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">class,foo,true<" in result

    def test_get_attribute_node_missing(self):
        html = """<html><body>
            <div id="target"></div>
            <div id="r"></div><script>
            var attr = document.getElementById('target').getAttributeNode('class');
            document.getElementById('r').textContent = String(attr);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result


class TestAttachShadow:
    """element.attachShadow()."""

    def test_attach_shadow_returns_fragment(self):
        html = """<html><body>
            <div id="host"></div>
            <div id="r"></div><script>
            var shadow = document.getElementById('host').attachShadow({mode: 'open'});
            document.getElementById('r').textContent = typeof shadow;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">object<" in result


class TestElementLocalName:
    """element.localName, namespaceURI."""

    def test_local_name(self):
        html = """<html><body>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.body.localName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">body<" in result

    def test_namespace_uri(self):
        html = """<html><body>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.body.namespaceURI;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">http://www.w3.org/1999/xhtml<" in result


class TestElementAttributes:
    """element.attributes NamedNodeMap."""

    def test_attributes_length(self):
        html = """<html><body>
            <div id="target" class="foo" data-x="1"></div>
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.getElementById('target').attributes.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">3<" in result

    def test_attributes_get_named_item(self):
        html = """<html><body>
            <div id="target" class="foo"></div>
            <div id="r"></div><script>
            var attr = document.getElementById('target').attributes.getNamedItem('class');
            document.getElementById('r').textContent = attr.value;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">foo<" in result


class TestElementSlot:
    """element.slot / assignedSlot."""

    def test_slot_empty(self):
        html = """<html><body>
            <div id="r"></div><script>
            var v = document.body.slot;
            document.getElementById('r').textContent = typeof v + ':' + (v === '' ? 'empty' : v);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">string:empty<" in result

    def test_assigned_slot_null(self):
        html = """<html><body>
            <div id="r"></div><script>
            document.getElementById('r').textContent = String(document.body.assignedSlot);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result


class TestElementAfterBeforeReplaceWith:
    """element.after(), before(), replaceWith()."""

    def test_element_after(self):
        html = """<html><body>
            <div id="target">target</div><script>
            var el = document.getElementById('target');
            var span = document.createElement('span');
            span.textContent = 'after';
            el.after(span);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "target" in result
        assert "<span>after</span>" in result

    def test_element_before(self):
        html = """<html><body>
            <div id="target">target</div><script>
            var el = document.getElementById('target');
            var span = document.createElement('span');
            span.textContent = 'before';
            el.before(span);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "<span>before</span>" in result

    def test_element_replace_with(self):
        html = """<html><body>
            <div id="target">old</div><script>
            var el = document.getElementById('target');
            var span = document.createElement('span');
            span.textContent = 'new';
            el.replaceWith(span);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "<span>new</span>" in result
        assert "old" not in result.split("script")[0]

    def test_insert_adjacent_text(self):
        html = """<html><body>
            <div id="target">mid</div><script>
            var el = document.getElementById('target');
            el.insertAdjacentText('afterbegin', 'start');
            el.insertAdjacentText('beforeend', 'end');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "start" in result
        assert "end" in result


class TestElementAnimate:
    """element.animate() / getAnimations()."""

    def test_animate_returns_object(self):
        html = """<html><body><div id="r"></div><script>
            var anim = document.body.animate([{opacity: 0}, {opacity: 1}], 1000);
            document.getElementById('r').textContent = typeof anim.finished;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">object<" in result

    def test_get_animations_empty(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.body.getAnimations().length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 5: Edge cases & polish
# ══════════════════════════════════════════════════════════════════════════════


class TestPerformanceExtended:
    """Extended performance object stubs."""

    def test_performance_get_entries(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                Array.isArray(performance.getEntries()) + ',' +
                Array.isArray(performance.getEntriesByType('resource')) + ',' +
                Array.isArray(performance.getEntriesByName('foo'));
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,true,true<" in result

    def test_performance_mark_measure_no_crash(self):
        html = """<html><body><div id="r">ok</div><script>
            performance.mark('start');
            performance.measure('test', 'start');
            performance.clearMarks();
            performance.clearMeasures();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result

    def test_performance_timing(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = typeof performance.timing.navigationStart;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">number<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 6: DOM Methods
# ══════════════════════════════════════════════════════════════════════════════


class TestCreateElementNS:
    """document.createElementNS() for SVG and other namespaces."""

    def test_create_svg_element(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElementNS("http://www.w3.org/2000/svg", "svg");
            document.getElementById('r').textContent = el.nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">svg<" in result

    def test_create_svg_child(self):
        html = """<html><body><div id="c"></div><script>
            var svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
            var rect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
            svg.appendChild(rect);
            document.getElementById('c').appendChild(svg);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "<rect>" in result or "<rect><" in result or "<rect/>" in result

    def test_create_html_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElementNS("http://www.w3.org/1999/xhtml", "div");
            document.getElementById('r').textContent = el.nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">DIV<" in result

    def test_create_null_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElementNS(null, "custom");
            document.getElementById('r').textContent = typeof el;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">object<" in result


class TestGetElementsByName:
    """document.getElementsByName()."""

    def test_find_by_name(self):
        html = """<html><body>
            <input name="user" value="a">
            <input name="user" value="b">
            <div id="r"></div><script>
            document.getElementById('r').textContent = document.getElementsByName('user').length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_no_match(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.getElementsByName('nonexistent').length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0<" in result

    def test_single_match(self):
        html = """<html><body>
            <input name="email" value="test@test.com">
            <div id="r"></div><script>
            var els = document.getElementsByName('email');
            document.getElementById('r').textContent = els.length + ',' + els[0].nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1,INPUT<" in result


class TestCurrentScript:
    """document.currentScript."""

    def test_current_script_exists(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = (document.currentScript !== null);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_current_script_nodename(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = document.currentScript.nodeName;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">SCRIPT<" in result

    def test_current_script_data_attribute(self):
        html = """<html><body><div id="r"></div><script data-config="abc">
            document.getElementById('r').textContent = document.currentScript.getAttribute('data-config');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">abc<" in result


class TestNamespacedAttributes:
    """element.setAttributeNS/getAttributeNS/hasAttributeNS/removeAttributeNS."""

    def test_set_get_attribute_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElementNS("http://www.w3.org/2000/svg", "use");
            el.setAttributeNS("http://www.w3.org/1999/xlink", "xlink:href", "#icon");
            document.getElementById('r').textContent = el.getAttributeNS("http://www.w3.org/1999/xlink", "href");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">#icon<" in result

    def test_has_attribute_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElement('div');
            el.setAttributeNS("http://example.com", "ex:foo", "bar");
            document.getElementById('r').textContent = el.hasAttributeNS("http://example.com", "foo");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_remove_attribute_ns(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElement('div');
            el.setAttributeNS("http://example.com", "ex:foo", "bar");
            el.removeAttributeNS("http://example.com", "foo");
            document.getElementById('r').textContent = el.hasAttributeNS("http://example.com", "foo");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_get_attribute_ns_missing(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.createElement('div');
            document.getElementById('r').textContent = String(el.getAttributeNS("http://example.com", "nope"));
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 7: Observer & Constructor Stubs
# ══════════════════════════════════════════════════════════════════════════════


class TestMessageChannel:
    """MessageChannel constructor."""

    def test_ports_exist(self):
        html = """<html><body><div id="r"></div><script>
            var mc = new MessageChannel();
            document.getElementById('r').textContent =
                (typeof mc.port1) + ',' + (typeof mc.port2);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">object,object<" in result

    def test_port_methods(self):
        html = """<html><body><div id="r"></div><script>
            var mc = new MessageChannel();
            document.getElementById('r').textContent =
                (typeof mc.port1.postMessage) + ',' + (typeof mc.port1.close);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">function,function<" in result

    def test_port_onmessage_settable(self):
        html = """<html><body><div id="r">ok</div><script>
            var mc = new MessageChannel();
            mc.port1.onmessage = function() {};
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestWorkerStub:
    """Worker constructor stub."""

    def test_worker_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var w = new Worker("worker.js");
            document.getElementById('r').textContent =
                (typeof w.postMessage) + ',' + (typeof w.terminate);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">function,function<" in result

    def test_worker_onmessage_settable(self):
        html = """<html><body><div id="r">ok</div><script>
            var w = new Worker("worker.js");
            w.onmessage = function() {};
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestCustomElements:
    """customElements registry."""

    def test_define_and_get(self):
        html = """<html><body><div id="r"></div><script>
            customElements.define("my-el", class extends HTMLElement {});
            document.getElementById('r').textContent = (customElements.get("my-el") !== undefined);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_get_undefined(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = String(customElements.get("nonexistent"));
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">undefined<" in result

    def test_when_defined(self):
        html = """<html><body><div id="r"></div><script>
            var p = customElements.whenDefined("x-foo");
            document.getElementById('r').textContent = (typeof p.then);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">function<" in result


class TestIntersectionObserverUpgrade:
    """IntersectionObserver with threshold options."""

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


class TestMatchMediaUpgrade:
    """matchMedia() with actual query evaluation."""

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
            document.getElementById('r').textContent = matchMedia("(min-width: 1200px)").matches;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_max_width_narrow(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = matchMedia("(max-width: 500px)").matches;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_prefers_color_scheme(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = matchMedia("(prefers-color-scheme: light)").matches;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 8: Event Constructors
# ══════════════════════════════════════════════════════════════════════════════


class TestMouseEvent:
    """MouseEvent constructor."""

    def test_basic_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var e = new MouseEvent("click");
            document.getElementById('r').textContent = e.type + ',' + e.clientX + ',' + e.clientY;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">click,0,0<" in result

    def test_with_init(self):
        html = """<html><body><div id="r"></div><script>
            var e = new MouseEvent("click", {clientX: 10, clientY: 20, bubbles: true});
            document.getElementById('r').textContent = e.clientX + ',' + e.clientY + ',' + e.bubbles;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">10,20,true<" in result

    def test_get_modifier_state(self):
        html = """<html><body><div id="r"></div><script>
            var e = new MouseEvent("click");
            document.getElementById('r').textContent = e.getModifierState("Control");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result


class TestKeyboardEvent:
    """KeyboardEvent constructor."""

    def test_basic(self):
        html = """<html><body><div id="r"></div><script>
            var e = new KeyboardEvent("keydown", {key: "Enter", code: "Enter"});
            document.getElementById('r').textContent = e.key + ',' + e.code;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">Enter,Enter<" in result

    def test_defaults(self):
        html = """<html><body><div id="r"></div><script>
            var e = new KeyboardEvent("keyup");
            document.getElementById('r').textContent = JSON.stringify(e.key) + ',' + e.repeat;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert '>"",false<' in result or '>""' in result


class TestFocusEvent:
    """FocusEvent constructor."""

    def test_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var e = new FocusEvent("focus");
            document.getElementById('r').textContent = e.type + ',' + String(e.relatedTarget);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">focus,null<" in result


class TestInputEvent:
    """InputEvent constructor."""

    def test_with_data(self):
        html = """<html><body><div id="r"></div><script>
            var e = new InputEvent("input", {data: "x", inputType: "insertText"});
            document.getElementById('r').textContent = e.data + ',' + e.inputType;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">x,insertText<" in result

    def test_defaults(self):
        html = """<html><body><div id="r"></div><script>
            var e = new InputEvent("input");
            document.getElementById('r').textContent = String(e.data) + ',' + JSON.stringify(e.inputType);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result or "null," in result


class TestPointerEvent:
    """PointerEvent constructor."""

    def test_with_init(self):
        html = """<html><body><div id="r"></div><script>
            var e = new PointerEvent("pointerdown", {pointerId: 1, pointerType: "mouse"});
            document.getElementById('r').textContent = e.pointerId + ',' + e.pointerType;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1,mouse<" in result

    def test_defaults(self):
        html = """<html><body><div id="r"></div><script>
            var e = new PointerEvent("pointerdown");
            document.getElementById('r').textContent = e.width + ',' + e.height;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1,1<" in result


class TestErrorEvent:
    """ErrorEvent constructor."""

    def test_with_init(self):
        html = """<html><body><div id="r"></div><script>
            var e = new ErrorEvent("error", {message: "oops", lineno: 42});
            document.getElementById('r').textContent = e.message + ',' + e.lineno;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">oops,42<" in result

    def test_defaults(self):
        html = """<html><body><div id="r"></div><script>
            var e = new ErrorEvent("error");
            document.getElementById('r').textContent = JSON.stringify(e.message) + ',' + e.lineno + ',' + String(e.error);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert '>""' in result or ">,0," in result


class TestHashChangeEvent:
    """HashChangeEvent constructor."""

    def test_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var e = new HashChangeEvent("hashchange", {oldURL: "http://x.com/#a", newURL: "http://x.com/#b"});
            document.getElementById('r').textContent = e.oldURL + '|' + e.newURL;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">http://x.com/#a|http://x.com/#b<" in result


class TestPopStateEvent:
    """PopStateEvent constructor."""

    def test_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var e = new PopStateEvent("popstate", {state: {page: 1}});
            document.getElementById('r').textContent = e.state.page;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 9: XMLHttpRequest
# ══════════════════════════════════════════════════════════════════════════════


class TestXMLHttpRequest:
    """XMLHttpRequest constructor and methods."""

    @pytest.fixture(autouse=True)
    def _server(self):
        """Start a local HTTP server for every test in this class."""
        pytest.importorskip("pytest_httpserver")
        from pytest_httpserver import HTTPServer

        self.server = HTTPServer(host="127.0.0.1")
        self.server.start()
        yield
        self.server.clear()
        if self.server.is_running():
            self.server.stop()

    def test_basic_get(self):
        self.server.expect_request("/data").respond_with_data(
            "hello-xhr", content_type="text/plain"
        )
        url = self.server.url_for("/data")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("GET", "{url}", false);
            xhr.send();
            document.getElementById('r').textContent = xhr.responseText;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">hello-xhr<" in result

    def test_status_code(self):
        self.server.expect_request("/ok").respond_with_data(
            "ok", content_type="text/plain"
        )
        url = self.server.url_for("/ok")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("GET", "{url}", false);
            xhr.send();
            document.getElementById('r').textContent = xhr.status;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">200<" in result

    def test_response_headers(self):
        self.server.expect_request("/h").respond_with_data(
            "x", content_type="text/plain"
        )
        url = self.server.url_for("/h")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("GET", "{url}", false);
            xhr.send();
            var ct = xhr.getResponseHeader("content-type");
            document.getElementById('r').textContent = ct;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "text/plain" in result

    def test_get_all_response_headers(self):
        self.server.expect_request("/ah").respond_with_data(
            "x", content_type="text/plain"
        )
        url = self.server.url_for("/ah")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("GET", "{url}", false);
            xhr.send();
            var h = xhr.getAllResponseHeaders();
            document.getElementById('r').textContent = (typeof h === 'string' && h.length > 0) ? 'ok' : 'fail';
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result

    def test_instance_constants(self):
        html = """<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            document.getElementById('r').textContent =
                xhr.UNSENT + ',' +
                xhr.OPENED + ',' +
                xhr.HEADERS_RECEIVED + ',' +
                xhr.LOADING + ',' +
                xhr.DONE;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0,1,2,3,4<" in result

    def test_static_constants(self):
        """Per XHR spec, constants must be on the constructor too."""
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                XMLHttpRequest.UNSENT + ',' +
                XMLHttpRequest.OPENED + ',' +
                XMLHttpRequest.HEADERS_RECEIVED + ',' +
                XMLHttpRequest.LOADING + ',' +
                XMLHttpRequest.DONE;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0,1,2,3,4<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 10: Element Constructors
# ══════════════════════════════════════════════════════════════════════════════


class TestImageConstructor:
    """new Image() constructor."""

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
        """Image.src property assignment should reflect to DOM attribute."""
        html = """<html><body><div id="c"></div><script>
            var img = new Image();
            img.src = "test.png";
            document.getElementById('c').appendChild(img);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "<img" in result
        assert 'src="test.png"' in result


class TestAudioConstructor:
    """new Audio() constructor."""

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
    """new Option() constructor."""

    def test_basic(self):
        """Option .value property should reflect the value attribute."""
        html = """<html><body><div id="r"></div><script>
            var o = new Option("Label", "val");
            document.getElementById('r').textContent = o.textContent + ',' + o.value;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">Label,val<" in result

    def test_selected(self):
        html = """<html><body><div id="c"></div><script>
            var o = new Option("L", "v", false, true);
            document.getElementById('c').appendChild(o);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "selected" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 12: FormData, Headers, Blob
# ══════════════════════════════════════════════════════════════════════════════


class TestFormData:
    """FormData constructor."""

    def test_append_and_get(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("name", "Alice");
            document.getElementById('r').textContent = fd.get("name");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">Alice<" in result

    def test_get_all(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("tag", "a");
            fd.append("tag", "b");
            document.getElementById('r').textContent = fd.getAll("tag").length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2<" in result

    def test_has_and_delete(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("key", "val");
            var had = fd.has("key");
            fd.delete("key");
            document.getElementById('r').textContent = had + ',' + fd.has("key");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,false<" in result

    def test_set_replaces(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("k", "old1");
            fd.append("k", "old2");
            fd.set("k", "new");
            document.getElementById('r').textContent = fd.getAll("k").length + ',' + fd.get("k");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1,new<" in result

    def test_entries_keys_values(self):
        html = """<html><body><div id="r"></div><script>
            var fd = new FormData();
            fd.append("a", "1");
            fd.append("b", "2");
            var keys = fd.keys();
            var vals = fd.values();
            document.getElementById('r').textContent = keys.join(',') + '|' + vals.join(',');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "a,b" in result
        assert "1,2" in result


class TestHeaders:
    """Headers constructor."""

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
            h.append("X-Custom", "a");
            h.append("X-Custom", "b");
            document.getElementById('r').textContent = h.get("X-Custom");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "a" in result and "b" in result

    def test_has_delete(self):
        html = """<html><body><div id="r"></div><script>
            var h = new Headers({"X-Key": "val"});
            var had = h.has("X-Key");
            h.delete("X-Key");
            document.getElementById('r').textContent = had + ',' + h.has("X-Key");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,false<" in result


class TestBlob:
    """Blob constructor."""

    def test_size_and_type(self):
        html = """<html><body><div id="r"></div><script>
            var b = new Blob(["hello"], {type: "text/plain"});
            document.getElementById('r').textContent = b.size + ',' + b.type;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">5,text/plain<" in result

    def test_text(self):
        html = """<html><body><div id="r"></div><script>
            var b = new Blob(["hello", " ", "world"]);
            b.text().then(function(t) {
                document.getElementById('r').textContent = t;
            });
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">hello world<" in result

    def test_empty(self):
        html = """<html><body><div id="r"></div><script>
            var b = new Blob();
            document.getElementById('r').textContent = b.size + ',' + JSON.stringify(b.type);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert '0,""' in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 12 Upgrades: getComputedStyle, window.dispatchEvent
# ══════════════════════════════════════════════════════════════════════════════


class TestGetComputedStyleUpgrade:
    """getComputedStyle() reading inline styles."""

    def test_inline_styles_parsed(self):
        html = """<html><body>
            <div id="target" style="color: red"></div>
            <div id="r"></div><script>
            document.getElementById('r').textContent = getComputedStyle(document.getElementById('target')).color;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">red<" in result

    def test_default_empty(self):
        html = """<html><body>
            <div id="target"></div>
            <div id="r"></div><script>
            var v = getComputedStyle(document.getElementById('target')).backgroundColor;
            document.getElementById('r').textContent = JSON.stringify(v);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert '""' in result


class TestWindowDispatchEvent:
    """window.dispatchEvent() and addEventListener()."""

    def test_fires_listener(self):
        html = """<html><body><div id="r"></div><script>
            var fired = false;
            window.addEventListener("custom", function() { fired = true; });
            window.dispatchEvent(new Event("custom"));
            document.getElementById('r').textContent = fired;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_returns_true(self):
        html = """<html><body><div id="r"></div><script>
            var result = window.dispatchEvent(new Event("test"));
            document.getElementById('r').textContent = result;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Batch 13: Lenient Selectors
# ══════════════════════════════════════════════════════════════════════════════


class TestLenientSelectors:
    """Invalid CSS selectors return null/empty instead of throwing."""

    def test_invalid_selector_returns_null(self):
        html = """<html><body><div id="r"></div><script>
            var el = document.querySelector(":invalid-pseudo-class-xyz");
            document.getElementById('r').textContent = String(el);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result

    def test_invalid_selector_all_returns_empty(self):
        html = """<html><body><div id="r"></div><script>
            var els = document.querySelectorAll(":invalid-pseudo-class-xyz");
            document.getElementById('r').textContent = els.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Intl APIs (ICU data integration)
# ══════════════════════════════════════════════════════════════════════════════


class TestIntlAPIs:
    """Intl.* APIs requiring full ICU data."""

    def test_number_format(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                new Intl.NumberFormat('en-US').format(1234567.89);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1,234,567.89<" in result

    def test_date_time_format(self):
        html = """<html><body><div id="r"></div><script>
            var d = new Date(2025, 0, 15);
            var fmt = new Intl.DateTimeFormat('en-US', {year: 'numeric', month: 'long', day: 'numeric'});
            document.getElementById('r').textContent = fmt.format(d);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "January" in result
        assert "15" in result
        assert "2025" in result

    def test_list_format(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                new Intl.ListFormat('en', {type: 'conjunction'}).format(['a', 'b', 'c']);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "a, b, and c" in result

    def test_plural_rules(self):
        html = """<html><body><div id="r"></div><script>
            var pr = new Intl.PluralRules('en');
            document.getElementById('r').textContent = pr.select(1) + ',' + pr.select(2);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">one,other<" in result

    def test_collator(self):
        html = """<html><body><div id="r"></div><script>
            var c = new Intl.Collator('en');
            var arr = ['banana', 'apple', 'cherry'];
            arr.sort(c.compare);
            document.getElementById('r').textContent = arr.join(',');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">apple,banana,cherry<" in result

    def test_relative_time_format(self):
        html = """<html><body><div id="r"></div><script>
            var rtf = new Intl.RelativeTimeFormat('en', {numeric: 'auto'});
            document.getElementById('r').textContent = rtf.format(-1, 'day');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "yesterday" in result

    def test_display_names(self):
        html = """<html><body><div id="r"></div><script>
            var dn = new Intl.DisplayNames(['en'], {type: 'region'});
            document.getElementById('r').textContent = dn.of('US');
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "United States" in result

    def test_segmenter(self):
        html = """<html><body><div id="r"></div><script>
            var seg = new Intl.Segmenter('en', {granularity: 'word'});
            var segments = Array.from(seg.segment('Hello World'));
            document.getElementById('r').textContent = segments.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        # "Hello", " ", "World" = 3 segments
        assert ">3<" in result


# ══════════════════════════════════════════════════════════════════════════════
# Navigator & Location APIs
# ══════════════════════════════════════════════════════════════════════════════


class TestNavigatorAPIs:
    """navigator.* properties."""

    def test_user_agent(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent =
                (typeof navigator.userAgent === 'string' && navigator.userAgent.length > 0) ? 'ok' : 'fail';
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result

    def test_language(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = navigator.language;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">en<" in result

    def test_languages(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = Array.isArray(navigator.languages) + ',' + navigator.languages[0];
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,en<" in result

    def test_online(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = navigator.onLine;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_hardware_concurrency(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = typeof navigator.hardwareConcurrency;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">number<" in result

    def test_cookie_enabled(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = navigator.cookieEnabled;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_send_beacon(self):
        html = """<html><body><div id="r">ok</div><script>
            navigator.sendBeacon("/track", "data");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestLocationAPIs:
    """location.* properties."""

    def test_href(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = typeof location.href;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">string<" in result

    def test_protocol(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = location.protocol;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "about:" in result or "http:" in result

    def test_origin(self):
        html = """<html><body><div id="r"></div><script>
            document.getElementById('r').textContent = typeof location.origin;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">string<" in result

    def test_assign_no_crash(self):
        html = """<html><body><div id="r">ok</div><script>
            location.assign("http://example.com");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result


class TestStorageAPIs:
    """localStorage/sessionStorage."""

    def test_local_storage_set_get(self):
        html = """<html><body><div id="r"></div><script>
            localStorage.setItem("key", "value");
            document.getElementById('r').textContent = localStorage.getItem("key");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">value<" in result

    def test_local_storage_remove(self):
        html = """<html><body><div id="r"></div><script>
            localStorage.setItem("k", "v");
            localStorage.removeItem("k");
            document.getElementById('r').textContent = String(localStorage.getItem("k"));
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result

    def test_local_storage_clear(self):
        """After clear(), length should be 0 (as a getter property, not a method)."""
        html = """<html><body><div id="r"></div><script>
            localStorage.setItem("a", "1");
            localStorage.setItem("b", "2");
            localStorage.clear();
            document.getElementById('r').textContent = localStorage.length;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0<" in result

    def test_session_storage(self):
        html = """<html><body><div id="r"></div><script>
            sessionStorage.setItem("s", "session");
            document.getElementById('r').textContent = sessionStorage.getItem("s");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">session<" in result
