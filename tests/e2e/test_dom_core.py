"""E2E tests: Core DOM operations (importNode, adoptNode, TreeWalker, NodeIterator, MutationObserver, type hierarchy)"""

from .conftest import text_of, render
import blazeweb

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


class TestChildrenItem:
    def test_children_item_method(self):
        """Element.children should have .item() method."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var div = document.createElement("div");
            var s1 = document.createElement("span");
            var s2 = document.createElement("span");
            s1.textContent = "A"; s2.textContent = "B";
            div.appendChild(s1); div.appendChild(s2);
            document.getElementById('result').textContent = div.children.item(1).textContent;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "B"

    def test_children_item_first(self):
        """children.item(0) should return the first element child."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var div = document.createElement("div");
            var span = document.createElement("span");
            span.textContent = "first";
            div.appendChild(span);
            document.getElementById('result').textContent = div.children.item(0).textContent;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "first"

    def test_children_item_out_of_range(self):
        """children.item(999) should return null, not throw."""
        html = render("""<html><body>
        <div id="result"></div>
        <script>
            var div = document.createElement("div");
            document.getElementById('result').textContent =
                (div.children.item(0) === null).toString();
        </script>
        </body></html>""")
        assert text_of(html, "result") == "true"


# ─── Round 2 Phase 2: Global Constructors & Window Methods ──────────────────


class TestDOMPrototypes:
    def test_element_prototype_has_methods(self):
        """Element.prototype should have real DOM methods."""
        html = render("""<html><body><div id="result"></div><script>
            var methods = ["getAttribute", "querySelector", "setAttribute"];
            document.getElementById('result').textContent =
                methods.every(function(m){return typeof Element.prototype[m]==="function"}).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_node_prototype_has_methods(self):
        """Node.prototype should have real DOM methods."""
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof Node.prototype.appendChild === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_instanceof_element(self):
        """createElement result should be instanceof Element."""
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (document.createElement("div") instanceof Element).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_instanceof_htmlelement(self):
        """createElement result should be instanceof HTMLElement."""
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (document.createElement("div") instanceof HTMLElement).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_instanceof_node(self):
        """createElement result should be instanceof Node."""
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (document.createElement("div") instanceof Node).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_prototype_patching(self):
        """Libraries should be able to patch Element.prototype."""
        html = render("""<html><body><div id="result"></div><script>
            Element.prototype.__testPatch = function() { return "patched"; };
            var el = document.createElement("div");
            document.getElementById('result').textContent = el.__testPatch();
        </script></body></html>""")
        assert text_of(html, "result") == "patched"


class TestDOMTypeHierarchy:
    def test_div_instanceof_htmlelement(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (document.createElement("div") instanceof HTMLElement).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_div_instanceof_element(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (document.createElement("div") instanceof Element).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_div_instanceof_node(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (document.createElement("div") instanceof Node).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_video_instanceof_htmlmediaelement(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (document.createElement("video") instanceof HTMLMediaElement).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_video_instanceof_htmlelement(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (document.createElement("video") instanceof HTMLElement).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_characterdata_prototype_has_data(self):
        """CharacterData.prototype.data should exist (not undefined)."""
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                ("data" in CharacterData.prototype).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_htmlmediaelement_prototype_has_play(self):
        """HTMLMediaElement.prototype.play should be a function."""
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof HTMLMediaElement.prototype.play === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_cdatasection_exists(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof CDATASection === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_processing_instruction_exists(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof ProcessingInstruction === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_document_type_exists(self):
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof DocumentType === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_text_inherits_characterdata(self):
        """Text nodes should have CharacterData methods via prototype chain."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createTextNode("hello");
            document.getElementById('result').textContent =
                (typeof t.substringData === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_comment_inherits_characterdata(self):
        """Comment nodes should have CharacterData methods via prototype chain."""
        html = render("""<html><body><div id="result"></div><script>
            var c = document.createComment("test");
            document.getElementById('result').textContent =
                (typeof c.appendData === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_element_prototype_patching_works(self):
        """Polyfills patching Element.prototype should affect all elements."""
        html = render("""<html><body><div id="result"></div><script>
            Element.prototype.__testHierarchy = function() { return "patched"; };
            var div = document.createElement("div");
            document.getElementById('result').textContent = div.__testHierarchy();
        </script></body></html>""")
        assert text_of(html, "result") == "patched"

    def test_htmlelement_prototype_chain(self):
        """HTMLElement.prototype should inherit from Element.prototype."""
        html = render("""<html><body><div id="result"></div><script>
            document.getElementById('result').textContent =
                (typeof HTMLElement.prototype.getAttribute === "function").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

