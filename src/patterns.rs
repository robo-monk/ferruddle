//! Structural removal of repeated page furniture and article metadata.
use crate::{
    DebugRemoval, Metadata,
    dom::{closest, is_before, select},
    selection::HEADINGS,
    utils::{attribute, contains, count_words, normalize_text, tag, utf16_len, words_at_most},
};
use dom_query::{NodeData, NodeRef};
use regex::Regex;
use std::sync::LazyLock;
use url::Url;

macro_rules! re {
    ($p:expr) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new($p).unwrap());
        &*RE
    }};
}
const CONTENT: &str = "img,picture,video,audio,iframe,figure,table,pre,code,blockquote,math,svg,[data-mathml],.katex,.katex-mathml,.katex-display,.MathJax,.MathJax_Display,.MathJax_SVG,mjx-container";
const NO_IMG: &str = "video,audio,iframe,figure,table,pre,code,blockquote,math,svg,[data-mathml],.katex,.katex-mathml,.katex-display,.MathJax,.MathJax_Display,.MathJax_SVG,mjx-container";
const DATE: &str = r"(?i)(?:(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}|\d{1,2}(?:st|nd|rd|th)?\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*|\d{4}[-/]\d{1,2}[-/]\d{1,2})";
const READ_TIME: &str =
    r"(?i)\d+\s*min(?:ute)?s?\s+read\b|(?:read(?:ing)?\s+time)\s*:?\s*\d+\s*min(?:ute)?s?\b";
const RELATED: &str = r"(?i)^(?:related (?:posts?|articles?|content|stories|reads?|reading)|you (?:might|may|could) (?:also )?(?:like|enjoy|be interested in)|read (?:next|more|also)|further reading|see also|more (?:from .*|from|articles?|posts?|like this)|more to (?:read|explore)|explore more|about (?:the )?author|latest (?:news|events?|posts?|articles?|stories)(?:\s*[&+]\s*(?:news|events?|posts?|articles?|stories))?)$";
const CTA: &str = r"(?i)^(?:subscribe|sign up|follow us|share this|stay (?:updated|connected)|join (?:us|our)|search (?:the |our )?(?:site|blog|archives?|newsroom|website|catalog|store|shop|database))$";
const NEWSLETTER: &str = r"(?i)\bsubscribe\b(?s:.){0,40}\bnewsletter\b|\bnewsletter\b(?s:.){0,40}\bsubscribe\b|\bsign[- ]up\b(?s:.){0,80}\b(?:newsletter|email alert)|\b(?:don[’']?t (?:want to )?miss|never miss)\b(?s:.){0,80}\b(?:latest|best|exclusive|reports?|updates?|source)";

fn remove(node: NodeRef<'_>, reason: &str, debug: bool, removals: &mut Vec<DebugRemoval>) {
    if debug {
        removals.push(DebugRemoval {
            step: "removeByContentPattern".into(),
            selector: None,
            reason: Some(reason.into()),
            text: node.text().trim().chars().take(200).collect(),
        });
    }
    node.remove_from_parent();
}
/// Equivalent to `closest(node, PROTECTED).is_some()` without the selector
/// engine. Matching is never in quirks mode, so the ID comparison is exact;
/// like the engine, it reads `id` in any namespace.
fn protected(node: NodeRef<'_>) -> bool {
    std::iter::once(node)
        .chain(node.ancestors_it(None))
        .any(|node| {
            node.query_or(false, |node| {
                let NodeData::Element(element) = &node.data else {
                    return false;
                };
                matches!(element.name.local.as_ref(), "pre" | "code")
                    || element
                        .attrs
                        .iter()
                        .any(|attribute| match attribute.name.local.as_ref() {
                            "id" => attribute.value.as_ref() == "footnotes",
                            "data-defuddle" => attribute.name.ns.is_empty(),
                            _ => false,
                        })
            })
        })
}
#[cfg(test)]
const PROTECTED: &str = "pre,code,#footnotes,[data-defuddle]";
fn words(node: NodeRef<'_>) -> usize {
    crate::utils::text_words(node)
}
fn wrapper<'a>(mut node: NodeRef<'a>, root: NodeRef<'a>) -> NodeRef<'a> {
    while let Some(parent) = node.parent().filter(|p| p.id != root.id) {
        if parent.text().trim() != node.text().trim() {
            break;
        }
        node = parent;
    }
    node
}
fn isolated<'a>(mut node: NodeRef<'a>, root: NodeRef<'a>) -> NodeRef<'a> {
    while let Some(parent) = node.parent().filter(|p| p.id != root.id) {
        let mut sibling = node.prev_element_sibling();
        let mut count = 0;
        while let Some(s) = sibling {
            count += words(s);
            sibling = s.prev_element_sibling();
            if count > 10 {
                break;
            }
        }
        if count > 10 {
            break;
        }
        node = parent;
    }
    node
}
fn preceding_prose_words(node: NodeRef<'_>, root: NodeRef<'_>) -> usize {
    let mut total = 0;
    for current in
        std::iter::once(node).chain(node.ancestors_it(None).take_while(|n| n.id != root.id))
    {
        let mut previous = current.prev_element_sibling();
        while let Some(node) = previous {
            total += if node.is("p") {
                words(node)
            } else {
                select(node, "p").into_iter().map(words).sum()
            };
            previous = node.prev_element_sibling();
        }
    }
    total
}
fn following_prose(node: NodeRef<'_>, min: usize) -> bool {
    let mut next = node.next_element_sibling();
    while let Some(n) = next {
        if (tag(n) == "p" && words(n) >= min) || select(n, "p").iter().any(|p| words(*p) >= min) {
            return true;
        }
        next = n.next_element_sibling();
    }
    false
}
fn trailing(node: NodeRef<'_>, root: NodeRef<'_>, debug: bool, removals: &mut Vec<DebugRemoval>) {
    let ancestors: Vec<_> = node
        .ancestors_it(None)
        .take_while(|p| p.id != root.id)
        .collect();
    for current in std::iter::once(node).chain(ancestors) {
        let mut next = current.next_element_sibling();
        while let Some(n) = next {
            next = n.next_element_sibling();
            if attribute(n, "id") != "footnotes" {
                remove(n, "trailing non-content", debug, removals)
            }
        }
        if current.id == node.id {
            remove(current, "boilerplate text", debug, removals)
        }
    }
}
fn prose(node: NodeRef<'_>) -> bool {
    let name = tag(node);
    if !["p", "div", "section", "article", "blockquote", "font"].contains(&name.as_str())
        || closest(
            node,
            "aside,nav,header,footer,form,[role=dialog],[role=alertdialog]",
        )
        .is_some()
    {
        return false;
    }
    if crate::dom::exists(node, "script,style,[role=dialog],[role=alertdialog]") {
        return false;
    }
    if name == "div" && !crate::dom::exists(node, "p") {
        return false;
    }
    let text = node.text();
    let w = count_words(&text);
    w >= 7
        && text.contains(['.', '!', '?'])
        && !(w < 15 && re!(r"(?i)^by\s+\S").is_match(text.trim()))
        && !(w < 20 && re!(DATE).is_match(text.trim()))
        && !re!(r"\b(?:isHidden(?:-[A-Za-z0-9_]+)?|is-hidden)\b")
            .is_match(&attribute(node, "class"))
        && select(node, "a")
            .iter()
            .map(|a| a.text().len())
            .sum::<usize>() as f64
            / (text.len().max(1) as f64)
            <= 0.7
}
fn content_start<'a>(root: NodeRef<'a>, title: &str) -> Option<NodeRef<'a>> {
    let normalized = normalize_text(title);
    let title = select(root, "h1,h2")
        .into_iter()
        .find(|h| !normalized.is_empty() && normalize_text(&h.text()) == normalized);
    let mut candidates = select(root, "p,div,section,article,blockquote,font")
        .into_iter()
        .filter(|n| title.is_none_or(|h| is_before(h, *n)) && prose(*n));
    let first = candidates.next();
    let leaf = std::iter::once(first)
        .flatten()
        .chain(candidates)
        .find(|n| ["p", "blockquote", "font"].contains(&tag(*n).as_str()));
    let container = first.map(|mut node| {
        loop {
            let children: Vec<_> = node
                .element_children()
                .into_iter()
                .filter(|n| prose(*n))
                .collect();
            if children.len() != 1 {
                break;
            }
            node = children[0];
        }
        node
    });
    leaf.or(container).or_else(|| {
        if title.is_some() {
            content_start(root, "")
        } else {
            None
        }
    })
}

pub fn eyebrow(root: NodeRef<'_>, debug: bool, removals: &mut Vec<DebugRemoval>) {
    let Some(mut current) = select(root, "h1")
        .first()
        .or(select(root, "h2").first())
        .copied()
    else {
        return;
    };
    while current.prev_element_sibling().is_none() {
        let Some(parent) = current.parent().filter(|p| p.id != root.id) else {
            break;
        };
        current = parent;
    }
    let Some(prev) = current.prev_element_sibling() else {
        return;
    };
    let text = prev.text();
    let w = count_words(&text);
    if (1..=6).contains(&w)
        && text.trim().chars().count() <= 40
        && !text.contains(['.', '!', '?'])
        && !re!(DATE).is_match(&text)
        && !crate::dom::exists(
            prev,
            "img,picture,video,iframe,figure,table,pre,code,time,[datetime],h1,h2,h3,h4,h5,h6,ul,ol,blockquote",
        )
    {
        remove(prev, "eyebrow label", debug, removals)
    }
}

pub fn remove_patterns(
    root: NodeRef<'_>,
    page_url: Option<&str>,
    metadata: &Metadata,
    debug: bool,
    removals: &mut Vec<DebugRemoval>,
) {
    let boundary = content_start(root, &metadata.title);
    let pre = |n| boundary.is_some_and(|start| is_before(n, start));
    let title = normalize_text(&metadata.title);
    let desc = normalize_text(&metadata.description);
    let content_text = root.text().to_string();
    let content_length = utf16_len(&content_text);
    let pos = |n: NodeRef<'_>| {
        content_text
            .find(&n.text().trim().chars().take(60).collect::<String>())
            .map(|index| utf16_len(&content_text[..index]))
            .unwrap_or(content_length)
    };
    let near_end =
        |n: NodeRef<'_>, limit: usize| pos(n) + utf16_len(&n.text()) + limit >= content_length;

    if let Some(list) = select(root, "ul,ol").first().copied() {
        let items = list.element_children();
        let links = select(list, "a[href]");
        if (2..=8).contains(&items.len())
            && items.iter().all(|n| words(*n) <= 10)
            && !links.is_empty()
            && links.len() < items.len()
            && links.iter().all(|a| {
                let h = attribute(*a, "href");
                h.starts_with('/') && !h.starts_with("//")
            })
            && links.iter().any(|a| {
                let h = attribute(*a, "href");
                h == "/" || h.trim_matches('/').split('/').count() == 1
            })
            && !crate::dom::exists(list, "img,p,figure,blockquote")
        {
            remove(
                wrapper(list, root),
                "breadcrumb navigation list",
                debug,
                removals,
            )
        }
    }
    if let Some(h1) = select(root, "h1").first().copied() {
        for a in select(root, "a[href]") {
            if is_before(a, h1)
                && words(a) <= 25
                && crate::dom::exists(a, "div")
                && !crate::dom::exists(a, "img,picture,video")
                && !re!(r"[.!?]\s").is_match(&a.text())
            {
                remove(a, "promotional banner link", debug, removals)
            }
        }
    }
    // Entire header clusters prevent title/author avatars surviving as body content.
    for time in select(root, "time,[datetime]") {
        if !pre(time) || !contains(root, time) {
            continue;
        }
        let mut best = None;
        let mut current = time.parent();
        while let Some(n) = current.filter(|n| n.id != root.id) {
            if crate::dom::exists(n, "h1,h2") && pre(n) {
                let mut metadata = Vec::new();
                for node in select(n, "h1,h2,h3,time,[aria-label]") {
                    if !metadata.iter().any(|existing| contains(*existing, node)) {
                        metadata.push(node);
                    }
                }
                let metadata_words: usize = metadata.into_iter().map(words).sum();
                if words(n).saturating_sub(metadata_words) < 30 {
                    best = Some(n)
                } else {
                    break;
                }
            }
            current = n.parent();
        }
        if let Some(n) = best {
            remove(n, "hero header", debug, removals)
        }
    }
    for media in select(root, "audio,video") {
        if !media.has_attr("src") && !crate::dom::exists(media, "source") {
            continue;
        }
        let mut target = media;
        while let Some(parent) = target
            .parent()
            .filter(|p| p.id != root.id && words(*p) <= 25)
        {
            target = parent
        }
        if re!(r"(?i)\blisten\s+to\s+(?:this\s+)?(?:article|story|post|episode|podcast)\b")
            .is_match(&target.text())
            || pre(target)
        {
            remove(target, "audio player widget", debug, removals)
        }
    }
    for list in select(root, "ul,ol") {
        if protected(list)
            || pos(list) > content_length * 3 / 10
            || crate::dom::exists(list, CONTENT)
        {
            continue;
        }
        let links = select(list, "a[href]");
        if links.len() < 3 {
            continue;
        }
        let anchors = links
            .iter()
            .filter(|a| {
                let href = attribute(**a, "href");
                href.starts_with('#')
                    || page_url
                        .and_then(|s| Url::parse(s).ok())
                        .is_some_and(|base| {
                            base.join(&href).is_ok_and(|u| {
                                u.fragment().is_some()
                                    && u.host_str() == base.host_str()
                                    && u.path() == base.path()
                            })
                        })
            })
            .count();
        if anchors < 3 || anchors * 10 < links.len() * 8 {
            continue;
        }
        let target = wrapper(list, root);
        if let Some(prev) = target.prev_element_sibling()
            && re!(r"(?i)^(?:table of )?contents$|^on this page$|^in this (?:article|guide|post)$")
                .is_match(prev.text().trim())
        {
            remove(prev, "table of contents heading", debug, removals)
        }
        for sibling in [target.prev_element_sibling(), target.next_element_sibling()]
            .into_iter()
            .flatten()
        {
            if tag(sibling) == "hr" {
                sibling.remove_from_parent()
            }
        }
        remove(target, "table of contents", debug, removals);
        break;
    }
    let mut byline = false;
    for node in select(root, "p,span,div,time") {
        if !contains(root, node) || protected(node) {
            continue;
        }
        let text = node.text();
        let text = text.trim();
        let w = count_words(text);
        if w == 0 || w > 15 || crate::dom::exists(node, NO_IMG) {
            continue;
        }
        // Ancestor text contains the node's text, so word counts only grow.
        if std::iter::once(node)
            .chain(node.ancestors_it(None).take_while(|p| p.id != root.id))
            .take_while(|n| words_at_most(*n, 15))
            .any(|n| {
                re!(r"(?i)^(?:date|published|updated|posted|from|to|subject)\s*:")
                    .is_match(n.text().trim())
            })
        {
            continue;
        }
        let normalized = normalize_text(text);
        let has_date = re!(DATE).is_match(text);
        let readtime = re!(READ_TIME).is_match(text)
            && (if has_date {
                !crate::dom::exists(node, "p,div,section,article")
            } else {
                w <= 5 && pre(node)
            })
            && re!(r"(?i)\b(?:estimated|reading|read|time|minutes?|mins?)\b|[0-9\s/|·•—–,:.\-]")
                .replace_all(text, "")
                .is_empty();
        let standalone_date=re!(DATE).is_match(text)&&re!(r"(?i)\b(?:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|Jun(?:e)?|Jul(?:y)?|Aug(?:ust)?|Sep(?:t(?:ember)?)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?|Mon(?:day)?|Tue(?:s(?:day)?)?|Wed(?:nesday)?|Thu(?:rs(?:day)?)?|Fri(?:day)?|Sat(?:urday)?|Sun(?:day)?|st|nd|rd|th)\b|[0-9\s/|·•—–,:.\-]").replace_all(text,"").is_empty();
        let by = pre(node)
            && !byline
            && w >= 2
            && !re!(r"[.!?]$").is_match(text)
            && re!(r"(?i)^(?:posted\s+)?by\s+\S").is_match(text);
        let duplicate = w >= 3
            && pre(node)
            && ((!title.is_empty() && normalized == title)
                || (!desc.is_empty() && normalized == desc));
        let timezone = re!(r"(?i)^current time in$").is_match(text) && pos(node) <= 300;
        let pinned = re!(r"(?i)^pinned$").is_match(text);
        let relative = pre(node)
            && re!(r"(?i)^\d+\s+(?:second|minute|hour|day|week|month|year)s?\s+ago$")
                .is_match(text);
        if timezone {
            let target = node.parent().filter(|p| p.id != root.id).unwrap_or(node);
            remove(target, "timezone widget", debug, removals);
            continue;
        }
        let header_metadata = matches!(tag(node).as_str(), "div" | "p")
            && w <= 10
            && pre(node)
            && (has_date
                || re!(r"(?i)\b\d+\s+(?:second|minute|hour|day|week|month|year)s?\s+ago\b")
                    .is_match(text))
            && !text.contains(['.', '!', '?'])
            && !select(node, "p,h1,h2,h3,h4,h5,h6")
                .iter()
                .any(|n| words(*n) > 8);
        if readtime
            || header_metadata
            || duplicate
            || by
            || timezone
            || pinned
            || relative
            || (standalone_date && pre(node))
        {
            if by {
                byline = true
            }
            remove(wrapper(node, root), "article metadata", debug, removals);
            continue;
        }
        if re!(r"(?i)^\d+\s+(?:Likes?|Comments?|Shares?|Retweets?|Reposts?|Restacks?)$")
            .is_match(text)
            && (near_end(node, 200) || closest(node, "a:not([href])").is_some())
            && closest(node, "a[href]").is_none()
        {
            remove(
                wrapper(node, root),
                "social engagement counter",
                debug,
                removals,
            )
        }
    }
    for time in select(root, "time") {
        if !contains(root, time) || protected(time) || words(time) > 10 {
            continue;
        }
        let mut target = time;
        while let Some(parent) = target.parent().filter(|p| p.id != root.id) {
            let name = tag(parent);
            if parent.text().trim() != target.text().trim() {
                break;
            }
            if name == "p" {
                target = parent;
                break;
            }
            if !["i", "em", "span", "b", "strong", "small"].contains(&name.as_str()) {
                break;
            }
            target = parent;
        }
        if words(target) <= 10 && (pre(target) || pos(target) < 200 || near_end(target, 200)) {
            remove(target, "boundary time", debug, removals)
        }
    }
    for list in select(root, "ul,ol,dl") {
        if protected(list) || !contains(root, list) || crate::dom::exists(list, CONTENT) {
            continue;
        }
        let items: Vec<_> = list
            .element_children()
            .into_iter()
            .filter(|n| {
                if tag(list) == "dl" {
                    tag(*n) == "dd"
                } else {
                    tag(*n) == "li"
                }
            })
            .collect();
        if items.is_empty()
            || items.len() > 8
            || (items.len() < 2 && tag(list) != "dl")
            || words(list) > 30
            || items
                .iter()
                .any(|n| words(*n) > 8 || re!(r"[.!?]\s*$").is_match(n.text().trim()))
        {
            continue;
        }
        if pos(list) > 500 && !near_end(list, 500) {
            continue;
        }
        let prev = list.prev_element_sibling();
        if prev.is_some_and(|p| {
            HEADINGS.split(',').any(|h| h == tag(p))
                || crate::dom::exists(p, HEADINGS)
                || p.text().trim().ends_with(':')
        }) {
            continue;
        }
        remove(
            wrapper(list, root),
            "boundary metadata list",
            debug,
            removals,
        )
    }
    // Parent-path navigation is a common unlabeled back link.
    if let Some(base) = page_url.and_then(|s| Url::parse(s).ok()) {
        for a in select(root, "a[href]") {
            if words(a) > 8 || protected(a) || crate::dom::exists(a, CONTENT) {
                continue;
            }
            let href = attribute(a, "href");
            let Ok(u) = base.join(&href) else { continue };
            let parent = base
                .path()
                .trim_end_matches('/')
                .rsplit_once('/')
                .map(|(p, _)| p)
                .unwrap_or("");
            if !parent.is_empty()
                && u.host_str() == base.host_str()
                && u.path().trim_end_matches('/') == parent
                && (pre(a) || closest(a, "p").is_none())
            {
                remove(wrapper(a, root), "back navigation", debug, removals)
            }
        }
    }
    // A final heading plus off-site link list is an affiliate/resource footer.
    if let Some(base) = page_url.and_then(|s| Url::parse(s).ok()) {
        let host = base
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.");
        for heading in select(root, "h2,h3,h4,h5,h6") {
            let Some(list) = heading.next_element_sibling().filter(|n| n.is("ul,ol")) else {
                continue;
            };
            let items: Vec<_> = list
                .element_children()
                .into_iter()
                .filter(|n| n.is("li"))
                .collect();
            if items.len() < 2 || protected(list) {
                continue;
            }
            let mut more = false;
            for current in
                std::iter::once(list).chain(list.ancestors_it(None).take_while(|n| n.id != root.id))
            {
                let mut next = current.next_element_sibling();
                while let Some(n) = next {
                    if !n.text().trim().is_empty() {
                        more = true;
                        break;
                    }
                    next = n.next_element_sibling();
                }
                if more {
                    break;
                }
            }
            if more {
                continue;
            }
            let external = items.iter().all(|item| {
                let links = select(*item, "a[href]");
                !links.is_empty()
                    && links.iter().all(|link| {
                        base.join(&attribute(*link, "href")).is_ok_and(|url| {
                            url.host_str()
                                .unwrap_or_default()
                                .trim_start_matches("www.")
                                != host
                        })
                    })
                    && links
                        .iter()
                        .map(|a| utf16_len(a.text().trim()))
                        .sum::<usize>() as f64
                        >= utf16_len(item.text().trim()) as f64 * 0.6
            });
            if external {
                remove(list, "trailing external link list", debug, removals);
                remove(heading, "trailing external link list", debug, removals);
            }
        }
    }
    for node in select(root, "div,section,aside,ul") {
        if !contains(root, node) || protected(node) || crate::dom::exists(node, CONTENT) {
            continue;
        }
        let limit = if tag(node) == "ul" { 30 } else { 60 };
        let text = node.text();
        if !(2..=limit).contains(&count_words(&text)) {
            continue;
        }
        let normalized = re!(r"([a-z])([A-Z])").replace_all(&text, "$1 $2");
        if re!(NEWSLETTER).is_match(&normalized) {
            let mut target = node;
            let max = words(node) * 2 + 15;
            while let Some(parent) = target
                .parent()
                .filter(|p| p.id != root.id && words(*p) <= max && !crate::dom::exists(*p, CONTENT))
            {
                target = parent
            }
            remove(target, "newsletter subscription", debug, removals);
        }
    }
    // Recommendation clusters have multiple link-heavy paragraphs, allowing
    // punctuation inside linked titles while requiring prose outside links to be absent.
    let mut last = root.element_children().last().copied();
    while last.is_some_and(|n| n.is("hr,br")) {
        last = last.and_then(|n| n.prev_element_sibling());
    }
    if let Some(last) = last.filter(|n| n.is("div,section,aside")) {
        let children: Vec<_> = last
            .element_children()
            .into_iter()
            .filter(|n| !n.text().trim().is_empty())
            .collect();
        if children.iter().filter(|n| n.is("p")).count() >= 2
            && children.iter().all(|n| n.is("p,br"))
        {
            let cards = children.iter().filter(|n| n.is("p")).all(|n| {
                let text = normalize_text(&n.text());
                let links = select(*n, "a[href]");
                let len: usize = links.iter().map(|n| utf16_len(n.text().trim())).sum();
                let mut residual = text.clone();
                for link in &links {
                    residual = residual.replace(&normalize_text(&link.text()), "");
                }
                !links.is_empty()
                    && len as f64 > utf16_len(&text) as f64 * 0.6
                    && !residual.contains(['.', '!', '?'])
            });
            if cards {
                remove(last, "trailing related posts block", debug, removals);
            }
        }
    }
    for node in select(root, "div,section") {
        if !contains(root, node)
            || protected(node)
            || !(2..=40).contains(&words(node))
            || !near_end(node, 300)
        {
            continue;
        }
        let text = node.text();
        let label = select(node, "div,span,p,dt,dd,li").iter().any(|n| {
            re!(r"(?i)^(?:written by|(?:author|contact|reporter|correspondent)s?)$")
                .is_match(n.text().trim())
        });
        let contact = re!(r"[\w.-]+@[\w.-]+\.\w+|\(?\d{3}\)?[\s.‑–-]?\d{3}[\s.‑–-]?\d{4}")
            .is_match(&text)
            || crate::dom::exists(node, "a[href^='mailto:']");
        if label && contact {
            remove(
                isolated(node, root),
                "author contact block",
                debug,
                removals,
            );
            break;
        }
    }
    for node in select(root, "p,div,span") {
        if !contains(root, node) || protected(node) {
            continue;
        }
        let text = node.text();
        let t = text.trim();
        let w = count_words(&text);
        if w<=50&&re!(r"(?i)^(?:This (?:article|story|piece) (?:appeared|was published|originally appeared) in\b|A version of this (?:article|story) (?:appeared|was published) in\b|Originally (?:published|appeared) (?:in|on|at)\b|Any re-?use permitted\b|©\s*(?:Copyright\s+)?\d{4}|Comments?$|Leave a (?:comment|reply)$|Loading\.{3}$|Affiliate links\b.*\b(?:earn|commission)|Thank you for (?:being part of|joining) our community\b)|\bRead our Comment Policy\b").is_match(t)&&pos(node)>200 {
            let mut target=node;while target.next_element_sibling().is_none(){let Some(parent)=target.parent().filter(|p|p.id!=root.id)else{break};target=parent}
            if pos(target) < 200 {
                if target.id != node.id && node.next_element_sibling().is_none() { remove(node,"boilerplate text",debug,removals); }
                continue;
            }
            if target.id != root.id { trailing(target,root,debug,removals); }
            continue;
        }
        if w <= 15 && re!(r"(?i)^(?:share|follow|authors?|written\s+by)$").is_match(t) {
            let mut target = node;
            while let Some(parent) = target
                .parent()
                .filter(|p| p.id != root.id && words(*p) <= 15 && !crate::dom::exists(*p, NO_IMG))
            {
                target = parent
            }
            remove(target, "share or author label", debug, removals);
        }
        if tag(node) == "div"
            && (1..=10).contains(&w)
            && near_end(node, 300)
            && !text.contains(['.', '!', '?'])
            && !crate::dom::exists(node, CONTENT)
        {
            let link_len: usize = select(node, "a").iter().map(|a| a.text().len()).sum();
            if link_len as f64 / (text.trim().len().max(1) as f64) >= 0.8 && link_len > 0 {
                remove(wrapper(node, root), "trailing tags", debug, removals)
            }
        }
    }
    for node in select(root, "a:not([href])") {
        if contains(root, node)
            && !protected(node)
            && re!(r"(?i)^\d+\s+(?:Likes?|Comments?|Shares?|Retweets?|Reposts?|Restacks?)$")
                .is_match(node.text().trim())
        {
            remove(
                wrapper(node, root),
                "social engagement counter",
                debug,
                removals,
            );
        }
    }
    for h in select(root, HEADINGS) {
        if !contains(root, h) || protected(h) {
            continue;
        }
        let text = h.text();
        let cta = re!(CTA).is_match(text.trim());
        if !cta && !(pos(h) > 500 && re!(RELATED).is_match(text.trim())) {
            continue;
        }
        let target = isolated(h, root);
        if following_prose(target, 25) {
            if target.id == h.id {
                continue;
            }
            remove(target, "inline related content", debug, removals);
            continue;
        }
        if target.id == h.id && !cta {
            continue;
        }
        if let Some(prev) = target.prev_element_sibling() {
            let preceded = prev.prev_element_sibling().is_some_and(|p| {
                crate::dom::exists(p, HEADINGS) || HEADINGS.split(',').any(|h| h == tag(p))
            });
            if words(prev) < 50
                && !crate::dom::exists(prev, CONTENT)
                && !preceded
                && !HEADINGS.split(',').any(|h| h == tag(prev))
            {
                remove(prev, "thin preceding section", debug, removals)
            }
        }
        trailing(target, root, debug, removals)
    }
    for node in select(root, "p") {
        if contains(root, node)
            && !protected(node)
            && words(node) <= 20
            && !crate::dom::exists(node, CONTENT)
            && re!(r"(?i)^for more (?:on|about)\b").is_match(node.text().trim())
        {
            remove(node, "related content intro", debug, removals);
        }
    }
    for node in select(root, "div,section,aside,ul,ol") {
        if !contains(root, node) || protected(node) {
            continue;
        }
        let children = node.element_children();
        let first_text = children
            .first()
            .map(|n| n.text().trim().chars().take(30).collect::<String>())
            .unwrap_or_default();
        let follows_content = if first_text.chars().count() >= 5 {
            content_text
                .find(&first_text)
                .is_some_and(|offset| utf16_len(&content_text[..offset]) >= 500)
        } else {
            children.iter().all(|n| crate::dom::exists(*n, "a[href]"))
                && preceding_prose_words(node, root) >= 100
        };
        if children.len() >= 2
            && children
                .iter()
                .filter(|c| {
                    crate::dom::exists(**c, "img,picture")
                        && crate::dom::exists(**c, "h2,h3,h4,a[href]")
                })
                .count()
                * 10
                >= children.len() * 7
            && follows_content
            && words(node) * 10 <= words(root) * 3
            && !following_prose(node, 25)
        {
            let target = isolated(node, root);
            if target.id == node.id
                || words(target) > words(node) * 2 + 15
                || following_prose(target, 25)
            {
                continue;
            }
            let mut sibling = target.next_element_sibling();
            while let Some(next) = sibling {
                sibling = next.next_element_sibling();
                if attribute(next, "id") != "footnotes" {
                    remove(next, "trailing non-content", debug, removals);
                }
            }
            remove(target, "related post cards", debug, removals);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom_query::Document;

    #[test]
    fn protection_equals_selector_on_every_real_input_and_unusual_markup() {
        let unusual = Document::from(concat!(
            "<div ID=footnotes><p>x</p></div><div id=Footnotes><p>x</p></div>",
            "<div id=' footnotes'><p>x</p></div><div class=footnotes><p>x</p></div>",
            "<span DATA-DEFUDDLE><b>x</b></span><span data-defuddle-x><b>x</b></span>",
            "<PRE><b>x</b></PRE><CODE>x</CODE><svg><code><text>x</text></code>",
            "<g id=footnotes><text>x</text></g><a xlink:href='#' data-defuddle>x</a></svg>",
            "<math><pre><mi>x</mi></pre></math><template><pre>x</pre></template>",
        ));
        for doc in crate::tests::upstream_documents()
            .into_iter()
            .chain([unusual])
        {
            for node in crate::dom::subtree(doc.root()) {
                assert_eq!(
                    protected(node),
                    closest(node, PROTECTED).is_some(),
                    "{}",
                    node.html()
                );
            }
        }
    }
}
