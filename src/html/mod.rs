//! HTML documents and fragments.

#[cfg(feature = "errors")]
use std::borrow::Cow;

use ego_tree::iter::Nodes;
use ego_tree::Tree;
use html5ever::serialize::SerializeOpts;
use html5ever::tree_builder::QuirksMode;
use html5ever::QualName;
use html5ever::{driver, serialize};
use tendril::TendrilSink;

use crate::selector::Selector;
use crate::{ElementRef, Node};

/// An HTML tree.
///
/// Parsing does not fail hard. Instead, the `quirks_mode` is set and errors are added to the
/// `errors` field. The `tree` will still be populated as best as possible.
///
/// Implements the `TreeSink` trait from the `html5ever` crate, which allows HTML to be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Html {
    #[cfg(feature = "errors")]
    /// Parse errors.
    pub errors: Vec<Cow<'static, str>>,

    /// The quirks mode.
    pub quirks_mode: QuirksMode,

    /// The node tree.
    pub tree: Tree<Node>,
}

impl Html {
    /// Creates an empty HTML document.
    pub fn new_document() -> Self {
        Html {
            #[cfg(feature = "errors")]
            errors: Vec::new(),
            quirks_mode: QuirksMode::NoQuirks,
            tree: Tree::new(Node::Document),
        }
    }

    /// Creates an empty HTML fragment.
    pub fn new_fragment() -> Self {
        Html {
            #[cfg(feature = "errors")]
            errors: Vec::new(),
            quirks_mode: QuirksMode::NoQuirks,
            tree: Tree::new(Node::Fragment),
        }
    }

    /// Parses a string of HTML as a document.
    ///
    /// This is a convenience method for the following:
    ///
    /// ```
    /// # extern crate html5ever;
    /// # extern crate scraper;
    /// # extern crate tendril;
    /// # fn main() {
    /// # let document = "";
    /// use html5ever::driver::{self, ParseOpts};
    /// use scraper::Html;
    /// use tendril::TendrilSink;
    ///
    /// let parser = driver::parse_document(Html::new_document(), ParseOpts::default());
    /// let html = parser.one(document);
    /// # }
    /// ```
    pub fn parse_document(document: &str) -> Self {
        let parser = driver::parse_document(Self::new_document(), Default::default());
        parser.one(document)
    }

    /// Parses a string of HTML as a fragment.
    pub fn parse_fragment(fragment: &str) -> Self {
        let parser = driver::parse_fragment(
            Self::new_fragment(),
            Default::default(),
            QualName::new(None, ns!(html), local_name!("body")),
            Vec::new(),
        );
        parser.one(fragment)
    }

    /// Returns an iterator over elements matching a selector.
    pub fn select<'a, 'b>(&'a self, selector: &'b Selector) -> Select<'a, 'b> {
        Select {
            inner: self.tree.nodes(),
            selector,
        }
    }

    /// Returns the root `<html>` element.
    pub fn root_element(&self) -> ElementRef {
        let root_node = self
            .tree
            .root()
            .children()
            .find(|child| child.value().is_element())
            .expect("html node missing");
        ElementRef::wrap(root_node).unwrap()
    }

    /// Serialize entire document into HTML.
    pub fn html(&self) -> String {
        let opts = SerializeOpts {
            scripting_enabled: false, // It's not clear what this does.
            traversal_scope: html5ever::serialize::TraversalScope::IncludeNode,
            create_missing_parent: false,
        };
        let mut buf = Vec::new();
        serialize(&mut buf, self, opts).unwrap();
        String::from_utf8(buf).unwrap()
    }
}

/// Iterator over elements matching a selector.
#[derive(Debug)]
pub struct Select<'a, 'b> {
    inner: Nodes<'a, Node>,
    selector: &'b Selector,
}

impl<'a, 'b> Iterator for Select<'a, 'b> {
    type Item = ElementRef<'a>;

    fn next(&mut self) -> Option<ElementRef<'a>> {
        for node in self.inner.by_ref() {
            if let Some(element) = ElementRef::wrap(node) {
                if element.parent().is_some() && self.selector.matches(&element) {
                    return Some(element);
                }
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_lower, upper) = self.inner.size_hint();

        (0, upper)
    }
}

impl<'a, 'b> DoubleEndedIterator for Select<'a, 'b> {
    fn next_back(&mut self) -> Option<Self::Item> {
        for node in self.inner.by_ref().rev() {
            if let Some(element) = ElementRef::wrap(node) {
                if element.parent().is_some() && self.selector.matches(&element) {
                    return Some(element);
                }
            }
        }
        None
    }
}

mod serializable;
mod tree_sink;

#[cfg(test)]
mod tests {
    use super::Html;
    use super::Selector;

    #[test]
    fn root_element_fragment() {
        let html = Html::parse_fragment(r#"<a href="http://github.com">1</a>"#);
        let root_ref = html.root_element();
        let href = root_ref
            .select(&Selector::parse("a").unwrap())
            .next()
            .unwrap();
        assert_eq!(href.inner_html(), "1");
        assert_eq!(href.value().attr("href").unwrap(), "http://github.com");
    }

    #[test]
    fn root_element_document_doctype() {
        let html = Html::parse_document("<!DOCTYPE html>\n<title>abc</title>");
        let root_ref = html.root_element();
        let title = root_ref
            .select(&Selector::parse("title").unwrap())
            .next()
            .unwrap();
        assert_eq!(title.inner_html(), "abc");
    }

    #[test]
    fn root_element_document_comment() {
        let html = Html::parse_document("<!-- comment --><title>abc</title>");
        let root_ref = html.root_element();
        let title = root_ref
            .select(&Selector::parse("title").unwrap())
            .next()
            .unwrap();
        assert_eq!(title.inner_html(), "abc");
    }

    #[test]
    fn select_is_reversible() {
        let html = Html::parse_document("<p>element1</p><p>element2</p><p>element3</p>");
        let selector = Selector::parse("p").unwrap();
        let result: Vec<_> = html
            .select(&selector)
            .rev()
            .map(|e| e.inner_html())
            .collect();
        assert_eq!(result, vec!["element3", "element2", "element1"]);
    }

    #[test]
    fn select_has_a_size_hint() {
        let html = Html::parse_document("<p>element1</p><p>element2</p><p>element3</p>");
        let selector = Selector::parse("p").unwrap();
        let (lower, upper) = html.select(&selector).size_hint();
        assert_eq!(lower, 0);
        assert_eq!(upper, Some(10));
    }

    #[cfg(feature = "atomic")]
    #[test]
    fn html_is_send() {
        fn send_sync<S: Send>() {}
        send_sync::<Html>();
    }
}
