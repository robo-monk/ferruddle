//! URL context and image deduplication after structural normalization.
use crate::{
    Metadata,
    dom::{self, closest, select},
    utils::attribute,
};
use dom_query::{Document, NodeRef};
use regex::Regex;
use std::{collections::HashMap, sync::LazyLock};
use url::Url;

/// Resolve the page and first authored base once, before attempts mutate clones.
pub(crate) fn base_url(original: &Document, page_url: Option<&str>) -> Option<Url> {
    let base = Url::parse(page_url?).ok()?;
    Some(
        select(original.root(), "base[href]")
            .first()
            .and_then(|n| n.attr("href"))
            .and_then(|href| base.join(&href).ok())
            .unwrap_or(base),
    )
}

pub(crate) fn urls(root: NodeRef<'_>, base: Option<&Url>) {
    let Some(base) = base else { return };
    let resolve = |value: &str| {
        let value = value.trim().trim_matches(['\\', '\'', '"']);
        if value.starts_with('#') {
            value.to_string()
        } else {
            base.join(value)
                .map(|u| u.to_string())
                .unwrap_or_else(|_| value.to_string())
        }
    };
    for node in dom::elements(root) {
        for attr in ["href", "src", "poster"] {
            if let Some(value) = node.attr(attr).filter(|v| !v.is_empty()) {
                node.set_attr(attr, &resolve(&value))
            }
        }
        if let Some(srcset) = node.attr("srcset") {
            let srcset = dom::srcset_candidates(&srcset)
                .into_iter()
                .map(|(url, descriptor)| {
                    let url = resolve(&url);
                    if descriptor.is_empty() {
                        url
                    } else {
                        format!("{url} {descriptor}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            node.set_attr("srcset", &srcset);
        }
    }
}
fn normalized(src: &str) -> String {
    let Ok(mut url) = Url::parse(src) else {
        return src.to_string();
    };
    // Retain identity-bearing query parameters such as ?id=... . Width and
    // quality transforms describe variants of the same source image.
    let query: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| {
            !matches!(
                key.as_ref(),
                "w" | "width"
                    | "h"
                    | "height"
                    | "q"
                    | "quality"
                    | "fit"
                    | "auto"
                    | "dpr"
                    | "fm"
                    | "format"
                    | "crop"
                    | "imwidth"
                    | "imheight"
            )
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    url.set_query(None);
    url.set_fragment(None);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    url.to_string()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string()
}
fn identity(src: &str) -> String {
    static SIZE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:[-_](?:small|medium|large|thumb|thumbnail|\d+x\d+))?\.(?:jpe?g|png|webp|avif)(\?|$)").unwrap()
    });
    SIZE_SUFFIX.replace(&normalized(src), "$1").into_owned()
}
fn rank(image: NodeRef<'_>) -> (u8, usize) {
    static WIDTH: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:width[=:/]|[/,?&]w[_:=])(\d+)").unwrap());
    let tier = if image.has_attr("srcset") {
        2
    } else if closest(image, "picture").is_some() {
        1
    } else {
        0
    };
    let width = WIDTH
        .captures(&attribute(image, "src"))
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(0);
    (tier, width)
}
fn keep_best(group: &[NodeRef<'_>]) {
    let Some((&first, rest)) = group.split_first() else {
        return;
    };
    let mut best = first;
    for &image in rest {
        if rank(image) > rank(best) {
            best.remove_from_parent();
            best = image
        } else {
            image.remove_from_parent()
        }
    }
}
fn adjacent(a: NodeRef<'_>, b: NodeRef<'_>) -> bool {
    fn next<'a>(n: NodeRef<'a>) -> Option<NodeRef<'a>> {
        if let Some(child) = n.first_child() {
            return Some(child);
        }
        let mut current = Some(n);
        while let Some(node) = current {
            if let Some(sibling) = node.next_sibling() {
                return Some(sibling);
            }
            current = node.parent()
        }
        None
    }
    let mut cursor = next(a);
    while let Some(n) = cursor {
        if n.id == b.id {
            return true;
        }
        if n.is_text() && !n.text().trim().is_empty() {
            return false;
        }
        cursor = next(n);
    }
    false
}
pub fn images(root: NodeRef<'_>, metadata: &mut Metadata) {
    for figure in select(root, "figure") {
        let mut groups: HashMap<String, Vec<NodeRef<'_>>> = HashMap::new();
        for image in select(figure, "img") {
            let src = attribute(image, "src");
            if src.is_empty() || src.starts_with("data:") || closest(image, "noscript").is_some() {
                continue;
            }
            groups
                .entry(format!(
                    "{}\n{}",
                    attribute(image, "alt").trim(),
                    identity(&src)
                ))
                .or_default()
                .push(image);
        }
        for (key, group) in groups {
            let alt = key.split('\n').next().unwrap_or("");
            if !alt.is_empty() && group.iter().all(|n| n.attr("src") == group[0].attr("src")) {
                continue;
            }
            keep_best(&group);
        }
    }
    let images = select(root, "img");
    for pair in images.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a.parent().is_none()
            || b.parent().is_none()
            || closest(a, "figure,noscript").is_some()
            || closest(b, "figure,noscript").is_some()
        {
            continue;
        }
        let alt = attribute(a, "alt");
        let src = attribute(a, "src");
        let other = attribute(b, "src");
        if !alt.trim().is_empty()
            && alt.trim() == attribute(b, "alt").trim()
            && !src.is_empty()
            && !other.is_empty()
            && !src.starts_with("data:")
            && !other.starts_with("data:")
            && src != other
            && identity(&src) == identity(&other)
            && adjacent(a, b)
        {
            keep_best(&[a, b])
        }
    }
    for image in select(root, "img") {
        if closest(image, "a,figure,noscript").is_some() {
            continue;
        }
        let Some(parent) = image.parent() else {
            continue;
        };
        let src = attribute(image, "src");
        if src.is_empty() || src.starts_with("data:") {
            continue;
        }
        if parent.element_children().iter().any(|n| {
            crate::utils::tag(*n) == "a"
                && dom::exists(*n, "img")
                && normalized(&attribute(*n, "href")) == normalized(&src)
        }) {
            image.remove_from_parent()
        }
    }
    if metadata.image.is_empty() {
        return;
    }
    for image in select(root, "img") {
        let src = attribute(image, "src");
        if src.is_empty()
            || src.starts_with("data:")
            || normalized(&src) != normalized(&metadata.image)
        {
            continue;
        }
        let best = image
            .attr("srcset")
            .and_then(|s| {
                dom::srcset_candidates(&s)
                    .into_iter()
                    .filter(|(s, _)| !crate::safety::is_dangerous_url(s, true))
                    .max_by(|a, b| {
                        let value =
                            |d: &str| d.trim_end_matches(['w', 'x']).parse::<f64>().unwrap_or(0.0);
                        value(&a.1).total_cmp(&value(&b.1))
                    })
                    .map(|(s, _)| s)
            })
            .unwrap_or(src);
        metadata.image = best;
        if !closest(image, "figure").is_some_and(|f| dom::exists(f, "figcaption")) {
            image.remove_from_parent()
        }
        break;
    }
}
