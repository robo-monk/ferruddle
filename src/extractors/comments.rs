use super::*;

pub(super) fn gmail(doc: &Document, options: &Options, metadata: &Metadata) -> Option<Extracted> {
    let rows = nodes(doc.root(), ".adn.ads");
    if rows.is_empty() {
        return None;
    }
    let mut messages = Vec::new();
    for row in rows {
        let Some(body) = q(row, ".a3s") else { continue };
        let fragment = Document::fragment(body.html());
        let body = q(fragment.root(), ".a3s")?;
        for n in nodes(
            body,
            ".gmail_quote,.gmail_attr,.gmail_extra,blockquote[type=cite],.moz-cite-prefix,blockquote[style*='border-left'],.yahoo_quoted,.a6S,.adL,.h5,.adm,.ajR,.ajT,.h4,.yj6qo",
        ) {
            n.remove_from_parent();
        }
        strip_quote_lines(body);
        trim_blanks(body);
        let content = h(Some(body)).trim().to_string();
        if content.is_empty() {
            continue;
        }
        let sender = q(row, ".gD");
        let author = fallback(
            fallback(a(sender, "name").trim().to_string(), &t(sender)),
            "Unknown",
        );
        let date = fallback(
            a(q(row, ".g3"), "title").trim().to_string(),
            &t(q(row, ".g3")),
        );
        messages.push(Comment {
            author,
            date,
            content,
            ..Comment::default()
        });
    }
    let count = messages.len();
    let mut meta = metadata.clone();
    meta.title = fallback(
        t(q(doc.root(), "h2.hP")),
        &re!(r" - [^-]+ - Gmail$").replace(&t(q(doc.root(), "title")), ""),
    );
    meta.title = fallback(meta.title, "Gmail thread");
    meta.site = "Gmail".into();
    meta.author = messages
        .first()
        .map(|m| m.author.clone())
        .unwrap_or_default();
    meta.published = messages
        .first()
        .map(|m| locale_date(&m.date))
        .unwrap_or_default();
    meta.description = if meta.author.is_empty() {
        format!("Gmail thread with {count} messages")
    } else {
        format!("Gmail thread from {} with {count} messages", meta.author)
    };
    if !replies(options) {
        messages.truncate(1);
    }
    Some(direct(
        "GmailExtractor",
        format!(
            "<article data-defuddle><div class=\"gmail comments\">{}</div></article>",
            comment_tree(&messages)
        ),
        meta,
    ))
}
fn trim_blanks(root: NodeRef<'_>) {
    while let Some(last) = root.last_child() {
        if !last.text().trim().is_empty()
            || last.is("img,picture,video,audio,iframe,svg,canvas,object,embed")
            || q(
                last,
                "img,picture,video,audio,iframe,svg,canvas,object,embed",
            )
            .is_some()
        {
            break;
        }
        last.remove_from_parent();
    }
    if let Some(last) = root.last_child().filter(|n| n.is_element()) {
        trim_blanks(last);
    }
}
fn strip_quote_lines(root: NodeRef<'_>) {
    fn walk<'a>(root: NodeRef<'a>, lines: &mut Vec<Vec<NodeRef<'a>>>, line: &mut Vec<NodeRef<'a>>) {
        for n in root.children() {
            if !n.is_element() { line.push(n); }
            else if n.is("br") { line.push(n); lines.push(std::mem::take(line)); }
            else if n.is("a,abbr,b,bdi,bdo,big,code,em,font,i,img,kbd,label,mark,s,samp,small,span,strike,strong,sub,sup,time,tt,u,var,wbr") {
                if q(n, "br").is_some() { walk(n, lines, line); } else { line.push(n); }
            } else { lines.push(std::mem::take(line)); walk(n, lines, line); lines.push(std::mem::take(line)); }
        }
    }
    let mut lines = Vec::new();
    let mut line = Vec::new();
    walk(root, &mut lines, &mut line);
    lines.push(line);
    for line in lines {
        let text: String = line.iter().map(|n| n.text().to_string()).collect();
        if text.trim_start().starts_with('>') {
            for n in line {
                n.remove_from_parent();
            }
        }
    }
}

pub(super) fn hackernews(
    doc: &Document,
    options: &Options,
    metadata: &Metadata,
) -> Option<Extracted> {
    let main = q(doc.root(), ".fatitem");
    let stories = nodes(doc.root(), "tr.athing");
    let mut meta = metadata.clone();
    meta.site = "Hacker News".into();
    if main.is_none() && stories.len() > 1 {
        let mut html = String::from("<ol>");
        for row in stories {
            let Some(title) = q(row, ".titleline a") else {
                continue;
            };
            html.push_str(&format!(
                "<li>{}",
                link(&a(Some(title), "href"), &t(Some(title)))
            ));
            let site = t(q(row, ".sitestr"));
            if !site.is_empty() {
                html.push_str(&format!(" <small>({})</small>", esc(&site)));
            }
            let mut pieces = Vec::new();
            if let Some(sub) = row.next_element_sibling() {
                let score = t(q(sub, ".score"));
                let author = t(q(sub, ".hnuser"));
                if !score.is_empty() {
                    pieces.push(esc(&score));
                }
                if !author.is_empty() {
                    pieces.push(format!("by {}", esc(&author)));
                }
                if let Some(last) = nodes(sub, "td.subtext a").last() {
                    let text = t(Some(*last)).replace('\u{a0}', " ");
                    if re!(r"\d+\s*comment").is_match(&text) {
                        pieces.push(link(
                            &format!(
                                "https://news.ycombinator.com/item?id={}",
                                a(Some(row), "id")
                            ),
                            &text,
                        ));
                    }
                }
            }
            if !pieces.is_empty() {
                html.push_str(&format!("<br><small>{}</small>", pieces.join(" · ")));
            }
            html.push_str("</li>");
        }
        html.push_str("</ol>");
        if let Some(more) = q(doc.root(), ".morelink") {
            html.push_str(&format!(
                "<p>{}</p>",
                link(&a(Some(more), "href"), &fallback(t(Some(more)), "More"))
            ));
        }
        meta.title = fallback(
            re!(r"\s*\|\s*Hacker News$")
                .replace(&t(q(doc.root(), "title")), "")
                .trim()
                .to_string(),
            "Hacker News",
        );
        return Some(direct("HackerNewsExtractor", html, meta));
    }
    let main = main?;
    let is_comment = q(main, ".onstory").is_some() && q(main, ".titleline").is_none();
    let body;
    meta.author = t(q(main, ".hnuser"));
    meta.published = a(q(main, ".age"), "title")
        .split('T')
        .next()
        .unwrap_or_default()
        .to_string();
    if is_comment {
        let row = q(main, "tr.athing");
        let text = row.and_then(|r| q(r, ".commtext"));
        let author = fallback(t(row.and_then(|r| q(r, ".hnuser"))), "[deleted]");
        let raw = t(text);
        meta.title = format!(
            "Comment by {author}: {}{}",
            raw.chars().take(50).collect::<String>(),
            if raw.chars().count() > 50 { "..." } else { "" }
        );
        body = comment(&Comment {
            author,
            date: meta.published.clone(),
            content: h(text),
            score: t(row.and_then(|r| q(r, ".score"))),
            ..Comment::default()
        });
        meta.description = format!("Comment by {} on Hacker News", meta.author);
    } else {
        meta.title = t(q(main, ".titleline"));
        let url = a(q(main, "tr.athing .titleline a"), "href");
        body = format!(
            "{}{}",
            if url.is_empty() {
                String::new()
            } else {
                format!("<p>{}</p>", link(&url, &url))
            },
            q(main, ".toptext")
                .map(|n| format!("<div class=\"post-text\">{}</div>", h(Some(n))))
                .unwrap_or_default()
        );
        meta.description = format!("{} - by {} on Hacker News", meta.title, meta.author);
    }
    let mut comments = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if replies(options) {
        for row in nodes(doc.root(), "tr.comtr") {
            let id = a(Some(row), "id");
            if id.is_empty() || !seen.insert(id.clone()) {
                continue;
            }
            let Some(text) = q(row, ".commtext") else {
                continue;
            };
            comments.push(Comment {
                author: fallback(t(q(row, ".hnuser")), "[deleted]"),
                date: a(q(row, ".age"), "title")
                    .split('T')
                    .next()
                    .unwrap_or_default()
                    .into(),
                content: h(Some(text)),
                depth: a(q(row, ".ind img"), "width").parse::<usize>().unwrap_or(0) / 40,
                score: t(q(row, ".score")),
                url: format!("https://news.ycombinator.com/item?id={id}"),
            });
        }
    }
    Some(direct(
        "HackerNewsExtractor",
        post("hackernews", &body, &comments),
        meta,
    ))
}

pub(super) fn reddit(
    doc: &Document,
    url: &str,
    options: &Options,
    metadata: &Metadata,
) -> Option<Extracted> {
    let old = q(doc.root(), ".thing.link");
    let new = q(doc.root(), "shreddit-post");
    let mut meta = metadata.clone();
    let mut comments = Vec::new();
    let body;
    if let Some(post) = old {
        meta.title = t(q(post, "a.title"));
        meta.author = a(Some(post), "data-author");
        meta.site = format!("r/{}", a(Some(post), "data-subreddit"));
        body = h(q(post, ".usertext-body .md"));
        fn collect(root: NodeRef<'_>, depth: usize, comments: &mut Vec<Comment>) {
            for row in root
                .element_children()
                .into_iter()
                .filter(|n| n.is(".thing.comment"))
            {
                let permalink = a(Some(row), "data-permalink");
                comments.push(Comment {
                    author: a(Some(row), "data-author"),
                    date: date(&a(q(row, ".entry .tagline time[datetime]"), "datetime")),
                    content: h(q(row, ".entry .usertext-body .md")),
                    depth,
                    score: t(q(row, ".entry .tagline .score.unvoted")),
                    url: if permalink.is_empty() {
                        String::new()
                    } else {
                        format!("https://reddit.com{permalink}")
                    },
                });
                if let Some(child) = q(row, ".child > .sitetable") {
                    collect(child, depth + 1, comments);
                }
            }
        }
        if replies(options)
            && let Some(area) = q(doc.root(), ".commentarea .sitetable")
        {
            collect(area, 0, &mut comments);
        }
    } else {
        let post = new?;
        meta.title = t(q(doc.root(), "h1"));
        meta.author = a(Some(post), "author");
        meta.site = format!(
            "r/{}",
            re!(r"/r/([^/]+)")
                .captures(url)
                .map(|c| c[1].to_string())
                .unwrap_or_default()
        );
        body = h(q(post, "[slot=text-body]")) + &outer(q(post, "#post-image"));
        if replies(options) {
            for row in nodes(doc.root(), "shreddit-comment") {
                let permalink = a(Some(row), "permalink");
                comments.push(Comment {
                    author: a(Some(row), "author"),
                    date: date(&fallback(
                        a(Some(row), "created"),
                        &a(q(row, "time"), "datetime"),
                    )),
                    content: h(q(row, "[slot=comment]")),
                    depth: a(Some(row), "depth").parse().unwrap_or(0),
                    score: format!("{} points", fallback(a(Some(row), "score"), "0")),
                    url: if permalink.is_empty() {
                        String::new()
                    } else {
                        format!("https://reddit.com{permalink}")
                    },
                });
            }
        }
    }
    meta.description = html_preview(&body);
    Some(direct(
        "RedditExtractor",
        post("reddit", &body, &comments),
        meta,
    ))
}

pub(super) fn github(
    doc: &Document,
    url: &str,
    options: &Options,
    metadata: &Metadata,
) -> Option<Extracted> {
    q(
        doc.root(),
        "meta[name=expected-hostname][content='github.com'],meta[name=octolytics-url],meta[name=github-keyboard-shortcuts],.js-header-wrapper,#js-repo-pjax-container",
    )?;
    let is_pr = re!(r"/pull/\d+").is_match(url);
    let is_issue = re!(r"/issues/\d+").is_match(url);
    if !(is_pr
        && q(
            doc.root(),
            ".pull-discussion-timeline,.discussion-timeline,.gh-header-title,.js-issue-title",
        )
        .is_some()
        || is_issue
            && q(
                doc.root(),
                "[data-testid=issue-metadata-sticky],[data-testid=issue-title]",
            )
            .is_some())
    {
        return None;
    }
    let mut meta = metadata.clone();
    meta.title = t(q(doc.root(), "title"));
    let repo = re!(r"github\.com/([^/]+)/([^/]+)")
        .captures(url)
        .map(|c| format!("{}/{}", &c[1], &c[2]))
        .unwrap_or_else(|| {
            re!(r"([^/\s]+)/([^/\s]+)")
                .captures(&meta.title)
                .map(|c| format!("{}/{}", &c[1], &c[2]))
                .unwrap_or_else(|| "/".into())
        });
    meta.site = format!("GitHub - {repo}");
    let main = if is_pr {
        q(doc.root(), "[id^=pullrequest-]").or_else(|| q(doc.root(), ".timeline-comment"))
    } else {
        q(doc.root(), "[data-testid=issue-viewer-issue-container]")
    };
    let body = if is_pr {
        main.and_then(|n| q(n, ".comment-body.markdown-body"))
            .or_else(|| q(doc.root(), ".comment-body.markdown-body"))
    } else {
        main.and_then(|n| q(n, "[data-testid=issue-body-viewer] .markdown-body"))
    };
    let body = body.map(clean_github_body).unwrap_or_default();
    meta.author = if is_pr {
        fallback(
            t(main.and_then(|n| q(n, ".author"))),
            &t(q(doc.root(), ".gh-header-meta .author")),
        )
    } else {
        main.map(|n| {
            github_author(
                n,
                &[
                    "a[data-testid=issue-body-header-author]",
                    ".IssueBodyHeaderAuthor-module__authorLoginLink--_S7aT",
                    ".ActivityHeader-module__AuthorLink--iofTU",
                    "a[href*='/users/'][data-hovercard-url*='/users/']",
                    "a[aria-label*=profile]",
                ],
            )
        })
        .unwrap_or_default()
    };
    meta.published = a(main.and_then(|n| q(n, "relative-time")), "datetime");
    let mut comments = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if replies(options) {
        for row in nodes(
            doc.root(),
            if is_pr {
                ".timeline-comment,.review-comment"
            } else {
                "[data-wrapper-timeline-id]"
            },
        ) {
            if is_pr && main.is_some_and(|m| crate::utils::contains(m, row)) {
                continue;
            }
            let row = if is_pr {
                row
            } else {
                let id = a(Some(row), "data-wrapper-timeline-id");
                if id.is_empty() || !seen.insert(id) {
                    continue;
                }
                let Some(inner) = q(row, ".react-issue-comment") else {
                    continue;
                };
                inner
            };
            let Some(b) = q(
                row,
                if is_pr {
                    ".comment-body.markdown-body"
                } else {
                    ".markdown-body"
                },
            ) else {
                continue;
            };
            let content = clean_github_body(b);
            if content.is_empty() {
                continue;
            }
            comments.push(Comment {
                author: if is_pr {
                    t(q(row, ".author"))
                } else {
                    github_author(
                        row,
                        &[
                            ".ActivityHeader-module__AuthorLink--iofTU",
                            "a[data-testid=avatar-link]",
                            "a[href^='/'][data-hovercard-url*='/users/']",
                        ],
                    )
                },
                date: date(&a(q(row, "relative-time"), "datetime")),
                content,
                ..Comment::default()
            });
        }
    }
    let html = post("github", &body, &comments);
    meta.description = html_preview(&html);
    Some(direct("GitHubExtractor", html, meta))
}
fn github_author(root: NodeRef<'_>, selectors: &[&str]) -> String {
    for selector in selectors {
        let href = a(q(root, selector), "href");
        if let Some(name) = href.strip_prefix('/') {
            return name.into();
        }
        if let Some(c) = re!(r"github\.com/([^/?#]+)").captures(&href) {
            return c[1].to_string();
        }
    }
    "Unknown".into()
}
fn clean_github_body(node: NodeRef<'_>) -> String {
    let doc = Document::fragment(cleaned_html(
        node,
        "button,[data-testid*=button],[data-testid*=menu],.js-clipboard-copy,.zeroclipboard-container",
    ));
    for pre in nodes(doc.root(), "div.highlight pre") {
        let Some(wrapper) = pre.parent() else {
            continue;
        };
        let lang = re!(r"highlight-source-(\w+)")
            .captures(&a(Some(wrapper), "class"))
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let content = fallback(
            a(Some(wrapper), "data-snippet-clipboard-copy-content"),
            &pre.text(),
        );
        let html = format!(
            "<pre><code{}>{}</code></pre>",
            if lang.is_empty() {
                String::new()
            } else {
                format!(
                    " class=\"language-{}\" data-lang=\"{}\"",
                    esc(&lang),
                    esc(&lang)
                )
            },
            esc(&content)
        );
        for n in crate::dom::append_fragment(doc.root(), &html) {
            wrapper.insert_before(&n);
        }
        wrapper.remove_from_parent();
    }
    doc.html().trim().into()
}

pub(super) fn discourse(
    doc: &Document,
    url: &Url,
    options: &Options,
    metadata: &Metadata,
) -> Option<Extracted> {
    if !a(q(doc.root(), "meta[name=generator]"), "content").starts_with("Discourse") {
        return None;
    }
    let posts = nodes(doc.root(), ".topic-post");
    let first = *posts.first()?;
    let op = posts.iter().copied().find(|n| n.is(".topic-owner"));
    let author = |n| {
        let el = q(n, ".names a[data-user-card]");
        fallback(a(el, "data-user-card"), &t(el))
    };
    let content = |n| {
        q(n, ".cooked")
            .map(|n| cleaned_html(n, ".cooked-selection-barrier,a.anchor"))
            .unwrap_or_default()
    };
    let mut meta = metadata.clone();
    meta.title = if let Some(title) = q(doc.root(), ".fancy-title") {
        t(Some(title))
    } else {
        q(doc.root(), "h1[data-topic-id]")
            .map(|n| {
                Document::fragment(cleaned_html(n, "svg,.topic-statuses"))
                    .text()
                    .trim()
                    .to_string()
            })
            .unwrap_or_default()
    };
    meta.site = fallback(
        a(q(doc.root(), "meta[property='og:site_name']"), "content"),
        "Discourse",
    );
    meta.author = author(op.unwrap_or(first));
    meta.description = op.map(|n| preview(&t(q(n, ".cooked")))).unwrap_or_default();
    let published = date(&a(
        q(doc.root(), "meta[property='article:published_time']"),
        "content",
    ));
    if !published.is_empty() {
        meta.published = published;
    }
    let mut comments = Vec::new();
    if replies(options) {
        for row in posts
            .into_iter()
            .filter(|n| op.is_none_or(|op| !crate::dom::same_node(*n, op)))
        {
            let time = a(q(row, ".relative-date[data-time]"), "data-time")
                .parse::<i64>()
                .unwrap_or(0);
            let date = chrono::DateTime::from_timestamp_millis(time)
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            let href = a(q(row, "a.post-date[href]"), "href");
            let url = if href.is_empty() {
                String::new()
            } else {
                url.join(&href).ok().map(|u| u.to_string()).unwrap_or(href)
            };
            let likes = t(q(row, "button.like-count"));
            comments.push(Comment {
                author: author(row),
                date,
                url,
                content: content(row),
                score: if likes.is_empty() {
                    String::new()
                } else {
                    format!("{likes} likes")
                },
                ..Comment::default()
            });
        }
    }
    Some(direct(
        "DiscourseExtractor",
        post("discourse", &op.map(content).unwrap_or_default(), &comments),
        meta,
    ))
}
pub(super) fn lwn(doc: &Document, options: &Options, metadata: &Metadata) -> Option<Extracted> {
    q(doc.root(), ".PageHeadline")?;
    q(doc.root(), ".ArticleText")?;
    let main = q(doc.root(), ".ArticleText main");
    let mut meta = metadata.clone();
    meta.title = t(q(doc.root(), ".PageHeadline h1"));
    let byline = t(q(doc.root(), ".Byline"));
    meta.author = re!(r"(?i)by\s+(\w+)")
        .captures(&byline)
        .map(|c| c[1].to_string())
        .unwrap_or_default();
    meta.site = "LWN.net".into();
    meta.published = lwn_date(&byline);
    meta.description = a(q(doc.root(), "meta[property='og:description']"), "content");
    let mut comments = Vec::new();
    let body = if let Some(main) = main {
        if replies(options) {
            for row in nodes(main, "details.CommentBox") {
                let summary = row.element_children().into_iter().find(|n| n.is("summary"));
                let Some(poster) = summary.and_then(|n| q(n, ".CommentPoster")) else {
                    continue;
                };
                let author = t(q(poster, "b"));
                let path = a(q(poster, "a[href^='/Articles/']"), "href");
                let title = t(summary.and_then(|n| q(n, "h3.CommentTitle")));
                let parent = row
                    .parent()
                    .and_then(|n| crate::dom::closest(n, "details.CommentBox"));
                let parent_title = t(parent
                    .and_then(|n| n.element_children().into_iter().find(|n| n.is("summary")))
                    .and_then(|n| q(n, "h3.CommentTitle")));
                let mut content = if !title.is_empty() && title != parent_title {
                    format!("<p><strong>{}</strong></p>", esc(&title))
                } else {
                    String::new()
                };
                if let Some(formatted) = row
                    .element_children()
                    .into_iter()
                    .find(|n| n.is(".FormattedComment"))
                {
                    content.push_str(&h(Some(formatted)));
                } else {
                    for child in row.children() {
                        if child.is("summary,details,form,.CommentReplyButton,a[name^=CommAnchor]")
                            || child.is("p") && child.text().trim().is_empty()
                        {
                            continue;
                        }
                        content.push_str(&outer(Some(child)));
                    }
                }
                comments.push(Comment {
                    author,
                    date: lwn_date(&poster.text()),
                    content,
                    depth: row
                        .ancestors_it(None)
                        .take_while(|n| !crate::dom::same_node(*n, main))
                        .filter(|n| n.is("details.CommentBox"))
                        .count(),
                    url: if path.is_empty() {
                        String::new()
                    } else {
                        format!("https://lwn.net{path}")
                    },
                    ..Comment::default()
                });
            }
        }
        let fragment =
            Document::fragment(cleaned_html(main, "details.CommentBox,form,a[name^=Comm]"));
        while let Some(last) = fragment.html_root().element_children().last().copied() {
            if last.is("hr,br[clear]") {
                last.remove_from_parent();
            } else {
                break;
            }
        }
        fragment.html().to_string()
    } else {
        String::new()
    };
    Some(direct("LwnExtractor", post("lwn", &body, &comments), meta))
}
fn lwn_date(s: &str) -> String {
    re!(r"Posted\s+(\w+\s+\d+,\s+\d{4})")
        .captures(s)
        .map(|c| locale_date(&c[1]))
        .unwrap_or_default()
}
