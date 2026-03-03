"""Conformance tests: blazeweb vs headless Chromium.

Each test renders the same HTML through both blazeweb.render() and
Playwright (headless Chromium), then compares the resulting DOM trees.
"""

from __future__ import annotations

import pytest

lxml_html = pytest.importorskip("lxml.html")
from lxml.etree import tostring  # noqa: E402

import blazeweb  # noqa: E402

pytestmark = pytest.mark.conformance


# ── Helpers ──────────────────────────────────────────────────────────────────


def render_both(html: str, page) -> tuple[str, str]:
    """Render HTML through both blazeweb and Chromium."""
    bc_output = blazeweb.render(html)
    page.set_content(html, wait_until="load")
    chrome_output = page.content()
    return bc_output, chrome_output


def normalize_dom(html_string: str):
    """Parse HTML and normalize for structural comparison.

    - Removes <script> and <style> elements
    - Collapses whitespace in text nodes
    - Strips whitespace-only text nodes
    """
    doc = lxml_html.document_fromstring(html_string)
    # Remove script and style elements
    for el in doc.xpath("//script | //style"):
        el.getparent().remove(el)
    _normalize_text(doc)
    return doc


def _normalize_text(el):
    """Recursively normalize whitespace in text content."""
    if el.text:
        normalized = " ".join(el.text.split())
        el.text = normalized if normalized else None
    if el.tail:
        normalized = " ".join(el.tail.split())
        el.tail = normalized if normalized else None
    for child in el:
        _normalize_text(child)


def elements_equal(e1, e2, path="") -> tuple[bool, str]:
    """Recursively compare two lxml elements."""
    path = path or f"<{e1.tag}>"

    if e1.tag != e2.tag:
        return False, f"Tag mismatch at {path}: {e1.tag!r} vs {e2.tag!r}"

    # Attributes (sorted)
    a1 = sorted(e1.attrib.items())
    a2 = sorted(e2.attrib.items())
    if a1 != a2:
        return False, f"Attr mismatch at {path}: {dict(a1)} vs {dict(a2)}"

    # Text
    t1 = (e1.text or "").strip()
    t2 = (e2.text or "").strip()
    if t1 != t2:
        return False, f"Text mismatch at {path}: {t1!r} vs {t2!r}"

    # Tail
    tail1 = (e1.tail or "").strip()
    tail2 = (e2.tail or "").strip()
    if tail1 != tail2:
        return False, f"Tail mismatch at {path}: {tail1!r} vs {tail2!r}"

    # Children
    children1 = list(e1)
    children2 = list(e2)
    if len(children1) != len(children2):
        tags1 = [c.tag for c in children1]
        tags2 = [c.tag for c in children2]
        return False, f"Child count mismatch at {path}: {tags1} vs {tags2}"

    for i, (c1, c2) in enumerate(zip(children1, children2)):
        child_path = f"{path} > <{c1.tag}>[{i}]"
        ok, msg = elements_equal(c1, c2, child_path)
        if not ok:
            return False, msg

    return True, ""


def assert_dom_equal(bc_html: str, chrome_html: str):
    """Assert two HTML strings produce equivalent normalized DOM trees."""
    bc_dom = normalize_dom(bc_html)
    chrome_dom = normalize_dom(chrome_html)
    ok, msg = elements_equal(bc_dom, chrome_dom)
    if not ok:
        bc_ser = tostring(bc_dom, encoding="unicode", method="html")
        ch_ser = tostring(chrome_dom, encoding="unicode", method="html")
        raise AssertionError(
            f"DOM mismatch: {msg}\n\n"
            f"--- blazeweb (normalized) ---\n{bc_ser}\n\n"
            f"--- chromium (normalized) ---\n{ch_ser}"
        )


def get_element_text(html_string: str, css_selector: str) -> str:
    """Extract text content of element matching CSS selector."""
    doc = lxml_html.document_fromstring(html_string)
    els = doc.cssselect(css_selector)
    if not els:
        return ""
    return els[0].text_content()


def assert_text_equal(bc_html: str, chrome_html: str, selector: str, expected: str):
    """Assert element text matches between both engines and equals expected."""
    bc_text = get_element_text(bc_html, selector)
    ch_text = get_element_text(chrome_html, selector)
    assert bc_text == expected, (
        f"blazeweb #{selector} text: {bc_text!r}, expected: {expected!r}"
    )
    assert ch_text == expected, (
        f"chromium #{selector} text: {ch_text!r}, expected: {expected!r}"
    )


# ── A: Basic DOM Mutation ────────────────────────────────────────────────────


class TestDOMMutation:

    def test_set_text_content(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <p id="result">old</p>
            <script>document.getElementById('result').textContent = 'new';</script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)
        assert_text_equal(bc, ch, "#result", "new")

    def test_set_inner_html(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result">old</div>
            <script>document.getElementById('result').innerHTML = '<b>bold</b><i>italic</i>';</script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_set_attribute(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result"></div>
            <script>
                var el = document.getElementById('result');
                el.setAttribute('class', 'active');
                el.setAttribute('data-count', '5');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_remove_attribute(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result" class="old" data-x="y"></div>
            <script>
                var el = document.getElementById('result');
                el.removeAttribute('class');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)


# ── B: Tree Manipulation ────────────────────────────────────────────────────


class TestTreeManipulation:

    def test_create_and_append(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="container"></div>
            <script>
                var el = document.createElement('span');
                el.textContent = 'dynamic';
                document.getElementById('container').appendChild(el);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_remove_child(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="parent"><span id="child">remove me</span><p>keep me</p></div>
            <script>
                var parent = document.getElementById('parent');
                var child = document.getElementById('child');
                parent.removeChild(child);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_insert_before(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <ul id="list"><li id="second">B</li></ul>
            <script>
                var list = document.getElementById('list');
                var first = document.createElement('li');
                first.textContent = 'A';
                list.insertBefore(first, document.getElementById('second'));
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_element_remove(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="container"><span id="target">gone</span><p>stays</p></div>
            <script>document.getElementById('target').remove();</script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_clone_node_deep(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="original"><span>child</span></div>
            <div id="target"></div>
            <script>
                var orig = document.getElementById('original');
                var clone = orig.cloneNode(true);
                clone.id = 'cloned';
                document.getElementById('target').appendChild(clone);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)


# ── C: DOM Traversal ────────────────────────────────────────────────────────


class TestDOMTraversal:

    def test_traversal_chain(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="parent"><span>first</span><span>second</span></div>
            <div id="result"></div>
            <script>
                var div = document.getElementById('parent');
                var first = div.firstElementChild;
                var second = first.nextElementSibling;
                document.getElementById('result').textContent =
                    first.textContent + '+' + second.textContent;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "first+second")

    def test_child_nodes_iteration(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <ul id="list"><li>a</li><li>b</li><li>c</li></ul>
            <div id="result"></div>
            <script>
                var items = document.getElementById('list').childNodes;
                var texts = [];
                for (var i = 0; i < items.length; i++) {
                    if (items[i].nodeType === 1) texts.push(items[i].textContent);
                }
                document.getElementById('result').textContent = texts.join(',');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "a,b,c")

    def test_parent_element(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="parent"><p id="child">text</p></div>
            <div id="result"></div>
            <script>
                var child = document.getElementById('child');
                document.getElementById('result').textContent = child.parentElement.id;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "parent")


# ── D: Document API ──────────────────────────────────────────────────────────


class TestDocumentAPI:

    def test_get_elements_by_tag_name(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <p>one</p><p>two</p><p>three</p>
            <div id="result"></div>
            <script>
                var ps = document.getElementsByTagName('p');
                document.getElementById('result').textContent = ps.length.toString();
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "3")

    def test_get_elements_by_class_name(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div class="a b">1</div>
            <div class="a">2</div>
            <div class="b">3</div>
            <div class="a b c">4</div>
            <div id="result"></div>
            <script>
                var els = document.getElementsByClassName('a b');
                var texts = [];
                for (var i = 0; i < els.length; i++) texts.push(els[i].textContent);
                document.getElementById('result').textContent = texts.join(',');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "1,4")

    def test_document_title(self, page):
        html = """<!DOCTYPE html><html><head><title>Original</title></head><body>
            <div id="result"></div>
            <script>
                var old = document.title;
                document.title = 'New Title';
                document.getElementById('result').textContent = old + '|' + document.title;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "Original|New Title")

    def test_document_body(self, page):
        html = """<!DOCTYPE html><html><head></head><body id="thebody">
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = document.body.id;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "thebody")

    def test_document_head(self, page):
        html = """<!DOCTYPE html><html><head id="thehead"></head><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = document.head.id;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "thehead")

    def test_document_element(self, page):
        html = """<!DOCTYPE html><html id="thehtml"><head></head><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = document.documentElement.id;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "thehtml")

    def test_create_text_node(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result"></div>
            <script>
                var t = document.createTextNode('hello world');
                document.getElementById('result').appendChild(t);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_create_document_fragment(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result"></div>
            <script>
                var frag = document.createDocumentFragment();
                var a = document.createElement('span');
                a.textContent = 'A';
                var b = document.createElement('span');
                b.textContent = 'B';
                frag.appendChild(a);
                frag.appendChild(b);
                document.getElementById('result').appendChild(frag);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)


# ── E: Element Properties ───────────────────────────────────────────────────


class TestElementProperties:

    def test_tag_name(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target"></div>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent =
                    document.getElementById('target').tagName;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "DIV")

    def test_id_property(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="original"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('original');
                var old = el.id;
                el.id = 'changed';
                document.getElementById('result').textContent = old + '|' + el.id;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "original|changed")

    def test_class_name_property(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target" class="foo bar"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var old = el.className;
                el.className = 'baz qux';
                document.getElementById('result').textContent = old + '|' + el.className;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "foo bar|baz qux")

    def test_element_children_api(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="parent"><span>A</span><span>B</span><span>C</span></div>
            <div id="result"></div>
            <script>
                var p = document.getElementById('parent');
                var parts = [
                    p.children.length,
                    p.childElementCount,
                    p.firstElementChild.textContent,
                    p.lastElementChild.textContent
                ];
                document.getElementById('result').textContent = parts.join('|');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "3|3|A|C")

    def test_element_sibling_traversal(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div><span id="a">A</span><span id="b">B</span><span id="c">C</span></div>
            <div id="result"></div>
            <script>
                var b = document.getElementById('b');
                document.getElementById('result').textContent =
                    b.previousElementSibling.textContent + '|' + b.nextElementSibling.textContent;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "A|C")


# ── F: Multiple Scripts ──────────────────────────────────────────────────────


class TestMultipleScripts:

    def test_shared_global_state(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <script>var counter = 0;</script>
            <script>counter += 10;</script>
            <script>counter += 32;</script>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = counter.toString();
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "42")

    def test_error_non_fatal(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <script>throw new Error('boom');</script>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = 'survived';
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "survived")


# ── G: Node API ──────────────────────────────────────────────────────────────


class TestNodeAPI:

    def test_node_type(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="el">text</div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('el');
                var types = [
                    document.nodeType,
                    el.nodeType,
                    el.firstChild.nodeType
                ];
                document.getElementById('result').textContent = types.join(',');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "9,1,3")

    def test_node_predicates(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="parent"><span id="child">text</span></div>
            <div id="result"></div>
            <script>
                var parent = document.getElementById('parent');
                var child = document.getElementById('child');
                var parts = [
                    parent.hasChildNodes(),
                    parent.contains(child),
                    child.isSameNode(child),
                    child.isSameNode(parent)
                ];
                document.getElementById('result').textContent = parts.join(',');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "true,true,true,false")

    def test_owner_document(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="el"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('el');
                var parts = [
                    el.ownerDocument === document,
                    document.ownerDocument === null
                ];
                document.getElementById('result').textContent = parts.join(',');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "true,true")


# ── H: Stubbed APIs (xfail) ─────────────────────────────────────────────────


class TestStubbedAPIs:

    def test_query_selector(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <p class="target">found</p>
            <div id="result"></div>
            <script>
                var el = document.querySelector('.target');
                document.getElementById('result').textContent = el ? el.textContent : 'null';
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "found")

    def test_query_selector_all(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <p class="item">a</p><p class="item">b</p>
            <div id="result"></div>
            <script>
                var els = document.querySelectorAll('.item');
                document.getElementById('result').textContent = els.length.toString();
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "2")

    def test_query_selector_complex(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div class="container"><span data-x="1">A</span><span data-x="2">B</span></div>
            <div id="result"></div>
            <script>
                var el = document.querySelector('.container span[data-x="2"]');
                document.getElementById('result').textContent = el ? el.textContent : 'null';
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "B")

    def test_element_matches(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target" class="foo bar"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var parts = [
                    el.matches('.foo'),
                    el.matches('.baz'),
                    el.matches('div.bar')
                ];
                document.getElementById('result').textContent = parts.join(',');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "true,false,true")

    def test_element_closest(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div class="outer"><div class="inner"><span id="target">text</span></div></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var closest = el.closest('.outer');
                document.getElementById('result').textContent = closest ? closest.className : 'null';
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "outer")


class TestTimerAPIs:

    def test_set_timeout_fires(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result">before</div>
            <script>
                setTimeout(function() {
                    document.getElementById('result').textContent = 'after';
                }, 0);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "after")

    def test_clear_timeout_cancels(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result">unchanged</div>
            <script>
                var id = setTimeout(function() {
                    document.getElementById('result').textContent = 'changed';
                }, 0);
                clearTimeout(id);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "unchanged")

    def test_set_interval_fires(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result">0</div>
            <script>
                var count = 0;
                var id = setInterval(function() {
                    count++;
                    document.getElementById('result').textContent = count.toString();
                }, 10);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        # blazeweb fires intervals once (SSR semantics), Chrome fires repeatedly
        bc_text = get_element_text(bc, "#result")
        assert bc_text == "1", f"blazeweb interval should fire once, got: {bc_text}"


class TestEventSystem:

    def test_dom_content_loaded(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result">pending</div>
            <script>
                document.addEventListener('DOMContentLoaded', function() {
                    document.getElementById('result').textContent = 'loaded';
                });
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "loaded")

    def test_window_dom_content_loaded(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result">pending</div>
            <script>
                window.addEventListener('DOMContentLoaded', function() {
                    document.getElementById('result').textContent = 'window-loaded';
                });
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "window-loaded")


class TestClassList:

    def test_classlist_add(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target"></div>
            <script>
                var el = document.getElementById('target');
                el.classList.add('foo', 'bar');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_classlist_remove(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target" class="foo bar baz"></div>
            <script>
                var el = document.getElementById('target');
                el.classList.remove('bar');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_classlist_toggle(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target" class="foo bar"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var r1 = el.classList.toggle('bar');
                var r2 = el.classList.toggle('baz');
                document.getElementById('result').textContent = r1 + ',' + r2 + ',' + el.className;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "false,true,foo baz")

    def test_classlist_contains(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target" class="foo bar"></div>
            <div id="result"></div>
            <script>
                var el = document.getElementById('target');
                var parts = [
                    el.classList.contains('foo'),
                    el.classList.contains('baz')
                ];
                document.getElementById('result').textContent = parts.join(',');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "true,false")


class TestStyleProperty:

    def test_style_set_display(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target"></div>
            <script>
                document.getElementById('target').style.display = 'none';
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_style_read(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target" style="color: red; font-size: 14px"></div>
            <div id="result"></div>
            <script>
                var s = document.getElementById('target').style;
                document.getElementById('result').textContent = s.color + '|' + s.fontSize;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "red|14px")

    def test_style_get_property_value(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target" style="background-color: blue"></div>
            <div id="result"></div>
            <script>
                var s = document.getElementById('target').style;
                document.getElementById('result').textContent = s.getPropertyValue('background-color');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_text_equal(bc, ch, "#result", "blue")


class TestWindowAPIs:

    def test_navigator_user_agent(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent =
                    typeof navigator.userAgent === 'string' ? 'ok' : 'fail';
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        # Both should have a string userAgent
        bc_text = get_element_text(bc, "#result")
        ch_text = get_element_text(ch, "#result")
        assert bc_text == "ok"
        assert ch_text == "ok"

    def test_location_href(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent =
                    typeof location.href === 'string' ? 'ok' : 'fail';
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        bc_text = get_element_text(bc, "#result")
        ch_text = get_element_text(ch, "#result")
        assert bc_text == "ok"
        assert ch_text == "ok"

    def test_document_ready_state(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = document.readyState;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        # Both should say "complete" (blazeweb always returns "complete", Chrome may say "loading" during set_content)
        bc_text = get_element_text(bc, "#result")
        assert bc_text == "complete"


class TestMiscDOM:

    def test_replace_child(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="parent"><span id="old">old</span></div>
            <script>
                var parent = document.getElementById('parent');
                var old = document.getElementById('old');
                var newEl = document.createElement('em');
                newEl.textContent = 'new';
                parent.replaceChild(newEl, old);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_insert_adjacent_html(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target">middle</div>
            <script>
                var el = document.getElementById('target');
                el.insertAdjacentHTML('afterbegin', '<b>start</b>');
                el.insertAdjacentHTML('beforeend', '<i>end</i>');
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_append_and_prepend(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target"><span>middle</span></div>
            <script>
                var el = document.getElementById('target');
                var first = document.createElement('b');
                first.textContent = 'first';
                var last = document.createElement('i');
                last.textContent = 'last';
                el.prepend(first);
                el.append(last);
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        assert_dom_equal(bc, ch)

    def test_get_bounding_client_rect(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="target">box</div>
            <div id="result"></div>
            <script>
                var rect = document.getElementById('target').getBoundingClientRect();
                document.getElementById('result').textContent =
                    typeof rect.width + ',' + typeof rect.height;
            </script>
        </body></html>"""
        bc, ch = render_both(html, page)
        bc_text = get_element_text(bc, "#result")
        ch_text = get_element_text(ch, "#result")
        assert bc_text == "number,number"
        assert ch_text == "number,number"


# ── H: Fetch API Conformance ────────────────────────────────────────────────


class TestFetchConformance:
    """Fetch API: blazeweb vs Chrome, using a local HTTP server."""

    @pytest.fixture(autouse=True)
    def _server(self):
        from pytest_httpserver import HTTPServer

        self.server = HTTPServer(host="127.0.0.1")
        self.server.start()
        yield
        self.server.clear()
        if self.server.is_running():
            self.server.stop()

    @property
    def base(self):
        return self.server.url_for("").rstrip("/")

    def render_fetch(self, html, page):
        """Render through blazeweb (with base_url) and Playwright."""
        bc_output = blazeweb.render(html, base_url=self.base)
        # For Chrome, navigate to the HTTP server with inline HTML via data URI
        # won't work for fetch — Chrome needs the page served from the same origin.
        # Serve the HTML from the server instead.
        self.server.expect_request("/test-page").respond_with_data(
            html, content_type="text/html"
        )
        page.goto(f"{self.base}/test-page", wait_until="networkidle")
        chrome_output = page.content()
        return bc_output, chrome_output

    def test_fetch_basic_text(self, page):
        self.server.expect_request("/api/text").respond_with_data(
            "hello-world", content_type="text/plain"
        )
        html = f"""<!DOCTYPE html><html><head></head><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.base}/api/text')
                    .then(function(r) {{ return r.text(); }})
                    .then(function(t) {{
                        document.getElementById('result').textContent = t;
                    }});
            </script>
        </body></html>"""
        bc, ch = self.render_fetch(html, page)
        assert get_element_text(bc, "#result") == "hello-world"
        assert get_element_text(ch, "#result") == "hello-world"

    def test_fetch_json_parse(self, page):
        self.server.expect_request("/api/json").respond_with_json(
            {"x": 42, "y": "test"}
        )
        html = f"""<!DOCTYPE html><html><head></head><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.base}/api/json')
                    .then(function(r) {{ return r.json(); }})
                    .then(function(d) {{
                        document.getElementById('result').textContent = d.x + '-' + d.y;
                    }});
            </script>
        </body></html>"""
        bc, ch = self.render_fetch(html, page)
        assert get_element_text(bc, "#result") == "42-test"
        assert get_element_text(ch, "#result") == "42-test"

    def test_fetch_status_properties(self, page):
        self.server.expect_request("/api/ok").respond_with_data("ok", status=200)
        html = f"""<!DOCTYPE html><html><head></head><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.base}/api/ok')
                    .then(function(r) {{
                        document.getElementById('result').textContent =
                            r.status + ',' + r.ok;
                    }});
            </script>
        </body></html>"""
        bc, ch = self.render_fetch(html, page)
        assert get_element_text(bc, "#result") == "200,true"
        assert get_element_text(ch, "#result") == "200,true"

    def test_fetch_404(self, page):
        self.server.expect_request("/api/nope").respond_with_data(
            "nope", status=404
        )
        html = f"""<!DOCTYPE html><html><head></head><body>
            <div id="result">pending</div>
            <script>
                fetch('{self.base}/api/nope')
                    .then(function(r) {{
                        document.getElementById('result').textContent =
                            r.status + ',' + r.ok;
                    }});
            </script>
        </body></html>"""
        bc, ch = self.render_fetch(html, page)
        assert get_element_text(bc, "#result") == "404,false"
        assert get_element_text(ch, "#result") == "404,false"

    def test_fetch_promise_all(self, page):
        self.server.expect_request("/api/a").respond_with_data("A")
        self.server.expect_request("/api/b").respond_with_data("B")
        html = f"""<!DOCTYPE html><html><head></head><body>
            <div id="result">pending</div>
            <script>
                Promise.all([
                    fetch('{self.base}/api/a').then(function(r) {{ return r.text(); }}),
                    fetch('{self.base}/api/b').then(function(r) {{ return r.text(); }})
                ]).then(function(results) {{
                    document.getElementById('result').textContent = results.join(',');
                }});
            </script>
        </body></html>"""
        bc, ch = self.render_fetch(html, page)
        assert get_element_text(bc, "#result") == "A,B"
        assert get_element_text(ch, "#result") == "A,B"

    def test_fetch_error_catch(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result">pending</div>
            <script>
                fetch('http://127.0.0.1:1/nope')
                    .then(function(r) {
                        document.getElementById('result').textContent = 'no-error';
                    })
                    .catch(function(err) {
                        document.getElementById('result').textContent = 'caught';
                    });
            </script>
        </body></html>"""
        bc, ch = self.render_fetch(html, page)
        assert get_element_text(bc, "#result") == "caught"
        assert get_element_text(ch, "#result") == "caught"

    def test_fetch_typeof(self, page):
        html = """<!DOCTYPE html><html><head></head><body>
            <div id="result"></div>
            <script>
                document.getElementById('result').textContent = typeof fetch;
            </script>
        </body></html>"""
        bc, ch = self.render_fetch(html, page)
        assert get_element_text(bc, "#result") == "function"
        assert get_element_text(ch, "#result") == "function"
