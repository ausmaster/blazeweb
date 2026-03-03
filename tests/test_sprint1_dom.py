"""Comprehensive tests for Sprint 1 DOM improvements.

Tests cover:
1. Event bubbling & capture (WHATWG §2.4 dispatch)
2. Pre-insertion validation (ensure_pre_insertion_validity)
3. NodeFlags and isConnected property
4. Live childNodes NodeList
5. DocumentFragment behavior
"""

import blazeweb
import pytest


def run_js(js: str) -> str:
    """Run JS inside a minimal HTML page, return #result textContent."""
    html = f"""<!DOCTYPE html><html><head></head><body>
<div id="result"></div>
<script>
try {{
    {js}
}} catch(e) {{
    document.getElementById('result').textContent = 'ERROR:' + e.name + ':' + e.message;
}}
</script></body></html>"""
    result = blazeweb.render(html)
    import re
    m = re.search(r'<div id="result">(.*?)</div>', result, re.DOTALL)
    return m.group(1) if m else ""


# ═══════════════════════════════════════════════════════════════════════════════
# 1. EVENT BUBBLING & CAPTURE
# ═══════════════════════════════════════════════════════════════════════════════

class TestEventBubbling:
    """Test full WHATWG DOM §2.4 event dispatch with capture→at-target→bubble."""

    def test_event_bubbles_to_parent(self):
        """Events with bubbles:true should bubble up to parent listeners."""
        result = run_js("""
            var log = [];
            var parent = document.createElement('div');
            var child = document.createElement('span');
            parent.appendChild(child);
            document.body.appendChild(parent);
            parent.addEventListener('click', function(e) { log.push('parent'); });
            child.addEventListener('click', function(e) { log.push('child'); });
            var evt = new Event('click', {bubbles: true});
            child.dispatchEvent(evt);
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "child,parent"

    def test_event_bubbles_to_grandparent(self):
        """Events should bubble through multiple ancestor levels."""
        result = run_js("""
            var log = [];
            var gp = document.createElement('div');
            var parent = document.createElement('div');
            var child = document.createElement('span');
            gp.appendChild(parent);
            parent.appendChild(child);
            document.body.appendChild(gp);
            gp.addEventListener('click', function(e) { log.push('gp'); });
            parent.addEventListener('click', function(e) { log.push('parent'); });
            child.addEventListener('click', function(e) { log.push('child'); });
            var evt = new Event('click', {bubbles: true});
            child.dispatchEvent(evt);
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "child,parent,gp"

    def test_event_no_bubble_when_bubbles_false(self):
        """Events with bubbles:false should NOT bubble."""
        result = run_js("""
            var log = [];
            var parent = document.createElement('div');
            var child = document.createElement('span');
            parent.appendChild(child);
            document.body.appendChild(parent);
            parent.addEventListener('focus', function(e) { log.push('parent'); });
            child.addEventListener('focus', function(e) { log.push('child'); });
            var evt = new Event('focus', {bubbles: false});
            child.dispatchEvent(evt);
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "child"

    def test_event_bubbles_to_document(self):
        """Events should bubble all the way to document."""
        result = run_js("""
            var log = [];
            var div = document.createElement('div');
            document.body.appendChild(div);
            document.addEventListener('click', function(e) { log.push('doc'); });
            div.addEventListener('click', function(e) { log.push('div'); });
            var evt = new Event('click', {bubbles: true});
            div.dispatchEvent(evt);
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "div,doc"

    def test_event_bubbles_to_window(self):
        """Events should bubble from target → ... → document → window."""
        result = run_js("""
            var log = [];
            var div = document.createElement('div');
            document.body.appendChild(div);
            window.addEventListener('click', function(e) { log.push('window'); });
            document.addEventListener('click', function(e) { log.push('doc'); });
            div.addEventListener('click', function(e) { log.push('div'); });
            var evt = new Event('click', {bubbles: true});
            div.dispatchEvent(evt);
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "div,doc,window"


class TestEventCapture:
    """Test the capture phase of event dispatch."""

    def test_capture_fires_before_bubble(self):
        """Capture listeners fire before bubble listeners."""
        result = run_js("""
            var log = [];
            var parent = document.createElement('div');
            var child = document.createElement('span');
            parent.appendChild(child);
            document.body.appendChild(parent);
            parent.addEventListener('click', function(e) { log.push('parent-bubble'); }, false);
            parent.addEventListener('click', function(e) { log.push('parent-capture'); }, true);
            child.addEventListener('click', function(e) { log.push('child'); });
            var evt = new Event('click', {bubbles: true});
            child.dispatchEvent(evt);
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "parent-capture,child,parent-bubble"

    def test_capture_phase_order(self):
        """Capture phase should fire from outermost to innermost."""
        result = run_js("""
            var log = [];
            var gp = document.createElement('div');
            var parent = document.createElement('div');
            var child = document.createElement('span');
            gp.appendChild(parent);
            parent.appendChild(child);
            document.body.appendChild(gp);
            gp.addEventListener('click', function() { log.push('gp-cap'); }, true);
            parent.addEventListener('click', function() { log.push('parent-cap'); }, true);
            child.addEventListener('click', function() { log.push('child'); });
            gp.addEventListener('click', function() { log.push('gp-bub'); }, false);
            parent.addEventListener('click', function() { log.push('parent-bub'); }, false);
            child.dispatchEvent(new Event('click', {bubbles: true}));
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "gp-cap,parent-cap,child,parent-bub,gp-bub"

    def test_at_target_fires_all_listeners(self):
        """At-target phase fires both capture and non-capture listeners."""
        result = run_js("""
            var log = [];
            var div = document.createElement('div');
            document.body.appendChild(div);
            div.addEventListener('click', function() { log.push('capture'); }, true);
            div.addEventListener('click', function() { log.push('bubble'); }, false);
            div.dispatchEvent(new Event('click', {bubbles: true}));
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "capture,bubble"


class TestStopPropagation:
    """Test stopPropagation and stopImmediatePropagation."""

    def test_stop_propagation(self):
        """stopPropagation prevents event from reaching ancestors."""
        result = run_js("""
            var log = [];
            var parent = document.createElement('div');
            var child = document.createElement('span');
            parent.appendChild(child);
            document.body.appendChild(parent);
            parent.addEventListener('click', function() { log.push('parent'); });
            child.addEventListener('click', function(e) {
                log.push('child');
                e.stopPropagation();
            });
            child.dispatchEvent(new Event('click', {bubbles: true}));
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "child"

    def test_stop_immediate_propagation(self):
        """stopImmediatePropagation prevents subsequent listeners on same target."""
        result = run_js("""
            var log = [];
            var div = document.createElement('div');
            document.body.appendChild(div);
            div.addEventListener('click', function(e) {
                log.push('first');
                e.stopImmediatePropagation();
            });
            div.addEventListener('click', function() { log.push('second'); });
            div.dispatchEvent(new Event('click', {bubbles: true}));
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "first"

    def test_stop_propagation_in_capture(self):
        """stopPropagation during capture prevents target and bubble phases."""
        result = run_js("""
            var log = [];
            var parent = document.createElement('div');
            var child = document.createElement('span');
            parent.appendChild(child);
            document.body.appendChild(parent);
            parent.addEventListener('click', function(e) {
                log.push('parent-capture');
                e.stopPropagation();
            }, true);
            child.addEventListener('click', function() { log.push('child'); });
            parent.addEventListener('click', function() { log.push('parent-bubble'); });
            child.dispatchEvent(new Event('click', {bubbles: true}));
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "parent-capture"


class TestEventProperties:
    """Test event properties during dispatch (target, currentTarget, eventPhase)."""

    def test_event_target_is_dispatch_target(self):
        """event.target should be the node dispatchEvent was called on."""
        result = run_js("""
            var log = [];
            var parent = document.createElement('div');
            parent.id = 'parent';
            var child = document.createElement('span');
            child.id = 'child';
            parent.appendChild(child);
            document.body.appendChild(parent);
            parent.addEventListener('click', function(e) {
                log.push('target:' + e.target.id);
                log.push('currentTarget:' + e.currentTarget.id);
            });
            child.dispatchEvent(new Event('click', {bubbles: true}));
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "target:child,currentTarget:parent"

    def test_event_phase_values(self):
        """eventPhase should be 1 (capture), 2 (at-target), 3 (bubble)."""
        result = run_js("""
            var log = [];
            var parent = document.createElement('div');
            var child = document.createElement('span');
            parent.appendChild(child);
            document.body.appendChild(parent);
            parent.addEventListener('click', function(e) { log.push('cap:' + e.eventPhase); }, true);
            child.addEventListener('click', function(e) { log.push('target:' + e.eventPhase); });
            parent.addEventListener('click', function(e) { log.push('bub:' + e.eventPhase); });
            child.dispatchEvent(new Event('click', {bubbles: true}));
            document.getElementById('result').textContent = log.join(',');
        """)
        assert result == "cap:1,target:2,bub:3"

    def test_event_phase_none_after_dispatch(self):
        """eventPhase should be 0 (NONE) after dispatch completes."""
        result = run_js("""
            var evt = new Event('click', {bubbles: true});
            var div = document.createElement('div');
            document.body.appendChild(div);
            div.dispatchEvent(evt);
            document.getElementById('result').textContent = String(evt.eventPhase);
        """)
        assert result == "0"

    def test_composed_path(self):
        """composedPath() should return the propagation path during dispatch."""
        result = run_js("""
            var log = [];
            var parent = document.createElement('div');
            var child = document.createElement('span');
            parent.appendChild(child);
            document.body.appendChild(parent);
            child.addEventListener('click', function(e) {
                var path = e.composedPath();
                log.push('len:' + path.length);
                log.push('first:' + path[0].nodeName);
            });
            child.dispatchEvent(new Event('click', {bubbles: true}));
            document.getElementById('result').textContent = log.join(',');
        """)
        assert "len:" in result
        assert "first:SPAN" in result

    def test_composed_path_empty_after_dispatch(self):
        """composedPath() should return [] after dispatch."""
        result = run_js("""
            var evt = new Event('click', {bubbles: true});
            var div = document.createElement('div');
            document.body.appendChild(div);
            div.dispatchEvent(evt);
            document.getElementById('result').textContent = String(evt.composedPath().length);
        """)
        assert result == "0"


class TestDispatchReturnValue:
    """Test the return value of dispatchEvent."""

    def test_dispatch_returns_true_no_prevent(self):
        """dispatchEvent returns true when no preventDefault."""
        result = run_js("""
            var div = document.createElement('div');
            document.body.appendChild(div);
            var r = div.dispatchEvent(new Event('click', {bubbles: true}));
            document.getElementById('result').textContent = String(r);
        """)
        assert result == "true"

    def test_dispatch_returns_false_with_prevent(self):
        """dispatchEvent returns false when preventDefault is called."""
        result = run_js("""
            var div = document.createElement('div');
            document.body.appendChild(div);
            div.addEventListener('click', function(e) { e.preventDefault(); });
            var r = div.dispatchEvent(new Event('click', {bubbles: true, cancelable: true}));
            document.getElementById('result').textContent = String(r);
        """)
        assert result == "false"


class TestListenerDeduplication:
    """Test that addEventListener deduplicates by (callback, capture)."""

    def test_same_function_same_capture_deduped(self):
        """Adding same function with same capture flag should not add twice."""
        result = run_js("""
            var count = 0;
            var div = document.createElement('div');
            document.body.appendChild(div);
            function handler() { count++; }
            div.addEventListener('click', handler);
            div.addEventListener('click', handler);
            div.dispatchEvent(new Event('click'));
            document.getElementById('result').textContent = String(count);
        """)
        assert result == "1"

    def test_same_function_different_capture_not_deduped(self):
        """Adding same function with different capture should add both."""
        result = run_js("""
            var count = 0;
            var div = document.createElement('div');
            document.body.appendChild(div);
            function handler() { count++; }
            div.addEventListener('click', handler, false);
            div.addEventListener('click', handler, true);
            div.dispatchEvent(new Event('click'));
            document.getElementById('result').textContent = String(count);
        """)
        assert result == "2"


class TestOnceListener:
    """Test {once: true} listener option."""

    def test_once_listener_fires_once(self):
        """A {once: true} listener should be removed after first dispatch."""
        result = run_js("""
            var count = 0;
            var div = document.createElement('div');
            document.body.appendChild(div);
            div.addEventListener('click', function() { count++; }, {once: true});
            div.dispatchEvent(new Event('click'));
            div.dispatchEvent(new Event('click'));
            document.getElementById('result').textContent = String(count);
        """)
        assert result == "1"


# ═══════════════════════════════════════════════════════════════════════════════
# 2. PRE-INSERTION VALIDATION
# ═══════════════════════════════════════════════════════════════════════════════

class TestPreInsertionValidation:
    """Test ensure_pre_insertion_validity (WHATWG DOM §4.4)."""

    def test_append_child_to_text_throws(self):
        """Cannot appendChild to a Text node — HierarchyRequestError."""
        result = run_js("""
            var text = document.createTextNode('hello');
            document.body.appendChild(text);
            var div = document.createElement('div');
            text.appendChild(div);
            document.getElementById('result').textContent = 'no error';
        """)
        assert "HierarchyRequestError" in result

    def test_append_child_to_comment_throws(self):
        """Cannot appendChild to a Comment node — HierarchyRequestError."""
        result = run_js("""
            var comment = document.createComment('test');
            document.body.appendChild(comment);
            var div = document.createElement('div');
            comment.appendChild(div);
            document.getElementById('result').textContent = 'no error';
        """)
        assert "HierarchyRequestError" in result

    def test_append_ancestor_to_descendant_throws(self):
        """Cannot insert a node into its own descendant — HierarchyRequestError."""
        result = run_js("""
            var parent = document.createElement('div');
            var child = document.createElement('span');
            parent.appendChild(child);
            document.body.appendChild(parent);
            child.appendChild(parent);
            document.getElementById('result').textContent = 'no error';
        """)
        assert "HierarchyRequestError" in result

    def test_append_self_throws(self):
        """Cannot insert a node as its own child — HierarchyRequestError."""
        result = run_js("""
            var div = document.createElement('div');
            document.body.appendChild(div);
            div.appendChild(div);
            document.getElementById('result').textContent = 'no error';
        """)
        assert "HierarchyRequestError" in result

    def test_remove_child_not_child_throws(self):
        """removeChild with non-child throws NotFoundError."""
        result = run_js("""
            var div = document.createElement('div');
            var other = document.createElement('span');
            document.body.appendChild(div);
            document.body.appendChild(other);
            div.removeChild(other);
            document.getElementById('result').textContent = 'no error';
        """)
        assert "NotFoundError" in result

    def test_replace_child_not_child_throws(self):
        """replaceChild where old node is not a child throws NotFoundError."""
        result = run_js("""
            var div = document.createElement('div');
            var child = document.createElement('span');
            var other = document.createElement('p');
            div.appendChild(child);
            document.body.appendChild(div);
            div.replaceChild(document.createElement('b'), other);
            document.getElementById('result').textContent = 'no error';
        """)
        assert "NotFoundError" in result

    def test_valid_append_child_works(self):
        """Normal appendChild should still work fine."""
        html = """<!DOCTYPE html><html><head></head><body>
        <div id="container"></div>
        <script>
            var div = document.getElementById('container');
            var span = document.createElement('span');
            span.textContent = 'hello';
            div.appendChild(span);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "<span>hello</span>" in result

    def test_valid_insert_before_works(self):
        """Normal insertBefore should still work fine."""
        html = """<!DOCTYPE html><html><head></head><body>
        <div id="container"></div>
        <script>
            var div = document.getElementById('container');
            var a = document.createElement('a');
            var b = document.createElement('b');
            div.appendChild(b);
            div.insertBefore(a, b);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "<a></a><b></b>" in result

    def test_valid_replace_child_works(self):
        """Normal replaceChild should still work fine."""
        html = """<!DOCTYPE html><html><head></head><body>
        <div id="container"></div>
        <script>
            var div = document.getElementById('container');
            var old = document.createElement('span');
            old.textContent = 'old';
            div.appendChild(old);
            var replacement = document.createElement('em');
            replacement.textContent = 'new';
            div.replaceChild(replacement, old);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert "<em>new</em>" in result


# ═══════════════════════════════════════════════════════════════════════════════
# 3. NODE FLAGS AND isConnected
# ═══════════════════════════════════════════════════════════════════════════════

class TestIsConnected:
    """Test Node.isConnected property backed by O(1) NodeFlags."""

    def test_document_is_connected(self):
        """document.isConnected should always be true."""
        result = run_js("""
            document.getElementById('result').textContent = String(document.isConnected);
        """)
        assert result == "true"

    def test_body_is_connected(self):
        """document.body.isConnected should be true."""
        result = run_js("""
            document.getElementById('result').textContent = String(document.body.isConnected);
        """)
        assert result == "true"

    def test_existing_element_is_connected(self):
        """An element in the document tree is connected."""
        result = run_js("""
            var el = document.getElementById('result');
            el.textContent = String(el.isConnected);
        """)
        assert result == "true"

    def test_created_element_not_connected(self):
        """A newly created element (not attached) is NOT connected."""
        result = run_js("""
            var div = document.createElement('div');
            document.getElementById('result').textContent = String(div.isConnected);
        """)
        assert result == "false"

    def test_becomes_connected_on_append(self):
        """isConnected becomes true after appending to the document."""
        result = run_js("""
            var div = document.createElement('div');
            var before = div.isConnected;
            document.body.appendChild(div);
            var after = div.isConnected;
            document.getElementById('result').textContent = before + ',' + after;
        """)
        assert result == "false,true"

    def test_becomes_disconnected_on_remove(self):
        """isConnected becomes false after removing from the document."""
        result = run_js("""
            var div = document.createElement('div');
            document.body.appendChild(div);
            var before = div.isConnected;
            document.body.removeChild(div);
            var after = div.isConnected;
            document.getElementById('result').textContent = before + ',' + after;
        """)
        assert result == "true,false"

    def test_deep_child_connected(self):
        """A deeply nested child is connected when its ancestor is in the document."""
        result = run_js("""
            var a = document.createElement('div');
            var b = document.createElement('div');
            var c = document.createElement('div');
            a.appendChild(b);
            b.appendChild(c);
            var before = c.isConnected;
            document.body.appendChild(a);
            var after = c.isConnected;
            document.getElementById('result').textContent = before + ',' + after;
        """)
        assert result == "false,true"

    def test_deep_child_disconnected_on_ancestor_remove(self):
        """When an ancestor is removed, all descendants become disconnected."""
        result = run_js("""
            var a = document.createElement('div');
            var b = document.createElement('div');
            var c = document.createElement('span');
            a.appendChild(b);
            b.appendChild(c);
            document.body.appendChild(a);
            var before = c.isConnected;
            document.body.removeChild(a);
            var after = c.isConnected;
            document.getElementById('result').textContent = before + ',' + after;
        """)
        assert result == "true,false"

    def test_reparent_connected(self):
        """Moving a node from one connected parent to another stays connected."""
        result = run_js("""
            var a = document.createElement('div');
            var b = document.createElement('div');
            var child = document.createElement('span');
            document.body.appendChild(a);
            document.body.appendChild(b);
            a.appendChild(child);
            var r1 = child.isConnected;
            b.appendChild(child);
            var r2 = child.isConnected;
            document.getElementById('result').textContent = r1 + ',' + r2;
        """)
        assert result == "true,true"

    def test_text_node_connected(self):
        """Text nodes also track isConnected."""
        result = run_js("""
            var text = document.createTextNode('hello');
            var before = text.isConnected;
            document.body.appendChild(text);
            var after = text.isConnected;
            document.getElementById('result').textContent = before + ',' + after;
        """)
        assert result == "false,true"


# ═══════════════════════════════════════════════════════════════════════════════
# 4. LIVE childNodes NODELIST
# ═══════════════════════════════════════════════════════════════════════════════

class TestLiveChildNodes:
    """Test live NodeList returned by childNodes."""

    def test_identity(self):
        """childNodes should return the same object each time (===)."""
        result = run_js("""
            var div = document.createElement('div');
            document.body.appendChild(div);
            var cn1 = div.childNodes;
            var cn2 = div.childNodes;
            document.getElementById('result').textContent = String(cn1 === cn2);
        """)
        assert result == "true"

    def test_live_length(self):
        """childNodes.length should update live as children are added."""
        result = run_js("""
            var div = document.createElement('div');
            document.body.appendChild(div);
            var cn = div.childNodes;
            var results = [];
            results.push(cn.length);
            div.appendChild(document.createElement('span'));
            results.push(cn.length);
            div.appendChild(document.createElement('p'));
            results.push(cn.length);
            document.getElementById('result').textContent = results.join(',');
        """)
        assert result == "0,1,2"

    def test_live_length_after_remove(self):
        """childNodes.length should decrease when children are removed."""
        result = run_js("""
            var div = document.createElement('div');
            var a = document.createElement('a');
            var b = document.createElement('b');
            div.appendChild(a);
            div.appendChild(b);
            document.body.appendChild(div);
            var cn = div.childNodes;
            var before = cn.length;
            div.removeChild(a);
            var after = cn.length;
            document.getElementById('result').textContent = before + ',' + after;
        """)
        assert result == "2,1"

    def test_indexed_access(self):
        """childNodes[i] should return the correct child live."""
        result = run_js("""
            var div = document.createElement('div');
            var a = document.createElement('a');
            var b = document.createElement('b');
            div.appendChild(a);
            div.appendChild(b);
            document.body.appendChild(div);
            var cn = div.childNodes;
            document.getElementById('result').textContent =
                cn[0].nodeName + ',' + cn[1].nodeName;
        """)
        assert result == "A,B"

    def test_indexed_access_updates_after_mutation(self):
        """childNodes[0] should reflect changes after DOM mutation."""
        result = run_js("""
            var div = document.createElement('div');
            var a = document.createElement('a');
            var b = document.createElement('b');
            div.appendChild(a);
            div.appendChild(b);
            document.body.appendChild(div);
            var cn = div.childNodes;
            var before = cn[0].nodeName;
            div.removeChild(a);
            var after = cn[0].nodeName;
            document.getElementById('result').textContent = before + ',' + after;
        """)
        assert result == "A,B"

    def test_out_of_bounds_returns_undefined(self):
        """childNodes[n] for out-of-bounds index should return undefined."""
        result = run_js("""
            var div = document.createElement('div');
            document.body.appendChild(div);
            var cn = div.childNodes;
            document.getElementById('result').textContent =
                String(cn[0]) + ',' + String(cn[99]);
        """)
        assert result == "undefined,undefined"

    def test_foreach(self):
        """childNodes.forEach should iterate over all children."""
        result = run_js("""
            var div = document.createElement('div');
            div.appendChild(document.createElement('a'));
            div.appendChild(document.createElement('b'));
            div.appendChild(document.createElement('i'));
            document.body.appendChild(div);
            var names = [];
            div.childNodes.forEach(function(node) { names.push(node.nodeName); });
            document.getElementById('result').textContent = names.join(',');
        """)
        assert result == "A,B,I"

    def test_text_and_element_children(self):
        """childNodes should include text nodes, not just elements."""
        result = run_js("""
            var div = document.createElement('div');
            div.appendChild(document.createTextNode('hello'));
            div.appendChild(document.createElement('span'));
            div.appendChild(document.createTextNode('world'));
            document.body.appendChild(div);
            var cn = div.childNodes;
            var types = [];
            for (var i = 0; i < cn.length; i++) {
                types.push(cn[i].nodeType);
            }
            document.getElementById('result').textContent = types.join(',');
        """)
        assert result == "3,1,3"


# ═══════════════════════════════════════════════════════════════════════════════
# 5. DOCUMENT FRAGMENT
# ═══════════════════════════════════════════════════════════════════════════════

class TestDocumentFragment:
    """Test DocumentFragment behavior with new NodeData::DocumentFragment variant."""

    def test_node_type(self):
        """DocumentFragment.nodeType should be 11."""
        result = run_js("""
            var frag = document.createDocumentFragment();
            document.getElementById('result').textContent = String(frag.nodeType);
        """)
        assert result == "11"

    def test_append_to_fragment(self):
        """Can append children to a DocumentFragment."""
        result = run_js("""
            var frag = document.createDocumentFragment();
            frag.appendChild(document.createElement('a'));
            frag.appendChild(document.createElement('b'));
            document.getElementById('result').textContent = String(frag.childNodes.length);
        """)
        assert result == "2"

    def test_append_fragment_moves_children(self):
        """Appending a DocumentFragment moves its children into the parent."""
        html = """<!DOCTYPE html><html><head></head><body>
        <div id="container"></div>
        <script>
            var frag = document.createDocumentFragment();
            frag.appendChild(document.createElement('span'));
            frag.appendChild(document.createElement('em'));
            document.getElementById('container').appendChild(frag);
        </script></body></html>"""
        result = blazeweb.render(html)
        assert '<div id="container"><span></span><em></em></div>' in result

    def test_fragment_empty_after_append(self):
        """After appending a fragment, it should be empty."""
        result = run_js("""
            var frag = document.createDocumentFragment();
            frag.appendChild(document.createElement('a'));
            frag.appendChild(document.createElement('b'));
            var div = document.createElement('div');
            document.body.appendChild(div);
            div.appendChild(frag);
            document.getElementById('result').textContent = String(frag.childNodes.length);
        """)
        assert result == "0"

    def test_insert_before_with_fragment(self):
        """insertBefore with a fragment should insert all its children."""
        html = """<!DOCTYPE html><html><head></head><body>
<div id="container"></div>
<script>
var div = document.getElementById('container');
var existing = document.createElement('p');
div.appendChild(existing);
var frag = document.createDocumentFragment();
frag.appendChild(document.createElement('a'));
frag.appendChild(document.createElement('b'));
div.insertBefore(frag, existing);
</script></body></html>"""
        result = blazeweb.render(html)
        assert '<div id="container"><a></a><b></b><p></p></div>' in result

    def test_fragment_is_not_connected(self):
        """A DocumentFragment is not connected to the document."""
        result = run_js("""
            var frag = document.createDocumentFragment();
            document.getElementById('result').textContent = String(frag.isConnected);
        """)
        assert result == "false"

    def test_fragment_children_not_connected(self):
        """Children of a fragment are NOT connected."""
        result = run_js("""
            var frag = document.createDocumentFragment();
            var div = document.createElement('div');
            frag.appendChild(div);
            document.getElementById('result').textContent = String(div.isConnected);
        """)
        assert result == "false"

    def test_fragment_children_become_connected_on_append(self):
        """Children of a fragment become connected when fragment is appended to document."""
        result = run_js("""
            var frag = document.createDocumentFragment();
            var div = document.createElement('div');
            frag.appendChild(div);
            var before = div.isConnected;
            document.body.appendChild(frag);
            var after = div.isConnected;
            document.getElementById('result').textContent = before + ',' + after;
        """)
        assert result == "false,true"

    def test_multiple_elements_in_fragment(self):
        """A fragment can hold multiple elements (unlike Document)."""
        result = run_js("""
            var frag = document.createDocumentFragment();
            for (var i = 0; i < 5; i++) {
                frag.appendChild(document.createElement('div'));
            }
            document.getElementById('result').textContent = String(frag.childNodes.length);
        """)
        assert result == "5"
