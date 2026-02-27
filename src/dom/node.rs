use markup5ever::{ns, Attribute, QualName};

/// The data payload for each node in the arena.
#[derive(Debug, Clone)]
pub enum NodeData {
    /// The root document node.
    Document,

    /// An HTML element with a qualified name and attributes.
    Element(ElementData),

    /// A text node.
    Text(String),

    /// An HTML comment.
    Comment(String),

    /// Doctype declaration.
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct ElementData {
    pub name: QualName,
    pub attrs: Vec<Attribute>,
    /// For <template> elements, stores the content document fragment.
    pub template_contents: Option<super::NodeId>,
    /// Set to true after a <script> has been executed.
    pub script_already_started: bool,
    /// For <annotation-xml> elements with encoding="text/html" or "application/xhtml+xml".
    pub mathml_annotation_xml_integration_point: bool,
}

impl ElementData {
    pub fn new(name: QualName, attrs: Vec<Attribute>) -> Self {
        Self {
            name,
            attrs,
            template_contents: None,
            script_already_started: false,
            mathml_annotation_xml_integration_point: false,
        }
    }

    /// Get an attribute value by local name.
    pub fn get_attribute(&self, local_name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| &*a.name.local == local_name)
            .map(|a| &*a.value)
    }

    /// Set an attribute value by local name. Creates the attribute if it doesn't exist.
    pub fn set_attribute(&mut self, local_name: &str, value: &str) {
        if let Some(attr) = self.attrs.iter_mut().find(|a| &*a.name.local == local_name) {
            attr.value = value.into();
        } else {
            self.attrs.push(Attribute {
                name: QualName::new(None, ns!(), local_name.into()),
                value: value.into(),
            });
        }
    }

    /// Remove an attribute by local name. Returns true if it existed.
    pub fn remove_attribute(&mut self, local_name: &str) -> bool {
        let len_before = self.attrs.len();
        self.attrs.retain(|a| &*a.name.local != local_name);
        self.attrs.len() < len_before
    }
}
