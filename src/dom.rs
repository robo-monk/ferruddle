//! DOM operations with explicit tree ownership and inert HTML parsing.
//!
//! `dom_query` does not visit `template.content` in ordinary selectors, and its
//! selector matcher does not establish a `:scope` element. Use direct child
//! iteration for scoped child queries and [`subtree`] for security traversals.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::{cell::RefCell, rc::Rc};

use ::selectors::context::SelectorCaches;
use dom_query::{Document, Matcher, NodeData, NodeRef, Tree};
use regex::Regex;

const HTML_NS: &str = "http://www.w3.org/1999/xhtml";

mod selectors;

thread_local! {
    // A bounded cache amortizes the many repeated CSS queries in cleanup.
    // Per-thread ownership avoids synchronization between independent pages.
    static MATCHERS: RefCell<HashMap<String,Rc<Matcher>>> = RefCell::new(HashMap::new());
}

pub(crate) fn matcher(selector: &str) -> Option<Rc<Matcher>> {
    MATCHERS.with(|cache| {
        if let Some(matcher) = cache.borrow().get(selector).cloned() {
            return Some(matcher);
        }
        let matcher = Rc::new(Matcher::new(selector).ok()?);
        if selector.len() <= 16_384 {
            let mut cache = cache.borrow_mut();
            if cache.len() >= 512 {
                cache.clear()
            }
            cache.insert(selector.to_owned(), matcher.clone());
        }
        Some(matcher)
    })
}

/// Select descendants in document order. Invalid selectors return no matches.
/// The root and inert template contents are excluded, as in `querySelectorAll`.
pub fn select<'a>(root: NodeRef<'a>, selector: &str) -> Vec<NodeRef<'a>> {
    let Some(matcher) = matcher(selector) else {
        return Vec::new();
    };
    matching(root.descendants_it(), &matcher).collect()
}

/// Filter elements with one selector cache instead of a fresh cache per node.
/// Use it for a single pass over an unchanged tree: the cache records sibling
/// positions, which become stale after mutation.
pub(crate) fn matching<'a>(
    nodes: impl Iterator<Item = NodeRef<'a>>,
    matcher: &Matcher,
) -> impl Iterator<Item = NodeRef<'a>> {
    let mut caches = SelectorCaches::default();
    nodes.filter(move |node| {
        node.is_element() && matcher.match_element_with_caches(node, &mut caches)
    })
}

/// Test descendants without collecting all matches. Like `select`, this
/// excludes the root and inert template content.
pub(crate) fn exists(root: NodeRef<'_>, selector: &str) -> bool {
    matcher(selector)
        .is_some_and(|matcher| matching(root.descendants_it(), &matcher).next().is_some())
}

/// The fixed clutter list uses conservative keys to skip impossible selectors.
pub(crate) fn select_exact(root: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    selectors::select_exact(root)
}

/// Matches of each entry-point selector in document order, from one walk.
pub(crate) fn select_entry_points(root: NodeRef<'_>) -> Vec<Vec<NodeRef<'_>>> {
    selectors::select_entry_points(root)
}

/// Retag only with names chosen by the implementation, never page text.
pub(crate) fn retag(node: NodeRef<'_>, name: &'static str) {
    node.update(|node| {
        if let NodeData::Element(element) = &mut node.data {
            element.name.local = name.into();
        }
    });
}

/// Find the closest matching element, starting with the element itself.
pub fn closest<'a>(node: NodeRef<'a>, selector: &str) -> Option<NodeRef<'a>> {
    let matcher = matcher(selector)?;
    matching(
        std::iter::once(node).chain(node.ancestors_it(None)),
        &matcher,
    )
    .next()
}

/// All ordinary elements in preorder, including an element root.
pub fn elements(root: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    std::iter::once(root)
        .chain(root.descendants_it())
        .filter(NodeRef::is_element)
        .collect()
}

/// A template's inert fragment, which ordinary selectors do not traverse.
pub fn template_content(template: NodeRef<'_>) -> Option<NodeRef<'_>> {
    let id = template.element_ref()?.template_contents?;
    Some(NodeRef::new(id, template.tree))
}

/// All nodes in preorder, including the root and nested template fragments.
/// This snapshot can safely be used while removing or moving descendants.
pub fn subtree(root: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    let mut nodes = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        nodes.push(node);
        pending.extend(node.children_it(true));
        if let Some(content) = template_content(node) {
            pending.push(content);
        }
    }
    nodes
}

/// Whether two references identify the same node in the same tree.
pub fn same_node(left: NodeRef<'_>, right: NodeRef<'_>) -> bool {
    std::ptr::eq(left.tree, right.tree) && left.id == right.id
}

/// Whether `node` precedes `other`. An ancestor precedes its descendants;
/// disconnected trees and nodes without a shared root have no document order.
pub fn is_before(node: NodeRef<'_>, other: NodeRef<'_>) -> bool {
    if !std::ptr::eq(node.tree, other.tree) || same_node(node, other) {
        return false;
    }
    let mut left: Vec<_> = std::iter::once(node)
        .chain(node.ancestors_it(None))
        .collect();
    let mut right: Vec<_> = std::iter::once(other)
        .chain(other.ancestors_it(None))
        .collect();
    if !left
        .last()
        .zip(right.last())
        .is_some_and(|(a, b)| same_node(*a, *b))
    {
        return false;
    }
    while left
        .last()
        .zip(right.last())
        .is_some_and(|(a, b)| same_node(*a, *b))
    {
        left.pop();
        right.pop();
    }
    let Some(branch) = left.last() else {
        return true;
    };
    let Some(other_branch) = right.last() else {
        return false;
    };
    let mut sibling = branch.next_sibling();
    while let Some(current) = sibling {
        if same_node(current, *other_branch) {
            return true;
        }
        sibling = current.next_sibling();
    }
    false
}

/// Create an HTML element. Use attribute setters for page-derived values.
pub fn create_element<'a>(doc: &'a Document, tag: &str) -> NodeRef<'a> {
    doc.tree.new_element(tag)
}

/// Append text, relying on the HTML serializer to escape it.
pub fn append_text<'a>(parent: NodeRef<'a>, text: &str) -> NodeRef<'a> {
    let node = parent.tree.new_text(text);
    parent.append_child(&node);
    node
}

/// Copy a complete subtree into another tree, preserving namespaces and inert
/// template fragments. Unlike node IDs, the copy is safe across document clones.
pub fn clone_into<'a>(source: NodeRef<'_>, target: &'a Tree) -> NodeRef<'a> {
    fn shallow<'a>(source: NodeRef<'_>, target: &'a Tree) -> NodeRef<'a> {
        let mut data = source
            .query(|node| node.data.clone())
            .expect("existing DOM node");
        if let NodeData::Element(element) = &mut data {
            element.template_contents = None;
        }
        NodeRef::new(target.create_node(data), target)
    }
    let root = shallow(source, target);
    let mut pending = vec![(source, root)];
    while let Some((original, copied)) = pending.pop() {
        // Snapshot: the iterator borrows the arena, and source and target may
        // be the same tree. Holding it while creating nodes would RefCell-panic.
        for child in original.children() {
            let copy = shallow(child, target);
            copied.append_child(&copy);
            pending.push((child, copy));
        }
        if let Some(content) = template_content(original) {
            let copy = shallow(content, target);
            copied.update(|node| {
                if let NodeData::Element(element) = &mut node.data {
                    element.template_contents = Some(copy.id);
                }
            });
            pending.push((content, copy));
        }
    }
    root
}

/// Parse an existing HTML fragment inertly, then append independent node copies.
/// Never construct `html` by interpolating unescaped page-derived strings.
pub fn append_fragment<'a>(parent: NodeRef<'a>, html: &str) -> Vec<NodeRef<'a>> {
    let fragment = Document::fragment(html);
    let nodes: Vec<_> = fragment
        .html_root()
        .children_it(false)
        .map(|child| clone_into(child, parent.tree))
        .collect();
    for node in &nodes {
        parent.append_child(node);
    }
    nodes
}

/// Replace a target's children by moving a source's children in order. Both
/// nodes must belong to the same tree; a cycle is rejected without changes.
pub fn transfer_children(source: NodeRef<'_>, target: NodeRef<'_>) {
    if !std::ptr::eq(source.tree, target.tree)
        || same_node(source, target)
        || target
            .ancestors_it(None)
            .any(|node| same_node(node, source))
    {
        return;
    }
    target.remove_children();
    for child in source.children() {
        target.append_child(&child);
    }
}

/// Replace a wrapper by its children, preserving text and element order.
pub fn unwrap(node: NodeRef<'_>) {
    if node.parent().is_none() {
        return;
    }
    for child in node.children() {
        node.insert_before(&child);
    }
    node.remove_from_parent();
}

/// Whether an element uses a particular HTML tag, rather than a foreign tag
/// with the same local name.
pub fn is_html_element(node: NodeRef<'_>, tag: &str) -> bool {
    node.qual_name_ref().is_some_and(|name| {
        name.ns.as_ref() == HTML_NS && name.local.as_ref().eq_ignore_ascii_case(tag)
    })
}

/// Read srcset URL/descriptor pairs using the HTML token boundaries. Commas
/// within a URL (data images and image-CDN transformations) are preserved.
/// A trailing URL comma or a comma after descriptors separates candidates.
pub fn srcset_candidates(value: &str) -> Vec<(String, String)> {
    let bytes = value.as_bytes();
    let mut cursor = 0;
    let mut candidates = Vec::new();
    while cursor < bytes.len() {
        while cursor < bytes.len() && (bytes[cursor].is_ascii_whitespace() || bytes[cursor] == b',')
        {
            cursor += 1;
        }
        let start = cursor;
        while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let token = &value[start..cursor];
        if token.is_empty() {
            break;
        }
        let url = token.trim_end_matches(',');
        if token.ends_with(',') {
            if !url.is_empty() {
                candidates.push((url.to_owned(), String::new()));
            }
            continue;
        }
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let descriptor_start = cursor;
        let mut parentheses = 0usize;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b',' if parentheses == 0 => break,
                b'(' => parentheses += 1,
                b')' => parentheses = parentheses.saturating_sub(1),
                _ => (),
            }
            cursor += 1;
        }
        candidates.push((
            url.to_owned(),
            value[descriptor_start..cursor].trim().to_owned(),
        ));
        if cursor < bytes.len() {
            cursor += 1;
        }
    }
    candidates
}

/// Reconstruct static content before selection, without loading resources or
/// evaluating JavaScript. Call the unconditional sanitizer after preprocessing.
/// Returns whether image fallback markup was materialized or upgraded.
pub fn preprocess(doc: &Document) -> bool {
    flatten_declarative_shadow_roots(doc);
    resolve_streamed_content(doc);
    resolve_noscript_images(doc)
}

/// Hoist valid declarative shadow contents at their template's exact position.
/// Invalid/inert templates are never exposed. Nested roots need no depth limit.
pub fn flatten_declarative_shadow_roots(doc: &Document) {
    let mut pending = vec![doc.root()];
    while let Some(node) = pending.pop() {
        if is_html_element(node, "template") {
            let mode = node
                .attr("shadowrootmode")
                .or_else(|| node.attr("shadowroot"))
                .unwrap_or_default();
            if !(mode.eq_ignore_ascii_case("open") || mode.eq_ignore_ascii_case("closed")) {
                continue;
            }
            if node.parent().is_none() {
                continue;
            }
            let source = template_content(node).unwrap_or(node);
            let children = source.children();
            for child in &children {
                node.insert_before(child);
            }
            node.remove_from_parent();
            pending.extend(children.into_iter().rev());
        } else {
            pending.extend(node.children_it(true));
        }
    }
}

/// Resolve literal React `$RC("B:n", "S:n")` streaming swaps. All structure is
/// validated before mutation, including nested suspense closing markers, so a
/// truncated stream does not destroy its fallback or unrelated article text.
pub fn resolve_streamed_content(doc: &Document) {
    static SWAP: OnceLock<Regex> = OnceLock::new();
    let swap_re = SWAP.get_or_init(|| {
        Regex::new(r#"\$RC\(\s*["'](B:\d+)["']\s*,\s*["'](S:\d+)["']\s*\)"#).expect("literal regex")
    });
    let swaps: Vec<_> = select(doc.root(), "script")
        .into_iter()
        .filter(|script| script.attr("src").is_none())
        .flat_map(|script| {
            swap_re
                .captures_iter(&script.text())
                .map(|capture| (capture[1].to_owned(), capture[2].to_owned()))
                .collect::<Vec<_>>()
        })
        .collect();
    if swaps.is_empty() {
        return;
    }
    let by_id: HashMap<_, _> = elements(doc.root())
        .into_iter()
        .filter_map(|node| node.id_attr().map(|id| (id.to_string(), node)))
        .collect();
    for (template_id, content_id) in swaps {
        let Some(&template) = by_id.get(&template_id) else {
            continue;
        };
        let Some(&content) = by_id.get(&content_id) else {
            continue;
        };
        if !is_html_element(template, "template")
            || !is_html_element(content, "div")
            || template.parent().is_none()
            || content.parent().is_none()
            || template
                .ancestors_it(None)
                .any(|node| same_node(node, content))
        {
            continue;
        }
        let mut fallback = Vec::new();
        let mut cursor = template.next_sibling();
        let mut depth = 0usize;
        let mut found_marker = false;
        while let Some(node) = cursor {
            cursor = node.next_sibling();
            fallback.push(node);
            if node.is_comment() {
                let text = comment_text(node);
                match text.as_str() {
                    "$" | "$?" | "$!" => depth += 1,
                    "/$" if depth == 0 => {
                        found_marker = true;
                        break;
                    }
                    "/$" => depth -= 1,
                    _ => (),
                }
            }
        }
        if !found_marker
            || fallback.iter().any(|node| {
                same_node(*node, content)
                    || content
                        .ancestors_it(None)
                        .any(|ancestor| same_node(ancestor, *node))
            })
        {
            continue;
        }
        for node in fallback {
            node.remove_from_parent();
        }
        for child in content.children() {
            template.insert_before(&child);
        }
        template.remove_from_parent();
        content.remove_from_parent();
    }
}

fn comment_text(node: NodeRef<'_>) -> String {
    node.query_or(String::new(), |node| match &node.data {
        NodeData::Comment { contents } => contents.to_string(),
        _ => String::new(),
    })
}

/// Restore real noscript image sources only in a lazy-loading image context or
/// for an explicit placeholder with a matching nonempty alt. Tracking-only
/// noscripts are not promoted. Attribute values are copied through setters.
pub fn resolve_noscript_images(doc: &Document) -> bool {
    let mut changed = false;
    for noscript in select(doc.root(), "noscript") {
        let Some(parent) = noscript.parent() else {
            continue;
        };
        let parsed;
        let direct = select(noscript, "img").into_iter().next();
        let image = if let Some(image) = direct {
            image
        } else {
            let text = noscript.text();
            if !text.to_ascii_lowercase().contains("<img") {
                continue;
            }
            parsed = Document::fragment(text);
            let Some(image) = select(parsed.root(), "img").into_iter().next() else {
                continue;
            };
            image
        };
        let src = image.attr_or("src", "");
        if src.trim().is_empty()
            || src.trim_start().to_ascii_lowercase().starts_with("data:")
            || crate::safety::is_dangerous_url(&src, false)
        {
            continue;
        }
        let alt = image.attr_or("alt", "");
        let matched = if !alt.is_empty() {
            parent.element_children().into_iter().find(|sibling| {
                is_html_element(*sibling, "img")
                    && sibling
                        .attr_or("src", "")
                        .trim_start()
                        .to_ascii_lowercase()
                        .starts_with("data:")
                    && sibling.attr_or("alt", "") == alt
            })
        } else {
            None
        };
        if let Some(placeholder) = matched {
            changed = true;
            placeholder.set_attr("src", &src);
            if let Some(srcset) = image.attr("srcset") {
                placeholder.set_attr("srcset", &srcset);
            }
            continue;
        }
        let figure = closest(noscript, "figure");
        let parent_class = parent.class().unwrap_or_default().to_ascii_lowercase();
        let lazy_context = figure.is_some()
            || ["image", "img", "picture", "photo", "media"]
                .iter()
                .any(|part| parent_class.contains(part))
            || parent.element_children().into_iter().any(|sibling| {
                !same_node(sibling, noscript)
                    && sibling
                        .class()
                        .is_some_and(|class| class.to_ascii_lowercase().contains("lazy"))
            });
        if !lazy_context {
            continue;
        }
        let container = figure.unwrap_or(parent);
        let has_real_image = select(container, "img").into_iter().any(|existing| {
            if closest(existing, "noscript").is_some() {
                return false;
            }
            let src = existing.attr_or("src", "");
            !src.is_empty()
                && !src.trim_start().to_ascii_lowercase().starts_with("data:")
                && !crate::safety::is_dangerous_url(&src, false)
        });
        if !has_real_image {
            let promoted = clone_into(image, noscript.tree);
            noscript.insert_before(&promoted);
            changed = true;
        }
    }
    changed
}
