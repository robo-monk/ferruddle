//! Normalize static callouts before selector and hidden-content removal.

use std::sync::LazyLock;

use dom_query::NodeRef;
use regex::Regex;

use crate::dom::{self, elements, select};

static HIDDEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)display\s*:\s*none\s*;?").unwrap());

fn title_case(kind: &str) -> String {
    let mut chars = kind.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn create(node: NodeRef<'_>, kind: &str, title: &str, source: NodeRef<'_>) {
    let title_div = node.tree.new_element("div");
    title_div.set_attr("class", "callout-title");
    let inner = node.tree.new_element("div");
    inner.set_attr("class", "callout-title-inner");
    dom::append_text(inner, title);
    title_div.append_child(&inner);
    let content = node.tree.new_element("div");
    content.set_attr("class", "callout-content");
    dom::transfer_children(source, content);
    node.remove_children();
    node.remove_all_attrs();
    dom::retag(node, "div");
    node.set_attr("class", "callout");
    node.set_attr("data-callout", kind);
    node.append_child(&title_div);
    node.append_child(&content);
}

/// Preserve Obsidian fold state and normalize GitHub, Hugo/Docsy, aside,
/// and Bootstrap callouts with setter-only construction of page-derived text.
pub fn standardize(root: NodeRef<'_>) {
    for node in elements(root) {
        if node.has_class("callout")
            && (node.has_class("is-collapsed") || node.has_class("is-collapsible"))
        {
            let collapsed = node.has_class("is-collapsed");
            node.remove_class("is-collapsed");
            node.remove_class("is-collapsible");
            if !node.has_attr("data-callout-fold") {
                node.set_attr("data-callout-fold", if collapsed { "-" } else { "+" });
            }
            for fold in select(node, ".callout-fold") {
                fold.remove_from_parent();
            }
            for content in select(node, ".callout-content") {
                if let Some(style) = content.attr("style") {
                    let cleaned = HIDDEN.replace_all(&style, "");
                    if cleaned.trim().is_empty() {
                        content.remove_attr("style");
                    } else {
                        content.set_attr("style", cleaned.trim());
                    }
                }
                content.remove_attr("hidden");
                content.remove_attr("aria-hidden");
            }
        }
        if node.has_attr("data-callout") {
            continue;
        }
        let classes = node.class().unwrap_or_default().to_string();
        let class_tokens: Vec<_> = classes.split_ascii_whitespace().collect();

        let (kind, title_selector, content_selector) = if node.has_class("markdown-alert") {
            (
                class_tokens
                    .iter()
                    .find_map(|class| class.strip_prefix("markdown-alert-"))
                    .unwrap_or("note"),
                ".markdown-alert-title",
                "",
            )
        } else if node.has_name("aside") && classes.contains("callout") {
            (
                class_tokens
                    .iter()
                    .find_map(|class| class.strip_prefix("callout-"))
                    .unwrap_or("note"),
                "",
                ".callout-content",
            )
        } else if node.has_class("admonition") {
            (
                class_tokens
                    .iter()
                    .copied()
                    .find(|class| {
                        matches!(
                            *class,
                            "info"
                                | "warning"
                                | "note"
                                | "tip"
                                | "danger"
                                | "caution"
                                | "important"
                                | "abstract"
                                | "success"
                                | "question"
                                | "failure"
                                | "bug"
                                | "example"
                                | "quote"
                        )
                    })
                    .unwrap_or("note"),
                ".admonition-title",
                ".admonition-content, .details-content",
            )
        } else if node.has_class("alert") && classes.contains("alert-") {
            (
                class_tokens
                    .iter()
                    .filter(|class| **class != "alert-dismissible")
                    .find_map(|class| class.strip_prefix("alert-"))
                    .unwrap_or("note"),
                ".alert-heading, .alert-title",
                "",
            )
        } else {
            continue;
        };
        let title_node = if title_selector.is_empty() {
            None
        } else {
            select(node, title_selector).into_iter().next()
        };
        let title = if node.has_class("markdown-alert") {
            title_case(kind)
        } else if let Some(title_node) = title_node {
            let text = title_node.text().trim().to_owned();
            if text.is_empty() {
                title_case(kind)
            } else {
                text
            }
        } else {
            title_case(kind)
        };
        if let Some(title) = title_node {
            title.remove_from_parent();
        }
        let source = if content_selector.is_empty() {
            node
        } else {
            select(node, content_selector)
                .into_iter()
                .next()
                .unwrap_or(node)
        };
        create(node, kind, &title, source);
    }
}
