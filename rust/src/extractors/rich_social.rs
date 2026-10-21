use super::*;
use serde_json::Value;
fn quoted(author: &str, date: &str, content: &str, url: &str) -> String {
    format!(
        "<blockquote class=\"quoted-post\">{}{}{}{}</blockquote>",
        if author.is_empty() {
            String::new()
        } else {
            format!("<p><strong>{}</strong>", esc(author))
        },
        if author.is_empty() {
            String::new()
        } else if date.is_empty() {
            "</p>".into()
        } else {
            format!(" · {}</p>", esc(date))
        },
        content,
        if url.is_empty() {
            String::new()
        } else {
            format!("<p>{}</p>", link(url, url))
        }
    )
}
fn cleaned_text(node: NodeRef<'_>, remove: &str) -> String {
    Document::fragment(cleaned_html(node, remove))
        .text()
        .trim()
        .into()
}
fn simple_text(node: NodeRef<'_>, mode: &str) -> String {
    let fragment = Document::fragment(cleaned_html(
        node,
        ".visually-hidden,.feed-shared-inline-show-more-text__see-more-less-toggle",
    ));
    for a_node in nodes(fragment.root(), "a") {
        let text = t(Some(a_node));
        let mut href = a(Some(a_node), "href");
        if mode == "bluesky" {
            if href.contains("/profile/") && text.starts_with('@') {
                href = format!("https://bsky.app/profile/{}", text.trim_start_matches('@'));
            } else if href.starts_with("/profile/") {
                href = format!("https://bsky.app{href}");
            }
        }
        let html = link(&href, &text);
        for n in crate::dom::append_fragment(fragment.root(), &html) {
            a_node.insert_before(&n);
        }
        a_node.remove_from_parent();
    }
    for n in nodes(fragment.root(), "span,div") {
        crate::dom::unwrap(n);
    }
    let html = fragment
        .html()
        .replace(['\u{200e}', '\u{200f}', '\u{200b}'], "");
    let html = re!(r"(?s)<!--.*?-->").replace_all(&html, "");
    if mode == "bluesky" {
        html.split('\n')
            .map(normalize)
            .filter(|p| !p.is_empty())
            .map(|p| format!("<p>{p}</p>"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        re!(r"(?:<br\s*/?>\s*){2,}|\n{2,}")
            .split(&html)
            .map(|p| normalize(&re!(r"<br\s*/?>").replace_all(p, " ")))
            .filter(|p| !p.is_empty())
            .map(|p| format!("<p>{p}</p>"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
pub(super) fn linkedin(
    doc: &Document,
    options: &Options,
    metadata: &Metadata,
) -> Option<Extracted> {
    let article = q(doc.root(), "[role=article].feed-shared-update-v2")?;
    let quoted_wrapper = q(article, ".feed-shared-update-v2__update-content-wrapper");
    let text = q(
        article,
        ".update-components-text.update-components-update-v2__commentary",
    )
    .filter(|n| quoted_wrapper.is_none_or(|w| !crate::utils::contains(w, *n)));
    let mut body = text.map(|n| simple_text(n, "linkedin")).unwrap_or_default();
    for img in nodes(
        article,
        ".update-components-image img,.feed-shared-image img",
    ) {
        let src = a(Some(img), "src");
        if !src.is_empty() && !src.contains("profile-displayphoto") && !src.contains("avm-avatar") {
            body.push_str(&image(&src, &a(Some(img), "alt")));
        }
    }
    if let Some(video) = q(article, ".update-components-linkedin-video video[poster]") {
        body.push_str(&image(&a(Some(video), "poster"), "Video thumbnail"));
    }
    if let Some(wrapper) = quoted_wrapper {
        let author=q(wrapper,".update-components-actor__title").map(|n|cleaned_text(n,".visually-hidden,.update-components-actor__supplementary-actor-info,.text-view-model__verified-icon")).unwrap_or_default();
        let sub = q(wrapper, ".update-components-actor__sub-description");
        let raw = t(sub.and_then(|n| q(n, "[aria-hidden=true]")).or(sub));
        let date = re!(r"^(\d+\w+)")
            .captures(&raw)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let content = q(
            wrapper,
            ".update-components-text.update-components-update-v2__commentary",
        )
        .map(|n| simple_text(n, "linkedin"))
        .unwrap_or_default();
        let url = a(
            q(
                wrapper,
                "a.update-components-mini-update-v2__link-to-details-page",
            ),
            "href",
        );
        let url = absolute(&url, "https://www.linkedin.com")
            .split('?')
            .next()
            .unwrap_or_default()
            .to_string();
        body.push_str(&quoted(&author, &date, &content, &url));
    }
    let mut comments = Vec::new();
    if replies(options) {
        for row in nodes(
            article,
            "article.comments-comment-entity:not(.comments-comment-entity--reply)",
        ) {
            for (row, depth) in std::iter::once((row, 0)).chain(
                nodes(
                    row,
                    ".comments-replies-list article.comments-comment-entity--reply",
                )
                .into_iter()
                .map(|n| (n, 1)),
            ) {
                let author = t(q(row, ".comments-comment-meta__description-title"));
                if author.is_empty() {
                    continue;
                }
                let content = q(
                    row,
                    ".comments-comment-entity__content .update-components-text",
                )
                .map(|n| simple_text(n, "linkedin"))
                .unwrap_or_default();
                let date = t(q(row, "time.comments-comment-meta__data"));
                let profile = a(
                    q(row, "a.comments-comment-meta__description-container"),
                    "href",
                );
                let profile = profile.split('?').next().unwrap_or_default();
                let reactions = t(q(
                    row,
                    ".comments-comment-social-bar__reactions-count--cr span.v-align-middle",
                ));
                comments.push(Comment {
                    author,
                    date,
                    content,
                    depth,
                    url: absolute(profile, "https://www.linkedin.com"),
                    score: if reactions.is_empty() {
                        String::new()
                    } else {
                        format!("{reactions} reactions")
                    },
                });
            }
        }
    }
    let mut meta = metadata.clone();
    meta.author=q(article,".update-components-actor__title").map(|n|cleaned_text(n,".visually-hidden,.text-view-model__verified-icon,.update-components-actor__supplementary-actor-info")).unwrap_or_default();
    meta.title = format!("Post by {} on LinkedIn", meta.author);
    meta.site = "LinkedIn".into();
    meta.description = text
        .map(|n| preview(&cleaned_text(n, ".visually-hidden")))
        .unwrap_or_default();
    Some(direct(
        "LinkedInExtractor",
        post("linkedin", &body, &comments),
        meta,
    ))
}
fn absolute(url: &str, base: &str) -> String {
    if url.is_empty() {
        String::new()
    } else if url.starts_with("http") {
        url.into()
    } else {
        format!("{base}{url}")
    }
}
fn bs_handle(n: NodeRef<'_>) -> String {
    a(Some(n), "data-testid")
        .trim_start_matches("postThreadItem-by-")
        .into()
}
fn bs_name(n: NodeRef<'_>) -> String {
    let label = a(q(n, "a[aria-label*=avatar]"), "aria-label");
    if let Some(c) = re!(r"^(.+)'s avatar$").captures(&label) {
        return c[1].to_string();
    }
    nodes(n, "a[href^='/profile/']")
        .iter()
        .map(|n| t(Some(*n)))
        .find(|s| !s.is_empty() && !s.starts_with('@') && !s.contains("avatar") && !s.contains('·'))
        .unwrap_or_default()
}
fn bs_content(item: NodeRef<'_>, include_quote: bool) -> String {
    let mut html = q(item, "div[data-word-wrap='1']")
        .map(|n| simple_text(n, "bluesky"))
        .unwrap_or_default();
    for img in nodes(
        item,
        "img[src*='/feed_thumbnail/'],img[src*='/feed_fullsize/']",
    ) {
        let src = a(Some(img), "src");
        if !src.is_empty() {
            html.push_str(&image(
                &src.replace("/feed_thumbnail/", "/feed_fullsize/"),
                "",
            ));
        }
    }
    for card in nodes(item, "a[aria-label][href^=http]") {
        if q(card, "div[style*=border]").is_none() {
            continue;
        }
        let href = a(Some(card), "href");
        let title = a(Some(card), "aria-label");
        if title.is_empty() {
            continue;
        }
        if let Some(img) = q(card, "img") {
            html.push_str(&format!(
                "<a href=\"{}\">{}</a>",
                esc(&href),
                image(&a(Some(img), "src"), &title)
            ));
        }
        html.push_str(&format!("<p>{}</p>", link(&href, &title)));
        break;
    }
    if include_quote && let Some(quote) = q(item, "[data-testid^=postThreadItem-by-]") {
        let handle = bs_handle(quote);
        let name = bs_name(quote);
        let author = if name.is_empty() {
            format!("@{handle}")
        } else {
            format!("{name} @{handle}")
        };
        let text = q(quote, "div[data-word-wrap='1']")
            .map(|n| simple_text(n, "bluesky"))
            .unwrap_or_default();
        html.push_str(&quoted(&author, "", &text, ""));
    }
    html
}
pub(super) fn bluesky(doc: &Document, options: &Options, metadata: &Metadata) -> Option<Extracted> {
    let screen = q(doc.root(), "[data-testid=postThreadScreen]")?;
    let items = nodes(screen, "[data-testid^=postThreadItem-by-]");
    let main = *items.first()?;
    let main_handle = bs_handle(main);
    let mut ended = false;
    let mut parts = Vec::new();
    let mut comments = Vec::new();
    let mut depth = 0;
    for item in items {
        let handle = bs_handle(item);
        if !ended && handle == main_handle {
            parts.push(bs_content(item, true));
        } else {
            ended = true;
            if replies(options) {
                let connector = item.element_children().first().copied();
                if connector.is_some_and(|c| {
                    nodes(c, "div").iter().any(|n| {
                        let style = a(Some(*n), "style");
                        style.contains("width: 2px") && style.contains("background-color")
                    })
                }) {
                    depth += 1;
                } else {
                    depth = 0;
                }
                let name = bs_name(item);
                let time = q(item, "a[href*='/post/']");
                comments.push(Comment {
                    author: if name.is_empty() {
                        format!("@{handle}")
                    } else {
                        format!("{name} @{handle}")
                    },
                    date: locale_date(&a(time, "aria-label").replace(" at ", " ")),
                    url: absolute(&a(time, "href"), "https://bsky.app"),
                    depth,
                    content: bs_content(item, true),
                    ..Comment::default()
                });
            }
        }
    }
    let mut meta = metadata.clone();
    meta.author = fallback(bs_name(main), &format!("@{main_handle}"));
    meta.title = format!("Post by {} on Bluesky", meta.author);
    meta.site = "Bluesky".into();
    meta.description = preview(
        &t(q(main, "div[data-word-wrap='1']")).replace(['\u{200e}', '\u{200f}', '\u{200b}'], ""),
    );
    let published = date(&a(q(doc.root(), "meta[name='twitter:value1']"), "content"));
    if !published.is_empty() {
        meta.published = published;
    }
    Some(direct(
        "BlueskyExtractor",
        post("bluesky", &parts.join("\n<hr>\n"), &comments),
        meta,
    ))
}
fn th_user(n: NodeRef<'_>) -> String {
    for link in nodes(n, "a[href^='/@'][role=link]") {
        let text = t(Some(link));
        if !text.is_empty() && !text.contains("profile picture") {
            return text;
        }
    }
    re!(r"/@([^/]+)")
        .captures(&a(q(n, "a[href^='/@']"), "href"))
        .map(|c| c[1].into())
        .unwrap_or_default()
}
fn th_redirect(href: &str) -> String {
    Url::parse(href)
        .ok()
        .and_then(|u| {
            u.query_pairs()
                .find(|(k, _)| k == "u")
                .map(|(_, v)| v.to_string())
        })
        .unwrap_or_else(|| href.into())
}
fn th_span(span: NodeRef<'_>) -> String {
    let fragment = Document::fragment(h(Some(span)));
    for n in nodes(fragment.root(), "div") {
        if re!(r"^\d+/\d+$").is_match(&t(Some(n))) && nodes(n, "span").len() >= 2 {
            n.remove_from_parent();
        }
    }
    for n in nodes(fragment.root(), "a") {
        let href = a(Some(n), "href");
        if re!(r"/@[\w.]+/post/").is_match(&href) {
            n.remove_from_parent();
            continue;
        }
        let text = t(Some(n));
        let (href, text) = if href.contains("l.threads.com") {
            (th_redirect(&href), text)
        } else if let Some(user) = href.strip_prefix("/@") {
            (
                format!("https://www.threads.com/@{user}"),
                format!("@{user}"),
            )
        } else {
            (absolute(&href, "https://www.threads.com"), text)
        };
        for x in crate::dom::append_fragment(fragment.root(), &link(&href, &text)) {
            n.insert_before(&x);
        }
        n.remove_from_parent();
    }
    for n in nodes(fragment.root(), "span,div") {
        crate::dom::unwrap(n);
    }
    normalize(&re!(r"(?s)<!--.*?-->").replace_all(&fragment.html(), ""))
}
fn th_content(container: NodeRef<'_>, quote_mode: bool) -> String {
    let mut parts = Vec::new();
    for span in nodes(container, "span[dir=auto]") {
        if crate::dom::closest(span, "[role=button],time").is_some() {
            continue;
        }
        let link = crate::dom::closest(
            span,
            "a[href^='/@'],a[href*='/post/'],a[href*='l.threads.com']",
        );
        if link.is_some_and(|n| !quote_mode || !a(Some(n), "href").contains("/post/")) {
            continue;
        }
        let text = t(Some(span));
        if text.is_empty()
            || ["Author", "·", "Top", "View activity"].contains(&text.as_str())
            || re!(r"^\d{2}/\d{2}/\d{2}$|^@?\w+/post/\w+$").is_match(&text)
        {
            continue;
        }
        if re!(r"\s*\d+\s*/\s*\d+\s*$")
            .replace(&text, "")
            .trim()
            .is_empty()
        {
            continue;
        }
        let html = if quote_mode {
            esc(&re!(r"\s*\d+\s*/\s*\d+\s*$").replace(&text, ""))
        } else {
            th_span(span)
        };
        if !html.is_empty() {
            parts.push(format!("<p>{html}</p>"));
        }
    }
    if !quote_mode {
        for img in nodes(container, "img") {
            let src = a(Some(img), "src");
            let alt = a(Some(img), "alt");
            let width = a(Some(img), "width").parse::<usize>().unwrap_or(0);
            if src.is_empty()
                || alt.contains("profile picture")
                || crate::dom::closest(img, "a[href*='l.threads.com']").is_some()
                || (1..=48).contains(&width)
            {
                continue;
            }
            parts.push(image(&src, &alt));
        }
        for card in nodes(container, "a[href*='l.threads.com']") {
            if let Some(img) = q(card, "img") {
                parts.push(format!(
                    "<a href=\"{}\">{}</a>",
                    esc(&th_redirect(&a(Some(card), "href"))),
                    image(&a(Some(img), "src"), &a(Some(img), "alt"))
                ));
                break;
            }
        }
        if let Some(quote) = q(container, "[data-pressable-container]") {
            let author = th_user(quote);
            let author = if author.is_empty() {
                author
            } else {
                format!("@{author}")
            };
            parts.push(quoted(
                &author,
                &date(&a(q(quote, "time[datetime]"), "datetime")),
                &th_content(quote, true),
                "",
            ));
        } else {
            for link_node in nodes(container, "a[href*='/post/']") {
                let text = t(Some(link_node));
                if re!(r"^\d{2}/\d{2}/\d{2}$").is_match(&text) {
                    continue;
                }
                let href = a(Some(link_node), "href");
                if let Some(c) = re!(r"/@([^/]+)/post/").captures(&href) {
                    parts.push(quoted(
                        &format!("@{}", &c[1]),
                        "",
                        &format!("<p>{}</p>", esc(&text)),
                        &absolute(&href, "https://www.threads.com"),
                    ));
                    break;
                }
            }
        }
    }
    parts.join("\n")
}
fn json_posts(value: &Value, depth: usize, out: &mut Vec<(String, String)>) {
    if depth > 35 {
        return;
    }
    fn text(v: &Value, depth: usize) -> Option<String> {
        if depth > 10 {
            return None;
        }
        if let Some(f) = v
            .pointer("/text_fragments/fragments")
            .and_then(Value::as_array)
        {
            return Some(
                f.iter()
                    .map(|f| {
                        f.get("plaintext")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .or_else(|| {
                                f.pointer("/mention_fragment/username")
                                    .and_then(Value::as_str)
                                    .map(|s| format!("@{s}"))
                            })
                            .or_else(|| {
                                f.get("linkified_web_url")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned)
                            })
                            .unwrap_or_default()
                    })
                    .collect(),
            );
        }
        match v {
            Value::Object(o) => o
                .iter()
                .filter(|(k, _)| *k != "quoted_post")
                .find_map(|(_, v)| text(v, depth + 1)),
            Value::Array(a) => a.iter().find_map(|v| text(v, depth + 1)),
            _ => None,
        }
    }
    if let Some(user) = value.pointer("/user/username").and_then(Value::as_str)
        && let Some(text) = text(value, 0).filter(|s| !s.is_empty())
    {
        out.push((user.into(), text));
    }
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                if k != "quoted_post" {
                    json_posts(v, depth + 1, out);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                json_posts(v, depth + 1, out);
            }
        }
        _ => {}
    }
}
pub(super) fn threads(doc: &Document, options: &Options, metadata: &Metadata) -> Option<Extracted> {
    let pagelets: Vec<_> = nodes(doc.root(), "[data-pagelet^=threads_post_page_]")
        .into_iter()
        .filter(|n| q(*n, "a[href^='/@'],time[datetime]").is_some())
        .collect();
    let mut comments = Vec::new();
    let mut parts = Vec::new();
    let main;
    let author;
    if pagelets.is_empty() {
        main = q(doc.root(), "div[role=region]").filter(|n| q(*n, "a[href^='/@']").is_some())?;
        author = th_user(main);
        if author.is_empty() {
            return None;
        }
        parts.push(th_content(main, false));
        if replies(options) {
            let mut posts = Vec::new();
            for script in nodes(doc.root(), "script[type='application/json']") {
                let raw = script.text();
                if raw.matches("\"text_fragments\"").count() < 2 || !raw.contains("\"username\"") {
                    continue;
                }
                if let Ok(v) = serde_json::from_str(&raw) {
                    json_posts(&v, 0, &mut posts);
                }
            }
            let mut seen = std::collections::HashSet::new();
            posts.retain(|(u, t)| {
                seen.insert(format!("{u}:{}", t.chars().take(80).collect::<String>()))
            });
            if posts.len() >= 2 {
                let mut first = true;
                for (u, text) in posts {
                    if first && u == author {
                        first = false;
                        continue;
                    }
                    comments.push(Comment {
                        author: format!("@{u}"),
                        content: format!("<p>{}</p>", esc(&text)),
                        ..Comment::default()
                    });
                }
            }
        }
    } else {
        main = pagelets[0];
        author = th_user(main);
        let mut ended = false;
        for pagelet in pagelets {
            let containers: Vec<_> = nodes(pagelet, "[data-pressable-container]")
                .into_iter()
                .filter(|n| {
                    n.parent()
                        .and_then(|p| crate::dom::closest(p, "[data-pressable-container]"))
                        .is_none()
                        && !th_user(*n).is_empty()
                })
                .collect();
            if containers.is_empty() {
                continue;
            }
            if !ended && containers.len() == 1 && th_user(containers[0]) == author {
                parts.push(th_content(containers[0], false));
            } else {
                ended = true;
                if replies(options) {
                    for (i, n) in containers.into_iter().enumerate() {
                        comments.push(Comment {
                            author: format!("@{}", th_user(n)),
                            date: date(&a(q(n, "time[datetime]"), "datetime")),
                            url: absolute(
                                &a(q(n, "a[href*='/post/']"), "href"),
                                "https://www.threads.com",
                            ),
                            content: th_content(n, false),
                            depth: i,
                            ..Comment::default()
                        });
                    }
                }
            }
        }
    }
    let mut meta = metadata.clone();
    meta.author = format!("@{author}");
    meta.title = format!("Post by {} on Threads", meta.author);
    meta.site = "Threads".into();
    meta.description = html_preview(parts.first().map(String::as_str).unwrap_or_default());
    let published = date(&a(q(main, "time[datetime]"), "datetime"));
    if !published.is_empty() {
        meta.published = published;
    }
    Some(direct(
        "ThreadsExtractor",
        post("threads", &parts.join("\n<hr>\n"), &comments),
        meta,
    ))
}
