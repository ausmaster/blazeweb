"""E2E tests: Events, timers, EventTarget, event constructors"""

from .conftest import text_of, render
import blazeweb

class TestTimers:
    def test_request_animation_frame(self):
        html = render("""<html><body>
        <div id="result">pending</div>
        <script>
            requestAnimationFrame(function() {
                document.getElementById('result').textContent = 'raf-fired';
            });
        </script></body></html>""")
        assert text_of(html, "result") == "raf-fired"

    def test_nested_timers(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var order = [];
            setTimeout(function() {
                order.push('outer');
                setTimeout(function() {
                    order.push('inner');
                    document.getElementById('result').textContent = order.join(',');
                }, 0);
            }, 0);
        </script></body></html>""")
        assert text_of(html, "result") == "outer,inner"

    def test_timer_ordering(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var order = [];
            setTimeout(function() { order.push('a'); }, 10);
            setTimeout(function() { order.push('b'); }, 0);
            setTimeout(function() {
                order.push('c');
                document.getElementById('result').textContent = order.join(',');
            }, 20);
        </script></body></html>""")
        assert text_of(html, "result") == "b,a,c"


# ─── Events ──────────────────────────────────────────────────────────────────


class TestEvents:
    def test_remove_event_listener(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var count = 0;
            function handler() { count++; }
            var el = document.getElementById('result');
            el.addEventListener('click', handler);
            el.removeEventListener('click', handler);
            el.dispatchEvent(new Event('click'));
            el.textContent = String(count);
        </script></body></html>""")
        assert text_of(html, "result") == "0"

    def test_custom_event(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var detail = null;
            document.addEventListener('myevent', function(e) {
                detail = e.detail;
            });
            document.dispatchEvent(new CustomEvent('myevent',
                { detail: 'payload' }));
            document.getElementById('result').textContent = String(detail);
        </script></body></html>""")
        assert text_of(html, "result") == "payload"


# ─── Web APIs ────────────────────────────────────────────────────────────────


class TestEventTargetConstructor:
    def test_typeof_event_target(self):
        """EventTarget should be a function."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = typeof EventTarget;
        </script></body></html>""")
        assert text_of(html, "result") == "function"

    def test_event_target_has_methods(self):
        """new EventTarget() should have addEventListener."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var et = new EventTarget();
            document.getElementById('result').textContent =
                (typeof et.addEventListener === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_event_target_dispatch(self):
        """EventTarget should support add/dispatch."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var et = new EventTarget();
            var result = "";
            et.addEventListener("test", function(e) { result = e.type; });
            et.dispatchEvent(new Event("test"));
            document.getElementById('result').textContent = result;
        </script></body></html>""")
        assert text_of(html, "result") == "test"

    def test_event_target_instanceof(self):
        """new EventTarget() should pass instanceof check."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var et = new EventTarget();
            document.getElementById('result').textContent =
                (et instanceof EventTarget).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_event_target_extends(self):
        """class extending EventTarget should work."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            class MyEmitter extends EventTarget {
                constructor() { super(); this.x = 42; }
            }
            var em = new MyEmitter();
            var result = "";
            em.addEventListener("ping", function() { result = "pong"; });
            em.dispatchEvent(new Event("ping"));
            document.getElementById('result').textContent =
                result + ":" + em.x + ":" + (em instanceof EventTarget);
        </script></body></html>""")
        assert text_of(html, "result") == "pong:42:true"

    def test_event_target_listeners_not_enumerable(self):
        """Internal listener storage should not be enumerable."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var et = new EventTarget();
            document.getElementById('result').textContent =
                Object.keys(et).length.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "0"


class TestEventConstructorExpansion:
    def test_uievent_detail(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = new UIEvent("test").detail.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "0"

    def test_wheel_event(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = (typeof WheelEvent === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_touch_event(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = (typeof TouchEvent === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_transition_event_property(self):
        html = render("""<html><body><div id="result"></div><script>
            var e = new TransitionEvent("transitionend", {propertyName: "opacity"});
            document.getElementById('result').textContent = e.propertyName;
        </script></body></html>""")
        assert text_of(html, "result") == "opacity"

    def test_animation_event_name(self):
        html = render("""<html><body><div id="result"></div><script>
            var e = new AnimationEvent("animationend", {animationName: "fade"});
            document.getElementById('result').textContent = e.animationName;
        </script></body></html>""")
        assert text_of(html, "result") == "fade"

    def test_message_event_data(self):
        html = render("""<html><body><div id="result"></div><script>
            var e = new MessageEvent("message", {data: "hello"});
            document.getElementById('result').textContent = e.data;
        </script></body></html>""")
        assert text_of(html, "result") == "hello"

    def test_progress_event_loaded(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = new ProgressEvent("load").loaded.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "0"

    def test_promise_rejection_event(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = (typeof PromiseRejectionEvent === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_storage_event(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = (typeof StorageEvent === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_clipboard_event(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = (typeof ClipboardEvent === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_drag_event(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = (typeof DragEvent === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_submit_event(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent = (typeof SubmitEvent === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"


# ─── Round 2 Phase 4: Navigator Stubs + Window on* Handlers ─────────────────



