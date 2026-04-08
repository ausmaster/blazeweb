"""E2E tests: Script execution (modules, data URLs, PerformanceObserver)"""

from .conftest import text_of, render
import blazeweb

class TestModuleEdgeCases:
    def test_module_timers(self):
        html = render("""<html><body>
        <div id="result">pending</div>
        <script type="module">
            setTimeout(function() {
                document.getElementById('result').textContent = 'timer-in-module';
            }, 0);
        </script></body></html>""")
        assert text_of(html, "result") == "timer-in-module"

    def test_module_add_event_listener(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script type="module">
            document.addEventListener('custom', function(e) {
                document.getElementById('result').textContent = 'heard';
            });
            document.dispatchEvent(new Event('custom'));
        </script></body></html>""")
        assert text_of(html, "result") == "heard"

    def test_classic_cannot_see_module_const(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script type="module">
            const MODULE_SECRET = 42;
        </script>
        <script>
            document.getElementById('result').textContent =
                typeof MODULE_SECRET === 'undefined' ? 'hidden' : 'visible';
        </script></body></html>""")
        assert text_of(html, "result") == "hidden"

    def test_module_error_does_not_affect_classic(self):
        result = blazeweb.render("""<html><body>
        <div id="result"></div>
        <script>
            window.classicRan = true;
        </script>
        <script type="module">
            throw new Error('module boom');
        </script>
        <script>
            document.getElementById('result').textContent =
                window.classicRan ? 'classic-ok' : 'classic-failed';
        </script></body></html>""")
        assert "classic-ok" in result

    def test_module_dom_content_loaded_fires_after_modules(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            window.order = [];
        </script>
        <script type="module">
            window.order.push('module');
        </script>
        <script>
            document.addEventListener('DOMContentLoaded', function() {
                window.order.push('dcl');
                document.getElementById('result').textContent =
                    window.order.join(',');
            });
        </script></body></html>""")
        assert text_of(html, "result") == "module,dcl"

    def test_module_arguments_not_defined(self):
        result = blazeweb.render("""<html><body>
        <div id="result"></div>
        <script type="module">
            try {
                void arguments;
                document.getElementById('result').textContent = 'has-arguments';
            } catch(e) {
                document.getElementById('result').textContent = e.name;
            }
        </script></body></html>""")
        assert "ReferenceError" in result

    def test_module_globalthis_is_window(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script type="module">
            document.getElementById('result').textContent =
                String(globalThis === window);
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_no_scripts_fast_path(self):
        """HTML without scripts should render without V8."""
        html = render("<html><body><p>Hello</p></body></html>")
        assert "<p>Hello</p>" in html


# ─── data: URL scripts ──────────────────────────────────────────────────────


class TestDataURLScripts:
    def test_data_url_script_src(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script src="data:text/javascript,document.getElementById('result').textContent='data-ok'"></script>
        </body></html>""")
        assert text_of(html, "result") == "data-ok"

    def test_data_url_base64_script(self):
        import base64
        js = "document.getElementById('result').textContent='b64-ok'"
        b64 = base64.b64encode(js.encode()).decode()
        html = render(f"""<html><body>
        <div id="result"></div>
        <script src="data:text/javascript;base64,{b64}"></script>
        </body></html>""")
        assert text_of(html, "result") == "b64-ok"

    def test_data_url_percent_encoded(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script src="data:text/javascript,var%20x%20%3D%2042%3B%0Adocument.getElementById('result').textContent%20%3D%20String(x)"></script>
        </body></html>""")
        assert text_of(html, "result") == "42"


# ─── PerformanceObserver ──────────────────────────────────────────────────────


class TestPerformanceObserver:
    def test_observe_receives_marks(self):
        """PerformanceObserver callback fires with mark entries during drain."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var entries = [];
            var observer = new PerformanceObserver(function(list) {
                var items = list.getEntries();
                for (var i = 0; i < items.length; i++) {
                    entries.push(items[i].name);
                }
            });
            observer.observe({entryTypes: ['mark']});
            performance.mark('test-mark');
            performance.mark('another-mark');
            // Callback fires asynchronously during drain — use setTimeout to read
            setTimeout(function() {
                document.getElementById('result').textContent = entries.join(',');
            }, 0);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "test-mark,another-mark"

    def test_observe_receives_measures(self):
        """PerformanceObserver callback fires with measure entries during drain."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var results = [];
            var observer = new PerformanceObserver(function(list) {
                var items = list.getEntries();
                for (var i = 0; i < items.length; i++) {
                    results.push(items[i].name + ':' + items[i].entryType);
                }
            });
            observer.observe({entryTypes: ['measure']});
            performance.mark('start');
            performance.measure('my-measure', 'start');
            setTimeout(function() {
                document.getElementById('result').textContent = results.join(',');
            }, 0);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "my-measure:measure"

    def test_disconnect_stops_observation(self):
        """After disconnect(), no more entries are delivered."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var count = 0;
            var observer = new PerformanceObserver(function(list) {
                count += list.getEntries().length;
            });
            observer.observe({entryTypes: ['mark']});
            performance.mark('before');
            observer.disconnect();
            performance.mark('after');
            setTimeout(function() {
                document.getElementById('result').textContent = String(count);
            }, 0);
        </script>
        </body></html>""")
        # 'before' was queued but disconnect() clears pending entries
        # 'after' was not queued because observer is disconnected
        assert text_of(html, "result") == "0"

    def test_take_records(self):
        """takeRecords() returns pending entries and clears the buffer."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var observer = new PerformanceObserver(function() {});
            observer.observe({entryTypes: ['mark']});
            performance.mark('m1');
            performance.mark('m2');
            var records = observer.takeRecords();
            var remaining = observer.takeRecords();
            document.getElementById('result').textContent =
                records.length + ',' + remaining.length;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "2,0"

    def test_supported_entry_types(self):
        """PerformanceObserver.supportedEntryTypes is accessible."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var types = PerformanceObserver.supportedEntryTypes;
            document.getElementById('result').textContent =
                Array.isArray(types) + ',' + types.length;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true,2"

    def test_entry_list_get_entries_by_type(self):
        """PerformanceObserverEntryList.getEntriesByType() filters correctly."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var result = '';
            var observer = new PerformanceObserver(function(list) {
                var marks = list.getEntriesByType('mark');
                var measures = list.getEntriesByType('measure');
                result = marks.length + ',' + measures.length;
            });
            observer.observe({entryTypes: ['mark', 'measure']});
            performance.mark('m1');
            performance.mark('m2');
            performance.measure('op');
            setTimeout(function() {
                document.getElementById('result').textContent = result;
            }, 0);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "2,1"

    def test_entry_list_get_entries_by_name(self):
        """PerformanceObserverEntryList.getEntriesByName() filters correctly."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var result = '';
            var observer = new PerformanceObserver(function(list) {
                var byName = list.getEntriesByName('target');
                result = String(byName.length);
            });
            observer.observe({entryTypes: ['mark']});
            performance.mark('target');
            performance.mark('other');
            performance.mark('target');
            setTimeout(function() {
                document.getElementById('result').textContent = result;
            }, 0);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "2"

    def test_performance_mark_returns_entry(self):
        """performance.mark() returns a PerformanceMark entry."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var entry = performance.mark('test');
            document.getElementById('result').textContent =
                entry.name + ',' + entry.entryType + ',' + entry.duration;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "test,mark,0"

    def test_performance_get_entries_by_type(self):
        """performance.getEntriesByType() returns timeline entries."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            performance.mark('a');
            performance.mark('b');
            performance.measure('m');
            var marks = performance.getEntriesByType('mark');
            var measures = performance.getEntriesByType('measure');
            document.getElementById('result').textContent =
                marks.length + ',' + measures.length;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "2,1"

    def test_performance_clear_marks(self):
        """performance.clearMarks() removes marks from timeline."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            performance.mark('a');
            performance.mark('b');
            performance.clearMarks('a');
            var marks = performance.getEntriesByType('mark');
            document.getElementById('result').textContent =
                marks.length + ',' + marks[0].name;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "1,b"

    def test_constructor_requires_callback(self):
        """PerformanceObserver constructor throws without callback."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            try {
                new PerformanceObserver();
                document.getElementById('result').textContent = 'no-error';
            } catch(e) {
                document.getElementById('result').textContent = 'error';
            }
        </script>
        </body></html>""")
        assert text_of(html, "result") == "error"

    def test_observe_single_type_mode(self):
        """observe({type: 'mark'}) works in single-type mode."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var count = 0;
            var observer = new PerformanceObserver(function(list) {
                count += list.getEntries().length;
            });
            observer.observe({type: 'mark'});
            performance.mark('x');
            setTimeout(function() {
                document.getElementById('result').textContent = String(count);
            }, 0);
        </script>
        </body></html>""")
        assert text_of(html, "result") == "1"


# ─── Legacy DOM (Phase 1) ────────────────────────────────────────────────────



