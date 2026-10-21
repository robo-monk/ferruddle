//! Strict rendered-content comparison, separate from lossy text triage.
//!
//! Markdown delimiter/fence choices and HTML serializer attribute ordering are
//! irrelevant. Block structure, exact code text, link/image destinations, list
//! hierarchy, table spans and the footnote reference graph remain significant.
use std::collections::{BTreeMap, HashMap, HashSet};

use dom_query::{Document, NodeRef};
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SemanticNode {
    Text {
        text: String,
    },
    Anchor {
        id: String,
    },
    PreservedText {
        text: String,
    },
    Element {
        name: String,
        attributes: Vec<(String, String)>,
        children: Vec<SemanticNode>,
    },
    Code {
        block: bool,
        language: String,
        text: String,
    },
}

fn gfm_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
}

/// Compare the visible semantic structure, including exact code payloads.
/// Internal footnote identifiers are renamed by first appearance while keeping
/// repeated references and reference/definition associations intact.
pub fn markdown_semantics(markdown: &str) -> Vec<SemanticNode> {
    html_semantics(&render_markdown(markdown))
}

/// Canonical HTML semantics, useful for comparing cleaned extraction output.
/// This intentionally retains meaningful tags and attributes rather than
/// claiming that equivalent plain text proves extraction correctness.
pub fn html_semantics(source: &str) -> Vec<SemanticNode> {
    let document = Document::from(source);
    let root = document.body().unwrap_or_else(|| document.root());
    let targets: HashSet<_> = root
        .descendants_it()
        .filter_map(|node| {
            node.attr("href")
                .and_then(|value| value.strip_prefix('#').map(str::to_owned))
        })
        .collect();
    children(root, true, false, &targets)
}

/// Plain-text triage only. Never substitute this for semantic equality.
pub fn canonical_text(source: &str) -> String {
    Document::from(source)
        .text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn markdown_text(markdown: &str) -> String {
    canonical_text(&render_markdown(markdown))
}

/// Prose-only preservation is an additional quality check, never a substitute
/// for the structural comparator. Code has its own exact-byte contract; math
/// conversion is excluded. Block and cell boundaries cannot concatenate words.
pub fn prose_words(source: &str, omit_leading_title: bool) -> BTreeMap<String, usize> {
    fn visit(node: NodeRef<'_>, text: &mut String, seen: &mut bool, omit_title: bool) {
        if node.is_text() {
            let value = node.text();
            *seen |= !value.trim().is_empty();
            text.push_str(&value);
            return;
        }
        let name = node.node_name().unwrap_or_default();
        if matches!(
            name.as_ref(),
            "code" | "pre" | "math" | "annotation" | "script" | "style" | "noscript" | "template"
        ) || [
            "data-math",
            "data-latex",
            "data-tex",
            "data-mathml",
            "data-katex",
        ]
        .iter()
        .any(|a| node.has_attr(a))
            || node.has_class("katex")
            || node.has_class("katex-mathml")
            || name.starts_with("mjx-")
            || node.has_class("footnote-backref")
        {
            text.push(' ');
            return;
        }
        if name.as_ref() == "h1" && omit_title && !*seen {
            *seen = true;
            return;
        }
        let block = matches!(
            name.as_ref(),
            "p" | "div"
                | "article"
                | "section"
                | "main"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "blockquote"
                | "ul"
                | "ol"
                | "li"
                | "table"
                | "thead"
                | "tbody"
                | "tfoot"
                | "tr"
                | "td"
                | "th"
                | "dl"
                | "dt"
                | "dd"
                | "br"
                | "hr"
                | "figure"
                | "figcaption"
                | "header"
                | "footer"
                | "aside"
        );
        if block {
            text.push(' ');
        }
        for child in node.children_it(false) {
            visit(child, text, seen, omit_title);
        }
        if block {
            text.push(' ');
        }
    }
    let doc = Document::from(source);
    let mut text = String::new();
    visit(
        doc.body().unwrap_or_else(|| doc.root()),
        &mut text,
        &mut false,
        omit_leading_title,
    );
    let mut words = BTreeMap::new();
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
    {
        *words.entry(word.to_lowercase()).or_default() += 1;
    }
    words
}

pub fn markdown_prose_words(markdown: &str) -> BTreeMap<String, usize> {
    prose_words(&render_markdown(markdown), false)
}

pub fn missing_prose(
    expected: &BTreeMap<String, usize>,
    actual: &BTreeMap<String, usize>,
) -> BTreeMap<String, usize> {
    expected
        .iter()
        .filter_map(|(word, count)| {
            let missing = count.saturating_sub(actual.get(word).copied().unwrap_or_default());
            (missing > 0).then(|| (word.clone(), missing))
        })
        .collect()
}

#[cfg(test)]
mod prose_tests {
    use super::*;

    #[test]
    fn headings_multiplicity_and_cell_boundaries_are_significant() {
        let expected = prose_words(
            "<h3>Quadratic Formula</h3><p>word word</p><table><tr><td>first</td><td>second</td></tr></table>",
            false,
        );
        let actual =
            markdown_prose_words("word\n\n<table><tr><td>first</td><td>second</td></tr></table>");
        assert_eq!(
            missing_prose(&expected, &actual),
            BTreeMap::from([
                ("formula".into(), 1),
                ("quadratic".into(), 1),
                ("word".into(), 1)
            ])
        );
    }

    #[test]
    fn formula_representations_do_not_hide_surrounding_prose() {
        let reference = prose_words(
            "<p>Before <math data-latex='x'><mi>x</mi></math> after.</p><h3>Keep this heading</h3>",
            false,
        );
        let actual = markdown_prose_words("Before `x` after.\n\n### Keep this heading");
        assert!(missing_prose(&reference, &actual).is_empty());
        let lost = markdown_prose_words("Before after.");
        assert!(missing_prose(&reference, &lost).contains_key("heading"));
    }
}

fn render_markdown(markdown: &str) -> String {
    let mut labels = HashMap::new();
    let events = Parser::new_ext(markdown, gfm_options()).map(|event| match event {
        Event::FootnoteReference(label) => {
            Event::FootnoteReference(canonical_label(label, &mut labels))
        }
        Event::Start(Tag::FootnoteDefinition(label)) => {
            Event::Start(Tag::FootnoteDefinition(canonical_label(label, &mut labels)))
        }
        event => event,
    });
    let mut output = String::new();
    html::push_html(&mut output, events);
    output
}

fn canonical_label<'a>(label: CowStr<'a>, labels: &mut HashMap<String, usize>) -> CowStr<'a> {
    let next = labels.len() + 1;
    let index = labels.entry(label.into_string()).or_insert(next);
    format!("note-{index}").into()
}

fn tag(node: NodeRef<'_>) -> String {
    node.node_name().unwrap_or_default().to_ascii_lowercase()
}

fn block(name: &str) -> bool {
    matches!(
        name,
        "p" | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "pre"
            | "ol"
            | "ul"
            | "li"
            | "table"
            | "tr"
            | "td"
            | "th"
            | "hr"
            | "dl"
            | "dt"
            | "dd"
            | "figure"
            | "figcaption"
            | "caption"
            | "details"
            | "summary"
            | "section"
            | "div"
            | "article"
            | "main"
            | "header"
            | "footer"
            | "aside"
            | "body"
            | "html"
    )
}

fn children(
    node: NodeRef<'_>,
    trim_boundaries: bool,
    foreign: bool,
    targets: &HashSet<String>,
) -> Vec<SemanticNode> {
    let mut result = Vec::new();
    for child in node.children_it(false) {
        result.extend(canonical_node(child, foreign, targets));
    }
    normalize(result, trim_boundaries)
}

fn canonical_node(
    node: NodeRef<'_>,
    foreign: bool,
    targets: &HashSet<String>,
) -> Vec<SemanticNode> {
    let mut result = Vec::new();
    if let Some(id) = node.attr("id").filter(|id| targets.contains(id.as_ref())) {
        result.push(SemanticNode::Anchor { id: id.to_string() });
    }
    result.extend(canonical_content(node, foreign, targets));
    result
}

fn canonical_content(
    node: NodeRef<'_>,
    foreign: bool,
    targets: &HashSet<String>,
) -> Vec<SemanticNode> {
    if node.is_text() {
        let text = node.text();
        if foreign && preserves_spacing(node) {
            return vec![SemanticNode::PreservedText {
                text: text.to_string(),
            }];
        }
        if foreign
            && text.trim().is_empty()
            && node.parent().is_some_and(|parent| {
                !matches!(
                    tag(parent).as_str(),
                    "text" | "tspan" | "textpath" | "mtext" | "mi" | "mn" | "mo" | "annotation"
                )
            })
        {
            return Vec::new();
        }
        return vec![SemanticNode::Text {
            text: collapse(&text),
        }];
    }
    if !node.is_element() {
        return Vec::new();
    }
    let name = tag(node);
    let foreign = foreign || matches!(name.as_str(), "svg" | "math");
    if name == "pre" && !foreign {
        let code = node
            .descendants_it()
            .find(|node| tag(*node) == "code")
            .unwrap_or(node);
        let language = code
            .class()
            .and_then(|classes| {
                classes
                    .split_whitespace()
                    .find_map(|class| class.strip_prefix("language-").map(str::to_owned))
            })
            .unwrap_or_default();
        let text = pre_payload(node);
        // Markdown renderers append one conventional terminal newline. Keep
        // every source space, indentation and internal/trailing blank line.
        let text = text.strip_suffix('\n').unwrap_or(&text).to_owned();
        return vec![SemanticNode::Code {
            block: true,
            language,
            text,
        }];
    }
    if name == "code" && !foreign {
        return vec![SemanticNode::Code {
            block: false,
            language: String::new(),
            text: node.text().to_string(),
        }];
    }
    let wrapper = !foreign
        && matches!(
            name.as_str(),
            "html"
                | "head"
                | "body"
                | "article"
                | "main"
                | "section"
                | "header"
                | "footer"
                | "aside"
                | "div"
                | "span"
                | "figure"
                | "picture"
                | "thead"
                | "tbody"
                | "tfoot"
                | "center"
        );
    if wrapper {
        let inner = children(node, block(&name), foreign, targets);
        if inner.len() == 1 && is_media(&inner[0]) {
            return inner;
        }
        if block(&name)
            && !matches!(
                name.as_str(),
                "html" | "head" | "body" | "thead" | "tbody" | "tfoot"
            )
            && !inner.is_empty()
            && inner.iter().all(|node| !is_block(node))
        {
            return vec![SemanticNode::Element {
                name: "p".into(),
                attributes: Vec::new(),
                children: inner,
            }];
        }
        return inner;
    }
    let name = if foreign {
        name
    } else {
        match name.as_str() {
            "b" => "strong".into(),
            "i" => "em".into(),
            "s" | "strike" => "del".into(),
            "figcaption" => "p".into(),
            _ => name,
        }
    };
    let mut inner = children(node, block(&name), foreign, targets);
    if name == "li" {
        // Tight and loose list syntax differ only by a single paragraph wrapper.
        // Multiple paragraphs, code blocks and nested lists stay explicit.
        let paragraph_count = inner
            .iter()
            .filter(|child| matches!(child, SemanticNode::Element { name, .. } if name == "p"))
            .count();
        if paragraph_count == 1 {
            inner = inner
                .into_iter()
                .flat_map(|child| match child {
                    SemanticNode::Element {
                        name,
                        attributes,
                        children,
                    } if name == "p" && attributes.is_empty() => children,
                    child => vec![child],
                })
                .collect();
            inner = normalize(inner, true);
        }
    }
    if name == "p" && inner.len() == 1 && is_media(&inner[0]) {
        return inner;
    }
    let attrs = attributes(node, &name, foreign);
    if name == "a" && attrs.iter().all(|(name, _)| name != "href") {
        return inner;
    }
    let mut result = Vec::new();
    // Whitespace at emphasis/link edges may move outside delimiters during
    // conversion without changing visible text or formatting boundaries.
    let inline = !block(&name) && !matches!(name.as_str(), "br" | "img" | "input") && !foreign;
    let (leading, trailing) = if inline {
        take_edge_spaces(&mut inner)
    } else {
        (false, false)
    };
    if leading {
        result.push(SemanticNode::Text { text: " ".into() });
    }
    result.push(SemanticNode::Element {
        name,
        attributes: attrs,
        children: inner,
    });
    if trailing {
        result.push(SemanticNode::Text { text: " ".into() });
    }
    result
}

fn attributes(node: NodeRef<'_>, tag: &str, foreign: bool) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    for attribute in node.attrs() {
        let local = attribute.name.local.as_ref();
        let name = attribute
            .name
            .prefix
            .as_ref()
            .map(|prefix| format!("{prefix}:{local}"))
            .unwrap_or_else(|| local.to_owned());
        let keep = foreign
            || match tag {
                "a" => matches!(local, "href" | "title"),
                "img" => matches!(local, "src" | "srcset" | "sizes" | "alt" | "title"),
                "ol" => matches!(local, "start" | "reversed" | "type"),
                "ul" => local == "type",
                "li" => local == "value",
                "td" | "th" => {
                    matches!(local, "colspan" | "rowspan" | "align" | "scope" | "headers")
                }
                "input" => matches!(local, "type" | "checked"),
                "audio" | "video" => matches!(
                    local,
                    "src" | "poster" | "controls" | "autoplay" | "loop" | "muted"
                ),
                "source" => matches!(local, "src" | "srcset" | "type" | "media"),
                "iframe" => matches!(
                    local,
                    "src" | "srcdoc" | "allow" | "allowfullscreen" | "sandbox"
                ),
                "details" => local == "open",
                _ => false,
            };
        if !keep {
            continue;
        }
        let value = if matches!(
            local,
            "checked"
                | "reversed"
                | "controls"
                | "autoplay"
                | "loop"
                | "muted"
                | "open"
                | "allowfullscreen"
        ) {
            "true".to_owned()
        } else {
            attribute.value.to_string()
        };
        if (local == "title" && value.is_empty())
            || (matches!(
                (tag, local),
                ("ol", "start")
                    | ("td", "colspan")
                    | ("th", "colspan")
                    | ("td", "rowspan")
                    | ("th", "rowspan")
            ) && value == "1")
        {
            continue;
        }
        attrs.push((name, value));
    }
    if matches!(tag, "td" | "th")
        && !attrs.iter().any(|(name, _)| name == "align")
        && let Some(style) = node.attr("style")
        && let Some(alignment) = style
            .split(';')
            .filter_map(|entry| entry.split_once(':'))
            .find_map(|(name, value)| {
                (name.trim().eq_ignore_ascii_case("text-align"))
                    .then(|| value.trim().to_ascii_lowercase())
            })
    {
        attrs.push(("align".into(), alignment));
    }
    attrs.sort();
    attrs
}

fn collapse(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut space = false;
    for ch in value.chars() {
        if ch.is_whitespace() {
            if !space {
                output.push(' ');
            }
            space = true;
        } else {
            output.push(ch);
            space = false;
        }
    }
    output
}

fn take_edge_spaces(nodes: &mut Vec<SemanticNode>) -> (bool, bool) {
    let mut leading = false;
    let mut trailing = false;
    if let Some(SemanticNode::Text { text }) = nodes.first_mut() {
        leading = text.starts_with(' ');
        if leading {
            *text = text.trim_start_matches(' ').to_owned();
        }
    }
    if let Some(SemanticNode::Text { text }) = nodes.last_mut() {
        trailing = text.ends_with(' ');
        if trailing {
            *text = text.trim_end_matches(' ').to_owned();
        }
    }
    nodes.retain(|node| !matches!(node, SemanticNode::Text { text } if text.is_empty()));
    (leading, trailing)
}

fn replaced_media(node: &SemanticNode) -> bool {
    match node {
        SemanticNode::Element { name, children, .. } if name == "a" => {
            children.len() == 1 && replaced_media(&children[0])
        }
        SemanticNode::Element { name, .. } => {
            matches!(name.as_str(), "img" | "video" | "audio" | "iframe")
        }
        _ => false,
    }
}

fn is_media(node: &SemanticNode) -> bool {
    match node {
        SemanticNode::Element { name, children, .. } if name == "a" => {
            children.len() == 1 && is_media(&children[0])
        }
        SemanticNode::Element { name, .. } => matches!(
            name.as_str(),
            "svg" | "math" | "img" | "video" | "audio" | "iframe"
        ),
        _ => false,
    }
}

fn is_block(node: &SemanticNode) -> bool {
    matches!(node, SemanticNode::Code { block: true, .. })
        || matches!(node, SemanticNode::Element { name, .. } if block(name))
}

fn normalize(nodes: Vec<SemanticNode>, trim_boundaries: bool) -> Vec<SemanticNode> {
    let mut merged = Vec::new();
    for node in nodes {
        match node {
            SemanticNode::PreservedText { text } => {
                if let Some(SemanticNode::PreservedText { text: previous }) = merged.last_mut() {
                    previous.push_str(&text);
                } else {
                    merged.push(SemanticNode::PreservedText { text });
                }
            }
            SemanticNode::Text { text } => {
                if text.is_empty() {
                    continue;
                }
                if let Some(SemanticNode::Text { text: previous }) = merged.last_mut() {
                    previous.push_str(&text);
                    *previous = collapse(previous);
                } else {
                    merged.push(SemanticNode::Text { text });
                }
            }
            node => merged.push(node),
        }
    }
    let count = merged.len();
    let mut result = Vec::new();
    for (index, node) in merged.iter().enumerate() {
        if let SemanticNode::Text { text } = node {
            let left_boundary = (index == 0 && trim_boundaries)
                || index.checked_sub(1).is_some_and(|previous| {
                    is_block(&merged[previous]) || replaced_media(&merged[previous])
                });
            let right_boundary = (index + 1 == count && trim_boundaries)
                || merged
                    .get(index + 1)
                    .is_some_and(|next| is_block(next) || replaced_media(next));
            let text = if left_boundary {
                text.trim_start_matches(' ')
            } else {
                text.as_str()
            };
            let text = if right_boundary {
                text.trim_end_matches(' ')
            } else {
                text
            };
            if !text.is_empty() {
                result.push(SemanticNode::Text {
                    text: text.to_owned(),
                });
            }
        } else {
            result.push(node.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimiter_and_wrapper_choices_are_equivalent() {
        assert_eq!(
            markdown_semantics("Heading\n=======\n\n__bold__ and _em_\n\n~~~rust\nlet x = 1;\n~~~"),
            markdown_semantics("# Heading\n\n**bold** and *em*\n\n```rust\nlet x = 1;\n```")
        );
        assert_eq!(
            html_semantics("<div><p>A <span>word</span>.</p></div>"),
            html_semantics("<article><p>A word.</p></article>")
        );
        assert_eq!(
            markdown_semantics("- A\n- B"),
            markdown_semantics("- A\n\n- B")
        );
    }

    #[test]
    fn meaningful_loss_is_never_hidden_by_text_equality() {
        for (left, right) in [
            ("# Heading", "Heading"),
            ("[same](https://a.example)", "[same](https://b.example)"),
            (
                "![same](https://a.example/image)",
                "![same](https://b.example/image)",
            ),
            ("- A\n  - B", "- A\n- B"),
            ("```\n    x\n```", "```\nx\n```"),
            (
                "<table><tr><td colspan=\"2\">X</td></tr></table>",
                "<table><tr><td>X</td></tr></table>",
            ),
            ("A  \nB", "A B"),
            ("**same**", "same"),
        ] {
            assert_ne!(
                markdown_semantics(left),
                markdown_semantics(right),
                "{left:?} vs {right:?}"
            );
        }
    }

    #[test]
    fn footnote_labels_are_internal_but_the_reference_graph_is_preserved() {
        assert_eq!(
            markdown_semantics("A[^x] B[^x]\n\n[^x]: Note"),
            markdown_semantics("A[^renamed] B[^renamed]\n\n[^renamed]: Note")
        );
        assert_ne!(
            markdown_semantics("A[^x] B[^x]\n\n[^x]: Note"),
            markdown_semantics("A[^x] B[^y]\n\n[^x]: Note\n\n[^y]: Note")
        );
    }

    #[test]
    fn harmless_html_serializer_differences_do_not_change_svg_semantics() {
        assert_eq!(
            markdown_semantics(
                "<svg viewBox=\"0 0 10 10\"><path fill=\"none\" d=\"M0 0\"></path></svg>"
            ),
            markdown_semantics(
                "<svg viewBox='0 0 10 10'>\n<path d='M0 0' fill='none'></path>\n</svg>"
            )
        );
    }
}

#[cfg(test)]
mod inline_space_tests {
    use super::*;
    #[test]
    fn trims_logical_block_edges_but_preserves_text_word_boundaries() {
        assert_eq!(
            markdown_semantics("<p><em> A </em> word</p>"),
            markdown_semantics("*A* word")
        );
        assert_ne!(
            markdown_semantics("*A*word"),
            markdown_semantics("*A* word")
        );
        assert_eq!(
            markdown_semantics("![A](/a.png) text"),
            markdown_semantics("![A](/a.png)text")
        );
        assert_eq!(
            markdown_semantics("![A](/a.png) ![B](/b.png)"),
            markdown_semantics("![A](/a.png)![B](/b.png)")
        );
    }
}

// Independently read every text-bearing pre child, including inter-code text and
// explicit display breaks. Language metadata still comes from the code wrapper.
fn pre_payload(root: NodeRef<'_>) -> String {
    let mut output = String::new();
    let mut pending = vec![(root, false)];
    while let Some((node, exiting)) = pending.pop() {
        if node.is_text() {
            output.push_str(&node.text());
            continue;
        }
        let name = tag(node);
        if name == "br" {
            output.push('\n');
            continue;
        }
        let block = matches!(name.as_str(), "div" | "p" | "li" | "tr");
        if block && !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        if exiting {
            continue;
        }
        if block {
            pending.push((node, true));
        }
        pending.extend(node.children_it(true).map(|child| (child, false)));
    }
    output
}

fn preserves_spacing(node: NodeRef<'_>) -> bool {
    for ancestor in node.ancestors_it(None) {
        for attribute in ancestor.attrs() {
            if attribute.name.local.as_ref() == "space"
                && attribute
                    .name
                    .prefix
                    .as_ref()
                    .is_some_and(|prefix| prefix.as_ref() == "xml")
            {
                return attribute.value.eq_ignore_ascii_case("preserve");
            }
        }
        if let Some(style) = ancestor.attr("style")
            && let Some(value) = style
                .split(';')
                .filter_map(|entry| entry.split_once(':'))
                .find_map(|(name, value)| {
                    name.trim()
                        .eq_ignore_ascii_case("white-space")
                        .then_some(value.trim())
                })
        {
            return matches!(value, "pre" | "pre-wrap" | "break-spaces");
        }
    }
    false
}
