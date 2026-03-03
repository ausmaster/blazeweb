use std::borrow::Cow;
use std::cell::RefCell;

use html5ever::tendril::StrTendril;
use html5ever::tendril::TendrilSink;
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{parse_document as html5_parse, Attribute, ExpandedName, QualName};
use markup5ever::ns;

use super::arena::{Arena, NodeId};
use super::node::{ElementData, NodeData};

/// TreeSink implementation that builds our Arena.
///
/// Uses RefCell for interior mutability since html5ever's TreeSink
/// trait takes &self but we need to mutate the arena.
pub struct ArenaSink {
    arena: RefCell<Arena>,
    quirks_mode: RefCell<QuirksMode>,
}

impl ArenaSink {
    pub fn new() -> Self {
        Self {
            arena: RefCell::new(Arena::new()),
            quirks_mode: RefCell::new(QuirksMode::NoQuirks),
        }
    }

    /// Consume the sink and return the built arena.
    pub fn into_arena(self) -> Arena {
        self.arena.into_inner()
    }
}

impl TreeSink for ArenaSink {
    type Handle = NodeId;
    type Output = Self;
    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> Self::Output {
        self
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {
        // html5ever parse errors are expected for real-world HTML; ignore.
    }

    fn get_document(&self) -> NodeId {
        self.arena.borrow().document
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> ExpandedName<'a> {
        // Safety: we need to return a reference that borrows from &'a self.
        // The arena lives inside self, and nodes are never removed during parsing,
        // so this reference is valid for the lifetime of &self.
        let arena = self.arena.as_ptr();
        let arena_ref = unsafe { &*arena };
        match &arena_ref.nodes[*target].data {
            NodeData::Element(data) => data.name.expanded(),
            _ => panic!("elem_name called on non-element"),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> NodeId {
        let is_template =
            name.ns == ns!(html) && &*name.local == "template";
        let mut arena = self.arena.borrow_mut();
        let mut elem = ElementData::new(name, attrs);
        if is_template {
            let frag = arena.new_node(NodeData::Document);
            elem.template_contents = Some(frag);
        }
        elem.mathml_annotation_xml_integration_point =
            flags.mathml_annotation_xml_integration_point;
        arena.new_node(NodeData::Element(elem))
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.arena
            .borrow_mut()
            .new_node(NodeData::Comment(text.to_string()))
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> NodeId {
        self.arena
            .borrow_mut()
            .new_node(NodeData::Comment(String::new()))
    }

    fn append(&self, parent: &NodeId, child: NodeOrText<NodeId>) {
        let mut arena = self.arena.borrow_mut();
        let child_id = match child {
            NodeOrText::AppendNode(id) => id,
            NodeOrText::AppendText(text) => {
                // Merge with previous text node if possible.
                if let Some(last) = arena.nodes[*parent].last_child {
                    if let NodeData::Text(ref mut existing) = arena.nodes[last].data {
                        existing.push_str(&text);
                        return;
                    }
                }
                arena.new_node(NodeData::Text(text.to_string()))
            }
        };
        arena.append_child(*parent, child_id);
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        let has_parent = self.arena.borrow().nodes[*element].parent.is_some();
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let mut arena = self.arena.borrow_mut();
        let doctype = arena.new_node(NodeData::Doctype {
            name: name.to_string(),
            public_id: public_id.to_string(),
            system_id: system_id.to_string(),
        });
        let doc = arena.document;
        arena.append_child(doc, doctype);
    }

    fn get_template_contents(&self, target: &NodeId) -> NodeId {
        let arena = self.arena.borrow();
        if let NodeData::Element(data) = &arena.nodes[*target].data {
            data.template_contents
                .expect("template element should always have template_contents")
        } else {
            panic!("get_template_contents called on non-element");
        }
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        *x == *y
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        *self.quirks_mode.borrow_mut() = mode;
    }

    fn append_before_sibling(&self, sibling: &NodeId, child: NodeOrText<NodeId>) {
        let mut arena = self.arena.borrow_mut();
        let child_id = match child {
            NodeOrText::AppendNode(id) => id,
            NodeOrText::AppendText(text) => {
                // Merge with the previous sibling if it's a text node,
                // matching RcDom's behavior (rcdom/lib.rs:460-465).
                if let Some(prev) = arena.nodes[*sibling].prev_sibling {
                    if let NodeData::Text(ref mut existing) = arena.nodes[prev].data {
                        existing.push_str(&text);
                        return;
                    }
                }
                arena.new_node(NodeData::Text(text.to_string()))
            }
        };
        arena.insert_before(*sibling, child_id);
    }

    fn add_attrs_if_missing(&self, target: &NodeId, attrs: Vec<Attribute>) {
        let mut arena = self.arena.borrow_mut();
        if let NodeData::Element(ref mut data) = arena.nodes[*target].data {
            for attr in attrs {
                if data.get_attribute(&attr.name.local).is_none() {
                    data.attrs.push(attr);
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeId) {
        self.arena.borrow_mut().detach(*target);
    }

    fn reparent_children(&self, node: &NodeId, new_parent: &NodeId) {
        self.arena.borrow_mut().reparent_children(*node, *new_parent);
    }

    fn mark_script_already_started(&self, target: &NodeId) {
        let mut arena = self.arena.borrow_mut();
        if let NodeData::Element(ref mut data) = arena.nodes[*target].data {
            data.script_already_started = true;
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, target: &NodeId) -> bool {
        let arena = self.arena.borrow();
        if let NodeData::Element(data) = &arena.nodes[*target].data {
            data.mathml_annotation_xml_integration_point
        } else {
            false
        }
    }

    // We intentionally do NOT override maybe_clone_an_option_into_selectedcontent
    // here. html5ever only calls it for explicit </option> end tags, not for
    // implicitly closed options (see html5ever issue #712). We handle this
    // as a post-processing pass in Arena::populate_selectedcontent() instead,
    // which runs after the full tree is constructed.
}

/// Parse an HTML document string into an Arena.
pub fn parse(html: &str) -> Arena {
    parse_with_options(html, true)
}

/// Parse an HTML document string with explicit scripting flag.
pub fn parse_with_options(html: &str, scripting_enabled: bool) -> Arena {
    use html5ever::tree_builder::TreeBuilderOpts;
    use html5ever::ParseOpts;

    let sink = ArenaSink::new();
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            scripting_enabled,
            ..Default::default()
        },
        ..Default::default()
    };
    let result = html5_parse(sink, opts)
        .from_utf8()
        .one(html.as_bytes());
    postprocess(result.into_arena())
}

/// Parse an HTML fragment with a context element.
///
/// `context` is the context element name, e.g. "div", "math math", "svg svg".
/// The format is "namespace localname" for non-HTML namespaces, or just "localname" for HTML.
pub fn parse_fragment(html: &str, context: &str, scripting_enabled: bool) -> Arena {
    use html5ever::parse_fragment;
    use html5ever::tree_builder::TreeBuilderOpts;
    use html5ever::ParseOpts;
    use markup5ever::ns;

    let (ns, local) = if let Some(rest) = context.strip_prefix("math ") {
        (ns!(mathml), rest)
    } else if let Some(rest) = context.strip_prefix("svg ") {
        (ns!(svg), rest)
    } else {
        (ns!(html), context)
    };
    let context_name = QualName::new(None, ns, local.into());

    let sink = ArenaSink::new();
    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            scripting_enabled,
            ..Default::default()
        },
        ..Default::default()
    };
    let result = parse_fragment(sink, opts, context_name, vec![], false)
        .from_utf8()
        .one(html.as_bytes());
    postprocess(result.into_arena())
}

/// Post-processing passes applied after tree construction.
fn postprocess(mut arena: Arena) -> Arena {
    // Workaround for html5ever issue #712: the maybe_clone_an_option_into_selectedcontent
    // callback only fires for explicit </option> end tags. We run the selectedcontent
    // population after the tree is fully built so it works for all closing modes.
    arena.clone_selectedcontent();
    arena
}


#[cfg(test)]
#[path = "treesink_tests.rs"]
mod tests;
