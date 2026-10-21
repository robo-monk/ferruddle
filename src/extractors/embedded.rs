use super::*;
use serde_json::Value;
fn strval(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}

pub(super) fn substack(doc: &Document, metadata: &Metadata) -> Option<Extracted> {
    let mut post_data = Value::Null;
    for script in nodes(doc.root(), "script") {
        let raw = script.text();
        if !raw.contains("window._preloads") || !raw.contains("body_html") {
            continue;
        }
        let Some(start) = raw.find("JSON.parse(\"").map(|i| i + "JSON.parse(".len()) else {
            continue;
        };
        let mut end = start + 1;
        let bytes = raw.as_bytes();
        while end < bytes.len() {
            if bytes[end] == b'\\' {
                end += 2;
            } else if bytes[end] == b'"' {
                break;
            } else {
                end += 1;
            }
        }
        if end >= raw.len() {
            continue;
        }
        if let Ok(inner) = serde_json::from_str::<String>(&raw[start..=end])
            && let Ok(value) = serde_json::from_str::<Value>(&inner)
            && let Some(post) = value
                .pointer("/feedData/initialPost/post")
                .filter(|p| !strval(p, "body_html").is_empty())
        {
            post_data = post.clone();
            break;
        }
    }
    let rendered = q(doc.root(), "div.body.markup").is_some();
    let body_html = strval(&post_data, "body_html");
    let mut meta = metadata.clone();
    meta.site = "Substack".into();
    if rendered || !body_html.is_empty() {
        meta.title = fallback(
            strval(&post_data, "title"),
            &a(q(doc.root(), "meta[property='og:title']"), "content"),
        );
        meta.description = fallback(
            strval(&post_data, "subtitle"),
            &a(q(doc.root(), "meta[property='og:description']"), "content"),
        );
        meta.author = fallback(
            post_data
                .pointer("/publishedBylines/0/name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            &t(q(doc.root(), "a[href*='substack.com/@']")),
        );
        let byline = re!(r"([a-z])([A-Z])")
            .replace_all(&t(q(doc.root(), "[class*=byline-wrapper]")), "$1 $2")
            .into_owned();
        let parsed_date =
            re!(r"\b((?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2},?\s+\d{4})\b")
                .captures(&byline)
                .map(|c| locale_date(&c[1]))
                .unwrap_or_default();
        meta.published = fallback(
            strval(&post_data, "post_date"),
            &if parsed_date.is_empty() {
                String::new()
            } else {
                parsed_date + "T00:00:00+00:00"
            },
        );
        if rendered {
            return Some(selected("SubstackExtractor", "div.body.markup", meta));
        }
        // Preserve the selector path so standard extraction runs over embedded HTML.
        let wrapper = crate::dom::create_element(doc, "div");
        wrapper.set_attr("data-defuddle-substack-post", "");
        crate::dom::append_fragment(wrapper, &body_html);
        q(doc.root(), "body")
            .unwrap_or(doc.root())
            .append_child(&wrapper);
        return Some(selected(
            "SubstackExtractor",
            "[data-defuddle-substack-post]",
            meta,
        ));
    }
    let scope = q(doc.root(), "[class*=feedPermalinkUnit]").unwrap_or(doc.root());
    let note = q(scope, "div.ProseMirror.FeedProseMirror")?;
    let mut content = outer(Some(note));
    if let Some(comment) = crate::dom::closest(
        note,
        "[class*=feedCommentBody]:not([class*=feedCommentBodyInner])",
    ) {
        let candidates = [
            comment.next_element_sibling(),
            comment.parent().and_then(|p| p.next_element_sibling()),
        ];
        if let Some(grid) = candidates
            .into_iter()
            .flatten()
            .find(|n| a(Some(*n), "class").contains("imageGrid"))
        {
            let src = fallback(
                a(q(doc.root(), "meta[property='og:image']"), "content"),
                &q(grid, "img").map(largest_src).unwrap_or_default(),
            );
            if !src.is_empty() {
                content.push_str(&image(&src, ""));
            }
        }
    }
    meta.title = a(q(doc.root(), "meta[property='og:title']"), "content");
    meta.author = re!(r"\s*\(@[^)]+\)\s*$")
        .replace(&meta.title, "")
        .trim()
        .into();
    meta.description = a(q(doc.root(), "meta[property='og:description']"), "content");
    Some(direct("SubstackExtractor", content, meta))
}
fn largest_src(img: NodeRef<'_>) -> String {
    let srcset = a(Some(img), "srcset");
    let best = re!(r"(.+?)\s+(\d+(?:\.\d+)?)w")
        .captures_iter(&srcset)
        .map(|c| {
            (
                c[2].parse::<f64>().unwrap_or(0.0),
                c[1].trim().trim_start_matches(',').trim().to_string(),
            )
        })
        .max_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, src)) = best {
        re!(r",w_\d+|,c_\w+").replace_all(&src, "").into_owned()
    } else {
        a(Some(img), "src")
    }
}

pub(super) fn bbcode_data(doc: &Document, metadata: &Metadata) -> Option<Extracted> {
    let config = q(doc.root(), "#application_config")?;
    let parse = |key| {
        serde_json::from_str::<Value>(&a(Some(config), key))
            .ok()
            .map(|v| if v.is_array() { v[0].clone() } else { v })
    };
    let event = parse("data-partnereventstore")?;
    let body = event.get("announcement_body")?;
    let raw = body.get("body")?.as_str()?;
    if raw.is_empty() {
        return None;
    }
    let mut meta = metadata.clone();
    meta.title = fallback(strval(body, "headline"), &strval(&event, "event_name"));
    meta.author = parse("data-groupvanityinfo")
        .map(|v| strval(&v, "group_name"))
        .unwrap_or_default();
    meta.published = body
        .get("posttime")
        .and_then(Value::as_i64)
        .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default();
    Some(direct("BbcodeDataExtractor", bbcode(raw), meta))
}
fn bbcode(raw: &str) -> String {
    // Tokenize before escaping: plain text never becomes markup, and parameters
    // are escaped independently at their construction sites.
    let mut out = String::new();
    let mut cursor = 0;
    let mut stack: Vec<(String, String)> = Vec::new();
    for token in re!(r"(?s)\[([^\]]+)\]").captures_iter(raw) {
        let m = token.get(0).unwrap();
        if m.start() < cursor {
            continue;
        }
        let plain = &raw[cursor..m.start()];
        cursor = m.end();
        let in_code = stack.last().is_some_and(|(name, _)| name == "code");
        out.push_str(&if in_code {
            esc(plain)
        } else {
            esc(plain).replace('\n', "<br>")
        });
        let token = &token[1];
        if in_code && !token.eq_ignore_ascii_case("/code") {
            out.push_str(&esc(m.as_str()));
            continue;
        }
        let close = token.starts_with('/');
        let token = token.trim_start_matches('/');
        let (name, param) = token.split_once('=').unwrap_or((token, ""));
        let name = name.to_lowercase();
        let param = param.trim_matches(['"', '\'']);
        if close {
            if let Some(index) = stack.iter().rposition(|(tag, _)| tag == &name) {
                while stack.len() > index {
                    out.push_str(&stack.pop().unwrap().1);
                }
            }
            continue;
        }
        if in_code {
            out.push_str(&esc(m.as_str()));
            continue;
        }
        if name == "img" {
            if let Some(closing) = re!(r"(?i-u)\[/img\]").find(&raw[cursor..]) {
                out.push_str(&image(raw[cursor..cursor + closing.start()].trim(), ""));
                cursor += closing.end();
            }
            continue;
        }
        // Already-consumed image source tokens must not get emitted twice.
        let (opening, ending) = match name.as_str() {
            "h1" | "h2" | "h3" | "p" | "u" | "s" => (format!("<{name}>"), format!("</{name}>")),
            "b" => ("<strong>".into(), "</strong>".into()),
            "i" => ("<em>".into(), "</em>".into()),
            "quote" => ("<blockquote>".into(), "</blockquote>".into()),
            "code" => ("<pre><code>".into(), "</code></pre>".into()),
            "spoiler" => (
                "<details><summary>Spoiler</summary>".into(),
                "</details>".into(),
            ),
            "list" => ("<ul>".into(), "</ul>".into()),
            "olist" => ("<ol>".into(), "</ol>".into()),
            "*" => {
                if stack.last().is_some_and(|(name, _)| name == "*") {
                    out.push_str(&stack.pop().unwrap().1);
                }
                ("<li>".into(), "</li>".into())
            }
            "url" if !crate::safety::is_dangerous_url(param, false) => {
                (format!("<a href=\"{}\">", esc(param)), "</a>".into())
            }
            "previewyoutube" => {
                out.push_str(&image(
                    &format!(
                        "https://www.youtube.com/watch?v={}",
                        param.split(';').next().unwrap_or_default()
                    ),
                    "",
                ));
                continue;
            }
            _ => continue,
        };
        out.push_str(&opening);
        stack.push((name, ending));
    }
    if cursor <= raw.len() {
        out.push_str(&esc(&raw[cursor..]).replace('\n', "<br>"));
    }
    while let Some((_, ending)) = stack.pop() {
        out.push_str(&ending);
    }
    out
}

pub(super) fn other(
    doc: &Document,
    _url: &str,
    host: &str,
    _options: &Options,
    metadata: &Metadata,
) -> Option<Extracted> {
    if host.contains("wikipedia.org") && q(doc.root(), "#mw-content-text").is_some() {
        let mut meta = metadata.clone();
        meta.title = re!(r"\s*[-–—]\s*Wikipedia\s*$")
            .replace(
                &a(q(doc.root(), "meta[property='og:title']"), "content"),
                "",
            )
            .into_owned();
        meta.author = "Wikipedia".into();
        meta.site = "Wikipedia".into();
        return Some(selected("WikipediaExtractor", "#mw-content-text", meta));
    }
    if host.contains("nytimes.com")
        && let Some(out) = nyt(doc, metadata)
    {
        return Some(out);
    }
    if host.contains("medium.com")
        && let Some(out) = medium(doc, metadata)
    {
        return Some(out);
    }
    None
}

fn nyt(doc: &Document, metadata: &Metadata) -> Option<Extracted> {
    let mut article = None;
    for script in nodes(doc.root(), "script:not([src])") {
        let raw = script.text();
        let Some(c) = re!(r"(?s)window\.__preloadedData\s*=\s*(\{.+\})\s*;?\s*$").captures(&raw)
        else {
            continue;
        };
        let raw = re!(r":undefined([,}\]])").replace_all(&c[1], ":null$1");
        if let Ok(data) = serde_json::from_str::<Value>(&raw) {
            article = data.pointer("/initialData/data/article").cloned();
        }
        break;
    }
    let article = article?;
    let blocks = article
        .pointer("/sprinkledBody/content")
        .or_else(|| article.pointer("/body/content"))?
        .as_array()?;
    if blocks.is_empty() {
        return None;
    }
    fn inlines(value: &Value) -> String {
        value
            .as_array()
            .into_iter()
            .flatten()
            .map(|inline| {
                let mut text = esc(&strval(inline, "text"));
                for fmt in inline
                    .get("formats")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    match strval(fmt, "__typename").as_str() {
                        "BoldFormat" => text = format!("<strong>{text}</strong>"),
                        "ItalicFormat" => text = format!("<em>{text}</em>"),
                        "LinkFormat" => {
                            let url = strval(fmt, "url");
                            if !url.is_empty() && !crate::safety::is_dangerous_url(&url, false) {
                                text = format!("<a href=\"{}\">{text}</a>", esc(&url));
                            }
                        }
                        _ => {}
                    }
                }
                text
            })
            .collect()
    }
    let mut html = String::new();
    for block in blocks {
        match strval(block, "__typename").as_str() {
            "HeaderBasicBlock" | "Dropzone" => {}
            "ImageBlock" => {
                let media = &block["media"];
                let renditions: Vec<_> = media
                    .get("crops")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .flat_map(|crop| {
                        crop.get("renditions")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                    })
                    .collect();
                let src = ["superJumbo", "jumbo", "articleLarge"]
                    .iter()
                    .find_map(|name| {
                        renditions
                            .iter()
                            .find(|r| strval(r, "name") == *name && !strval(r, "url").is_empty())
                            .map(|r| strval(r, "url"))
                    })
                    .or_else(|| renditions.first().map(|r| strval(r, "url")))
                    .unwrap_or_default();
                if src.is_empty() {
                    continue;
                }
                let caption = media
                    .pointer("/caption/text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let credit = strval(media, "credit");
                let alt = fallback(strval(media, "altText"), caption);
                let image = image(&src, &alt);
                let caption = [caption, credit.as_str()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                if caption.is_empty() {
                    html.push_str(&image);
                } else {
                    html.push_str(&format!(
                        "<figure>{image}<figcaption>{}</figcaption></figure>",
                        esc(&caption)
                    ));
                }
            }
            kind => {
                if let Some(content) = block.get("content") {
                    let tag = match kind {
                        "Heading2Block" => "h2",
                        "Heading3Block" => "h3",
                        "Heading4Block" => "h4",
                        _ => "p",
                    };
                    html.push_str(&format!("<{tag}>{}</{tag}>", inlines(content)));
                }
            }
        }
    }
    let wrapper = crate::dom::create_element(doc, "div");
    wrapper.set_attr("data-defuddle-nyt", "");
    crate::dom::append_fragment(wrapper, &html);
    q(doc.root(), "body")
        .unwrap_or(doc.root())
        .append_child(&wrapper);
    let mut meta = metadata.clone();
    meta.title = article
        .pointer("/headline/default")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into();
    meta.description = strval(&article, "summary");
    meta.published = strval(&article, "firstPublished");
    meta.author = article
        .pointer("/bylines/0/creators")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|v| strval(v, "displayName"))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    Some(selected("NytimesExtractor", "[data-defuddle-nyt]", meta))
}
fn medium(doc: &Document, metadata: &Metadata) -> Option<Extracted> {
    let article = q(doc.root(), "article.meteredContent").or_else(|| q(doc.root(), "article"))?;
    if !article.is(".meteredContent")
        && a(q(doc.root(), "meta[property='og:site_name']"), "content") != "Medium"
        && a(
            q(doc.root(), "meta[property='al:android:app_name']"),
            "content",
        ) != "Medium"
    {
        return None;
    }
    let mut meta = metadata.clone();
    meta.title = fallback(
        t(q(doc.root(), "[data-testid=storyTitle]")),
        &t(q(article, "h1")),
    );
    meta.author = t(q(doc.root(), "[data-testid=authorName]"));
    meta.description = t(q(doc.root(), ".pw-subtitle-paragraph"));
    let site = a(q(doc.root(), "meta[property='og:site_name']"), "content");
    meta.site = if !site.is_empty() && site != "Medium" {
        site
    } else {
        metadata
            .schema_org_data
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|s| s.pointer("/publisher/name").and_then(Value::as_str))
            .unwrap_or("Medium")
            .into()
    };
    for n in nodes(article, "figure [role=button]") {
        crate::dom::unwrap(n);
    }
    for n in nodes(article, "[role=tooltip]") {
        n.remove_attr("role");
    }
    for n in nodes(article, "a[href*='medium.com/plans']") {
        if let Some(wrapper) =
            crate::dom::closest(n, "div").filter(|w| !crate::dom::same_node(*w, article))
        {
            wrapper.remove_from_parent();
        } else {
            n.remove_from_parent();
        }
    }
    for n in nodes(
        article,
        "[data-testid=post-preview],[data-testid*=Clap],[data-testid*=Bookmark],[data-testid*=Share],[data-testid*=Response],[data-testid=authorPhoto],[data-testid=authorName],[data-testid=storyReadTime]",
    ) {
        n.remove_from_parent();
    }
    for n in nodes(article, "p,span,div") {
        let text = t(Some(n));
        if [
            "Member-only story",
            "Listen",
            "Share",
            "Top highlight",
            "·",
            "Press enter or click to view image in full size",
        ]
        .contains(&text.as_str())
            || text.chars().count() < 30 && re!(r"^\w{3}\s+\d{1,2},\s+\d{4}").is_match(&text)
            || re!(r"^·\s*\d+\s*\w+\s*ago$|^·?\s*\d+\s*min\s*read$").is_match(&text)
        {
            n.remove_from_parent();
        }
    }
    if meta.description.is_empty() {
        meta.description = nodes(article, "p")
            .iter()
            .map(|n| t(Some(*n)))
            .find(|s| s.chars().count() >= 3 && s.chars().any(char::is_alphabetic))
            .map(|s| preview(&s))
            .unwrap_or_default();
    }
    Some(selected("MediumExtractor", "article", meta))
}
