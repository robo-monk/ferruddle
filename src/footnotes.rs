//! Structural footnote normalization. Definitions are copied before mutation,
//! and source IDs are matched as strings rather than interpolated selectors.

use std::collections::HashSet;
use std::sync::LazyLock;

use dom_query::NodeRef;
use regex::Regex;

use crate::constants::{BLOCK_LEVEL_ELEMENTS, FOOTNOTE_LIST_SELECTORS};
use crate::dom::{self, closest, elements, same_node, select};

static SECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(foot\s*notes?|end\s*notes?|notes?|references?)$").unwrap());
static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[?\(?(\d{1,4})\)?\]?$").unwrap());
static LEADING_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\d+[.)]\s*").unwrap());
static HIDDEN_STYLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:display\s*:\s*none|visibility\s*:\s*hidden)\s*;?").unwrap()
});

fn contains(root: NodeRef<'_>, node: NodeRef<'_>) -> bool {
    same_node(root, node)
        || node
            .ancestors_it(None)
            .any(|ancestor| same_node(root, ancestor))
}

fn fragment(node: NodeRef<'_>) -> String {
    node.attr_or("href", "")
        .rsplit_once('#')
        .map(|(_, id)| id.to_lowercase())
        .unwrap_or_default()
}

fn marker_number(node: NodeRef<'_>) -> Option<usize> {
    NUMBER
        .captures(node.text().trim())?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

fn aliases(id: &str) -> Vec<String> {
    let id = id.to_lowercase();
    let mut out = vec![id.clone()];
    for prefix in [
        "cite_note-",
        "cite_ref-",
        "bib.bib",
        "footnote-",
        "footnote_",
        "fn:",
        "fn-",
        "fn",
    ] {
        if let Some(value) = id.strip_prefix(prefix)
            && !value.is_empty()
        {
            out.push(value.to_owned());
        }
    }
    out
}

fn anchor_id(node: NodeRef<'_>) -> String {
    node.id_attr()
        .or_else(|| node.attr("name"))
        .map(|id| id.to_lowercase())
        .unwrap_or_default()
}

fn backref(node: NodeRef<'_>) -> bool {
    if !node.has_name("a") {
        return false;
    }
    let text = node.text().trim().replace(['\u{fe0e}', '\u{fe0f}'], "");
    (!text.is_empty() && text.chars().all(|ch| "^↩↥↑↵⤴⤵⏎".contains(ch)))
        || node.has_class("footnote-backref")
        || node.has_class("easy-footnote-to-top")
        || node.has_attr("data-footnote-backref")
        || fragment(node).starts_with("cite_ref-")
        || fragment(node).starts_with("_ftnref")
        || fragment(node).starts_with("ftnt_ref")
}

fn trim_front(node: NodeRef<'_>) {
    if let Some(first) = node.first_child().filter(NodeRef::is_text) {
        let text = first.text();
        first.set_text(text.trim_start());
    }
}

fn clean_definition(node: NodeRef<'_>) {
    for anchor in select(node, "a") {
        if backref(anchor) {
            if let Some(sup) = anchor
                .parent()
                .filter(|p| p.has_name("sup") && p.element_children().len() == 1)
            {
                sup.remove_from_parent();
            } else {
                anchor.remove_from_parent();
            }
        }
    }
    for element in elements(node) {
        element.remove_attr("hidden");
        element.remove_attr("aria-hidden");
        if let Some(style) = element.attr("style") {
            let style = HIDDEN_STYLE.replace_all(&style, "");
            if style.trim().is_empty() {
                element.remove_attr("style");
            } else {
                element.set_attr("style", style.trim());
            }
        }
    }
    while let Some(first) = node.first_child().filter(NodeRef::is_text) {
        let text = first.text();
        if text.contains('^')
            && text
                .chars()
                .all(|ch| ch.is_whitespace() || "^,.;".contains(ch))
        {
            first.remove_from_parent();
        } else {
            break;
        }
    }
    trim_front(node);
}

fn strip_leading_marker(node: NodeRef<'_>) {
    trim_front(node);
    let Some(first) = node.first_element_child() else {
        return;
    };
    if first.has_name("sup")
        || (first.has_name("strong") && marker_number(first).is_some())
        || (first.has_name("a") && (!first.has_attr("href") || marker_number(first).is_some()))
    {
        first.remove_from_parent();
    } else if matches!(first.node_name().as_deref(), Some("b" | "strong"))
        && let Some(sup) = first
            .first_element_child()
            .filter(|n| n.has_name("sup") && marker_number(*n).is_some())
    {
        sup.remove_from_parent();
        trim_front(first);
    }
    trim_front(node);
    if let Some(first) = node.first_child().filter(NodeRef::is_text) {
        let text = first.text();
        let stripped = LEADING_NUMBER.replace(&text, "");
        if stripped != text.as_ref() {
            first.set_text(stripped.as_ref());
        }
    }
}

fn copy_content(source: NodeRef<'_>, strip_marker: bool) -> NodeRef<'_> {
    let copy = dom::clone_into(source, source.tree);
    for label in select(
        copy,
        ".footnote-definition-label, .sidenote__id, .sidenote__label, .sn-backref, .easy-footnote-margin-adjust",
    ) {
        label.remove_from_parent();
    }
    if strip_marker {
        strip_leading_marker(copy);
    }
    clean_definition(copy);
    let content = source.tree.new_element("div");
    if copy.has_name("p") {
        content.append_child(&copy);
    } else {
        dom::transfer_children(copy, content);
    }
    content
}

struct Note<'a> {
    number: usize,
    ids: Vec<String>,
    content: NodeRef<'a>,
    refs: Vec<String>,
}

struct Collector<'a> {
    root: NodeRef<'a>,
    notes: Vec<Note<'a>>,
    removals: Vec<NodeRef<'a>>,
    definitions: HashSet<dom_query::NodeId>,
    seen: HashSet<String>,
}

impl<'a> Collector<'a> {
    fn new(root: NodeRef<'a>) -> Self {
        Self {
            root,
            notes: Vec::new(),
            removals: Vec::new(),
            definitions: HashSet::new(),
            seen: HashSet::new(),
        }
    }

    fn add(&mut self, id: &str, content: NodeRef<'a>, explicit: Option<usize>) -> Option<usize> {
        if id.is_empty() || !self.seen.insert(id.to_lowercase()) {
            return None;
        }
        let number = explicit
            .filter(|number| *number > 0 && !self.notes.iter().any(|note| note.number == *number))
            .unwrap_or_else(|| {
                (1..)
                    .find(|number| !self.notes.iter().any(|note| note.number == *number))
                    .unwrap()
            });
        self.notes.push(Note {
            number,
            ids: aliases(id),
            content,
            refs: Vec::new(),
        });
        Some(self.notes.len() - 1)
    }

    fn remove(&mut self, node: NodeRef<'a>) {
        if !same_node(node, self.root) {
            self.definitions.insert(node.id);
            self.removals.push(node);
        }
    }

    fn in_definition(&self, node: NodeRef<'_>) -> bool {
        self.definitions.contains(&node.id)
            || node
                .ancestors_it(None)
                .any(|ancestor| self.definitions.contains(&ancestor.id))
    }

    fn lookup(&self, id: &str) -> Option<usize> {
        let keys = aliases(id);
        self.notes
            .iter()
            .position(|note| keys.iter().any(|id| note.ids.contains(id)))
    }

    fn reference(&mut self, index: usize) -> NodeRef<'a> {
        let note = &mut self.notes[index];
        let id = if note.refs.is_empty() {
            format!("fnref:{}", note.number)
        } else {
            format!("fnref:{}-{}", note.number, note.refs.len() + 1)
        };
        note.refs.push(id.clone());
        let sup = self.root.tree.new_element("sup");
        sup.set_attr("id", &id);
        let anchor = self.root.tree.new_element("a");
        anchor.set_attr("href", &format!("#fn:{}", note.number));
        dom::append_text(anchor, &note.number.to_string());
        sup.append_child(&anchor);
        sup
    }

    fn collect_inline(&mut self) {
        for container in select(
            self.root,
            "span.footnote-container, span.sidenote-container, span.inline-footnote",
        ) {
            let Some(content) = select(
                container,
                "span.footnote, span.sidenote, span.footnoteContent",
            )
            .into_iter()
            .next() else {
                continue;
            };
            let number = self.notes.len() + 1;
            if let Some(index) = self.add(
                &format!("inline-{number}"),
                copy_content(content, false),
                None,
            ) {
                container.replace_with(&self.reference(index));
            }
        }
        for label in select(self.root, "label.footref") {
            let Some(next) = label.next_element_sibling() else {
                continue;
            };
            let note = if next.has_name("input") && next.has_class("footref-toggle") {
                next.next_element_sibling()
            } else {
                Some(next)
            };
            let Some(note) = note.filter(|n| n.has_class("sidenote")) else {
                continue;
            };
            let content = copy_content(note, true);
            let number = self.notes.len() + 1;
            if let Some(index) = self.add(&format!("org-inline-{number}"), content, None) {
                if next.has_name("input") {
                    next.remove_from_parent();
                }
                note.remove_from_parent();
                label.replace_with(&self.reference(index));
            }
        }
        if crate::dom::exists(self.root, "label.footref")
            || self
                .notes
                .iter()
                .any(|n| n.ids.iter().any(|id| id.starts_with("org-inline-")))
        {
            for footer in select(self.root, "footer")
                .into_iter()
                .filter(|footer| crate::dom::exists(*footer, ".footdef"))
            {
                self.remove(footer);
            }
        }
        for reference in select(self.root, "span[data-definition]") {
            let id = reference.attr_or("data-definition", "");
            let definition = select(self.root, "aside[id]")
                .into_iter()
                .find(|node| node.id_attr().is_some_and(|value| value == id));
            let Some(definition) = definition else {
                continue;
            };
            let index = self
                .lookup(&id)
                .or_else(|| self.add(&id, copy_content(definition, false), None));
            if let Some(index) = index {
                self.remove(definition);
                reference.replace_with(&self.reference(index));
            }
        }
    }

    fn collect_explicit(&mut self) {
        for container in select(self.root, FOOTNOTE_LIST_SELECTORS) {
            if self.in_definition(container) {
                continue;
            }
            let count = self.notes.len();
            if container.has_class("footnotes-segment") {
                for heading in select(container, "h5.footnote-body-heading") {
                    let id = select(heading, "a[id]")
                        .into_iter()
                        .next()
                        .map(anchor_id)
                        .unwrap_or_default();
                    let content = self.root.tree.new_element("div");
                    let mut next = heading.next_element_sibling();
                    while let Some(sibling) = next {
                        if sibling.has_class("footnote-body-heading") {
                            break;
                        }
                        content.append_child(&dom::clone_into(sibling, self.root.tree));
                        next = sibling.next_element_sibling();
                    }
                    self.add(&id, content, None);
                }
            } else if container.has_class("footnote-definitions") {
                for definition in select(container, ".footnote-definition") {
                    let id = select(definition, "sup[id]")
                        .into_iter()
                        .next()
                        .map(anchor_id)
                        .unwrap_or_default();
                    if let Some(body) = select(definition, ".footnote-body").into_iter().next() {
                        self.add(&id, copy_content(body, false), None);
                    }
                }
            } else if container.has_class("footnote-definition") {
                self.add(&anchor_id(container), copy_content(container, true), None);
            } else if container.has_class("footnotes-footer") {
                for definition in select(container, ".footnote-footer") {
                    let id = anchor_id(definition);
                    let content = copy_content(definition, true);
                    if let Some(text) = content.first_child().filter(NodeRef::is_text) {
                        let value = text.text();
                        text.set_text(value.trim_start().trim_start_matches('.').trim_start());
                    }
                    self.add(&id, content, None);
                }
            } else if container.attr_or("data-component-name", "").as_ref() == "FootnoteToDOM" {
                let id = select(container, "a.footnote-number")
                    .into_iter()
                    .next()
                    .map(anchor_id)
                    .unwrap_or_default();
                if let Some(content) = select(container, ".footnote-content").into_iter().next() {
                    self.add(&id, copy_content(content, false), None);
                }
            } else {
                let items = select(container, "li, div[role=listitem]");
                for item in items {
                    if item
                        .ancestors_it(None)
                        .take_while(|ancestor| !same_node(*ancestor, container))
                        .any(|ancestor| {
                            ancestor.has_name("li")
                                || ancestor.attr_or("role", "").as_ref() == "listitem"
                        })
                    {
                        continue;
                    }
                    let citation = select(item, ".citations[id]").into_iter().next();
                    let child_id = select(item, "[id], a[name]")
                        .into_iter()
                        .find(|node| !anchor_id(*node).is_empty());
                    let id = citation
                        .map(anchor_id)
                        .filter(|id| !id.is_empty())
                        .or_else(|| {
                            item.attr("data-counter")
                                .map(|n| n.trim_end_matches('.').to_lowercase())
                        })
                        .unwrap_or_else(|| {
                            let id = anchor_id(item);
                            if id.is_empty() {
                                child_id.map(anchor_id).unwrap_or_default()
                            } else {
                                id
                            }
                        });
                    let source = citation
                        .and_then(|node| select(node, ".citation-content").into_iter().next())
                        .unwrap_or(item);
                    let content = copy_content(source, false);
                    for anchor in select(content, "a[id], a[name]") {
                        if (anchor_id(anchor) == id.to_lowercase())
                            && (anchor.text().trim().is_empty()
                                || marker_number(anchor).is_some()
                                || LEADING_NUMBER.is_match(anchor.text().trim()))
                        {
                            anchor.remove_from_parent();
                        }
                    }
                    self.add(&id, content, None);
                }
            }
            if self.notes.len() > count {
                self.remove(container);
                if let Some(parent) = container.parent().filter(|parent| {
                    parent.has_class("footnotes")
                        && !same_node(*parent, self.root)
                        && !crate::dom::exists(*parent, "h1,h2,h3,h4,h5,h6")
                }) {
                    self.remove(parent);
                }
            }
        }
        for definition in select(
            self.root,
            "p[data-type=footnote][id], p.footnote, p[id^=ftnt]",
        ) {
            if self.in_definition(definition) {
                continue;
            }
            let id = if definition.has_class("footnote") {
                leading_number(definition)
                    .map(|number| number.to_string())
                    .unwrap_or_else(|| anchor_id(definition))
            } else {
                anchor_id(definition)
            };
            if !id.is_empty() {
                self.add(&id, copy_content(definition, true), None);
                self.remove(definition);
                if id.starts_with("ftnt") {
                    let scan = definition
                        .parent()
                        .filter(|parent| {
                            parent.has_name("div")
                                && !same_node(*parent, self.root)
                                && parent.element_children().len() == 1
                        })
                        .unwrap_or(definition);
                    if let Some(heading) = scan.prev_element_sibling().filter(|node| {
                        matches!(
                            node.node_name().as_deref(),
                            Some("h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                        ) && SECTION.is_match(node.text().trim())
                    }) {
                        self.remove(heading);
                    }
                }
            }
        }
        for list in select(self.root, "aside > ol[start]") {
            let Some(number) = list
                .attr_or("start", "")
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
            else {
                continue;
            };
            let content = self.root.tree.new_element("div");
            let items = list
                .element_children()
                .into_iter()
                .filter(|n| n.has_name("li"));
            for item in items {
                let paragraph = self.root.tree.new_element("p");
                let clone = dom::clone_into(item, self.root.tree);
                dom::transfer_children(clone, paragraph);
                content.append_child(&paragraph);
            }
            self.add(&number.to_string(), content, Some(number));
            self.remove(list.parent().unwrap());
        }
        for column in select(self.root, ".sidenotes-column") {
            for sidenote in select(column, ".sidenote[id]") {
                let number = select(sidenote, ".sidenote__id").first().and_then(|n| {
                    n.text()
                        .trim()
                        .trim_matches(|ch: char| !ch.is_ascii_digit())
                        .parse()
                        .ok()
                });
                self.add(&anchor_id(sidenote), copy_content(sidenote, false), number);
            }
            self.remove(column);
        }
        for anchor in select(self.root, "a[href*=\"#_ftnref\"]") {
            if self.in_definition(anchor) {
                continue;
            }
            let id = fragment(anchor).replacen("_ftnref", "_ftn", 1);
            let Some(definition) = closest(anchor, "p, li, div") else {
                continue;
            };
            self.add(&id, copy_content(definition, false), None);
            self.remove(definition);
        }
    }

    fn collect_br_definitions(&mut self) {
        let linked: HashSet<_> = select(self.root, "a[href*=\"#\"]")
            .into_iter()
            .filter(|node| marker_number(*node).is_some() && !self.in_definition(*node))
            .map(fragment)
            .collect();
        if linked.len() < 2 {
            return;
        }
        for block in select(self.root, "p, div") {
            if self.in_definition(block)
                || !block.element_children().iter().any(|n| n.has_name("br"))
            {
                continue;
            }
            let mut groups = vec![Vec::new()];
            for node in block.children() {
                if node.has_name("br") {
                    groups.push(Vec::new());
                } else {
                    groups.last_mut().unwrap().push(node);
                }
            }
            let mut definitions = Vec::new();
            for nodes in groups {
                let Some(anchor) = nodes
                    .iter()
                    .copied()
                    .find(|n| !n.is_text() || !n.text().trim().is_empty())
                else {
                    continue;
                };
                let id = anchor_id(anchor);
                if anchor.has_name("a") && linked.contains(&id) {
                    let content = self.root.tree.new_element("div");
                    for node in nodes.into_iter().filter(|node| !same_node(*node, anchor)) {
                        content.append_child(&dom::clone_into(node, self.root.tree));
                    }
                    clean_definition(content);
                    definitions.push((id, content));
                }
            }
            if definitions.len() >= 2 {
                for (id, content) in definitions {
                    self.add(&id, content, None);
                }
                self.remove(block);
                if let Some(hr) = block
                    .prev_element_sibling()
                    .filter(|node| node.has_name("hr"))
                {
                    self.remove(hr);
                }
                return;
            }
        }
    }

    fn collect_generic(&mut self) {
        if !self.notes.is_empty() {
            return;
        }
        let candidates: Vec<_> = select(self.root, "a[href*=\"#\"]")
            .into_iter()
            .filter(|node| marker_number(*node).is_some())
            .collect();
        let ids: HashSet<_> = candidates
            .iter()
            .copied()
            .map(fragment)
            .filter(|id| !id.is_empty())
            .collect();
        if ids.len() < 2 {
            return;
        }
        let matching = |container| -> Vec<(String, NodeRef<'a>)> {
            let mut seen = HashSet::new();
            select(container, "li, p, div")
                .into_iter()
                .filter_map(|node| {
                    let direct = anchor_id(node);
                    let (id, child) = if ids.contains(&direct) {
                        (direct, None)
                    } else {
                        let anchor = select(node, "a[id], a[name]")
                            .into_iter()
                            .find(|a| ids.contains(&anchor_id(*a)))?;
                        (anchor_id(anchor), Some(anchor))
                    };
                    // A wrapper div around many distinct definitions is not itself
                    // a definition. Keep the smallest meaningful paragraph/list item.
                    if node.has_name("div")
                        && child.is_some_and(|anchor| {
                            select(node, "li, p, div")
                                .iter()
                                .any(|descendant| contains(*descendant, anchor))
                        })
                    {
                        return None;
                    }
                    if seen.insert(id.clone()) {
                        Some((id, node))
                    } else {
                        None
                    }
                })
                .collect()
        };
        let mut best = None;
        for container in select(self.root, "div, section, aside, footer, ol, ul") {
            let found = matching(container);
            if found.len() < 2 {
                continue;
            }
            let external: HashSet<_> = candidates
                .iter()
                .copied()
                .filter(|node| !contains(container, *node))
                .map(fragment)
                .collect();
            let matched = found.iter().filter(|(id, _)| external.contains(id)).count();
            if matched < 2 || matched * 4 < external.len() * 3 {
                continue;
            }
            if best.as_ref().is_none_or(
                |(_, previous): &(NodeRef<'a>, Vec<(String, NodeRef<'a>)>)| {
                    found.len() >= previous.len()
                },
            ) {
                best = Some((container, found));
            }
        }
        if let Some((container, found)) = best {
            let definition_ids: HashSet<_> = found.iter().map(|(_, n)| n.id).collect();
            for (id, node) in found {
                let content = copy_content(node, false);
                for anchor in select(content, "a[id], a[name]") {
                    if anchor_id(anchor) == id
                        && (anchor.text().trim().is_empty()
                            || marker_number(anchor).is_some()
                            || LEADING_NUMBER.is_match(anchor.text().trim()))
                    {
                        anchor.remove_from_parent();
                    }
                }
                let first_block = content
                    .first_element_child()
                    .filter(|node| node.has_name("p"))
                    .unwrap_or(content);
                trim_front(first_block);
                if let Some(first) = first_block.first_child().filter(NodeRef::is_text) {
                    let text = first.text();
                    first.set_text(LEADING_NUMBER.replace(&text, "").as_ref());
                }
                // Named-anchor footnotes can continue through unnumbered siblings.
                let mut next = node.next_element_sibling();
                while let Some(sibling) = next {
                    if definition_ids.contains(&sibling.id)
                        || !anchor_id(sibling).is_empty()
                        || select(sibling, "a[id], a[name]")
                            .into_iter()
                            .any(|a| ids.contains(&anchor_id(a)))
                    {
                        break;
                    }
                    content.append_child(&dom::clone_into(sibling, self.root.tree));
                    next = sibling.next_element_sibling();
                }
                self.add(&id, content, None);
            }
            self.remove(container);
        }
    }

    fn collect_numbered(&mut self) {
        if !self.notes.is_empty() {
            return;
        }
        // A labeled footnote container supplies explicit evidence even for one
        // note; otherwise require two distinct inline superscript references.
        let labeled = select(self.root, "div, section, aside")
            .into_iter()
            .find(|node| {
                let name = format!(
                    "{} {}",
                    node.class().unwrap_or_default(),
                    node.id_attr().unwrap_or_default()
                )
                .to_lowercase();
                name.contains("footnote")
                    && select(*node, "h1,h2,h3,h4,h5,h6")
                        .iter()
                        .any(|heading| SECTION.is_match(heading.text().trim()))
            });
        let numbered: Vec<_> = select(labeled.unwrap_or(self.root), "p")
            .into_iter()
            .filter_map(|paragraph| leading_number(paragraph).map(|number| (number, paragraph)))
            .collect();
        if numbered.is_empty() {
            if let Some(container) = labeled
                && let Some(list) = select(container, "ol").into_iter().next()
            {
                for (index, item) in list
                    .element_children()
                    .into_iter()
                    .filter(|node| node.has_name("li"))
                    .enumerate()
                {
                    let id = anchor_id(item);
                    let id = if id.is_empty() {
                        (index + 1).to_string()
                    } else {
                        id
                    };
                    self.add(&id, copy_content(item, false), None);
                }
                if !self.notes.is_empty() {
                    self.remove(container);
                }
            }
            return;
        }
        let inline: HashSet<_> = select(self.root, "sup")
            .into_iter()
            .filter(|sup| !numbered.iter().any(|(_, p)| contains(*p, *sup)))
            .filter_map(marker_number)
            .collect();
        if labeled.is_none()
            && numbered
                .iter()
                .filter(|(number, _)| inline.contains(number))
                .count()
                < 2
        {
            return;
        }
        for (number, paragraph) in &numbered {
            let content = copy_content(*paragraph, true);
            let mut next = paragraph.next_element_sibling();
            while let Some(sibling) = next {
                if numbered.iter().any(|(_, node)| same_node(*node, sibling)) {
                    break;
                }
                let should_collect = labeled.is_some()
                    || paragraph
                        .prev_element_sibling()
                        .is_some_and(|n| n.has_name("hr"))
                    || numbered.iter().any(|(_, node)| {
                        node.prev_element_sibling()
                            .is_some_and(|n| n.has_name("hr"))
                    });
                if !should_collect
                    || matches!(
                        sibling.node_name().as_deref(),
                        Some("h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                    )
                {
                    break;
                }
                content.append_child(&dom::clone_into(sibling, self.root.tree));
                self.remove(sibling);
                next = sibling.next_element_sibling();
            }
            self.add(&number.to_string(), content, Some(*number));
            self.remove(*paragraph);
        }
        if let Some(container) = labeled {
            self.remove(container);
        }
    }

    fn assign_references(&mut self) {
        // Remove duplicate visual sidenotes only when formal definitions exist.
        if !self.notes.is_empty() {
            for sidenote in select(self.root, "span.sidenote") {
                if !self.in_definition(sidenote) {
                    self.remove(sidenote);
                }
            }
        }
        for group in select(self.root, "cite.ltx_cite, sup") {
            if self.in_definition(group) || closest(group, "sup[id^=\"fnref:\"]").is_some() {
                continue;
            }
            let matches: Vec<_> = select(group, "a[href*=\"#\"]")
                .into_iter()
                .filter_map(|anchor| self.lookup(&fragment(anchor)))
                .collect();
            if matches.is_empty() || (matches.len() < 2 && !group.has_name("cite")) {
                continue;
            }
            let container = outer_reference(group, self.root);
            for (position, index) in matches.into_iter().enumerate() {
                if position > 0 {
                    container.insert_before(&self.root.tree.new_text(" "));
                }
                container.insert_before(&self.reference(index));
            }
            container.remove_from_parent();
        }
        for anchor in select(self.root, "a[href*=\"#\"]") {
            if !contains(self.root, anchor)
                || self.in_definition(anchor)
                || closest(anchor, "sup[id^=\"fnref:\"]").is_some()
            {
                continue;
            }
            let Some(index) = self.lookup(&fragment(anchor)) else {
                continue;
            };
            if anchor.text().trim().is_empty() {
                continue;
            }
            let explicit = closest(
                anchor,
                "sup, .footnote-reference, .footnote-link, .footnote-anchor",
            )
            .is_some()
                || anchor.attr_or("data-type", "").as_ref() == "noteref"
                || anchor.attr_or("role", "").as_ref() == "doc-biblioref";
            if !explicit && marker_number(anchor).is_none() {
                continue;
            }
            let container = outer_reference(anchor, self.root);
            container.replace_with(&self.reference(index));
        }
        for reference in select(
            self.root,
            "sup, span.footnote-ref, span.footnote-reference, span.footnote-link",
        ) {
            if !contains(self.root, reference)
                || self.in_definition(reference)
                || closest(reference, "sup[id^=\"fnref:\"]").is_some()
            {
                continue;
            }
            let data_id = reference.attr("data-footnote-id");
            let number = marker_number(reference);
            let index = data_id
                .as_ref()
                .and_then(|id| self.lookup(id))
                .or_else(|| {
                    number
                        .and_then(|number| self.notes.iter().position(|note| note.number == number))
                })
                .or_else(|| number.and_then(|number| self.lookup(&number.to_string())));
            let Some(index) = index else { continue };
            let container = outer_reference(reference, self.root);
            container.replace_with(&self.reference(index));
        }
    }

    fn finish(mut self) {
        if self.notes.is_empty() {
            return;
        }
        self.assign_references();
        for node in &self.removals {
            node.remove_from_parent();
        }
        for anchor in select(self.root, ".easy-footnote-margin-adjust") {
            anchor.remove_from_parent();
        }
        let container = self.root.tree.new_element("div");
        container.set_attr("id", "footnotes");
        let list = self.root.tree.new_element("ol");
        self.notes.sort_by_key(|note| note.number);
        for note in self.notes {
            clean_definition(note.content);
            let item = self.root.tree.new_element("li");
            item.set_attr("id", &format!("fn:{}", note.number));
            item.set_attr("class", "footnote");
            let has_blocks = note.content.element_children().iter().any(|child| {
                child
                    .node_name()
                    .is_some_and(|name| BLOCK_LEVEL_ELEMENTS.contains(&name.as_ref()))
            });
            if has_blocks {
                dom::transfer_children(note.content, item);
            } else {
                let paragraph = self.root.tree.new_element("p");
                dom::transfer_children(note.content, paragraph);
                item.append_child(&paragraph);
            }
            let backrefs = select(item, "p")
                .into_iter()
                .rfind(|paragraph| {
                    !paragraph.text().trim().is_empty()
                        || crate::dom::exists(*paragraph, "img,video,audio,iframe,svg,math")
                })
                .unwrap_or(item);
            for (index, id) in note.refs.iter().enumerate() {
                let anchor = self.root.tree.new_element("a");
                anchor.set_attr("href", &format!("#{id}"));
                anchor.set_attr("class", "footnote-backref");
                anchor.set_attr("title", "return to article");
                dom::append_text(
                    anchor,
                    if index + 1 < note.refs.len() {
                        "↩ "
                    } else {
                        "↩"
                    },
                );
                backrefs.append_child(&anchor);
            }
            list.append_child(&item);
        }
        container.append_child(&list);
        self.root.append_child(&container);
        crate::standardize::remove_orphaned_dividers(self.root);
    }
}

fn leading_number(paragraph: NodeRef<'_>) -> Option<usize> {
    let first = paragraph.first_child().filter(NodeRef::is_element)?;
    if first.has_name("sup") || first.has_name("strong") {
        return marker_number(first);
    }
    if first.has_name("b") {
        return first
            .first_child()
            .filter(|node| node.has_name("sup"))
            .and_then(marker_number);
    }
    None
}

fn outer_reference<'a>(mut node: NodeRef<'a>, root: NodeRef<'_>) -> NodeRef<'a> {
    while let Some(parent) = node.parent() {
        if same_node(parent, root) || !matches!(parent.node_name().as_deref(), Some("sup" | "span"))
        {
            break;
        }
        if parent.children().into_iter().any(|child| {
            !same_node(child, node) && (!child.is_text() || !child.text().trim().is_empty())
        }) {
            break;
        }
        node = parent;
    }
    node
}

/// Move labeled external definitions (and nearby sidenote columns) into the
/// selected content before the page's outer wrappers are discarded.
pub fn adopt_external(doc: &dom_query::Document, root: NodeRef<'_>) {
    for node in select(doc.root(), "div, section, aside") {
        if contains(root, node) || contains(node, root) {
            continue;
        }
        let identity = format!(
            "{} {}",
            node.class().unwrap_or_default(),
            node.id_attr().unwrap_or_default()
        )
        .to_lowercase();
        let labeled = identity.contains("footnote")
            && select(node, "h1,h2,h3,h4,h5,h6")
                .iter()
                .any(|heading| SECTION.is_match(heading.text().trim()));
        let sidenote = node.has_class("sidenotes-column")
            && root
                .ancestors_it(Some(3))
                .any(|ancestor| contains(ancestor, node));
        if labeled || sidenote {
            root.append_child(&node);
        }
    }
}

/// Normalize footnotes before hidden and clutter removal can discard source
/// definitions. No HTML is built by interpolating page-derived strings.
pub fn standardize(root: NodeRef<'_>) {
    let mut collector = Collector::new(root);
    collector.collect_inline();
    collector.collect_explicit();
    collector.collect_br_definitions();
    collector.collect_generic();
    collector.collect_numbered();
    collector.finish();
}
