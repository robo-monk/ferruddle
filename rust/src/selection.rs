//! Main-content and clutter scores, following Defuddle's independent heuristics.
use std::sync::LazyLock;

use dom_query::{Document, NodeRef};
use regex::{Regex, RegexSet};

use crate::constants::{ENTRY_POINT_ELEMENTS, FOOTNOTE_INLINE_REFERENCES, FOOTNOTE_LIST_SELECTORS};
use crate::dom::{closest, select};
use crate::utils::{attribute, contains, count_words, parse_number, tag, utf16_len};

pub const BLOCKS: &str = "div,section,article,main,aside,header,footer,nav,content";
pub const HEADINGS: &str = "h1,h2,h3,h4,h5,h6";
const CONTENT: &[&str] = &[
    "admonition",
    "article",
    "content",
    "entry",
    "image",
    "img",
    "font",
    "figure",
    "figcaption",
    "pre",
    "main",
    "post",
    "story",
    "table",
];
const NEGATIVE: &[&str] = &[
    "advert",
    "ad-",
    "ads",
    "banner",
    "cookie",
    "copyright",
    "footer",
    "header",
    "homepage",
    "menu",
    "nav",
    "newsletter",
    "popular",
    "privacy",
    "recommended",
    "related",
    "rights",
    "share",
    "sidebar",
    "social",
    "sponsored",
    "subscribe",
    "terms",
    "trending",
    "widget",
];
const NAVIGATION: &[&str] = &[
    "advertisement",
    "all rights reserved",
    "banner",
    "cookie",
    "comments",
    "copyright",
    "follow me",
    "follow us",
    "footer",
    "header",
    "homepage",
    "login",
    "menu",
    "more articles",
    "more like this",
    "most read",
    "nav",
    "navigation",
    "newsletter",
    "popular",
    "privacy",
    "recommended",
    "register",
    "related",
    "responses",
    "share",
    "sidebar",
    "sign in",
    "sign up",
    "signup",
    "social",
    "sponsored",
    "subscribe",
    "terms",
    "trending",
];
static NAV_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!("(?i){}", NAVIGATION.join("|"))).unwrap());
static NAV_WORDS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new(
        NAVIGATION
            .iter()
            .map(|s| format!(r"(?i)(?-u:\b){}(?-u:\b)", s.replace(' ', r"\s+"))),
    )
    .unwrap()
});
static CONTENT_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?-u:\b)(?:(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s+\d{4}|\d{1,2}(?:st|nd|rd|th)?\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*,?\s+\d{4})(?-u:\b)").unwrap()
});
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}|\d{1,2}(?:st|nd|rd|th)?\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*)").unwrap()
});
static AUTHOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:by|written by|author:)\s+[A-Za-z\s]+\b").unwrap());
static BYLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bBy\s+[A-Z]").unwrap());
static TOC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:table of )?contents$|^on this page$|^in this (?:article|guide|post)$")
        .unwrap()
});

pub fn score(element: NodeRef<'_>) -> f64 {
    let text = element.text();
    let words = count_words(&text);
    let paragraphs = select(element, "p").len();
    let mut score = words as f64 + paragraphs as f64 * 10.0 + text.matches(',').count() as f64;
    score -= select(element, "img").len() as f64 / words.max(1) as f64 * 3.0;
    let style = attribute(element, "style");
    if style.contains("float: right")
        || style.contains("text-align: right")
        || attribute(element, "align") == "right"
    {
        score += 5.0;
    }
    if CONTENT_DATE.is_match(&text) {
        score += 10.0;
    }
    if AUTHOR.is_match(&text) {
        score += 10.0;
    }
    let class = attribute(element, "class").to_lowercase();
    if ["content", "article", "post"]
        .iter()
        .any(|s| class.contains(s))
    {
        score += 15.0;
    }
    if !select(element, FOOTNOTE_INLINE_REFERENCES).is_empty() {
        score += 10.0;
    }
    if !select(element, FOOTNOTE_LIST_SELECTORS).is_empty() {
        score += 10.0;
    }
    score -= select(element, "table").len() as f64 * 5.0;
    if tag(element) == "td"
        && let Some(table) = closest(element, "table")
    {
        let class = attribute(table, "class").to_lowercase();
        if parse_number(&attribute(table, "width")) > 400.0
            || attribute(table, "align") == "center"
            || class.contains("content")
            || class.contains("article")
        {
            let cells = select(table, "td");
            if let Some(index) = cells.iter().position(|cell| cell.id == element.id)
                && index > 0
                && index + 1 < cells.len()
            {
                score += 10.0;
            }
        }
    }
    let link_length: usize = select(element, "a")
        .iter()
        .map(|a| utf16_len(&a.text()))
        .sum();
    let density = (link_length as f64 / utf16_len(&text).max(1) as f64).min(0.5);
    score * (1.0 - density)
}

pub fn find_main<'a>(doc: &'a Document, selector: Option<&str>) -> Option<NodeRef<'a>> {
    if let Some(selector) = selector
        && let Some(node) = select(doc.root(), selector).first().copied()
    {
        return Some(expand_extractor(node));
    }
    let mut candidates = Vec::new();
    for (index, selector) in ENTRY_POINT_ELEMENTS.iter().enumerate() {
        for node in select(doc.root(), selector) {
            let value = (ENTRY_POINT_ELEMENTS.len() - index) as f64 * 40.0 + score(node);
            candidates.push((node, value, index));
        }
    }
    if candidates.is_empty() {
        return select(doc.root(), BLOCKS)
            .into_iter()
            .map(|node| (node, score(node)))
            .filter(|(_, score)| *score > 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(node, _)| node);
    }
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    if candidates.len() == 1
        && tag(candidates[0].0) == "body"
        && let Some(node) = table_content(doc)
    {
        return Some(node);
    }
    let top = candidates[0];
    let mut best = top;
    for child in candidates.iter().skip(1) {
        if child.2 < best.2 && contains(best.0, child.0) && count_words(&child.0.text()) > 50 {
            let sibling_count = candidates
                .iter()
                .filter(|c| c.2 == child.2 && contains(top.0, c.0))
                .take(2)
                .count();
            if sibling_count <= 1 {
                best = *child;
            }
        }
    }
    Some(expand_extractor(best.0))
}

fn expand_extractor(node: NodeRef<'_>) -> NodeRef<'_> {
    closest(node, "[data-defuddle]").unwrap_or(node)
}

fn table_content(doc: &Document) -> Option<NodeRef<'_>> {
    let layout = select(doc.root(), "table").into_iter().any(|table| {
        let class = attribute(table, "class").to_lowercase();
        parse_number(&attribute(table, "width")) > 400.0
            || attribute(table, "align") == "center"
            || class.contains("content")
            || class.contains("article")
            || select(table, "tr").into_iter().any(|row| {
                let cells: Vec<_> = row
                    .element_children()
                    .into_iter()
                    .filter(|n| tag(*n) == "td")
                    .collect();
                cells.len() >= 2 && cells.iter().any(|n| n.has_attr("width"))
            })
    });
    if !layout {
        return None;
    }
    let mut best = None;
    let mut best_score = 50.0;
    for cell in select(doc.root(), "td") {
        let score = score(cell);
        if score > best_score {
            best = Some(cell);
            best_score = score;
        }
    }
    let cell = best?;
    let body_words = doc
        .tree
        .body()
        .map(|body| count_words(&body.text()))
        .unwrap_or(0);
    (count_words(&cell.text()) * 2 >= body_words).then_some(cell)
}

pub fn is_card_grid(element: NodeRef<'_>, words: usize) -> bool {
    if !(3..500).contains(&words) {
        return false;
    }
    let headings = select(element, "h2,h3,h4");
    if headings.len() < 3 || select(element, "img").len() < 2 {
        return false;
    }
    let heading_words: usize = headings.iter().map(|h| count_words(&h.text())).sum();
    words.saturating_sub(heading_words) as f64 / (headings.len() as f64) < 20.0
}

fn has_social_profile(element: NodeRef<'_>) -> bool {
    static PROFILES: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i-u)(?:\b)(?:linkedin\.com/(?:in|company)/|(?:instagram\.com|threads\.net)/[A-Za-z0-9_]|mastodon\.[A-Za-z0-9_])").unwrap()
    });
    static HANDLES: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i-u)(?:\b)(twitter\.com|x\.com|facebook\.com)/([A-Za-z0-9_]+)").unwrap()
    });
    select(element, "a[href]").into_iter().any(|a| {
        let href = attribute(a, "href");
        PROFILES.is_match(&href)
            || HANDLES.captures_iter(&href).any(|c| {
                let excluded = if c[1].eq_ignore_ascii_case("facebook.com") {
                    "share"
                } else {
                    "intent"
                };
                !c[2].eq_ignore_ascii_case(excluded)
            })
    })
}

pub fn likely_content(element: NodeRef<'_>) -> bool {
    if ["article", "main", "contentinfo"].contains(&attribute(element, "role").as_str()) {
        return true;
    }
    let attrs = format!(
        "{} {}",
        attribute(element, "class"),
        attribute(element, "id")
    )
    .to_lowercase();
    if CONTENT.iter().any(|indicator| attrs.contains(indicator)) {
        return true;
    }
    if !select(element, "pre,table,figure,picture").is_empty() {
        return true;
    }
    let text = element.text();
    let words = count_words(&text);
    if let Some(heading) = select(element, HEADINGS).first() {
        let heading_text = heading.text();
        if !heading_text.trim().is_empty()
            && heading_text.trim() == text.trim()
            && !NAV_RE.is_match(&heading_text)
            && !TOC.is_match(heading_text.trim())
        {
            return true;
        }
    }
    if words < 1000
        && select(element, HEADINGS)
            .iter()
            .any(|h| NAV_RE.is_match(h.text().trim()))
        && (words < 200 || select(element, "a").len() as f64 / words.max(1) as f64 > 0.2)
    {
        return false;
    }
    if is_card_grid(element, words) || (words < 80 && has_social_profile(element)) {
        return false;
    }
    let blocks = select(element, "p,li").len();
    if (words > 50 && blocks > 1) || words > 100 || (words > 30 && blocks > 0) {
        return true;
    }
    words >= 10
        && text.contains(['.', '?', '!'])
        && select(element, "a").len() as f64 / (words as f64) < 0.1
}

pub fn clutter_score(element: NodeRef<'_>) -> f64 {
    if closest(element, FOOTNOTE_LIST_SELECTORS).is_some()
        || !select(element, FOOTNOTE_LIST_SELECTORS).is_empty()
    {
        return 0.0;
    }
    let text = element.text();
    let words = count_words(&text);
    if words < 3 {
        return 0.0;
    }
    let mut score =
        text.matches(',').count() as f64 - NAV_WORDS.matches(&text).iter().count() as f64 * 10.0;
    let links = select(element, "a");
    if links.len() as f64 / words.max(1) as f64 > 0.5 {
        score -= 15.0;
    }
    if links.len() > 1 && words < 80 {
        let link_text: usize = links.iter().map(|a| utf16_len(&a.text())).sum();
        if !text.is_empty() && link_text as f64 / utf16_len(&text).max(1) as f64 > 0.8 {
            score -= 15.0;
        }
    }
    let lists = select(element, "ul,ol").len();
    if lists > 0 && links.len() > lists * 3 {
        score -= 10.0;
    }
    if words < 80 && has_social_profile(element) {
        score -= 15.0;
    }
    if words < 15 && BYLINE.is_match(&text) && DATE.is_match(&text) {
        score -= 10.0;
    }
    if is_card_grid(element, words) {
        score -= 15.0;
    }
    let class = attribute(element, "class").to_lowercase();
    let id = attribute(element, "id").to_lowercase();
    score
        - NEGATIVE
            .iter()
            .filter(|pattern| class.contains(**pattern) || id.contains(**pattern))
            .count() as f64
            * 8.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_and_dates_use_javascript_ascii_word_boundaries() {
        assert!(NAV_WORDS.is_match("éshare漢"));
        assert!(NAV_WORDS.is_match("漢sign upé"));
        assert!(!NAV_WORDS.is_match("reshare"));
        assert!(!NAV_WORDS.is_match("share_again"));
        assert!(CONTENT_DATE.is_match("éMarch 3, 2024漢"));
        assert!(!CONTENT_DATE.is_match("_March 3, 2024_"));
    }

    #[test]
    fn social_profile_handles_and_intent_boundaries_match_source_rules() {
        for href in [
            "https://linkedin.com/in/person",
            "https://linkedin.com/company/example",
            "https://twitter.com/person",
            "https://x.com/person",
            "https://facebook.com/person",
            "https://instagram.com/person",
            "https://threads.net/person",
            "https://mastodon.social/@person",
            "https://twitter.com/intentional",
            "https://x.com/intent_person",
            "https://facebook.com/sharedname",
            "https://x.com/intent https://x.com/person",
        ] {
            let doc = Document::from(format!("<div><a href='{href}'>Profile</a></div>"));
            assert!(
                has_social_profile(doc.select_single("div").nodes()[0]),
                "{href}"
            );
        }
        for href in [
            "https://www.dropbox.com/s/example",
            "https://netflix.com/title/example",
            "https://vox.com/story",
            "https://joinmastodon.org/",
            "https://mastodon./",
            "https://notlinkedin.com/in/person",
            "https://unrelated.example.test/person",
            "https://x.com/intent/post",
            "https://twitter.com/intent?text=message",
            "https://facebook.com/share.php?url=example",
            "https://instagram.com/",
            "https://instagram.com/?next=person",
            "https://threads.net/",
            "https://twitter.com/é",
            "https://x.com.attacker.test/person",
        ] {
            let doc = Document::from(format!("<div><a href='{href}'>Source</a></div>"));
            assert!(
                !has_social_profile(doc.select_single("div").nodes()[0]),
                "{href}"
            );
        }
    }
}
