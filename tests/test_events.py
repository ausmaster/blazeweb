"""Tests for Batch 8: Event constructors — MouseEvent, KeyboardEvent,
FocusEvent, InputEvent, PointerEvent, ErrorEvent, HashChangeEvent, PopStateEvent."""

import blazeweb


class TestMouseEvent:
    def test_basic_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new MouseEvent("click");
            document.getElementById('r').textContent =
                evt.type + ',' + evt.clientX + ',' + evt.clientY;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">click,0,0<" in result

    def test_with_init(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new MouseEvent("click", {clientX: 10, clientY: 20, bubbles: true});
            document.getElementById('r').textContent =
                evt.clientX + ',' + evt.clientY + ',' + evt.bubbles;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">10,20,true<" in result

    def test_get_modifier_state(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new MouseEvent("click");
            document.getElementById('r').textContent =
                evt.getModifierState("Control").toString();
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_button_and_buttons(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new MouseEvent("mousedown", {button: 2, buttons: 2});
            document.getElementById('r').textContent =
                evt.button + ',' + evt.buttons;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">2,2<" in result

    def test_modifier_keys(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new MouseEvent("click", {altKey: true, ctrlKey: true, metaKey: false, shiftKey: true});
            document.getElementById('r').textContent =
                evt.altKey + ',' + evt.ctrlKey + ',' + evt.metaKey + ',' + evt.shiftKey;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,true,false,true<" in result


class TestKeyboardEvent:
    def test_basic(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new KeyboardEvent("keydown", {key: "Enter", code: "Enter"});
            document.getElementById('r').textContent =
                evt.type + ',' + evt.key + ',' + evt.code;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">keydown,Enter,Enter<" in result

    def test_defaults(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new KeyboardEvent("keyup");
            document.getElementById('r').textContent =
                evt.key + '|' + evt.repeat + '|' + evt.isComposing;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">|false|false<" in result

    def test_repeat_and_modifiers(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new KeyboardEvent("keydown", {key: "a", repeat: true, shiftKey: true});
            document.getElementById('r').textContent =
                evt.repeat + ',' + evt.shiftKey;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true,true<" in result


class TestFocusEvent:
    def test_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new FocusEvent("focus");
            document.getElementById('r').textContent =
                evt.type + ',' + String(evt.relatedTarget);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">focus,null<" in result

    def test_with_related_target(self):
        html = """<html><body><div id="a"></div><div id="r"></div><script>
            var target = document.getElementById('a');
            var evt = new FocusEvent("focusin", {relatedTarget: target});
            document.getElementById('r').textContent =
                evt.type + ',' + (evt.relatedTarget !== null);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">focusin,true<" in result


class TestInputEvent:
    def test_with_data(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new InputEvent("input", {data: "x", inputType: "insertText"});
            document.getElementById('r').textContent =
                evt.data + ',' + evt.inputType;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">x,insertText<" in result

    def test_defaults(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new InputEvent("input");
            document.getElementById('r').textContent =
                String(evt.data) + ',' + evt.inputType + ',' + evt.isComposing;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null,,false<" in result


class TestPointerEvent:
    def test_with_init(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new PointerEvent("pointerdown", {
                pointerId: 1, pointerType: "mouse", isPrimary: true, pressure: 0.5
            });
            document.getElementById('r').textContent =
                evt.pointerId + ',' + evt.pointerType + ',' + evt.isPrimary + ',' + evt.pressure;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1,mouse,true,0.5<" in result

    def test_defaults(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new PointerEvent("pointermove");
            document.getElementById('r').textContent =
                evt.width + ',' + evt.height + ',' + evt.isPrimary;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1,1,false<" in result


class TestErrorEvent:
    def test_with_init(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new ErrorEvent("error", {message: "oops", lineno: 42, colno: 10});
            document.getElementById('r').textContent =
                evt.message + ',' + evt.lineno + ',' + evt.colno;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">oops,42,10<" in result

    def test_defaults(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new ErrorEvent("error");
            document.getElementById('r').textContent =
                evt.message + '|' + evt.lineno + '|' + String(evt.error);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">|0|null<" in result


class TestHashChangeEvent:
    def test_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new HashChangeEvent("hashchange", {oldURL: "#old", newURL: "#new"});
            document.getElementById('r').textContent =
                evt.type + ',' + evt.oldURL + ',' + evt.newURL;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">hashchange,#old,#new<" in result


class TestPopStateEvent:
    def test_constructor(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new PopStateEvent("popstate", {state: {page: 1}});
            document.getElementById('r').textContent = evt.state.page;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">1<" in result

    def test_null_state(self):
        """Per HTML spec, PopStateEvent.state defaults to null when no init dict provided."""
        html = """<html><body><div id="r"></div><script>
            var evt = new PopStateEvent("popstate");
            document.getElementById('r').textContent = String(evt.state);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">null<" in result


class TestEventPreventDefault:
    def test_prevent_default_on_cancelable(self):
        """Per DOM spec, preventDefault() sets defaultPrevented only when cancelable is true."""
        html = """<html><body><div id="r"></div><script>
            var evt = new Event("test", {cancelable: true});
            evt.preventDefault();
            document.getElementById('r').textContent = evt.defaultPrevented;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">true<" in result

    def test_prevent_default_on_non_cancelable(self):
        """Per DOM spec, preventDefault() must NOT set defaultPrevented when cancelable is false."""
        html = """<html><body><div id="r"></div><script>
            var evt = new Event("test");
            evt.preventDefault();
            document.getElementById('r').textContent = evt.defaultPrevented;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">false<" in result

    def test_custom_event_detail(self):
        html = """<html><body><div id="r"></div><script>
            var evt = new CustomEvent("my-event", {detail: {foo: "bar"}});
            document.getElementById('r').textContent = evt.detail.foo;
        </script></body></html>"""
        result = blazeweb.render(html)
        assert ">bar<" in result
