//! Ordered structural cleanup of extracted HTML. Sanitization is a separate,
//! unconditional pass; this module never converts or renders mathematics.

use std::sync::LazyLock;

use dom_query::{Document, NodeData, NodeRef};
use regex::Regex;

use crate::constants::{
    ALLOWED_ATTRIBUTES, ALLOWED_EMPTY_ELEMENTS, BLOCK_LEVEL_ELEMENTS, INLINE_ELEMENTS,
    PRESERVE_ELEMENTS, TAILWIND_COLORS, TAILWIND_SPECIAL,
};
use crate::dom::{self, closest, elements, retag, same_node, select};
use crate::utils::tag;
use crate::{Metadata, Options, math, utils};

static SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\t\n\r ]+").unwrap());
static BEFORE_PUNCT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+([,.!?:;])").unwrap());
static BLOCK_STYLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)display\s*:\s*block").unwrap());
static PRE_STYLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)white-space\s*:\s*pre").unwrap());
static EXTRACTOR_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:comments?|posts?|quoted-post|x-article)(?:-[\w-]+)?$").unwrap()
});
static TW_COLOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(fill|stroke)-([a-z]+)-(\d{2,3})(?:/(\d+))?$").unwrap());

fn block(node: NodeRef<'_>) -> bool {
    BLOCK_LEVEL_ELEMENTS.contains(&tag(node).as_str())
}
fn inline(node: NodeRef<'_>) -> bool {
    INLINE_ELEMENTS.contains(&tag(node).as_str())
}
fn svg(node: NodeRef<'_>) -> bool {
    node.qual_name_ref()
        .is_some_and(|name| name.ns.as_ref() == "http://www.w3.org/2000/svg")
}
/// Equivalent to `closest("pre,code,svg")`: type selectors compare local names
/// in every namespace.
fn code_root(node: NodeRef<'_>) -> bool {
    node.qual_name_ref()
        .is_some_and(|name| matches!(name.local.as_ref(), "pre" | "code" | "svg"))
}

fn protected(node: NodeRef<'_>) -> bool {
    let mut nearest_source_seen = false;
    std::iter::once(node)
        .chain(node.ancestors_it(None))
        .filter(NodeRef::is_element)
        .any(|ancestor| {
            if code_root(ancestor) {
                return true;
            }
            if !nearest_source_seen && math::is_source_candidate(ancestor) {
                nearest_source_seen = true;
                return math::is_math_source(ancestor);
            }
            false
        })
}
fn attached(root: NodeRef<'_>, node: NodeRef<'_>) -> bool {
    utils::contains(root, node)
}

/// Normalize footnotes and callouts while source classes, hidden definitions,
/// and out-of-article definitions still exist.
pub fn before_removals(doc: &Document, root: NodeRef<'_>, _metadata: &Metadata) {
    crate::footnotes::adopt_external(doc, root);
    crate::footnotes::standardize(root);
    crate::callouts::standardize(root);
}

/// Full ordered cleanup of generic extracted content.
pub fn content(root: NodeRef<'_>, metadata: &Metadata, options: &Options) {
    drop_caps(root);
    for text in root.descendants().into_iter().filter(NodeRef::is_text) {
        let value = text.text();
        if value.contains('\u{a0}') && !protected(text) {
            text.set_text(value.replace('\u{a0}', " "));
        }
    }
    headings(root, &metadata.title);
    for code in select(root, "code") {
        if closest(code, "pre").is_none() && PRE_STYLE.is_match(&code.attr_or("style", "")) {
            let pre = root.tree.new_element("pre");
            code.insert_before(&pre);
            pre.append_child(&code);
        }
    }
    crate::code::standardize(root);
    crate::images::standardize(root);
    roles(root);
    embedded(root);
    resolve_svg_colors(root);
    if !options.debug {
        custom_elements(root);
        spans(root);
        layout_tables(root);
        flatten(root);
        remove_permalinks(root);
        strip_attributes(root, false, false);
        for span in select(root, "span").into_iter().rev() {
            if span.attrs().is_empty() && !protected(span) {
                dom::unwrap(span);
            }
        }
        special_links(root);
        remove_empty(root);
        remove_trailing_headings(root);
        remove_orphaned_dividers(root);
        flatten(root);
        remove_orphaned_dividers(root);
        clean_breaks(root);
        whitespace(root);
    } else {
        strip_attributes(root, true, false);
        remove_trailing_headings(root);
        clean_breaks(root);
    }
}

/// Cleanup for already-constructed extractor markup. Authored post/comment
/// classes remain available to readers; page styling and empty spacers do not.
pub fn extractor(root: NodeRef<'_>, options: &Options) {
    strip_attributes(root, options.debug, true);
    if !options.debug {
        remove_empty(root);
    }
    clean_breaks(root);
}

fn drop_caps(root: NodeRef<'_>) {
    for cap in select(root, "span[data-caps=initial]") {
        if let Some(next) = cap
            .next_element_sibling()
            .filter(|node| node.has_name("small"))
        {
            let merged = root.tree.new_text(format!("{}{}", cap.text(), next.text()));
            cap.insert_before(&merged);
            next.remove_from_parent();
            cap.remove_from_parent();
        } else {
            dom::unwrap(cap);
        }
    }
}

fn permalink(node: NodeRef<'_>) -> bool {
    if !node.has_name("a") {
        return false;
    }
    let href = node.attr_or("href", "");
    let class = node.class().unwrap_or_default().to_ascii_lowercase();
    href.starts_with('#')
        || node
            .attr_or("title", "")
            .to_ascii_lowercase()
            .contains("permalink")
        || ["permalink", "heading-anchor", "anchor-link"]
            .iter()
            .any(|part| class.contains(part))
        || matches!(node.text().trim(), "#" | "¶" | "§" | "🔗" | "\u{feff}")
}

fn remove_permalinks(root: NodeRef<'_>) {
    for anchor in select(
        root,
        "h1 a,h2 a,h3 a,h4 a,h5 a,h6 a,a.permalink,a.anchor-link,a.heading-anchor",
    ) {
        if permalink(anchor) {
            anchor.remove_from_parent();
        }
    }
}

fn headings(root: NodeRef<'_>, title: &str) {
    let headings = select(root, "h1,h2,h3,h4,h5,h6");
    for heading in &headings {
        if heading.has_name("h1") {
            retag(*heading, "h2");
        }
        let fallback = heading.text().trim().to_owned();
        for child in select(*heading, "*") {
            if child.has_name("button")
                || permalink(child)
                || child.has_class("anchor")
                || child.has_class("permalink-widget")
            {
                child.remove_from_parent();
            }
        }
        let text = heading.text().trim().to_owned();
        heading.set_text(if text.is_empty() { fallback } else { text });
    }
    if let Some(first) = headings.into_iter().find(|node| node.has_name("h2"))
        && !title.trim().is_empty()
        && utils::normalize_text(&first.text()) == utils::normalize_text(title)
    {
        first.remove_from_parent();
    }
}

fn roles(root: NodeRef<'_>) {
    for node in select(root, "div[data-testid^=paragraph],div[role=paragraph]") {
        retag(node, "p");
    }
    // Deepest first prevents a nested list's items from being adopted by its
    // ancestor. Preserve ordinary lists nested within an item's content.
    for list in select(root, "div[role=list]").into_iter().rev() {
        let items: Vec<_> = select(list, "div[role=listitem]")
            .into_iter()
            .filter(|item| {
                item.ancestors_it(None)
                    .find(|ancestor| ancestor.attr_or("role", "").as_ref() == "list")
                    .is_some_and(|ancestor| same_node(ancestor, list))
            })
            .collect();
        let ordered = items.first().is_some_and(|item| {
            select(*item, ".label").first().is_some_and(|label| {
                label
                    .text()
                    .trim()
                    .split_once(')')
                    .is_some_and(|(number, _)| {
                        !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
                    })
            })
        });
        retag(list, if ordered { "ol" } else { "ul" });
        list.remove_attr("role");
        for item in items {
            retag(item, "li");
            item.remove_attr("role");
            if let Some(content) = select(item, ".content").into_iter().next() {
                // Detach before clearing siblings, so the source remains intact.
                content.remove_from_parent();
                dom::transfer_children(content, item);
            } else {
                for label in select(item, ".label") {
                    label.remove_from_parent();
                }
            }
        }
    }
    for item in select(root, "div[role=listitem]") {
        retag(item, "li");
    }
}

fn embedded(root: NodeRef<'_>) {
    for outer in select(root, "span.ltx_note_outer") {
        outer.remove_from_parent();
    }
    for link in select(root, "a.ltx_ref") {
        if !select(link, ".ltx_ref_tag").is_empty() {
            dom::unwrap(link);
        }
    }
    for video in select(root, "video") {
        if !video.has_attr("controls") {
            video.set_attr("controls", "");
        }
    }
    for youtube in select(root, "lite-youtube") {
        let id = youtube.attr_or("videoid", "");
        if id.is_empty() {
            continue;
        }
        let iframe = root.tree.new_element("iframe");
        iframe.set_attr("width", "560");
        iframe.set_attr("height", "315");
        // The ID stays within a fixed HTTPS origin and is assigned as a value.
        iframe.set_attr("src", &format!("https://www.youtube.com/embed/{id}"));
        iframe.set_attr(
            "title",
            &youtube.attr_or("videotitle", "YouTube video player"),
        );
        iframe.set_attr("frameborder", "0");
        iframe.set_attr("allow", "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share");
        iframe.set_attr("allowfullscreen", "");
        youtube.replace_with(&iframe);
    }
    for table in select(root, "table") {
        let cells = select(table, "td,th");
        if !cells.is_empty()
            && cells.iter().all(|cell| cell.text().trim().is_empty())
            && select(table, "img,picture,video,audio,iframe,svg,math").is_empty()
        {
            hoist_table_captions(table);
            table.remove_from_parent();
        }
    }
}

fn custom_elements(root: NodeRef<'_>) {
    for node in select(root, "*").into_iter().rev() {
        let name = tag(node);
        if name.contains('-')
            && !name.starts_with("mjx-")
            && !INLINE_ELEMENTS.contains(&name.as_str())
            && dom::is_html_element(node, &name)
        {
            retag(node, "div");
        }
    }
}

fn spans(root: NodeRef<'_>) {
    for span in select(root, "span").into_iter().rev() {
        if protected(span) {
            continue;
        }
        let target = span.attr_or("data-as", "").to_ascii_lowercase();
        let target = match target.as_str() {
            "p" => Some("p"),
            "h1" => Some("h1"),
            "h2" => Some("h2"),
            "h3" => Some("h3"),
            "h4" => Some("h4"),
            "h5" => Some("h5"),
            "h6" => Some("h6"),
            "li" => Some("li"),
            "blockquote" => Some("blockquote"),
            _ => None,
        };
        if let Some(target) = target {
            retag(span, target);
        } else if !span.text().trim().is_empty()
            && (span.has_class("block") || BLOCK_STYLE.is_match(&span.attr_or("style", "")))
        {
            retag(span, "p");
        }
    }
}

fn direct_table(node: NodeRef<'_>, table: NodeRef<'_>) -> bool {
    node.ancestors_it(None)
        .find(|ancestor| ancestor.has_name("table"))
        .is_some_and(|ancestor| same_node(ancestor, table))
}

fn hoist_table_captions(table: NodeRef<'_>) {
    for caption in table
        .element_children()
        .into_iter()
        .filter(|node| node.has_name("caption"))
    {
        if !caption.text().trim().is_empty() {
            retag(caption, "p");
            table.insert_before(&caption);
        }
    }
}

fn layout_tables(root: NodeRef<'_>) {
    for table in select(root, "table").into_iter().rev() {
        if !attached(root, table) || protected(table) {
            continue;
        }
        let cells: Vec<_> = select(table, "td,th")
            .into_iter()
            .filter(|cell| direct_table(*cell, table))
            .collect();
        if cells.is_empty()
            || cells.iter().any(|cell| cell.has_name("th"))
            || select(table, "thead,tfoot")
                .into_iter()
                .any(|node| direct_table(node, table))
        {
            continue;
        }
        let rows: Vec<_> = select(table, "tr")
            .into_iter()
            .filter(|row| direct_table(*row, table))
            .collect();
        let single_column = !rows.is_empty()
            && rows.iter().all(|row| {
                cells
                    .iter()
                    .filter(|cell| cell.parent().is_some_and(|parent| same_node(parent, *row)))
                    .count()
                    <= 1
            });
        let nonempty: Vec<_> = cells
            .iter()
            .copied()
            .filter(|cell| {
                !cell.text().trim().is_empty() || !select(*cell, "img,video,svg,math").is_empty()
            })
            .collect();
        let single_content_cell = nonempty.len() == 1
            && nonempty[0]
                .element_children()
                .iter()
                .filter(|child| !child.text().trim().is_empty())
                .count()
                == 1
            && nonempty[0]
                .element_children()
                .iter()
                .any(|child| block(*child));
        if single_column || single_content_cell {
            hoist_table_captions(table);
            for cell in cells {
                for child in cell.children() {
                    table.insert_before(&child);
                }
            }
            table.remove_from_parent();
        }
    }
}

fn preserve_wrapper(node: NodeRef<'_>) -> bool {
    if PRESERVE_ELEMENTS.contains(&tag(node).as_str())
        || protected(node)
        || closest(node, "[data-callout],#footnotes").is_some()
    {
        return true;
    }
    let role = node.attr_or("role", "");
    if ["article", "main", "navigation", "banner", "contentinfo"].contains(&role.as_ref()) {
        return true;
    }
    let class = node.class().unwrap_or_default().to_ascii_lowercase();
    [
        "article",
        "main",
        "content",
        "footnote",
        "reference",
        "bibliography",
    ]
    .iter()
    .any(|part| class.contains(part))
}

fn flatten(root: NodeRef<'_>) {
    // Each pass either removes a wrapper or retags it as a paragraph; bounded
    // by the finite node count even for very deep, adversarial wrapper chains.
    for node in select(
        root,
        "div,section,article,main,aside,header,footer,nav,content",
    )
    .into_iter()
    .rev()
    {
        if !attached(root, node) || preserve_wrapper(node) {
            continue;
        }
        let children = node.children();
        let has_inline = children.iter().any(|child| {
            (child.is_text() && !child.text().trim().is_empty())
                || (child.is_element() && inline(*child))
        });
        let only_inline = !children.is_empty()
            && children.iter().all(|child| {
                child.is_text() || (inline(*child) && !child.descendants_it().any(block))
            });
        if only_inline && !node.text().trim().is_empty() {
            retag(node, "p");
        } else if !has_inline {
            dom::unwrap(node);
        }
    }
}

fn extractor_class(value: &str) -> bool {
    [
        "bluesky",
        "discourse",
        "github",
        "gmail",
        "hackernews",
        "linkedin",
        "lwn",
        "mastodon",
        "reddit",
        "threads",
        "twitter",
    ]
    .contains(&value)
        || EXTRACTOR_CLASS.is_match(value)
}

fn strip_attributes(root: NodeRef<'_>, debug: bool, extractor: bool) {
    for node in elements(root) {
        if svg(node) {
            if !debug {
                node.remove_attr("class");
            }
            continue;
        }
        let tag = tag(node);
        if matches!(tag.as_str(), "td" | "th")
            && let Some(style) = node.attr("style")
        {
            for declaration in style.split(';') {
                let Some((property, value)) = declaration.split_once(':') else {
                    continue;
                };
                if !property.trim().eq_ignore_ascii_case("text-align") {
                    continue;
                }
                let value = value
                    .split('!')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                if matches!(value.as_str(), "left" | "center" | "right") {
                    node.set_attr("align", &value);
                }
            }
        }
        node.update(|tree_node| {
            let NodeData::Element(element) = &mut tree_node.data else {
                return;
            };
            element.attrs.retain_mut(|attribute| {
                let name = attribute.name.local.as_ref().to_ascii_lowercase();
                let value = attribute.value.as_ref();
                if name == "id"
                    && (value == "footnotes"
                        || value.starts_with("fnref:")
                        || value.starts_with("fn:"))
                {
                    return true;
                }
                if name == "class" {
                    if (tag == "code"
                        && value
                            .split_ascii_whitespace()
                            .any(|class| class.starts_with("language-")))
                        || value == "footnote-backref"
                        || value
                            .split_ascii_whitespace()
                            .any(|class| class == "callout" || class.starts_with("callout-"))
                    {
                        return true;
                    }
                    if extractor && !debug {
                        let kept = value
                            .split_ascii_whitespace()
                            .filter(|class| extractor_class(class))
                            .collect::<Vec<_>>()
                            .join(" ");
                        if !kept.is_empty() {
                            attribute.value = kept.as_str().into();
                            return true;
                        }
                    }
                }
                if extractor && name == "data-defuddle" {
                    return true;
                }
                if (tag == "ol" && name == "reversed") || (tag == "li" && name == "value") {
                    return true;
                }
                // Math is deliberately unconverted. Keep safe source attributes
                // rather than stripping the only available TeX/MathML source.
                if math::SOURCE_ATTRIBUTES.contains(&name.as_str()) || name == "alttext" {
                    return true;
                }
                if name == "encoding" && matches!(tag.as_str(), "annotation" | "annotation-xml") {
                    return true;
                }
                ALLOWED_ATTRIBUTES.contains(&name.as_str())
                    || (debug && (name == "class" || name == "id" || name.starts_with("data-")))
            });
        });
    }
}

fn special_links(root: NodeRef<'_>) {
    for link in select(root, "a").into_iter().rev() {
        if !attached(root, link) {
            continue;
        }
        let href = link.attr_or("href", "");
        if closest(link, "code").is_some() || crate::safety::is_dangerous_url(&href, true) {
            dom::unwrap(link);
            continue;
        }
        let heading = link
            .element_children()
            .into_iter()
            .find(|node| matches!(tag(*node).as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6"));
        if let Some(heading) = heading {
            if !href.is_empty() && !href.starts_with('#') {
                let inner = root.tree.new_element("a");
                inner.set_attr("href", &href);
                dom::transfer_children(heading, inner);
                heading.append_child(&inner);
            }
            dom::unwrap(link);
        }
    }
}

fn remove_empty(root: NodeRef<'_>) {
    for node in select(root, "*").into_iter().rev() {
        if !attached(root, node) || ALLOWED_EMPTY_ELEMENTS.contains(&tag(node).as_str()) {
            continue;
        }
        if node
            .children()
            .iter()
            .all(|child| child.is_text() || child.has_name("br"))
            && node.text().trim().is_empty()
            && !protected(node)
        {
            node.remove_from_parent();
        }
    }
}

fn meaningful(node: NodeRef<'_>) -> bool {
    !node.text().trim().is_empty()
        || (node.is_element()
            && (matches!(
                tag(node).as_str(),
                "img" | "video" | "audio" | "iframe" | "svg" | "math"
            ) || math::is_math_source(node)
                || dom::exists(node, "img,video,audio,iframe,svg")
                || math::has_source_descendant(node)))
}

fn remove_trailing_headings(root: NodeRef<'_>) {
    for heading in select(root, "h1,h2,h3,h4,h5,h6").into_iter().rev() {
        let mut cursor = heading;
        let mut after = false;
        loop {
            let mut sibling = cursor.next_sibling();
            while let Some(node) = sibling {
                if meaningful(node) {
                    after = true;
                    break;
                }
                sibling = node.next_sibling();
            }
            if after {
                break;
            }
            let Some(parent) = cursor.parent() else { break };
            if same_node(parent, root) {
                break;
            }
            cursor = parent;
        }
        if after {
            break;
        }
        heading.remove_from_parent();
    }
}

fn sibling_content(node: NodeRef<'_>, next: bool) -> Option<NodeRef<'_>> {
    let mut sibling = if next {
        node.next_sibling()
    } else {
        node.prev_sibling()
    };
    while let Some(current) = sibling {
        if !current.is_text() || !current.text().trim().is_empty() {
            return Some(current);
        }
        sibling = if next {
            current.next_sibling()
        } else {
            current.prev_sibling()
        };
    }
    None
}

/// Remove leading/trailing dividers and duplicate adjacent dividers, while
/// preserving intentional separators between meaningful article sections.
pub fn remove_orphaned_dividers(root: NodeRef<'_>) {
    while let Some(node) = root
        .children()
        .into_iter()
        .find(|node| !node.is_text() || !node.text().trim().is_empty())
    {
        if node.has_name("hr") {
            node.remove_from_parent();
        } else {
            break;
        }
    }
    while let Some(node) = root
        .children()
        .into_iter()
        .rev()
        .find(|node| !node.is_text() || !node.text().trim().is_empty())
    {
        if node.has_name("hr") {
            node.remove_from_parent();
        } else {
            break;
        }
    }
    for hr in select(root, "hr") {
        while let Some(next) = sibling_content(hr, true).filter(|next| next.has_name("hr")) {
            next.remove_from_parent();
        }
    }
}

fn clean_breaks(root: NodeRef<'_>) {
    for br in select(root, "br") {
        if !attached(root, br) || protected(br) {
            continue;
        }
        let parent = br.parent().unwrap();
        let mut group = vec![br];
        let mut next = sibling_content(br, true);
        while let Some(candidate) = next.filter(|n| n.has_name("br")) {
            group.push(candidate);
            next = sibling_content(candidate, true);
        }
        let previous = sibling_content(br, false);
        if block(parent) || parent.has_name("body") {
            if previous.is_none()
                || (previous.is_some_and(block) && (next.is_none() || next.is_some_and(block)))
            {
                for node in group {
                    node.remove_from_parent();
                }
                continue;
            }
            if next.is_none() {
                for node in group {
                    node.remove_from_parent();
                }
                continue;
            }
        }
        for extra in group.into_iter().skip(2) {
            extra.remove_from_parent();
        }
    }
}

fn footnote_ref(node: NodeRef<'_>) -> bool {
    node.has_name("sup") && node.id_attr().is_some_and(|id| id.starts_with("fnref:"))
}
fn tight(node: NodeRef<'_>) -> bool {
    matches!(tag(node).as_str(), "sub" | "sup" | "code") && !footnote_ref(node)
}

// Read only as far as the first/last nonempty text descendant. This is the
// boundary of NodeRef::text(), without allocating the entire subtree text.
fn boundary_text_char(root: NodeRef<'_>, backwards: bool) -> Option<char> {
    let mut current = root;
    loop {
        let boundary = current.query_or(None, |node| match &node.data {
            NodeData::Text { contents } => {
                if backwards {
                    contents.chars().next_back()
                } else {
                    contents.chars().next()
                }
            }
            _ => None,
        });
        if boundary.is_some() {
            return boundary;
        }
        if let Some(child) = if backwards {
            current.last_child()
        } else {
            current.first_child()
        } {
            current = child;
            continue;
        }
        loop {
            if same_node(current, root) {
                return None;
            }
            if let Some(sibling) = if backwards {
                current.prev_sibling()
            } else {
                current.next_sibling()
            } {
                current = sibling;
                break;
            }
            current = current.parent()?;
        }
    }
}

fn whitespace(root: NodeRef<'_>) {
    for node in root.descendants().into_iter().filter(NodeRef::is_text) {
        if protected(node) {
            continue;
        }
        let value = node.text();
        let value = value.replace(
            ['\u{200b}', '\u{200d}', '\u{200e}', '\u{200f}', '\u{feff}'],
            "",
        );
        let value = SPACE.replace_all(&value, " ");
        let value = BEFORE_PUNCT.replace_all(&value, "$1");
        node.set_text(value.as_ref());
        if value.is_empty() {
            node.remove_from_parent();
        }
    }
    root.normalize();
    for node in elements(root).into_iter().rev() {
        if protected(node) {
            continue;
        }
        if block(node) || same_node(node, root) {
            while let Some(first) = node
                .first_child()
                .filter(|child| child.is_text() && child.text().trim().is_empty())
            {
                first.remove_from_parent();
            }
            while let Some(last) = node
                .last_child()
                .filter(|child| child.is_text() && child.text().trim().is_empty())
            {
                last.remove_from_parent();
            }
        } else if inline(node) {
            if let Some(first) = node.first_child().filter(NodeRef::is_text) {
                let text = first.text();
                if text.starts_with(char::is_whitespace) {
                    first.set_text(text.trim_start());
                    if node.prev_sibling().is_none_or(|previous| {
                        !previous.is_text() || !previous.text().ends_with(' ')
                    }) {
                        node.insert_before(&root.tree.new_text(" "));
                    }
                }
            }
            if let Some(last) = node.last_child().filter(NodeRef::is_text) {
                let text = last.text();
                if text.ends_with(char::is_whitespace) {
                    last.set_text(text.trim_end());
                    if node
                        .next_sibling()
                        .is_none_or(|next| !next.is_text() || !next.text().starts_with(' '))
                    {
                        node.insert_after(&root.tree.new_text(" "));
                    }
                }
            }
        }
        let children = node.children();
        for pair in children.windows(2) {
            let (current, next) = (pair[0], pair[1]);
            if !(current.is_element() || next.is_element())
                || block(current)
                || block(next)
                || tight(current)
                || tight(next)
                || footnote_ref(next)
            {
                continue;
            }
            let Some(left) = boundary_text_char(current, true) else {
                continue;
            };
            let Some(right) = boundary_text_char(next, false) else {
                continue;
            };
            if left.is_whitespace() || right.is_whitespace() {
                continue;
            }
            let next_punctuation = ",.!?:;)]”’—–".contains(right);
            let previous_punctuation = ",.!?:;([“‘—–".contains(left);
            if !next_punctuation && !previous_punctuation {
                next.insert_before(&root.tree.new_text(" "));
            }
        }
    }
    // Moving inline boundary whitespace can create adjacent text nodes just
    // before punctuation. Normalize once more so that punctuation stays tight
    // after links and emphasis, without changing preformatted or math text.
    root.normalize();
    for node in root.descendants().into_iter().filter(NodeRef::is_text) {
        let text = node.text();
        let cleaned = BEFORE_PUNCT.replace_all(&text, "$1");
        if cleaned != text.as_ref() && !protected(node) {
            node.set_text(cleaned.as_ref());
        }
    }
}

fn palette(color: &str, shade: &str) -> Option<&'static str> {
    TAILWIND_COLORS
        .iter()
        .find(|(name, level, _)| *name == color && *level == shade)
        .map(|(_, _, value)| *value)
}

fn semantic_color(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    let parts: Vec<_> = name.rsplit('-').take(2).collect();
    if parts.len() == 2
        && let Some(hex) = palette(parts[1], parts[0])
    {
        return hex.to_owned();
    }
    if name.ends_with("-black") {
        "#000".to_owned()
    } else if name.ends_with("-white") {
        "#fff".to_owned()
    } else if ["background", "card", "surface", "bg"]
        .iter()
        .any(|part| name.contains(part))
    {
        "Canvas".to_owned()
    } else if ["border", "divider", "separator"]
        .iter()
        .any(|part| name.contains(part))
    {
        "#ccc".to_owned()
    } else if ["muted", "subtle", "secondary", "placeholder"]
        .iter()
        .any(|part| name.contains(part))
    {
        "#888".to_owned()
    } else {
        "currentColor".to_owned()
    }
}

fn css_arguments(value: &str, open: usize) -> Option<(usize, Option<usize>)> {
    let mut depth = 0usize;
    let mut separator = None;
    for (offset, ch) in value[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some((open + offset, separator));
                }
            }
            ',' if depth == 1 && separator.is_none() => separator = Some(open + offset),
            _ => (),
        }
    }
    None
}

fn resolve_color(value: &str) -> String {
    fn resolve(value: &str, depth: usize) -> String {
        if depth > 16 {
            return "currentColor".to_owned();
        }
        let mut output = String::new();
        let mut rest = value;
        loop {
            let light = rest
                .find("light-dark(")
                .map(|position| (position, "light-dark("));
            let variable = rest.find("var(").map(|position| (position, "var("));
            let Some((start, function)) = [light, variable]
                .into_iter()
                .flatten()
                .min_by_key(|(start, _)| *start)
            else {
                output.push_str(rest);
                break;
            };
            output.push_str(&rest[..start]);
            let open = start + function.len() - 1;
            let Some((close, separator)) = css_arguments(rest, open) else {
                output.push_str(&rest[start..]);
                break;
            };
            let first_end = separator.unwrap_or(close);
            let first = rest[open + 1..first_end].trim();
            let resolved = if function == "light-dark(" {
                resolve(first, depth + 1)
            } else if let Some(separator) = separator {
                resolve(rest[separator + 1..close].trim(), depth + 1)
            } else {
                semantic_color(first.trim_start_matches("--"))
            };
            output.push_str(&resolved);
            rest = &rest[close + 1..];
        }
        output
    }
    resolve(value, 0)
}

fn tailwind_colors(node: NodeRef<'_>) {
    let classes = node.class().unwrap_or_default();
    let mut keep = Vec::new();
    let mut styles = Vec::new();
    for token in classes.split_ascii_whitespace() {
        if let Some(captures) = TW_COLOR.captures(token)
            && let Some(hex) = palette(&captures[2], &captures[3])
        {
            let value = if let Some(opacity) = captures
                .get(4)
                .and_then(|value| value.as_str().parse::<f64>().ok())
            {
                let red = u8::from_str_radix(&hex[1..3], 16).unwrap_or(0);
                let green = u8::from_str_radix(&hex[3..5], 16).unwrap_or(0);
                let blue = u8::from_str_radix(&hex[5..7], 16).unwrap_or(0);
                format!("rgba({red},{green},{blue},{})", opacity / 100.0)
            } else {
                hex.to_owned()
            };
            node.set_attr(&captures[1], &value);
            continue;
        }
        if let Some((property, special)) = token
            .split_once('-')
            .filter(|(property, _)| ["fill", "stroke"].contains(property))
            && let Some((_, color)) = TAILWIND_SPECIAL.iter().find(|(name, _)| *name == special)
        {
            node.set_attr(property, color);
            continue;
        }
        if let Some(size) = token
            .strip_prefix("text-[")
            .and_then(|value| value.strip_suffix(']'))
        {
            // Font-size utility values are numbers/units, never arbitrary CSS.
            if !size.is_empty()
                && size
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ".%+-".contains(ch))
            {
                styles.push(format!("font-size:{size}"));
                continue;
            }
        }
        let font = match token {
            "font-semibold" => "font-weight:600",
            "font-bold" => "font-weight:700",
            "font-medium" => "font-weight:500",
            "font-mono" => "font-family:monospace",
            _ => "",
        };
        if !font.is_empty() {
            styles.push(font.to_owned());
        } else {
            keep.push(token);
        }
    }
    if keep.is_empty() {
        node.remove_attr("class");
    } else {
        node.set_attr("class", &keep.join(" "));
    }
    if !styles.is_empty() {
        let existing = node.attr_or("style", "");
        let separator = if existing.is_empty() || existing.ends_with(';') {
            ""
        } else {
            ";"
        };
        node.set_attr(
            "style",
            &format!("{existing}{separator}{}", styles.join(";")),
        );
    }
}

fn resolve_svg_colors(root: NodeRef<'_>) {
    for svg_root in select(root, "svg") {
        let nodes = elements(svg_root);
        for node in &nodes {
            for name in [
                "fill",
                "stroke",
                "color",
                "stop-color",
                "flood-color",
                "lighting-color",
                "style",
            ] {
                if let Some(value) = node
                    .attr(name)
                    .filter(|value| value.contains("var(") || value.contains("light-dark("))
                {
                    node.set_attr(name, &resolve_color(&value));
                }
            }
            tailwind_colors(*node);
        }
        let unstyled = nodes.iter().any(|node| {
            matches!(
                tag(*node).as_str(),
                "path" | "rect" | "circle" | "ellipse" | "polygon"
            ) && node.has_attr("class")
                && !node.has_attr("fill")
                && !node.attr_or("style", "").contains("fill:")
                && closest(*node, "defs,clipPath,mask,pattern,marker").is_none()
        });
        if !unstyled {
            continue;
        }
        for node in nodes {
            if !node.has_attr("class")
                || closest(node, "defs,clipPath,mask,pattern,marker").is_some()
            {
                continue;
            }
            let name = tag(node);
            let filled = ["path", "rect", "circle", "ellipse", "polygon"].contains(&name.as_str());
            let stroke = ["line", "polyline"].contains(&name.as_str());
            let text = ["text", "tspan"].contains(&name.as_str());
            let style = node.attr_or("style", "");
            if text && !node.has_attr("fill") && !style.contains("fill:") {
                node.set_attr("fill", "currentColor");
            }
            let has_fill = node
                .attr("fill")
                .is_some_and(|fill| fill.as_ref() != "none");
            if filled && !node.has_attr("fill") && !style.contains("fill:") {
                node.set_attr("fill", "none");
            }
            if (stroke
                || (filled && !has_fill && !node.attr_or("d", "").trim_end().ends_with(['Z', 'z'])))
                && !node.has_attr("stroke")
                && !style.contains("stroke:")
            {
                node.set_attr("stroke", "currentColor");
                if stroke && !node.has_attr("stroke-opacity") {
                    node.set_attr("stroke-opacity", "0.2");
                }
            }
        }
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[test]
    fn one_protection_walk_equals_the_two_closest_queries() {
        let doc = Document::from(
            "<article><section class='tex2jax_ignore mathjax_ignore'><p>Prose <em>text</em>.</p></section><pre><code> a , </code><span data-math='x'>x</span></pre><svg><text>diagram</text><foreignObject><div>foreign</div></foreignObject></svg><span class='MaThJaX'><i>x</i></span><span class='latex'><b>prose</b></span><span data-tex='x'><em>x</em></span><math><mrow><mi>x</mi></mrow></math><mjx-container><mjx-math>x</mjx-math></mjx-container><template><span data-latex='y'>y</span></template></article>",
        );
        for node in dom::subtree(doc.root()) {
            let expected = closest(node, "pre,code,svg").is_some()
                || closest(node, math::SOURCE_SELECTOR).is_some_and(math::is_math_source);
            assert_eq!(protected(node), expected, "{}", node.html());
        }
    }

    #[test]
    fn code_root_equals_selector_on_every_real_input() {
        let code = dom_query::Matcher::new("pre,code,svg").unwrap();
        let foreign = Document::from(
            "<svg><code>x</code><pre>x</pre><SVG>x</SVG></svg><math><code>x</code></math><PRE>x</PRE>",
        );
        for doc in crate::tests::upstream_documents()
            .into_iter()
            .chain([foreign])
        {
            for node in dom::subtree(doc.root()) {
                assert_eq!(code_root(node), node.is_match(&code), "{}", node.html());
            }
        }
    }

    #[test]
    fn boundary_characters_equal_concatenated_dom_text() {
        let doc = Document::from(
            "<article><span></span> α<em><span></span>β</em><i>\n</i><!--ignore--><br><b>終</b><span></span><template>inert</template><script type='math/tex'>x^2</script></article>",
        );
        for node in dom::subtree(doc.root()) {
            let text = node.text();
            assert_eq!(
                boundary_text_char(node, false),
                text.chars().next(),
                "{}",
                node.html()
            );
            assert_eq!(
                boundary_text_char(node, true),
                text.chars().next_back(),
                "{}",
                node.html()
            );
        }
    }
}
