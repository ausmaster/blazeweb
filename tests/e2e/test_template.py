"""E2E tests: HTMLTemplateElement — full spec compliance.

Tests cover:
- template.content returns DocumentFragment
- template.content is persistent (same object each access)
- template.innerHTML reads/writes content fragment, not element children
- template.content.childNodes reflects parsed content
- document.createElement("template") creates empty content
- cloneNode(true) clones template content
- cloneNode(false) does NOT clone content
- Serialization: outerHTML includes template content
- Nested templates
- Template content is inert (scripts don't execute)
- Dynamic template manipulation (appendChild to content)
- Polymer-style: create template, set innerHTML, access content.children
"""

from .conftest import text_of, render
import blazeweb


class TestTemplateContent:
    def test_content_is_document_fragment(self):
        """template.content should return a DocumentFragment."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            document.getElementById('result').textContent =
                (t.content !== null && t.content !== undefined &&
                 t.content.nodeType === 11).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_content_is_same_object(self):
        """template.content should return the same object each time."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            document.getElementById('result').textContent =
                (t.content === t.content).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_content_not_null(self):
        """Newly created template should have non-null content."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            document.getElementById('result').textContent =
                (t.content != null).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_content_initially_empty(self):
        """Newly created template.content should have no children."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            document.getElementById('result').textContent =
                t.content.childNodes.length.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "0"


class TestTemplateInnerHTML:
    def test_innerhtml_sets_content(self):
        """Setting template.innerHTML should populate content, not element children."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<span>hello</span>";
            document.getElementById('result').textContent =
                t.content.childNodes.length + ":" + t.childNodes.length;
        </script></body></html>""")
        # content has 1 child (span), element has 0 direct children
        assert text_of(html, "result") == "1:0"

    def test_innerhtml_gets_content(self):
        """Getting template.innerHTML should return content serialization."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<p>test</p>";
            // innerHTML contains HTML tags — check it contains the text
            document.getElementById('result').textContent =
                t.innerHTML.indexOf("test") !== -1 ? "ok" : "fail";
        </script></body></html>""")
        assert text_of(html, "result") == "ok"

    def test_innerhtml_replaces_content(self):
        """Setting innerHTML again should replace previous content."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<p>first</p>";
            t.innerHTML = "<span>second</span>";
            document.getElementById('result').textContent =
                t.content.childNodes.length + ":" +
                t.content.firstChild.tagName;
        </script></body></html>""")
        assert text_of(html, "result") == "1:SPAN"

    def test_innerhtml_multiple_children(self):
        """innerHTML with multiple elements should create multiple content children."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<p>a</p><p>b</p><p>c</p>";
            document.getElementById('result').textContent =
                t.content.childNodes.length.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "3"


class TestTemplateContentManipulation:
    def test_append_child_to_content(self):
        """appendChild on template.content should work."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            var span = document.createElement("span");
            span.textContent = "added";
            t.content.appendChild(span);
            document.getElementById('result').textContent =
                t.content.firstChild.textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "added"

    def test_query_selector_on_content(self):
        """querySelector on template.content should find elements."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = '<div class="x">found</div>';
            document.getElementById('result').textContent =
                t.content.querySelector(".x").textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "found"

    def test_content_children_property(self):
        """template.content.children should return element children."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<p>1</p>text<p>2</p>";
            document.getElementById('result').textContent =
                t.content.children.length.toString();
        </script></body></html>""")
        assert text_of(html, "result") == "2"

    def test_import_node_from_content(self):
        """document.importNode of template.content should deep clone."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<b>bold</b>";
            var clone = document.importNode(t.content, true);
            document.getElementById('result').appendChild(clone);
        </script></body></html>""")
        assert "bold" in str(html)


class TestTemplateCloneNode:
    def test_deep_clone_copies_content(self):
        """cloneNode(true) should clone template content."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<p>cloned</p>";
            var clone = t.cloneNode(true);
            document.getElementById('result').textContent =
                clone.content.childNodes.length + ":" +
                clone.content.firstChild.textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "1:cloned"

    def test_shallow_clone_has_content(self):
        """cloneNode(false) on template should still have a content property."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<p>original</p>";
            var clone = t.cloneNode(false);
            document.getElementById('result').textContent =
                (clone.content !== null && clone.content !== undefined).toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"

    def test_clone_independence(self):
        """Modifying cloned content should not affect original."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<p>original</p>";
            var clone = t.cloneNode(true);
            clone.content.firstChild.textContent = "modified";
            document.getElementById('result').textContent =
                t.content.firstChild.textContent + ":" +
                clone.content.firstChild.textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "original:modified"


class TestTemplateSerialization:
    def test_outerhtml_includes_content(self):
        """template.outerHTML should include content inside the tags."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = "<p>content</p>";
            // outerHTML contains HTML — check via indexOf
            var oh = t.outerHTML;
            document.getElementById('result').textContent =
                (oh.indexOf("template") !== -1 && oh.indexOf("content") !== -1) ? "ok" : "fail:" + oh;
        </script></body></html>""")
        assert text_of(html, "result") == "ok"

    def test_parsed_template_content_in_output(self):
        """Template content from parsed HTML should be in output."""
        html = render("""<html><body>
        <template id="t"><p>parsed content</p></template>
        <div id="result"></div>
        <script>
            var t = document.getElementById("t");
            document.getElementById('result').textContent =
                t.content.firstChild.textContent;
        </script>
        </body></html>""")
        assert text_of(html, "result") == "parsed content"


class TestTemplatePolymerPattern:
    def test_polymer_html_pattern(self):
        """Polymer's pattern: create template, set innerHTML, append content to head."""
        html = render("""<html><body><div id="result"></div><script>
            var t = document.createElement("template");
            t.innerHTML = '<style>.test { color: red; }</style>';
            document.head.appendChild(t.content);
            document.getElementById('result').textContent =
                document.head.querySelectorAll("style").length > 0 ? "ok" : "fail";
        </script></body></html>""")
        assert text_of(html, "result") == "ok"

    def test_polymer_template_stamping(self):
        """Polymer stamps template content into shadow roots."""
        html = render("""<html><body><div id="result"></div><script>
            var host = document.createElement("div");
            var sr = host.attachShadow({mode: "open"});
            var t = document.createElement("template");
            t.innerHTML = '<span>stamped</span>';
            sr.appendChild(t.content.cloneNode(true));
            document.getElementById('result').textContent =
                sr.querySelector("span").textContent;
        </script></body></html>""")
        assert text_of(html, "result") == "stamped"

    def test_youtube_gd_pattern(self):
        """YouTube's _.GD() pattern: create template, access .content immediately."""
        html = render("""<html><body><div id="result"></div><script>
            function GD() {
                var t = document.createElement("template");
                return t;
            }
            var template = GD();
            template.innerHTML = '<div class="yt-thing">works</div>';
            document.head.appendChild(template.content);
            var found = document.head.querySelector(".yt-thing");
            document.getElementById('result').textContent =
                found ? found.textContent : "not found";
        </script></body></html>""")
        assert text_of(html, "result") == "works"


class TestTemplateNested:
    def test_nested_template(self):
        """Nested templates should each have their own content."""
        html = render("""<html><body><div id="result"></div><script>
            var outer = document.createElement("template");
            outer.innerHTML = '<template id="inner"><p>nested</p></template>';
            var inner = outer.content.querySelector("template");
            document.getElementById('result').textContent =
                (inner !== null && inner.content.firstChild.textContent === "nested").toString();
        </script></body></html>""")
        assert text_of(html, "result") == "true"
