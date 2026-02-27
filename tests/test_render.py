"""Integration tests for blazeclient.render()."""

import blazeclient
import pytest


class TestRenderBasic:
    """Basic HTML rendering without scripts."""

    def test_plain_html(self):
        result = blazeclient.render("<html><body><p>Hello</p></body></html>")
        assert "<p>Hello</p>" in result

    def test_bytes_input(self):
        result = blazeclient.render(b"<html><body>OK</body></html>")
        assert "OK" in result

    def test_str_input(self):
        result = blazeclient.render("<html><body>OK</body></html>")
        assert "OK" in result

    def test_no_scripts_passthrough(self):
        html = "<html><head><title>Test</title></head><body><div>Content</div></body></html>"
        result = blazeclient.render(html)
        assert "<div>Content</div>" in result
        assert "<title>Test</title>" in result


class TestInlineScripts:
    """Inline <script> execution."""

    def test_noop_script(self):
        result = blazeclient.render("<html><body><script>var x = 1;</script><p>Hi</p></body></html>")
        assert "<p>Hi</p>" in result

    def test_set_text_content(self):
        html = """<html><body>
            <p id="target">old</p>
            <script>document.getElementById('target').textContent = 'new';</script>
        </body></html>"""
        result = blazeclient.render(html)
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
        result = blazeclient.render(html)
        assert "<span>dynamic</span>" in result

    def test_set_attribute(self):
        html = """<html><body>
            <div id="target"></div>
            <script>document.getElementById('target').setAttribute('class', 'active');</script>
        </body></html>"""
        result = blazeclient.render(html)
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
        result = blazeclient.render(html)
        assert "remove me" not in result

    def test_inner_html_set(self):
        html = """<html><body>
            <div id="target">old</div>
            <script>document.getElementById('target').innerHTML = '<b>bold</b>';</script>
        </body></html>"""
        result = blazeclient.render(html)
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
        result = blazeclient.render(html)
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
        result = blazeclient.render(html)
        assert "survived" in result

    def test_script_type_json_ignored(self):
        html = """<html><body>
            <script type="application/json">{"not": "executed"}</script>
            <p>still here</p>
        </body></html>"""
        result = blazeclient.render(html)
        assert "still here" in result

    def test_script_type_module_ignored(self):
        html = """<html><body>
            <script type="module">import x from './x';</script>
            <p>still here</p>
        </body></html>"""
        result = blazeclient.render(html)
        assert "still here" in result

    def test_console_log_no_crash(self):
        html = "<html><body><script>console.log('hello'); console.warn('w'); console.error('e');</script></body></html>"
        result = blazeclient.render(html)
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
        result = blazeclient.render(html)
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
        result = blazeclient.render(html)
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
        result = blazeclient.render(html)
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
        result = blazeclient.render(html)
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
        result = blazeclient.render(html, base_url=httpserver.url_for("/"))
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
        result = blazeclient.render(html)
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
        result = blazeclient.render(html)
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
        client = blazeclient.Client()
        result = client.render("<html><body><p>hi</p></body></html>")
        assert "<p>hi</p>" in result

    def test_render_with_inline_script(self):
        client = blazeclient.Client()
        html = """<html><body>
            <div id="out"></div>
            <script>document.getElementById('out').textContent = 'ok';</script>
        </body></html>"""
        result = client.render(html)
        assert "ok" in result

    def test_render_str_input(self):
        """Client.render accepts str (auto-encoded to UTF-8 by PyO3)."""
        client = blazeclient.Client()
        result = client.render("<html><body>OK</body></html>")
        assert "OK" in result

    def test_cache_populated_by_external_scripts(self, httpserver):
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        client = blazeclient.Client()
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
        client = blazeclient.Client()
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
        client = blazeclient.Client()
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
        client = blazeclient.Client()
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
        client = blazeclient.Client()
        client.render(html, cache_read=False)
        assert client.cache_size == 1  # written even though read was disabled

    def test_class_level_cache_toggle(self, httpserver):
        httpserver.expect_request("/app.js").respond_with_data(
            "var x = 1;", content_type="application/javascript",
        )
        html = f"""<html><body>
            <script src="{httpserver.url_for('/app.js')}"></script>
        </body></html>"""
        client = blazeclient.Client()
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
        client = blazeclient.Client(cache=False)
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
        client = blazeclient.Client()
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
        c1 = blazeclient.Client()
        c2 = blazeclient.Client()
        c1.render(html)
        assert c1.cache_size == 1
        assert c2.cache_size == 0  # c2 has its own empty cache

    def test_constructor_defaults(self):
        client = blazeclient.Client()
        assert client.cache is True
        assert client.cache_read is True
        assert client.cache_write is True

    def test_constructor_kwargs(self):
        client = blazeclient.Client(cache=False, cache_read=False, cache_write=False)
        assert client.cache is False
        assert client.cache_read is False
        assert client.cache_write is False
