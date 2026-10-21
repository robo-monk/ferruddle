//! Ordered optional clutter-removal passes. Output sanitization is separate.
use std::{collections::HashSet, sync::LazyLock};

use dom_query::NodeRef;
use regex::Regex;

use crate::{
    DebugRemoval, Options,
    constants::*,
    dom::{closest, select, unwrap},
    math, selection,
    utils::{attribute, contains, matches, parse_number, tag, utf16_len},
};

static PARTIAL: LazyLock<Regex> = LazyLock::new(|| {
    let patterns: Vec<_> = PARTIAL_SELECTORS
        .iter()
        .copied()
        .filter(|p| !p.contains("(?<") && !p.contains("(?!"))
        .collect();
    Regex::new(&format!("(?i)(?:{})", patterns.join("|"))).expect("valid partial selector rules")
});
static PARTIAL_ANCHORED: LazyLock<Regex> = LazyLock::new(|| {
    let patterns: Vec<_> = PARTIAL_SELECTORS
        .iter()
        .copied()
        .filter(|p| !p.contains("(?<") && !p.contains("(?!"))
        .collect();
    Regex::new(&format!("(?i)^(?:{})$", patterns.join("|"))).expect("valid anchored selector rules")
});
static HIDDEN_STYLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|;\s*)(?:display\s*:\s*none|visibility\s*:\s*hidden|opacity\s*:\s*0)(?:\s*;|\s*$)").unwrap()
});
static RESPONSIVE_SHOW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(sm|md|lg|xl|2xl|min-\[|max-\[):(?:block|flex|grid|inline|table|contents)")
        .unwrap()
});
static LINK_ANNOTATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[\s,(–-]*(?:(?:link )?opens in (?:a )?new (?:window|tab)|new (?:window|tab)|external(?: link| site| website)?)[\s).]*$").unwrap()
});
static WIDTH_HINT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:width[=:/]|[/,?&]w[_:=])(\d+)").unwrap());
static STYLE_WIDTH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"width\s*:\s*(\d+)").unwrap());
static STYLE_HEIGHT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"height\s*:\s*(\d+)").unwrap());
static ONE_X: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\S+)\s+1x").unwrap());
static LATEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\[a-zA-Z]{2,}").unwrap());

pub fn responsive_show(node: NodeRef<'_>) -> bool {
    attribute(node, "class")
        .split_whitespace()
        .any(|token| RESPONSIVE_SHOW.is_match(token))
}

pub fn is_hidden(node: NodeRef<'_>) -> bool {
    if HIDDEN_STYLE.is_match(&attribute(node, "style")) || node.has_attr("hidden") {
        return true;
    }
    if responsive_show(node) {
        return false;
    }
    attribute(node, "class").split_whitespace().any(|token| {
        token == "hidden"
            || token == "invisible"
            || (!token.contains('[')
                && (token.ends_with(":hidden") || token.ends_with(":invisible")))
    })
}

pub fn record(
    node: NodeRef<'_>,
    step: &str,
    reason: &str,
    debug: bool,
    removals: &mut Vec<DebugRemoval>,
) {
    if debug {
        removals.push(DebugRemoval {
            step: step.into(),
            selector: None,
            reason: Some(reason.into()),
            text: node.text().trim().chars().take(200).collect(),
        });
    }
}

pub fn remove_hidden(root: NodeRef<'_>, debug: bool, removals: &mut Vec<DebugRemoval>) {
    for node in select(root, "*") {
        if !is_hidden(node) {
            continue;
        }
        if math::is_math_source(node) || math::has_source_descendant(node) {
            continue;
        }
        record(
            node,
            "removeHiddenElements",
            "hidden style or class",
            debug,
            removals,
        );
        node.remove_from_parent();
    }
}

fn exceptional_partial(value: &str, anchored: bool) -> bool {
    if anchored {
        return ["access-wall", "logo", "related"].contains(&value);
    }
    if value
        .match_indices("access-wall")
        .any(|(index, _)| !value[..index].ends_with("main-"))
    {
        return true;
    }
    if value.match_indices("related").any(|(index, _)| {
        let prefix = &value[..index];
        !prefix
            .as_bytes()
            .get(prefix.len().saturating_sub(3)..)
            .is_some_and(|p| {
                p.len() == 3 && p[0] == b'h' && (b'1'..=b'6').contains(&p[1]) && p[2] == b'-'
            })
    }) {
        return true;
    }
    let word_hyphen = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    value.match_indices("logo").any(|(index, _)| {
        !value[..index].chars().next_back().is_some_and(word_hyphen)
            || !value[index + 4..].chars().next().is_some_and(word_hyphen)
    })
}

fn partial_match(value: &str, anchored: bool) -> bool {
    if value.is_empty() {
        return false;
    }
    (if anchored {
        PARTIAL_ANCHORED.is_match(value)
    } else {
        PARTIAL.is_match(value)
    }) || exceptional_partial(&value.to_lowercase(), anchored)
}

pub fn remove_selectors(
    root: NodeRef<'_>,
    main: NodeRef<'_>,
    options: &Options,
    removals: &mut Vec<DebugRemoval>,
) {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    if options.remove_exact_selectors {
        for node in crate::dom::select_exact(root) {
            if closest(node, "pre,code").is_some() {
                continue;
            }
            if matches(node, HIDDEN_EXACT_SKIP_SELECTOR) && math::has_explicit_source(node) {
                continue;
            }
            if !options.remove_hidden_elements
                && (matches(node, HIDDEN_EXACT_SELECTOR)
                    || (closest(node, HIDDEN_EXACT_SKIP_SELECTOR).is_some()
                        && attribute(node, "role").eq_ignore_ascii_case("dialog")))
            {
                continue;
            }
            if matches(node, HIDDEN_EXACT_SELECTOR) && responsive_show(node) {
                continue;
            }
            if seen.insert(node.id) {
                targets.push((node, "exact selector match"));
            }
        }
    }
    if options.remove_partial_selectors {
        for node in select(
            root,
            "[class],[id],[data-component],[data-test],[data-testid],[data-test-id],[data-qa],[data-cy]",
        ) {
            if seen.contains(&node.id) {
                continue;
            }
            let heading = matches!(tag(node).as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
            let attrs = if heading {
                attribute(node, "class")
            } else {
                [
                    "class",
                    "data-component",
                    "data-test",
                    "data-testid",
                    "data-test-id",
                    "data-qa",
                    "data-cy",
                ]
                .iter()
                .map(|a| attribute(node, a))
                .collect::<Vec<_>>()
                .join(" ")
            };
            let id = if heading {
                String::new()
            } else {
                attribute(node, "id")
            };
            let delimited = id
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '_' | '-' | ':' | '.'));
            if (partial_match(&attrs, false) || partial_match(&id, !delimited))
                && closest(node, "code,pre,[data-defuddle]").is_none()
                && !crate::dom::exists(node, "pre")
                && seen.insert(node.id)
            {
                targets.push((node, "partial selector match"));
            }
        }
        for node in select(
            root,
            "a [class*=\"sr-only\"],a [class*=\"visually-hidden\"],a [class*=\"screen-reader-text\"]",
        ) {
            let Some(link) = closest(node, "a") else {
                continue;
            };
            let annotation = node.text();
            if LINK_ANNOTATION.is_match(&annotation)
                && link.text().trim() != annotation.trim()
                && closest(node, "code,pre,[data-defuddle]").is_none()
                && seen.insert(node.id)
            {
                targets.push((node, "screen-reader link annotation"));
            }
        }
    }
    for (node, reason) in targets {
        if contains(node, main)
            || (tag(node) == "a" && closest(node, selection::HEADINGS).is_some())
        {
            continue;
        }
        if matches(node, FOOTNOTE_LIST_SELECTORS)
            || !select(node, FOOTNOTE_LIST_SELECTORS).is_empty()
            || node
                .parent()
                .is_some_and(|p| matches(p, FOOTNOTE_LIST_SELECTORS))
            || (node.has_class("footnote-backref") && closest(node, "#footnotes").is_some())
        {
            continue;
        }
        if tag(node) == "button" {
            let media = select(node, "img,picture,video");
            if !media.is_empty() {
                for child in media {
                    node.insert_before(&child);
                }
                node.remove_from_parent();
                continue;
            }
            if closest(node, "p,li,td,th,span,h1,h2,h3,h4,h5,h6").is_some() {
                unwrap(node);
                continue;
            }
        }
        record(node, "removeBySelector", reason, options.debug, removals);
        node.remove_from_parent();
    }
}

pub fn remove_low_scoring(
    root: NodeRef<'_>,
    main: NodeRef<'_>,
    debug: bool,
    removals: &mut Vec<DebugRemoval>,
) {
    let targets: Vec<_> = select(root, selection::BLOCKS)
        .into_iter()
        .filter(|node| {
            !contains(*node, main)
                && closest(*node, "pre,[data-defuddle],td,th").is_none()
                && !selection::likely_content(*node)
        })
        .filter_map(|node| {
            let score = selection::clutter_score(node);
            (score < 0.0).then_some((node, score))
        })
        .collect();
    for (node, score) in targets {
        record(
            node,
            "scoreAndRemove",
            &format!("score: {score}"),
            debug,
            removals,
        );
        node.remove_from_parent();
    }
}

pub fn remove_small_images(root: NodeRef<'_>, debug: bool, removals: &mut Vec<DebugRemoval>) {
    for node in select(root, "img,svg") {
        let image = tag(node) == "img";
        let src = attribute(node, "src");
        let has_alternate = [
            "srcset",
            "data-src",
            "data-srcset",
            "data-lazy-src",
            "data-original",
        ]
        .iter()
        .any(|a| node.has_attr(a));
        let tiny_placeholder = src.starts_with("data:image/")
            && !src.starts_with("data:image/svg+xml")
            && src
                .split_once(";base64,")
                .is_some_and(|(_, data)| data.len() < 133);
        if image
            && ((src.is_empty() && !has_alternate)
                || (tiny_placeholder && !has_alternate && closest(node, "picture").is_none()))
        {
            record(
                node,
                "removeSmallImages",
                "empty or placeholder image",
                debug,
                removals,
            );
            node.remove_from_parent();
            continue;
        }
        let mut widths = vec![parse_number(&attribute(node, "width"))];
        let mut heights = vec![parse_number(&attribute(node, "height"))];
        let style = attribute(node, "style");
        widths.push(
            STYLE_WIDTH
                .captures(&style)
                .map(|c| parse_number(&c[1]))
                .unwrap_or(0.0),
        );
        heights.push(
            STYLE_HEIGHT
                .captures(&style)
                .map(|c| parse_number(&c[1]))
                .unwrap_or(0.0),
        );
        if !image {
            let view = attribute(node, "viewBox");
            let dimensions: Vec<_> = view
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|s| !s.is_empty())
                .collect();
            if dimensions.len() == 4 {
                widths.push(parse_number(dimensions[2]));
                heights.push(parse_number(dimensions[3]));
            }
        }
        if image
            && !widths.iter().chain(&heights).any(|n| *n > 0.0)
            && let Some(entry) = ONE_X.captures(&attribute(node, "srcset"))
            && let Some(hint) = WIDTH_HINT.captures(&entry[1])
        {
            widths.push(parse_number(&hint[1]));
        }
        let small = widths.iter().chain(&heights).any(|d| *d > 0.0 && *d < 33.0);
        let math = image
            && (LATEX.is_match(&attribute(node, "alt"))
                || node.has_class("latex")
                || node.has_class("tex")
                || node.has_attr("data-latex")
                || node.has_attr("data-math"));
        if small && !math {
            record(
                node,
                "removeSmallImages",
                "dimension below 33px",
                debug,
                removals,
            );
            node.remove_from_parent();
        }
    }
}

pub fn remove_metadata_block(main: NodeRef<'_>) {
    let Some(heading) = select(main, "h1").first().copied() else {
        return;
    };
    let mut next = heading.next_element_sibling();
    static DATE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\b(?:(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}[\s,]+\d{4}|\d{1,2}(?:st|nd|rd|th)?\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{4}|\d{4}[-/]\d{1,2}[-/]\d{1,2})\b").unwrap()
    });
    for _ in 0..3 {
        let Some(node) = next else { break };
        next = node.next_element_sibling();
        let text = node.text();
        if !text.trim().is_empty()
            && utf16_len(&text) < 300
            && (DATE.is_match(text.trim())
                || select(node, "p,time")
                    .iter()
                    .any(|p| DATE.is_match(p.text().trim())))
        {
            node.remove_from_parent();
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_cms_metadata_is_removed() {
        assert!(partial_match("single_post__author", false));
        assert!(partial_match("CopyContent", false));
        let doc = dom_query::Document::from(
            "<body><div class='single_post__author'>Author metadata</div><div class='CopyContent'>Article Text</div><p>Real prose.</p></body>",
        );
        let body = doc.tree.body().unwrap();
        let mut removals = Vec::new();
        remove_selectors(body, body, &Options::default(), &mut removals);
        assert!(!body.text().contains("metadata"));
        assert!(!body.text().contains("Article Text"));
    }
    #[test]
    fn cms_initial_removal_attempt_preserves_prose() {
        let case: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/data/upstream/selectors--arm-newsroom.json"
        ))
        .unwrap();
        let doc = dom_query::Document::from(case["inputHtml"].as_str().unwrap());
        let meta = crate::metadata::extract(&doc, case["url"].as_str());
        let options = Options {
            debug: true,
            ..Options::default()
        };
        let root = selection::find_main(&doc, None).unwrap();
        let body = doc.tree.body().unwrap();
        let mut logs = Vec::new();
        remove_metadata_block(root);
        crate::standardize::before_removals(&doc, root, &meta);
        remove_small_images(body, true, &mut logs);
        remove_hidden(body, true, &mut logs);
        crate::patterns::eyebrow(root, true, &mut logs);
        remove_selectors(body, root, &options, &mut logs);
        remove_low_scoring(body, root, true, &mut logs);
        crate::patterns::remove_patterns(root, case["url"].as_str(), &meta, true, &mut logs);
        assert!(root.text().contains("The company announced"));
    }
}
