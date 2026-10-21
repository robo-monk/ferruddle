//! Offline adapters for content present in the supplied DOM or embedded JSON.
use crate::{IncludeReplies, Metadata, Options};
use dom_query::{Document, NodeRef};
use regex::Regex;
use std::sync::LazyLock;
use url::Url;

#[path = "extractors/comments.rs"]
mod comments;
#[path = "extractors/conversations.rs"]
mod conversations;
#[path = "extractors/embedded.rs"]
mod embedded;
#[path = "extractors/rich_social.rs"]
mod rich_social;
#[path = "extractors/social.rs"]
mod social;
#[path = "extractors/video.rs"]
mod video;

macro_rules! re {
    ($pattern:expr) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).unwrap());
        &*RE
    }};
}
pub(crate) use re;

#[derive(Clone, Debug)]
pub struct Extracted {
    pub content: Content,
    pub extractor_type: String,
    pub metadata: Metadata,
}

/// Adapter output is either detached HTML or a selector into the mutated source.
#[derive(Clone, Debug)]
pub enum Content {
    Html(String),
    Selector(String),
}

/// Detect adapters in registry order. This API never fetches or executes scripts.
/// Adapters may materialize embedded data into `doc`; callers must finish all
/// adapter mutations before retaining node IDs for extraction attempts.
pub fn extract(
    doc: &mut Document,
    page_url: Option<&str>,
    options: &Options,
    metadata: &Metadata,
) -> Option<Extracted> {
    let mut result = extract_inner(doc, page_url, options, metadata)?;
    // Source adapters override response fields only with usable nonempty values.
    // image is taken from page metadata by buildExtractorResponse even when a
    // video adapter exposes a separate image variable.
    for (value, original) in [
        (&mut result.metadata.title, &metadata.title),
        (&mut result.metadata.description, &metadata.description),
        (&mut result.metadata.author, &metadata.author),
        (&mut result.metadata.published, &metadata.published),
        (&mut result.metadata.site, &metadata.site),
        (&mut result.metadata.language, &metadata.language),
    ] {
        if value.is_empty() {
            *value = original.clone();
        }
    }
    result.metadata.image = metadata.image.clone();
    Some(result)
}

fn extract_inner(
    doc: &mut Document,
    page_url: Option<&str>,
    options: &Options,
    metadata: &Metadata,
) -> Option<Extracted> {
    let url = page_url?;
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str().unwrap_or_default();
    if host.contains("x.com") || host.contains("twitter.com") {
        if let Some(out) = social::x_article(doc, url, metadata) {
            return Some(out);
        }
        if let Some(out) = social::twitter(doc, options, metadata) {
            return Some(out);
        }
    }
    if host.contains("mail.google.com")
        && let Some(out) = comments::gmail(doc, options, metadata)
    {
        return Some(out);
    }
    if host.contains("reddit.com")
        && let Some(out) = comments::reddit(doc, url, options, metadata)
    {
        return Some(out);
    }
    if host.contains("youtube.com") || host.contains("youtu.be") {
        return Some(video::youtube(doc, &parsed, metadata));
    }
    if host.contains("bilibili.com")
        && let Some(out) = video::bilibili(doc, &parsed, metadata)
    {
        return Some(out);
    }
    if host.contains("news.ycombinator.com")
        && let Some(out) = comments::hackernews(doc, options, metadata)
    {
        return Some(out);
    }
    if ((host == "chatgpt.com" && re!(r"^/(?:c|share)/").is_match(parsed.path()))
        || host.contains("claude.ai")
        || host == "grok.com"
        || host == "gemini.google.com")
        && let Some(out) = conversations::extract(doc, host, metadata)
    {
        return Some(out);
    }
    if host.contains("github.com")
        && let Some(out) = comments::github(doc, url, options, metadata)
    {
        return Some(out);
    }
    if host.contains("linkedin.com")
        && let Some(out) = rich_social::linkedin(doc, options, metadata)
    {
        return Some(out);
    }
    if (host.contains("threads.net") || host.contains("threads.com"))
        && let Some(out) = rich_social::threads(doc, options, metadata)
    {
        return Some(out);
    }
    if host.contains("bsky.app")
        && let Some(out) = rich_social::bluesky(doc, options, metadata)
    {
        return Some(out);
    }
    if host.contains("substack.com")
        && let Some(out) = embedded::substack(doc, metadata)
    {
        return Some(out);
    }
    if re!(r"/@[^/]+/\d+").is_match(url)
        && let Some(out) = social::mastodon(doc, &parsed, options, metadata)
    {
        return Some(out);
    }
    if host.contains("leetcode.com")
        && q(doc.root(), "[data-track-load=description_content]").is_some()
    {
        let mut metadata = metadata.clone();
        metadata.title = re!(r"\s*[-–—]\s*LeetCode\s*$")
            .replace(
                &a(q(doc.root(), "meta[property='og:title']"), "content"),
                "",
            )
            .to_string();
        metadata.site = "LeetCode".to_string();
        return Some(selected(
            "LeetCodeExtractor",
            "[data-track-load=description_content]",
            metadata,
        ));
    }
    if re!(r"/t/[^/]+/\d+").is_match(url)
        && let Some(out) = comments::discourse(doc, &parsed, options, metadata)
    {
        return Some(out);
    }
    if host.contains("lwn.net")
        && let Some(out) = comments::lwn(doc, options, metadata)
    {
        return Some(out);
    }
    if let Some(out) = embedded::other(doc, url, host, options, metadata) {
        return Some(out);
    }
    embedded::bbcode_data(doc, metadata)
}
fn direct(kind: &str, html: String, metadata: Metadata) -> Extracted {
    Extracted {
        content: Content::Html(html),
        extractor_type: kind.trim_end_matches("Extractor").to_lowercase(),
        metadata,
    }
}
fn selected(kind: &str, selector: &str, metadata: Metadata) -> Extracted {
    Extracted {
        content: Content::Selector(selector.into()),
        extractor_type: kind.trim_end_matches("Extractor").to_lowercase(),
        metadata,
    }
}
fn replies(options: &Options) -> bool {
    options.include_replies != IncludeReplies::None
}
fn nodes<'a>(root: NodeRef<'a>, selector: &str) -> Vec<NodeRef<'a>> {
    crate::dom::select(root, selector)
}
fn q<'a>(root: NodeRef<'a>, selector: &str) -> Option<NodeRef<'a>> {
    nodes(root, selector).into_iter().next()
}
fn a(node: Option<NodeRef<'_>>, key: &str) -> String {
    node.and_then(|n| n.attr(key))
        .map(|s| s.to_string())
        .unwrap_or_default()
}
fn t(node: Option<NodeRef<'_>>) -> String {
    node.map(|n| n.text().trim().to_string())
        .unwrap_or_default()
}
fn h(node: Option<NodeRef<'_>>) -> String {
    node.map(|n| n.inner_html().to_string()).unwrap_or_default()
}
fn outer(node: Option<NodeRef<'_>>) -> String {
    node.map(|n| n.html().to_string()).unwrap_or_default()
}
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn preview(s: &str) -> String {
    normalize(&s.chars().take(140).collect::<String>())
}
fn html_preview(s: &str) -> String {
    preview(Document::fragment(s).text().trim())
}
fn fallback(s: String, default: &str) -> String {
    if s.is_empty() { default.into() } else { s }
}
fn date(s: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}
fn locale_date(s: &str) -> String {
    // Gmail uses English abbreviated/full month names and local calendar parts.
    let Some(c) = re!(r"(?i)^\s*([A-Za-z]+)\s+(\d{1,2}),?\s+(\d{4})(?:,?\s+\d{1,2}:\d{2}(?::\d{2})?\s*(?:AM|PM)?)?\s*$").captures(s) else { return String::new() };
    let month = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ]
    .iter()
    .position(|m| c[1].to_lowercase().starts_with(*m))
    .map(|m| m + 1);
    month
        .and_then(|m| {
            chrono::NaiveDate::from_ymd_opt(c[3].parse().ok()?, m as u32, c[2].parse().ok()?)
        })
        .map(|d| d.to_string())
        .unwrap_or_default()
}
fn link(url: &str, label: &str) -> String {
    if crate::safety::is_dangerous_url(url, false) {
        esc(label)
    } else {
        format!("<a href=\"{}\">{}</a>", esc(url), esc(label))
    }
}
fn image(src: &str, alt: &str) -> String {
    if crate::safety::is_dangerous_url(src, true) {
        String::new()
    } else {
        format!("<img src=\"{}\" alt=\"{}\">", esc(src), esc(alt))
    }
}
#[derive(Default)]
struct Comment {
    author: String,
    date: String,
    content: String,
    depth: usize,
    score: String,
    url: String,
}
fn comment(c: &Comment) -> String {
    let date = if !c.url.is_empty() && !crate::safety::is_dangerous_url(&c.url, false) {
        format!(
            "<a href=\"{}\" class=\"comment-link\">{}</a>",
            esc(&c.url),
            esc(&c.date)
        )
    } else {
        format!("<span class=\"comment-date\">{}</span>", esc(&c.date))
    };
    let score = if c.score.is_empty() {
        String::new()
    } else {
        format!(" · <span class=\"comment-points\">{}</span>", esc(&c.score))
    };
    format!(
        "<div class=\"comment\"><div class=\"comment-metadata\"><span class=\"comment-author\"><strong>{}</strong></span> · {date}{score}</div><div class=\"comment-content\">{}</div></div>",
        esc(&c.author),
        c.content
    )
}
fn comment_tree(comments: &[Comment]) -> String {
    let mut out = String::new();
    let mut stack = Vec::new();
    for c in comments {
        if c.depth == 0 {
            while stack.pop().is_some() {
                out.push_str("</blockquote>");
            }
            out.push_str("<blockquote>");
            stack.push(0);
        } else {
            if stack.last().is_some_and(|d| c.depth < *d) {
                while stack.last().is_some_and(|d| *d >= c.depth) {
                    out.push_str("</blockquote>");
                    stack.pop();
                }
            }
            if stack.last().is_none_or(|d| c.depth > *d) {
                out.push_str("<blockquote>");
                stack.push(c.depth);
            }
        }
        out.push_str(&comment(c));
    }
    while stack.pop().is_some() {
        out.push_str("</blockquote>");
    }
    out
}
fn post(site: &str, body: &str, comments: &[Comment]) -> String {
    let comments = comment_tree(comments);
    let tail = if comments.is_empty() {
        String::new()
    } else {
        format!("<hr><div class=\"{site} comments\"><h2>Comments</h2>{comments}</div>")
    };
    format!(
        "<article data-defuddle><div class=\"{site} post\"><div class=\"post-content\">{body}</div></div>{tail}</article>"
    )
}
fn cleaned_html(node: NodeRef<'_>, remove: &str) -> String {
    // Clone nodes directly: reparsing a <td>/<tr> outside a table discards its
    // wrapper, which would mistake the first paragraph for the whole body.
    let doc = Document::fragment("");
    let root = crate::dom::clone_into(node, &doc.tree);
    doc.root().append_child(&root);
    for n in nodes(root, remove) {
        n.remove_from_parent();
    }
    h(Some(root)).trim().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsuccessful_adapters_do_not_mutate_the_shared_source() {
        for (host, html) in [
            (
                "sample.substack.com",
                "<article><p>Ordinary article.</p></article>",
            ),
            (
                "www.nytimes.com",
                "<script>window.__preloadedData = {\"initialData\":{\"data\":{\"article\":{\"body\":{\"content\":[]}}}}};</script><article>Ordinary article.</article>",
            ),
            (
                "medium.com",
                "<article><p>Ordinary article without Medium markers.</p></article>",
            ),
            (
                "chatgpt.com",
                "<article><p>No conversation markers.</p></article>",
            ),
            (
                "github.com",
                "<article><p>No discussion markers.</p></article>",
            ),
        ] {
            let mut doc = Document::from(html);
            let before = doc.html();
            let url = format!("https://{host}/share/example");
            assert!(
                extract(
                    &mut doc,
                    Some(&url),
                    &Options::default(),
                    &Metadata::default()
                )
                .is_none(),
                "{host}"
            );
            assert_eq!(doc.html(), before, "{host}");
        }
    }
}
