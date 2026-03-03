"""Tests for Batch 9: XMLHttpRequest."""

import blazeweb


class TestXMLHttpRequest:
    def test_basic_get(self, httpserver):
        httpserver.expect_request("/data").respond_with_data(
            "hello from server",
            content_type="text/plain",
        )
        url = httpserver.url_for("/data")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("GET", "{url}");
            xhr.onload = function() {{
                document.getElementById('r').textContent = xhr.responseText;
            }};
            xhr.send();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">hello from server<" in result

    def test_status_code(self, httpserver):
        httpserver.expect_request("/ok").respond_with_data("ok", status=200)
        url = httpserver.url_for("/ok")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("GET", "{url}");
            xhr.onload = function() {{
                document.getElementById('r').textContent = xhr.status;
            }};
            xhr.send();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">200<" in result

    def test_ready_state_transitions(self, httpserver):
        httpserver.expect_request("/rs").respond_with_data("ok")
        url = httpserver.url_for("/rs")
        html = f"""<html><body><div id="r"></div><script>
            var states = [];
            var xhr = new XMLHttpRequest();
            states.push(xhr.readyState);
            xhr.open("GET", "{url}");
            states.push(xhr.readyState);
            xhr.onreadystatechange = function() {{
                states.push(xhr.readyState);
            }};
            xhr.send();
            document.getElementById('r').textContent = states.join(',');
        </script></body></html>"""
        result = blazeweb.render(html)
        # Per XHR spec: 0 (UNSENT), 1 (OPENED), then 2,3,4 from onreadystatechange
        assert ">0,1,2,3,4<" in result

    def test_response_headers(self, httpserver):
        httpserver.expect_request("/hdr").respond_with_data(
            "ok",
            content_type="application/json",
        )
        url = httpserver.url_for("/hdr")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("GET", "{url}");
            xhr.onload = function() {{
                var ct = xhr.getResponseHeader("content-type");
                document.getElementById('r').textContent = ct;
            }};
            xhr.send();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "application/json" in result

    def test_get_all_response_headers(self, httpserver):
        httpserver.expect_request("/allhdr").respond_with_data("ok")
        url = httpserver.url_for("/allhdr")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("GET", "{url}");
            xhr.onload = function() {{
                var hdrs = xhr.getAllResponseHeaders();
                document.getElementById('r').textContent =
                    (typeof hdrs === 'string' && hdrs.length > 0) ? 'ok' : 'fail';
            }};
            xhr.send();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">ok<" in result

    def test_constants(self):
        html = """<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            document.getElementById('r').textContent =
                xhr.UNSENT + ',' + xhr.OPENED + ',' +
                xhr.HEADERS_RECEIVED + ',' + xhr.LOADING + ',' + xhr.DONE;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">0,1,2,3,4<" in result

    def test_set_request_header(self, httpserver):
        httpserver.expect_request("/custom").respond_with_data("ok")
        url = httpserver.url_for("/custom")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("GET", "{url}");
            xhr.setRequestHeader("X-Custom", "test-value");
            xhr.onload = function() {{
                document.getElementById('r').textContent = xhr.status;
            }};
            xhr.send();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">200<" in result

    def test_post_request(self, httpserver):
        httpserver.expect_request("/post", method="POST").respond_with_data("received")
        url = httpserver.url_for("/post")
        html = f"""<html><body><div id="r"></div><script>
            var xhr = new XMLHttpRequest();
            xhr.open("POST", "{url}");
            xhr.onload = function() {{
                document.getElementById('r').textContent = xhr.responseText;
            }};
            xhr.send("body data");
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">received<" in result
