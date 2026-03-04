"""End-to-end tests for DOM APIs and Web APIs.

These tests exercise the full blazeweb render pipeline from Python,
covering APIs that are NOT tested by other specialized test files
(test_modules.py, test_chrome_conformance.py, etc.).
"""

import re

import blazeweb


def text_of(html: str, element_id: str) -> str:
    """Extract text content of an element by id from rendered HTML."""
    pattern = rf'id="{re.escape(element_id)}"[^>]*>([^<]*)<'
    m = re.search(pattern, html)
    return m.group(1) if m else ""


def render(html: str) -> str:
    """Shortcut: render HTML and return the output string."""
    return blazeweb.render(html)


# ─── importNode ──────────────────────────────────────────────────────────────


class TestImportNode:
    def test_deep_preserves_children(self):
        html = render("""<html><body>
        <div id="src"><span>child</span></div>
        <div id="result"></div>
        <script>
            var src = document.getElementById('src');
            var clone = document.importNode(src, true);
            document.getElementById('result').textContent =
                clone.querySelector('span') ? 'has-child' : 'no-child';
        </script></body></html>""")
        assert text_of(html, "result") == "has-child"

    def test_deep_preserves_attributes(self):
        html = render("""<html><body>
        <div id="src" data-x="hello" class="foo"></div>
        <div id="result"></div>
        <script>
            var src = document.getElementById('src');
            var clone = document.importNode(src, true);
            document.getElementById('result').textContent =
                clone.getAttribute('data-x') + ',' + clone.className;
        </script></body></html>""")
        assert text_of(html, "result") == "hello,foo"

    def test_deep_is_different_node(self):
        html = render("""<html><body>
        <div id="src">text</div>
        <div id="result"></div>
        <script>
            var src = document.getElementById('src');
            var clone = document.importNode(src, true);
            document.getElementById('result').textContent =
                String(src !== clone);
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_shallow_no_children(self):
        html = render("""<html><body>
        <div id="src"><span>child</span></div>
        <div id="result"></div>
        <script>
            var src = document.getElementById('src');
            var clone = document.importNode(src, false);
            document.getElementById('result').textContent =
                String(clone.childNodes.length);
        </script></body></html>""")
        assert text_of(html, "result") == "0"

    def test_default_is_shallow(self):
        html = render("""<html><body>
        <div id="src"><span>child</span></div>
        <div id="result"></div>
        <script>
            var src = document.getElementById('src');
            var clone = document.importNode(src);
            document.getElementById('result').textContent =
                String(clone.childNodes.length);
        </script></body></html>""")
        assert text_of(html, "result") == "0"

    def test_text_node(self):
        html = render("""<html><body>
        <div id="src">hello</div>
        <div id="result"></div>
        <script>
            var textNode = document.getElementById('src').firstChild;
            var clone = document.importNode(textNode, true);
            document.getElementById('result').textContent =
                clone.nodeType + ':' + clone.textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "3:hello"

    def test_comment_node(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var comment = document.createComment('test comment');
            var clone = document.importNode(comment, true);
            document.getElementById('result').textContent =
                clone.nodeType + ':' + clone.nodeValue;
        </script></body></html>""")
        assert text_of(html, "result") == "8:test comment"

    def test_document_throws(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            try {
                document.importNode(document, true);
                document.getElementById('result').textContent = 'no-error';
            } catch(e) {
                document.getElementById('result').textContent = e.name;
            }
        </script></body></html>""")
        assert text_of(html, "result") == "NotSupportedError"

    def test_document_fragment(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var frag = document.createDocumentFragment();
            var span = document.createElement('span');
            span.textContent = 'in-frag';
            frag.appendChild(span);
            var clone = document.importNode(frag, true);
            document.getElementById('result').textContent =
                clone.childNodes.length + ':' + clone.firstChild.textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "1:in-frag"


# ─── adoptNode ───────────────────────────────────────────────────────────────


class TestAdoptNode:
    def test_removes_from_parent(self):
        html = render("""<html><body>
        <div id="parent"><span id="child">hi</span></div>
        <div id="result"></div>
        <script>
            var child = document.getElementById('child');
            document.adoptNode(child);
            document.getElementById('result').textContent =
                String(document.getElementById('parent').childNodes.length);
        </script></body></html>""")
        assert text_of(html, "result") == "0"

    def test_returns_same_node(self):
        html = render("""<html><body>
        <div id="target">hello</div>
        <div id="result"></div>
        <script>
            var target = document.getElementById('target');
            var adopted = document.adoptNode(target);
            document.getElementById('result').textContent =
                String(target === adopted);
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_document_throws(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            try {
                document.adoptNode(document);
                document.getElementById('result').textContent = 'no-error';
            } catch(e) {
                document.getElementById('result').textContent = e.name;
            }
        </script></body></html>""")
        assert text_of(html, "result") == "NotSupportedError"

    def test_orphan_works(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var el = document.createElement('div');
            el.textContent = 'orphan';
            var adopted = document.adoptNode(el);
            document.getElementById('result').textContent =
                adopted.textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "orphan"

    def test_can_reinsert(self):
        html = render("""<html><body>
        <div id="old"><span id="child">moved</span></div>
        <div id="new"></div>
        <div id="result"></div>
        <script>
            var child = document.getElementById('child');
            document.adoptNode(child);
            document.getElementById('new').appendChild(child);
            document.getElementById('result').textContent =
                document.getElementById('new').textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "moved"


# ─── TreeWalker ──────────────────────────────────────────────────────────────


class TestTreeWalker:
    TREE_HTML = """<html><body>
    <div id="root"><span>A</span><span>B</span><span>C</span></div>
    <div id="result"></div>"""

    def test_next_node_elements(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            var names = [];
            while (tw.nextNode()) names.push(tw.currentNode.tagName);
            document.getElementById('result').textContent = names.join(',');
        </script></body></html>""")
        assert text_of(html, "result") == "SPAN,SPAN,SPAN"

    def test_next_node_text(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
            var texts = [];
            while (tw.nextNode()) texts.push(tw.currentNode.textContent);
            document.getElementById('result').textContent = texts.join(',');
        </script></body></html>""")
        assert text_of(html, "result") == "A,B,C"

    def test_next_node_show_all(self):
        html = render("""<html><body>
        <div id="root"><span>X</span></div>
        <div id="result"></div>
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ALL);
            var count = 0;
            while (tw.nextNode()) count++;
            document.getElementById('result').textContent = String(count);
        </script></body></html>""")
        # span + text "X" = 2
        assert text_of(html, "result") == "2"

    def test_first_child(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            var fc = tw.firstChild();
            document.getElementById('result').textContent =
                fc ? fc.textContent : 'null';
        </script></body></html>""")
        assert text_of(html, "result") == "A"

    def test_last_child(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            var lc = tw.lastChild();
            document.getElementById('result').textContent =
                lc ? lc.textContent : 'null';
        </script></body></html>""")
        assert text_of(html, "result") == "C"

    def test_parent_node(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            tw.firstChild(); // span A
            var parent = tw.parentNode();
            document.getElementById('result').textContent =
                parent ? parent.id : 'null';
        </script></body></html>""")
        assert text_of(html, "result") == "root"

    def test_parent_node_at_root_returns_null(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            var parent = tw.parentNode();
            document.getElementById('result').textContent =
                String(parent);
        </script></body></html>""")
        assert text_of(html, "result") == "null"

    def test_next_sibling(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            tw.firstChild(); // span A
            var ns = tw.nextSibling();
            document.getElementById('result').textContent =
                ns ? ns.textContent : 'null';
        </script></body></html>""")
        assert text_of(html, "result") == "B"

    def test_previous_sibling(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            tw.lastChild(); // span C
            var ps = tw.previousSibling();
            document.getElementById('result').textContent =
                ps ? ps.textContent : 'null';
        </script></body></html>""")
        assert text_of(html, "result") == "B"

    def test_previous_node(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            tw.nextNode(); tw.nextNode(); tw.nextNode(); // at span C
            var pn = tw.previousNode();
            document.getElementById('result').textContent =
                pn ? pn.textContent : 'null';
        </script></body></html>""")
        assert text_of(html, "result") == "B"

    def test_filter_function(self):
        html = render("""<html><body>
        <div id="root"><span>A</span><div>B</div><span>C</span></div>
        <div id="result"></div>
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT,
                function(node) {
                    return node.tagName === 'SPAN'
                        ? NodeFilter.FILTER_ACCEPT
                        : NodeFilter.FILTER_SKIP;
                });
            var names = [];
            while (tw.nextNode()) names.push(tw.currentNode.textContent);
            document.getElementById('result').textContent = names.join(',');
        </script></body></html>""")
        assert text_of(html, "result") == "A,C"

    def test_filter_object(self):
        html = render("""<html><body>
        <div id="root"><span>A</span><div>B</div><span>C</span></div>
        <div id="result"></div>
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT, {
                acceptNode: function(node) {
                    return node.tagName === 'SPAN'
                        ? NodeFilter.FILTER_ACCEPT
                        : NodeFilter.FILTER_SKIP;
                }
            });
            var names = [];
            while (tw.nextNode()) names.push(tw.currentNode.textContent);
            document.getElementById('result').textContent = names.join(',');
        </script></body></html>""")
        assert text_of(html, "result") == "A,C"

    def test_current_node_settable(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            tw.firstChild(); // span A
            tw.currentNode = root;
            document.getElementById('result').textContent =
                tw.currentNode.id;
        </script></body></html>""")
        assert text_of(html, "result") == "root"

    def test_root_property(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var tw = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            document.getElementById('result').textContent =
                String(tw.root === root);
        </script></body></html>""")
        assert text_of(html, "result") == "true"


# ─── NodeIterator ────────────────────────────────────────────────────────────


class TestNodeIterator:
    TREE_HTML = """<html><body>
    <div id="root"><span>A</span><span>B</span><span>C</span></div>
    <div id="result"></div>"""

    def test_next_node_basic(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            var names = [];
            var node;
            while (node = ni.nextNode()) names.push(node.tagName);
            document.getElementById('result').textContent = names.join(',');
        </script></body></html>""")
        # root div + 3 spans
        assert text_of(html, "result") == "DIV,SPAN,SPAN,SPAN"

    def test_next_node_text_only(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var ni = document.createNodeIterator(root, NodeFilter.SHOW_TEXT);
            var texts = [];
            var node;
            while (node = ni.nextNode()) texts.push(node.textContent);
            document.getElementById('result').textContent = texts.join(',');
        </script></body></html>""")
        assert text_of(html, "result") == "A,B,C"

    def test_previous_node(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            ni.nextNode(); ni.nextNode(); ni.nextNode(); // at span B
            var prev = ni.previousNode();
            document.getElementById('result').textContent =
                prev ? prev.textContent : 'null';
        </script></body></html>""")
        assert text_of(html, "result") == "B"

    def test_roundtrip(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            var forward = [];
            var node;
            while (node = ni.nextNode()) forward.push(node.tagName);
            var backward = [];
            while (node = ni.previousNode()) backward.push(node.tagName);
            document.getElementById('result').textContent =
                forward.length + ',' + backward.length;
        </script></body></html>""")
        assert text_of(html, "result") == "4,4"

    def test_reference_node(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            ni.nextNode(); ni.nextNode(); // at span A
            document.getElementById('result').textContent =
                ni.referenceNode.tagName;
        </script></body></html>""")
        assert text_of(html, "result") == "SPAN"

    def test_root_property(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            document.getElementById('result').textContent =
                String(ni.root === root);
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_detach_is_noop(self):
        html = render(f"""{self.TREE_HTML}
        <script>
            var root = document.getElementById('root');
            var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT);
            ni.detach(); // should be a no-op per spec
            var node = ni.nextNode();
            document.getElementById('result').textContent =
                node ? node.id : 'null';
        </script></body></html>""")
        assert text_of(html, "result") == "root"

    def test_filter_function(self):
        html = render("""<html><body>
        <div id="root"><span>A</span><div>B</div><span>C</span></div>
        <div id="result"></div>
        <script>
            var root = document.getElementById('root');
            var ni = document.createNodeIterator(root, NodeFilter.SHOW_ELEMENT,
                function(node) {
                    return node.tagName === 'SPAN'
                        ? NodeFilter.FILTER_ACCEPT
                        : NodeFilter.FILTER_REJECT;
                });
            var texts = [];
            var node;
            while (node = ni.nextNode()) texts.push(node.textContent);
            document.getElementById('result').textContent = texts.join(',');
        </script></body></html>""")
        assert text_of(html, "result") == "A,C"


# ─── NodeFilter constants ────────────────────────────────────────────────────


class TestNodeFilterConstants:
    def test_filter_constants(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = [
                NodeFilter.FILTER_ACCEPT,
                NodeFilter.FILTER_REJECT,
                NodeFilter.FILTER_SKIP,
            ].join(',');
        </script></body></html>""")
        assert text_of(html, "result") == "1,2,3"

    def test_show_constants(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            document.getElementById('result').textContent = [
                NodeFilter.SHOW_ALL,
                NodeFilter.SHOW_ELEMENT,
                NodeFilter.SHOW_TEXT,
                NodeFilter.SHOW_COMMENT,
            ].join(',');
        </script></body></html>""")
        # SHOW_ALL=0xFFFFFFFF, SHOW_ELEMENT=1, SHOW_TEXT=4, SHOW_COMMENT=128
        assert text_of(html, "result") == "4294967295,1,4,128"


# ─── MutationObserver ────────────────────────────────────────────────────────


class TestMutationObserver:
    def test_constructor_requires_function(self):
        result = blazeweb.render("""<html><body>
        <div id="result"></div>
        <script>
            try {
                new MutationObserver('not a function');
                document.getElementById('result').textContent = 'no-error';
            } catch(e) {
                document.getElementById('result').textContent = e.name;
            }
        </script></body></html>""")
        assert "TypeError" in result

    def test_child_list_append(self):
        html = render("""<html><body>
        <div id="target"></div>
        <div id="result"></div>
        <script>
            var records = [];
            var obs = new MutationObserver(function(list) {
                records = list;
            });
            obs.observe(document.getElementById('target'), { childList: true });
            var span = document.createElement('span');
            document.getElementById('target').appendChild(span);
        </script>
        <script>
            document.getElementById('result').textContent =
                records.length + ':' + records[0].type + ':' + records[0].addedNodes.length;
        </script></body></html>""")
        assert text_of(html, "result") == "1:childList:1"

    def test_child_list_remove(self):
        html = render("""<html><body>
        <div id="target"><span id="child">x</span></div>
        <div id="result"></div>
        <script>
            var records = [];
            var obs = new MutationObserver(function(list) {
                records = list;
            });
            obs.observe(document.getElementById('target'), { childList: true });
            document.getElementById('target').removeChild(
                document.getElementById('child'));
        </script>
        <script>
            document.getElementById('result').textContent =
                records.length + ':' + records[0].removedNodes.length;
        </script></body></html>""")
        assert text_of(html, "result") == "1:1"

    def test_attributes(self):
        html = render("""<html><body>
        <div id="target"></div>
        <div id="result"></div>
        <script>
            var records = [];
            var obs = new MutationObserver(function(list) {
                records = list;
            });
            obs.observe(document.getElementById('target'), { attributes: true });
            document.getElementById('target').setAttribute('data-x', 'hello');
        </script>
        <script>
            document.getElementById('result').textContent =
                records.length + ':' + records[0].type + ':' + records[0].attributeName;
        </script></body></html>""")
        assert text_of(html, "result") == "1:attributes:data-x"

    def test_character_data(self):
        html = render("""<html><body>
        <div id="target">original</div>
        <div id="result"></div>
        <script>
            var records = [];
            var obs = new MutationObserver(function(list) {
                records = list;
            });
            var textNode = document.getElementById('target').firstChild;
            obs.observe(textNode, { characterData: true });
            textNode.textContent = 'changed';
        </script>
        <script>
            document.getElementById('result').textContent =
                records.length + ':' + records[0].type;
        </script></body></html>""")
        assert text_of(html, "result") == "1:characterData"

    def test_disconnect_stops_observation(self):
        html = render("""<html><body>
        <div id="target"></div>
        <div id="result"></div>
        <script>
            var records = [];
            var obs = new MutationObserver(function(list) {
                records = list;
            });
            obs.observe(document.getElementById('target'), { childList: true });
            obs.disconnect();
            document.getElementById('target').appendChild(
                document.createElement('span'));
        </script>
        <script>
            document.getElementById('result').textContent =
                String(records.length);
        </script></body></html>""")
        assert text_of(html, "result") == "0"

    def test_take_records(self):
        html = render("""<html><body>
        <div id="target"></div>
        <div id="result"></div>
        <script>
            var obs = new MutationObserver(function() {});
            obs.observe(document.getElementById('target'), { childList: true });
            document.getElementById('target').appendChild(
                document.createElement('span'));
            var taken = obs.takeRecords();
            document.getElementById('result').textContent =
                String(taken.length);
        </script></body></html>""")
        assert text_of(html, "result") == "1"

    def test_subtree(self):
        html = render("""<html><body>
        <div id="target"><div id="inner"></div></div>
        <div id="result"></div>
        <script>
            var records = [];
            var obs = new MutationObserver(function(list) {
                records = list;
            });
            obs.observe(document.getElementById('target'),
                { childList: true, subtree: true });
            document.getElementById('inner').appendChild(
                document.createElement('span'));
        </script>
        <script>
            document.getElementById('result').textContent =
                records.length + ':' + records[0].addedNodes.length;
        </script></body></html>""")
        assert text_of(html, "result") == "1:1"


# ─── Timers ──────────────────────────────────────────────────────────────────


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


class TestWebAPIs:
    def test_local_storage(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            localStorage.setItem('key', 'value');
            document.getElementById('result').textContent =
                localStorage.getItem('key');
        </script></body></html>""")
        assert text_of(html, "result") == "value"

    def test_atob_btoa(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var encoded = btoa('hello world');
            var decoded = atob(encoded);
            document.getElementById('result').textContent =
                encoded + '|' + decoded;
        </script></body></html>""")
        assert text_of(html, "result") == "aGVsbG8gd29ybGQ=|hello world"

    def test_url_constructor(self):
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var u = new URL('https://example.com/path?q=1#hash');
            document.getElementById('result').textContent =
                u.hostname + '|' + u.pathname + '|' + u.search + '|' + u.hash;
        </script></body></html>""")
        assert text_of(html, "result") == "example.com|/path|?q=1|#hash"

    def test_dataset(self):
        html = render("""<html><body>
        <div id="target" data-foo="bar" data-baz-qux="hello"></div>
        <div id="result"></div>
        <script>
            var ds = document.getElementById('target').dataset;
            document.getElementById('result').textContent =
                ds.foo + '|' + ds.bazQux;
        </script></body></html>""")
        assert text_of(html, "result") == "bar|hello"


# ─── Module edge cases (missing from test_modules.py) ────────────────────────


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
