use super::*;
struct Message {
    author: String,
    content: String,
}
struct Footnote {
    url: String,
    label: String,
}
fn clean(content: &str) -> String {
    let fragment = Document::fragment(content.replace('\u{200b}', ""));
    for n in nodes(
        fragment.root(),
        "h4.sr-only,h5.sr-only,h6.sr-only,span[data-state=closed]",
    ) {
        n.remove_from_parent();
    }
    for control in nodes(
        fragment.root(),
        "form,fieldset,button,select,textarea,input",
    ) {
        let task_checkbox = control.is("input")
            && a(Some(control), "type").eq_ignore_ascii_case("checkbox")
            && crate::dom::closest(control, "li").is_some()
            && !control.ancestors_it(None).any(|n| n.is("form,fieldset"));
        if !task_checkbox {
            control.remove_from_parent();
        }
    }
    for p in nodes(fragment.root(), "p") {
        if p.text().trim().is_empty() && p.element_children().is_empty() {
            p.remove_from_parent();
        }
    }
    fragment.html().trim().into()
}
fn citation(root: NodeRef<'_>, footnotes: &mut Vec<Footnote>, grok: bool) {
    for a_node in nodes(root, "a[href]") {
        let url = a(Some(a_node), "href");
        if grok && !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        if !grok
            && !(a(Some(a_node), "target") == "_blank"
                && a(Some(a_node), "rel") == "noopener"
                && a_node.parent().is_some_and(|n| n.is("span")))
        {
            continue;
        }
        let index = footnotes
            .iter()
            .position(|f| f.url == url)
            .unwrap_or_else(|| {
                let label = Url::parse(&url)
                    .ok()
                    .and_then(|u| {
                        u.host_str()
                            .map(|h| h.trim_start_matches("www.").to_string())
                    })
                    .unwrap_or_else(|| url.clone());
                footnotes.push(Footnote {
                    url: url.clone(),
                    label,
                });
                footnotes.len() - 1
            })
            + 1;
        let mut target = a_node;
        if !grok {
            while let Some(parent) = target.parent().filter(|n| n.is("span")) {
                target = parent;
            }
        }
        let html = format!(
            "{}<sup id=\"fnref:{index}\"><a href=\"#fn:{index}\">{index}</a></sup>",
            if grok { h(Some(a_node)) } else { String::new() }
        );
        for n in crate::dom::append_fragment(root, &html) {
            target.insert_before(&n);
        }
        target.remove_from_parent();
    }
}
pub(super) fn extract(doc: &Document, host: &str, metadata: &Metadata) -> Option<Extracted> {
    let mut messages = Vec::new();
    let mut footnotes = Vec::new();
    let site;
    let mut title = t(q(doc.root(), "title"));
    if host == "chatgpt.com" {
        let turns = nodes(doc.root(), "[data-testid^=conversation-turn-]");
        if turns.is_empty() {
            return None;
        }
        site = "ChatGPT";
        for turn in &turns {
            let author = t(q(*turn, "h4.sr-only,h5.sr-only,h6.sr-only"))
                .trim_end_matches(':')
                .trim()
                .to_string();
            let els: Vec<_> = nodes(*turn, "[data-message-author-role]")
                .into_iter()
                .filter(|n| {
                    crate::dom::closest(*n, "[data-testid^=conversation-turn-]")
                        .is_some_and(|p| crate::dom::same_node(p, *turn))
                })
                .collect();
            let mut parts = Vec::new();
            for el in els {
                let mut candidates = Vec::new();
                if el.is(".markdown,.whitespace-pre-wrap") {
                    candidates.push(el);
                }
                candidates.extend(nodes(el, ".markdown,.whitespace-pre-wrap"));
                let roots: Vec<_> = candidates
                    .iter()
                    .filter(|n| {
                        !candidates.iter().any(|p| {
                            !crate::dom::same_node(**n, *p) && crate::utils::contains(*p, **n)
                        })
                    })
                    .collect();
                if roots.is_empty() {
                    parts.push(h(Some(el)));
                } else {
                    parts.extend(roots.into_iter().map(|n| h(Some(*n))));
                }
            }
            if parts.is_empty() {
                parts.push(h(Some(*turn)));
            }
            let fragment = Document::fragment(parts.join("\n").replace('\u{200b}', ""));
            citation(fragment.root(), &mut footnotes, false);
            messages.push(Message {
                author,
                content: clean(&fragment.html()),
            });
        }
        if title.is_empty() || title == "ChatGPT" {
            title = turns
                .first()
                .and_then(|n| q(*n, ".text-message"))
                .map(|n| short_title(&n.text()))
                .unwrap_or_else(|| "ChatGPT Conversation".into());
        }
    } else if host.contains("claude.ai") {
        let articles = nodes(
            doc.root(),
            "div[data-testid=user-message],div[data-testid=assistant-message],div.font-claude-response",
        );
        if articles.is_empty() {
            return None;
        }
        site = "Claude";
        for el in articles {
            let (author, body) = if a(Some(el), "data-testid") == "user-message" {
                ("You", el)
            } else if el.attr("data-testid").is_none() && el.is(".font-claude-response") {
                ("Claude", q(el, ".standard-markdown").unwrap_or(el))
            } else {
                continue;
            };
            let content = clean(&h(Some(body)));
            if !content.is_empty() {
                messages.push(Message {
                    author: author.into(),
                    content,
                });
            }
        }
        if !title.is_empty() && title != "Claude" {
            title = title.strip_suffix(" - Claude").unwrap_or(&title).into();
        } else {
            title = fallback(
                t(q(doc.root(), "header .font-tiempos")),
                "Claude Conversation",
            );
        }
    } else if host == "gemini.google.com" {
        let containers = nodes(doc.root(), "div.conversation-container");
        if containers.is_empty() {
            return None;
        }
        site = "Gemini";
        for el in &containers {
            if let Some(query) = q(*el, "user-query .query-text") {
                messages.push(Message {
                    author: "You".into(),
                    content: h(Some(query)).trim().into(),
                });
            }
            if let Some(answer) = q(*el, "model-response").and_then(|n| {
                q(n, "#extended-response-markdown-content")
                    .or_else(|| q(n, ".model-response-text .markdown"))
            }) {
                let fragment = Document::fragment(h(Some(answer)));
                for n in nodes(fragment.root(), ".table-content") {
                    let class = a(Some(n), "class")
                        .split_whitespace()
                        .filter(|c| *c != "table-content")
                        .collect::<Vec<_>>()
                        .join(" ");
                    n.set_attr("class", &class);
                }
                messages.push(Message {
                    author: "Gemini".into(),
                    content: fragment.html().trim().into(),
                });
            }
        }
        for item in nodes(doc.root(), "browse-item") {
            let url = a(q(item, "a"), "href");
            let domain = t(q(item, ".domain"));
            let title = t(q(item, ".title"));
            if !url.is_empty() && (!domain.is_empty() || !title.is_empty()) {
                footnotes.push(Footnote {
                    url,
                    label: if title.is_empty() {
                        domain
                    } else {
                        format!("{domain}: {title}")
                    },
                });
            }
        }
        if title.is_empty() || title.contains("Gemini") {
            title = fallback(
                t(q(doc.root(), ".title-text")),
                &containers
                    .first()
                    .and_then(|n| q(*n, ".query-text"))
                    .map(|n| short_title(&n.text()))
                    .unwrap_or_else(|| "Gemini Conversation".into()),
            );
        }
    } else if host == "grok.com" {
        let containers = nodes(
            doc.root(),
            ".relative.group.flex.flex-col.justify-center.w-full",
        );
        if containers.is_empty() {
            return None;
        }
        site = "Grok";
        for el in &containers {
            let user = el.is(".items-end");
            if !user && !el.is(".items-start") {
                continue;
            }
            let Some(bubble) = q(*el, ".message-bubble") else {
                continue;
            };
            let content = if user {
                esc(&bubble.text())
            } else {
                let fragment = Document::fragment(h(Some(bubble)));
                if let Some(n) = q(
                    fragment.root(),
                    ".relative.border.border-border-l1.bg-surface-base",
                ) {
                    n.remove_from_parent();
                }
                citation(fragment.root(), &mut footnotes, true);
                fragment.html().to_string()
            };
            if !content.trim().is_empty() {
                messages.push(Message {
                    author: if user { "You" } else { "Grok" }.into(),
                    content: content.trim().into(),
                });
            }
        }
        if !title.is_empty() && title != "Grok" && !title.starts_with("Grok by ") {
            title = re!(r"\s-\s*Grok$").replace(&title, "").trim().into();
        } else {
            title = containers
                .iter()
                .find(|n| n.is(".items-end"))
                .and_then(|n| q(*n, ".message-bubble"))
                .map(|n| short_title(n.text().trim()))
                .unwrap_or_else(|| "Grok Conversation".into());
        }
    } else {
        return None;
    }
    // The upstream conversation adapter runs nested extraction independently of
    // outer pipeline options. Preserve task-list checkboxes while dropping UI.
    for message in &mut messages {
        message.content = clean(&message.content);
    }
    let mut html = String::from("<article>");
    for (i, message) in messages.iter().enumerate() {
        if i > 0 {
            html.push_str("<hr>");
        }
        html.push_str(&format!("<p><strong>{}</strong></p>", esc(&message.author)));
        if re!(r"(?is)<p\b[^>]*>.*?</p>").is_match(&message.content) {
            html.push_str(&message.content);
        } else {
            html.push_str(&format!("<p>{}</p>", message.content));
        }
    }
    if !footnotes.is_empty() {
        html.push_str("<div id=\"footnotes\"><ol>");
        for (i, footnote) in footnotes.iter().enumerate() {
            let num = i + 1;
            html.push_str(&format!("<li id=\"fn:{num}\"><p>{} <a class=\"footnote-backref\" href=\"#fnref:{num}\">↩</a></p></li>",link(&footnote.url,&footnote.label)));
        }
        html.push_str("</ol></div>");
    }
    html.push_str("</article>");
    let mut meta = metadata.clone();
    meta.title = title;
    meta.site = site.into();
    meta.description = format!("{site} conversation with {} messages", messages.len());
    Some(direct(&format!("{site}Extractor"), html, meta))
}
fn short_title(s: &str) -> String {
    format!(
        "{}{}",
        s.chars().take(50).collect::<String>(),
        if s.chars().count() > 50 { "..." } else { "" }
    )
}
