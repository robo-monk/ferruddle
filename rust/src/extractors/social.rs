use super::*;
fn upgrade(src: &str) -> String {
    if src.contains("&name=") {
        re!(r"&name=\w+").replace(src, "&name=large").into_owned()
    } else {
        format!(
            "{src}{}name=large",
            if src.contains('?') { "&" } else { "?" }
        )
    }
}
fn replace(node: NodeRef<'_>, html: &str) {
    for n in crate::dom::append_fragment(node.tree.root(), html) {
        node.insert_before(&n);
    }
    node.remove_from_parent();
}
fn emoji(root: NodeRef<'_>, selector: &str) {
    for img in nodes(root, selector) {
        let alt = a(Some(img), "alt");
        let text = img.tree.new_text(alt.as_str());
        img.insert_before(&text);
        img.remove_from_parent();
    }
}
pub(super) fn x_article(doc: &Document, url: &str, metadata: &Metadata) -> Option<Extracted> {
    let original = q(doc.root(), "[data-testid=twitterArticleRichTextView]")?;
    let fragment = Document::fragment(outer(Some(original)));
    let root = q(fragment.root(), "[data-testid=twitterArticleRichTextView]")?;
    for tweet in nodes(root, "[data-testid=simpleTweet]") {
        let links = q(tweet, "[data-testid=User-Name]")
            .map(|n| nodes(n, "a"))
            .unwrap_or_default();
        let full = t(links.first().copied());
        let handle = t(links.get(1).copied());
        let text = t(q(tweet, "[data-testid=tweetText]"));
        replace(
            tweet,
            &format!(
                "<blockquote>{}{}</blockquote>",
                if full.is_empty() && handle.is_empty() {
                    String::new()
                } else {
                    format!(
                        "<cite>{}</cite>",
                        esc(&if handle.is_empty() {
                            full
                        } else {
                            format!("{full} {handle}")
                        })
                    )
                },
                if text.is_empty() {
                    String::new()
                } else {
                    format!("<p>{}</p>", esc(&text))
                }
            ),
        );
    }
    for block in nodes(root, "[data-testid=markdown-code-block]") {
        let Some(code) = q(block, "code") else {
            continue;
        };
        if q(block, "pre").is_none() {
            continue;
        }
        let language = re!(r"language-(\w+)")
            .captures(&a(Some(code), "class"))
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| t(q(block, "span")));
        replace(
            block,
            &format!(
                "<pre><code{}>{}</code></pre>",
                if language.is_empty() {
                    String::new()
                } else {
                    format!(
                        " class=\"language-{}\" data-lang=\"{}\"",
                        esc(&language),
                        esc(&language)
                    )
                },
                esc(&code.text())
            ),
        );
    }
    for header in nodes(root, "h1,h2,h3,h4,h5,h6") {
        let text = t(Some(header));
        if text.is_empty() {
            continue;
        }
        let tag = crate::utils::tag(header);
        replace(header, &format!("<{tag}>{}</{tag}>", esc(&text)));
    }
    for img in nodes(root, "[data-testid=tweetPhoto] img") {
        let src = a(Some(img), "src");
        if src.is_empty() {
            continue;
        }
        if let Some(anchor) =
            crate::dom::closest(img, "a").filter(|n| crate::utils::contains(root, *n))
        {
            replace(
                anchor,
                &image(
                    &upgrade(&src),
                    &fallback(normalize(&a(Some(img), "alt")), "Image"),
                ),
            );
        } else {
            img.set_attr("src", &upgrade(&src));
        }
    }
    for span in nodes(root, "span[style*='font-weight: bold']") {
        replace(span, &format!("<strong>{}</strong>", esc(&span.text())));
    }
    fn draft(node: NodeRef<'_>, out: &mut String) {
        for child in node.children() {
            if child.is_text() {
                out.push_str(&esc(&child.text()));
            } else if child.is("strong,a,code") {
                let tag = crate::utils::tag(child);
                let attr = if tag == "a" {
                    format!(" href=\"{}\"", esc(&a(Some(child), "href")))
                } else {
                    String::new()
                };
                out.push_str(&format!("<{tag}{attr}>{}</{tag}>", esc(&child.text())));
            } else if child.is_element() {
                draft(child, out);
            }
        }
    }
    for div in nodes(root, ".longform-unstyled,.public-DraftStyleDefault-block") {
        let mut inner = String::new();
        draft(div, &mut inner);
        replace(div, &format!("<p>{inner}</p>"));
    }
    for node in nodes(root, "[data-offset-key]") {
        node.remove_attr("data-offset-key");
    }
    let header = q(doc.root(), "[data-testid=twitterArticleReadView]")
        .and_then(|r| q(r, "[data-testid=tweetPhoto] img"))
        .filter(|n| !crate::utils::contains(original, *n))
        .map(|n| {
            image(
                &upgrade(&a(Some(n), "src")),
                &fallback(normalize(&a(Some(n), "alt")), "Image"),
            )
        })
        .unwrap_or_default();
    let mut meta = metadata.clone();
    meta.title = fallback(
        t(q(doc.root(), "[data-testid=twitter-article-title]")),
        "Untitled X Article",
    );
    let author = q(doc.root(), "[itemprop=author]");
    let name = a(author.and_then(|n| q(n, "meta[itemprop=name]")), "content");
    let handle = a(
        author.and_then(|n| q(n, "meta[itemprop=additionalName]")),
        "content",
    );
    meta.author = if !name.is_empty() && !handle.is_empty() {
        format!("{name} (@{handle})")
    } else {
        fallback(name, &handle)
    };
    if meta.author.is_empty() {
        meta.author = re!(r"/([a-zA-Z0-9_][a-zA-Z0-9_]{0,14})/(?:article|status)/\d+")
            .captures(url)
            .map(|c| format!("@{}", &c[1]))
            .unwrap_or_else(|| {
                re!(r"^(?:\(\d+\)\s+)?(.+?)\s+on\s+X\s*:")
                    .captures(&a(q(doc.root(), "meta[property='og:title']"), "content"))
                    .map(|c| c[1].trim().into())
                    .unwrap_or_else(|| "Unknown".into())
            });
    }
    meta.site = "X (Twitter)".into();
    let text = t(Some(original));
    meta.description = format!(
        "{}{}",
        text.chars().take(140).collect::<String>(),
        if text.chars().count() > 140 {
            "..."
        } else {
            ""
        }
    );
    Some(direct(
        "XArticleExtractor",
        format!(
            "<article class=\"x-article\">{header}{}</article>",
            h(Some(root))
        ),
        meta,
    ))
}
fn handle(tweet: NodeRef<'_>) -> String {
    let Some(name) = q(tweet, "[data-testid=User-Name]") else {
        return String::new();
    };
    let links = nodes(name, "a");
    for link in &links {
        let text = t(Some(*link));
        if re!(r"^@\w{1,15}$").is_match(&text) {
            return text;
        }
    }
    for link in links {
        if let Some(c) =
            re!(r"^(?:https?://[^/]+)?/(\w{1,15})(?:/|$)").captures(&a(Some(link), "href"))
            && ![
                "i",
                "home",
                "explore",
                "search",
                "notifications",
                "messages",
                "settings",
                "compose",
                "hashtag",
                "intent",
            ]
            .contains(&c[1].to_lowercase().as_str())
        {
            return format!("@{}", &c[1]);
        }
    }
    re!(r"@(\w{1,15})")
        .captures(&name.text())
        .map(|c| format!("@{}", &c[1]))
        .unwrap_or_default()
}
fn user(tweet: NodeRef<'_>) -> (String, String, String, String) {
    let Some(name) = q(tweet, "[data-testid=User-Name]") else {
        return Default::default();
    };
    let handle = handle(tweet);
    let full = nodes(name, "a")
        .iter()
        .filter(|n| q(**n, "time").is_none())
        .map(|n| t(Some(*n)))
        .find(|s| !s.is_empty() && s != &handle && !s.starts_with('@'))
        .unwrap_or_else(|| {
            name.element_children()
                .first()
                .map(|n| t(Some(*n)))
                .filter(|s| !s.starts_with('@'))
                .unwrap_or_default()
        });
    let time = q(tweet, "time");
    let date = date(&a(time, "datetime"));
    let url = a(time.and_then(|n| crate::dom::closest(n, "a")), "href");
    (full, handle, date, url)
}
fn tweet_content(tweet: NodeRef<'_>, include_quote: bool) -> String {
    let fragment = Document::fragment(outer(Some(tweet)));
    let root = fragment.root();
    emoji(root, "img[src*='/emoji/']");
    let mut text = String::new();
    if let Some(body) = q(root, "[data-testid=tweetText]") {
        for a in nodes(body, "a") {
            replace(a, &esc(&t(Some(a))));
        }
        for n in nodes(body, "span,div") {
            crate::dom::unwrap(n);
        }
        text = h(Some(body))
            .split('\n')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("<p>{s}</p>"))
            .collect::<Vec<_>>()
            .join("\n");
    }
    let quoted = if include_quote {
        q(tweet, "[aria-labelledby*='id__']")
            .and_then(|n| q(n, "[data-testid=User-Name]"))
            .and_then(|n| crate::dom::closest(n, "[aria-labelledby*='id__']"))
    } else {
        None
    };
    for selector in [
        "[data-testid=tweetPhoto]",
        "[data-testid=tweet-image]",
        "img[src*=media]",
    ] {
        for img in nodes(tweet, selector) {
            if quoted.is_some_and(|q| crate::utils::contains(q, img)) {
                continue;
            }
            if img.is("img") && !a(Some(img), "alt").is_empty() {
                text.push_str(&image(
                    &re!(r"&name=\w+$").replace(&a(Some(img), "src"), "&name=large"),
                    &normalize(&a(Some(img), "alt")),
                ));
            }
        }
    }
    if let Some(card) = q(tweet, "[data-testid='card.wrapper']").and_then(|n| q(n, "a[href]")) {
        let url = a(Some(card), "href");
        let label = a(Some(card), "aria-label");
        let label = fallback(
            label.split('\n').next().unwrap_or_default().trim().into(),
            &url,
        );
        text.push_str(&format!("<p>{}</p>", link(&url, &label)));
    }
    if let Some(quoted) = quoted {
        let (full, handle, date, _) = user(quoted);
        let author = if full.is_empty() {
            handle
        } else {
            format!("{full} {handle}")
        };
        text.push_str(&format!(
            "<blockquote class=\"quoted-post\">{}{}{}</blockquote>",
            if author.is_empty() {
                String::new()
            } else {
                format!(
                    "<p><strong>{}</strong>{}</p>",
                    esc(&author),
                    if date.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", esc(&date))
                    }
                )
            },
            tweet_content(quoted, false),
            ""
        ));
    }
    text
}
pub(super) fn twitter(doc: &Document, options: &Options, metadata: &Metadata) -> Option<Extracted> {
    let mut cells = Vec::new();
    for n in nodes(doc.root(), "[data-testid=cellInnerDiv],section,h2") {
        if a(Some(n), "data-testid") == "cellInnerDiv" {
            cells.push(n);
        } else if !cells.is_empty()
            && crate::dom::closest(n, "article[data-testid=tweet]").is_none()
        {
            break;
        }
    }
    let mut main = None;
    let mut threads = Vec::new();
    let mut comments = Vec::new();
    let mut main_handle = String::new();
    let mut ended = false;
    let mut last = false;
    let mut depth = 0;
    for cell in cells {
        let Some(tweet) = q(cell, "article[data-testid=tweet]") else {
            last = false;
            continue;
        };
        if main.is_none() {
            main = Some(tweet);
            main_handle = handle(tweet);
            last = true;
            continue;
        }
        let h = handle(tweet);
        if !ended && !h.is_empty() && h == main_handle {
            threads.push(tweet);
            last = true;
            continue;
        }
        ended = true;
        depth = if last { depth + 1 } else { 0 };
        let (full, h, date, url) = user(tweet);
        if replies(options) {
            comments.push(Comment {
                author: if full.is_empty() {
                    h
                } else {
                    format!("{full} {h}")
                },
                date,
                url,
                depth,
                content: tweet_content(tweet, true),
                ..Comment::default()
            });
        }
        last = true;
    }
    let main = main.or_else(|| q(doc.root(), "article[data-testid=tweet]"))?;
    let mut parts = vec![tweet_content(main, true)];
    parts.extend(threads.iter().map(|n| tweet_content(*n, true)));
    let mut meta = metadata.clone();
    meta.author = fallback(handle(main), "@");
    meta.title = format!("Post by {} on X", meta.author);
    meta.site = "X (Twitter)".into();
    meta.description = preview(&t(q(main, "[data-testid=tweetText]")));
    Some(direct(
        "TwitterExtractor",
        post("twitter", &parts.join("\n<hr>\n"), &comments),
        meta,
    ))
}
fn mastodon_content(container: NodeRef<'_>) -> String {
    let mut parts = Vec::new();
    if let Some(text) = q(container, ".status__content .status__content__text") {
        let fragment = Document::fragment(outer(Some(text)));
        emoji(fragment.root(), "img.emojione");
        for n in nodes(fragment.root(), "span.invisible") {
            n.remove_from_parent();
        }
        for n in nodes(fragment.root(), "span") {
            crate::dom::unwrap(n);
        }
        parts.push(
            h(q(fragment.root(), ".status__content__text"))
                .trim()
                .into(),
        );
    }
    for n in nodes(container, ".media-gallery .media-gallery__item-thumbnail") {
        let src = a(Some(n), "href");
        if !src.is_empty() {
            parts.push(image(&src, &a(q(n, "img"), "alt")));
        }
    }
    if let Some(card) = q(container, "a.status-card[href]") {
        let href = a(Some(card), "href");
        let title = t(q(card, ".status-card__title"));
        let desc = t(q(card, ".status-card__description"));
        if !href.is_empty() || !title.is_empty() {
            let src = a(q(card, ".status-card__image-image"), "src");
            if !src.is_empty() {
                parts.push(format!(
                    "<a href=\"{}\">{}</a>",
                    esc(&href),
                    image(&src, &title)
                ));
            }
            parts.push(format!("<p>{}</p>", link(&href, &fallback(title, &href))));
            if !desc.is_empty() {
                parts.push(format!("<p>{}</p>", esc(&desc)));
            }
        }
    }
    parts.join("\n")
}
fn mastodon_name(n: NodeRef<'_>) -> String {
    let Some(name) = q(n, ".display-name__html") else {
        return String::new();
    };
    let fragment = Document::fragment(outer(Some(name)));
    emoji(fragment.root(), "img.emojione");
    fragment.text().trim().into()
}
fn mastodon_handle(n: NodeRef<'_>) -> String {
    t(q(n, ".display-name__account"))
        .trim_start_matches('@')
        .into()
}
pub(super) fn mastodon(
    doc: &Document,
    url: &Url,
    options: &Options,
    metadata: &Metadata,
) -> Option<Extracted> {
    let main = q(doc.root(), ".detailed-status__wrapper")?;
    if q(doc.root(), "#mastodon").is_none()
        && !q(doc.root(), "script#initial-state").is_some_and(|n| {
            n.text().contains("mastodon/mastodon") || n.text().contains("\"mastodon\"")
        })
        && !nodes(doc.root(), "link[rel=stylesheet]")
            .iter()
            .any(|n| a(Some(*n), "href").contains("mastodon"))
    {
        return None;
    }
    let main_handle = mastodon_handle(main);
    let mut parts = vec![mastodon_content(main)];
    let mut comments = Vec::new();
    let mut ended = false;
    let mut depth = 0;
    for row in nodes(doc.root(), ".status__wrapper")
        .into_iter()
        .filter(|n| q(*n, ".status[data-id]").is_some())
    {
        let handle = mastodon_handle(row);
        if !ended && handle.split('@').next() == main_handle.split('@').next() {
            parts.push(mastodon_content(row));
            continue;
        }
        ended = true;
        if replies(options) {
            if q(row, ".status--first-in-thread").is_some() || comments.is_empty() {
                depth = 0;
            } else {
                depth += 1;
            }
            let name = mastodon_name(row);
            let href = a(q(row, "a.status__relative-time[href]"), "href");
            let href = url.join(&href).ok().map(|u| u.to_string()).unwrap_or(href);
            comments.push(Comment {
                author: if name.is_empty() {
                    format!("@{handle}")
                } else {
                    format!("{name} @{handle}")
                },
                date: date(&a(q(row, "time[datetime]"), "datetime")),
                content: mastodon_content(row),
                depth,
                url: href,
                ..Comment::default()
            });
        }
    }
    let mut meta = metadata.clone();
    meta.author = fallback(mastodon_name(main), &format!("@{main_handle}"));
    meta.site = fallback(
        a(q(doc.root(), "meta[property='og:site_name']"), "content"),
        "Mastodon",
    );
    meta.title = format!("Post by {} on {}", meta.author, meta.site);
    meta.description = preview(&t(q(main, ".status__content__text")));
    let published = date(&a(
        q(doc.root(), "meta[property='og:published_time']"),
        "content",
    ));
    let published = fallback(published, &date(&a(q(main, "time[datetime]"), "datetime")));
    if !published.is_empty() {
        meta.published = published;
    }
    Some(direct(
        "MastodonExtractor",
        post(
            "mastodon",
            &parts
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("\n<hr>\n"),
            &comments,
        ),
        meta,
    ))
}
