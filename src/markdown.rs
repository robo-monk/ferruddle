//! Defuddle's Markdown dialect, rendered from the same inert DOM as extraction.
//!
//! Text, destinations and attribute labels each have separate escaping rules.
//! Kept HTML is serialized only after the unconditional safety pass.
use std::collections::HashSet;
use std::fmt::Write;
use std::sync::LazyLock;

use dom_query::{Document, NodeId, NodeRef};
use regex::Regex;
use url::Url;

use crate::utils::tag as node_tag;
use crate::{Error, MarkdownOutput};

static ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)&(?:#[0-9]+|#x[0-9a-f]+|[a-z][a-z0-9]*);").unwrap());
static ORDERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*([0-9]+)([.)]) ").unwrap());
static YOUTUBE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:www\.)?(?:youtube\.com|youtube-nocookie\.com|youtu\.be)$").unwrap()
});

/// Convert the supplied document, including static declarative shadow content.
/// Resolve relative URLs against the page URL and an authored `<base href>`.
/// Like extraction, this dialect omits a leading H1 article title.
pub fn convert(html: &str, page_url: Option<&str>) -> Result<MarkdownOutput, Error> {
    let document = Document::from(html);
    crate::dom::flatten_declarative_shadow_roots(&document);
    let warnings = crate::math::warnings(&document);
    let base_url = crate::finalize::base_url(&document, page_url);
    crate::safety::sanitize(document.root());
    let root = document.body().unwrap_or_else(|| document.root());
    crate::finalize::urls(root, base_url.as_ref());
    // URL resolution is a transform; enforce safety on its results as well.
    crate::safety::sanitize(root);
    let markdown = convert_node(root, base_url.as_ref().map(Url::as_str));
    Ok(MarkdownOutput { markdown, warnings })
}

/// Render a DOM which has already passed through `safety::sanitize`.
/// Kept private to the crate so callers cannot accidentally bypass sanitization.
pub(crate) fn convert_node(root: NodeRef<'_>, page_url: Option<&str>) -> String {
    let definitions = root
        .descendants_it()
        .filter_map(|node| {
            node.attr("id")
                .and_then(|id| id.strip_prefix("fn:").map(str::to_owned))
        })
        .collect();
    let base_url = page_url.and_then(|value| Url::parse(value).ok());
    let (prose_blocks, markdown_blocks) = block_containment(root);
    let renderer = Renderer {
        definitions,
        base_url,
        prose_blocks,
        markdown_blocks,
        #[cfg(test)]
        rendered: std::cell::RefCell::new(HashSet::new()),
    };
    let mut markdown = renderer.render(root, Context::default());
    markdown = markdown.trim().to_owned();
    // Defuddle excludes a leading article title from Markdown.
    if markdown.starts_with("# ")
        && let Some(end) = markdown.find('\n')
    {
        markdown = markdown[end..].trim_start().to_owned();
    } else if markdown.starts_with("# ") {
        markdown.clear();
    }
    markdown
}

#[derive(Clone, Copy, Default)]
struct Context {
    in_table: bool,
    depth: usize,
}

struct Renderer {
    definitions: HashSet<String>,
    base_url: Option<Url>,
    prose_blocks: HashSet<NodeId>,
    markdown_blocks: HashSet<NodeId>,
    #[cfg(test)]
    rendered: std::cell::RefCell<HashSet<NodeId>>,
}

/// Keep an inline node's undecorated content as well as its kind. A lookahead
/// may become the start of the next merged run, so caching only its final
/// Markdown would still render its descendants twice in that case.
enum RenderedChild {
    Other(String),
    Inline { kind: &'static str, content: String },
}

impl RenderedChild {
    fn is_empty(&self) -> bool {
        match self {
            Self::Other(output) => output.is_empty(),
            Self::Inline { content, .. } => content.is_empty(),
        }
    }

    fn into_string(self) -> String {
        match self {
            Self::Other(output) => output,
            Self::Inline { kind, content } => {
                if kind == "code" {
                    code_span(&content)
                } else {
                    delimit(&content, kind)
                }
            }
        }
    }
}

impl Renderer {
    fn children(&self, node: NodeRef<'_>, context: Context) -> String {
        let mut output = String::new();
        let mut inline = String::new();
        let mut children = node
            .children_it(false)
            .map(|child| (child, self.render_child(child, context)))
            .peekable();
        while let Some((child, rendered)) = children.next() {
            let rendered = if let RenderedChild::Inline { kind, content } = rendered {
                let mut siblings = vec![content];
                while let Some((next, rendered)) = children.peek() {
                    if matches!(rendered, RenderedChild::Inline { kind: next_kind, .. } if *next_kind == kind)
                    {
                        if let Some((_, RenderedChild::Inline { content, .. })) = children.next() {
                            siblings.push(content);
                        }
                    } else if !self.markdown_blocks.contains(&next.id) && rendered.is_empty() {
                        children.next();
                    } else {
                        break;
                    }
                }
                if siblings.len() > 1 {
                    let joined = if kind == "code" {
                        code_span(&siblings.concat())
                    } else {
                        let mut inner = String::new();
                        for sibling in siblings {
                            append(&mut inner, &sibling);
                        }
                        delimit(&inner, kind)
                    };
                    append(&mut inline, &joined);
                    continue;
                }
                RenderedChild::Inline {
                    kind,
                    content: siblings.pop().unwrap_or_default(),
                }
            } else {
                rendered
            };
            if self.markdown_blocks.contains(&child.id) || node_tag(child) == "br" {
                flush_inline(&mut output, &mut inline);
                append(&mut output, &rendered.into_string());
            } else {
                append(&mut inline, &rendered.into_string());
            }
        }
        flush_inline(&mut output, &mut inline);
        output
    }

    fn render(&self, node: NodeRef<'_>, context: Context) -> String {
        self.render_child(node, context).into_string()
    }

    fn render_child(&self, node: NodeRef<'_>, context: Context) -> RenderedChild {
        #[cfg(test)]
        assert!(
            self.rendered.borrow_mut().insert(node.id),
            "Markdown must render each node at most once"
        );
        if context.depth <= 512
            && let Some(kind) = inline_kind(node)
        {
            let content = if kind == "code" {
                let mut text = String::new();
                pre_text(node, &mut text, 0);
                text
            } else {
                self.children(
                    node,
                    Context {
                        depth: context.depth + 1,
                        ..context
                    },
                )
            };
            return RenderedChild::Inline { kind, content };
        }
        RenderedChild::Other(self.render_other(node, context))
    }

    fn render_other(&self, node: NodeRef<'_>, context: Context) -> String {
        if context.depth > 512 {
            return escape_text(&collapse_spaces(&node.text()));
        }
        let context = Context {
            depth: context.depth + 1,
            ..context
        };
        if node.is_text() {
            return escape_text(&collapse_spaces(&node.text()));
        }
        if node.is_comment() || node.is_doctype() {
            return String::new();
        }
        let tag = node_tag(node);
        if tag == "script"
            && node
                .attr("type")
                .is_some_and(|kind| kind.to_ascii_lowercase().starts_with("math/"))
        {
            let source = node.text();
            if node
                .attr("type")
                .is_some_and(|kind| kind.contains("mode=display"))
            {
                let fence = "`".repeat(3.max(longest_backticks(&source) + 1));
                return block(&format!("{fence}\n{source}\n{fence}"));
            }
            return code_span(&source);
        }
        if matches!(
            tag.as_str(),
            "script" | "style" | "head" | "template" | "noscript" | "wbr" | "input"
        ) {
            return String::new();
        }
        if node.has_class("footnote-backref")
            || node
                .attr("href")
                .is_some_and(|href| href.contains("#fnref"))
        {
            return String::new();
        }
        if tag == "sup"
            && let Some(reference) = node
                .attr("id")
                .and_then(|id| id.strip_prefix("fnref:").map(str::to_owned))
        {
            let linked = first(node, |n| {
                node_tag(n) == "a" && n.attr("href").is_some_and(|v| v.starts_with("#fn:"))
            })
            .and_then(|n| n.attr("href"))
            .map(|v| decode_fragment(&v[4..]));
            let primary = linked.unwrap_or_else(|| {
                if self.definitions.contains(&reference) {
                    reference.clone()
                } else {
                    reference
                        .rsplit_once('-')
                        .filter(|(_, suffix)| suffix.chars().all(|c| c.is_ascii_digit()))
                        .map(|(id, _)| id.to_owned())
                        .unwrap_or(reference)
                }
            });
            return format!("[^{}]", footnote_id(&primary));
        }
        if node.has_class("callout") && node.attr("data-callout").is_some() {
            return self.callout(node, context);
        }
        // Math is deliberately preserved as safe source, never translated to LaTeX.
        if crate::math::is_math_source(node)
            && !node
                .children_it(false)
                .any(|child| self.prose_blocks.contains(&child.id))
        {
            return retained_markdown(node, false);
        }
        match tag.as_str() {
            "pre" => self.pre(node, context),
            "table" => self.table(node, context),
            "ul" | "ol"
                if tag == "ol"
                    && node.parent().is_some_and(|n| {
                        n.attr("id").is_some_and(|id| id.as_ref() == "footnotes")
                    }) =>
            {
                self.footnotes(node, context)
            }
            "ul" | "ol" => self.list(node, context),
            "figure" => self.figure(node, context),
            "img" => self.image(node),
            "iframe" | "video" | "audio" => self.media(node),
            "sup" | "sub" | "svg" => retained_markdown(node, false),
            "br" => {
                if context.in_table {
                    "<br>".into()
                } else {
                    "  \n".into()
                }
            }
            "hr" => block("---"),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let body = self
                    .children(node, context)
                    .replace("  \n", " ")
                    .replace('\n', " ");
                block(&format!(
                    "{} {}",
                    "#".repeat(tag.as_bytes()[1] as usize - b'0' as usize),
                    body.trim()
                ))
            }
            "a" => self.link(node, context),
            "strong" | "b" => delimit(&self.children(node, context), "**"),
            "em" | "i" => delimit(&self.children(node, context), "*"),
            "del" | "s" | "strike" => delimit(&self.children(node, context), "~~"),
            "mark" => delimit(&self.children(node, context), "=="),
            "blockquote" => {
                let body = self.children(node, context);
                let mut lines = Vec::new();
                for line in body.trim().lines() {
                    let line = if line.is_empty() {
                        ">".to_owned()
                    } else {
                        format!("> {line}")
                    };
                    if line == ">" && lines.last().is_some_and(|previous| previous == ">") {
                        continue;
                    }
                    lines.push(line);
                }
                block(&lines.join("\n"))
            }
            "p" => block(&escape_block_prefix(self.children(node, context).trim())),
            "div" | "article" | "section" | "main" | "header" | "footer" | "aside"
            | "figcaption" | "address" | "details" | "summary" | "dl" | "dt" | "dd" | "center"
            | "fieldset" | "form" => block(self.children(node, context).trim()),
            _ => self.children(node, context),
        }
    }

    fn destination(&self, value: &str, image: bool) -> String {
        if dangerous_url(value, image) {
            return String::new();
        }
        // Both entry points resolve destinations before rendering. A missing or
        // invalid page URL leaves relative destinations as authored.
        destination(value)
    }

    fn link(&self, node: NodeRef<'_>, context: Context) -> String {
        let content = self.children(node, context);
        let href = node.attr("href").unwrap_or_default();
        let destination = self.destination(&href, false);
        if destination.is_empty() || content.trim().is_empty() {
            return content;
        }
        let title = title(node.attr("title").as_deref());
        if node.children_it(false).count() > 1
            && node.children_it(false).any(|n| {
                matches!(
                    node_tag(n).as_str(),
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                )
            })
        {
            return block(&format!(
                "{}\n\n[View original]({destination}{title})",
                content.trim()
            ));
        }
        let leading = &content[..content.len() - content.trim_start().len()];
        let trailing = &content[content.trim_end().len()..];
        format!(
            "{leading}[{}]({destination}{title}){trailing}",
            content.trim()
        )
    }

    fn image(&self, node: NodeRef<'_>) -> String {
        let source = best_image_src(node);
        let destination = self.destination(&source, true);
        if destination.is_empty() {
            return String::new();
        }
        let alt = escape_text(&collapse_spaces(&node.attr("alt").unwrap_or_default()));
        format!(
            "![{alt}]({destination}{})",
            title(node.attr("title").as_deref())
        )
    }

    fn figure(&self, node: NodeRef<'_>, context: Context) -> String {
        // Figure wrappers can also contain image links, credits and surrounding
        // prose. Render every child rather than selecting only one image/caption.
        block(self.children(node, context).trim())
    }

    fn media(&self, node: NodeRef<'_>) -> String {
        if let Some(source) = node.attr("src") {
            let parsed = Url::parse(&source).ok().or_else(|| {
                self.base_url
                    .as_ref()
                    .and_then(|base| base.join(&source).ok())
            });
            if let Some(url) = parsed {
                let host = url.host_str().unwrap_or_default();
                if YOUTUBE.is_match(host) {
                    let path: Vec<_> = url.path_segments().into_iter().flatten().collect();
                    let id = if host.ends_with("youtu.be") {
                        path.first().map(|s| (*s).to_owned())
                    } else if path.first() == Some(&"embed") {
                        path.get(1).map(|s| (*s).to_owned())
                    } else {
                        url.query_pairs()
                            .find(|(key, _)| key == "v")
                            .map(|(_, id)| id.into_owned())
                    };
                    if let Some(id) = id.filter(|id| {
                        !id.is_empty()
                            && id
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    }) {
                        return block(&format!("![](https://www.youtube.com/watch?v={id})"));
                    }
                }
                if matches!(
                    host,
                    "x.com" | "www.x.com" | "twitter.com" | "www.twitter.com"
                ) {
                    let path: Vec<_> = url.path_segments().into_iter().flatten().collect();
                    if path.len() >= 3
                        && path[1] == "status"
                        && path[0]
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_')
                        && path[2].chars().all(|c| c.is_ascii_digit())
                        && !path[2].is_empty()
                    {
                        return block(&format!(
                            "![](https://x.com/{}/status/{})",
                            path[0], path[2]
                        ));
                    }
                }
                if host == "platform.twitter.com"
                    && url.path() == "/embed/Tweet.html"
                    && let Some(id) = url
                        .query_pairs()
                        .find(|(key, id)| {
                            key == "id" && !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())
                        })
                        .map(|(_, id)| id.into_owned())
                {
                    return block(&format!("![](https://x.com/i/status/{id})"));
                }
            }
        }
        retained_markdown(node, true)
    }

    fn pre(&self, node: NodeRef<'_>, context: Context) -> String {
        let code = first(node, |n| node_tag(n) == "code").unwrap_or(node);
        let mut text = String::new();
        pre_text(node, &mut text, 0);
        if context.in_table {
            return code_span(&text);
        }
        let language = code
            .attr("data-lang")
            .or_else(|| code.attr("data-language"))
            .or_else(|| {
                code.attr("class").and_then(|classes| {
                    classes
                        .split_whitespace()
                        .find_map(|class| class.strip_prefix("language-").map(Into::into))
                })
            })
            .or_else(|| node.attr("data-language"))
            .unwrap_or_default();
        let language = language
            .split_whitespace()
            .next()
            .unwrap_or("")
            .replace('`', "");
        // Preserve indentation and internal blank lines. Remove only the customary
        // boundary newline, never significant source spaces.
        let text = text.strip_prefix('\n').unwrap_or(&text);
        let text = text.strip_suffix('\n').unwrap_or(text);
        let fence = "`".repeat(3.max(longest_backticks(text) + 1));
        block(&format!("{fence}{language}\n{text}\n{fence}"))
    }

    fn list(&self, node: NodeRef<'_>, context: Context) -> String {
        let ordered = node_tag(node) == "ol";
        let start = node
            .attr("start")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(1);
        let count = node
            .children_it(false)
            .filter(|child| node_tag(*child) == "li")
            .count();
        if ordered
            && (node.has_attr("reversed")
                || node
                    .children_it(false)
                    .any(|child| node_tag(child) == "li" && child.has_attr("value"))
                || node.attr("type").is_some_and(|kind| kind.as_ref() != "1")
                || start < 0
                || start.saturating_add(count.saturating_sub(1) as i64) > 999_999_999)
        {
            // CommonMark has no negative or ten-digit ordered list markers.
            return retained_markdown(node, true);
        }
        let mut items = Vec::new();
        for (index, item) in node
            .children_it(false)
            .filter(|n| node_tag(*n) == "li")
            .enumerate()
        {
            let mut body = self.children(item, context).trim().to_owned();
            let mut marker = if ordered {
                format!("{}. ", start.saturating_add(index as i64))
            } else {
                "- ".to_owned()
            };
            let continuation = if marker.len() <= 4 {
                "\t".to_owned()
            } else {
                " ".repeat(marker.len())
            };
            if let Some(checkbox) = first(item, |n| {
                node_tag(n) == "input"
                    && n.attr("type")
                        .is_some_and(|value| value.eq_ignore_ascii_case("checkbox"))
                    && n.ancestors_it(None)
                        .find(|ancestor| node_tag(*ancestor) == "li")
                        .is_some_and(|owner| owner.id == item.id)
            }) {
                marker.push_str(if checkbox.has_attr("checked") {
                    "[x] "
                } else {
                    "[ ] "
                });
            }
            // arXiv numbering is already supplied by the list marker.
            if node.has_class("ltx_enumerate")
                && let Some(label) = first(item, |n| n.has_class("ltx_tag_item"))
            {
                let prefix = escape_text(&collapse_spaces(&label.text()));
                body = body
                    .trim_start()
                    .strip_prefix(&prefix)
                    .unwrap_or(&body)
                    .trim_start()
                    .to_owned();
            }
            let mut lines = body.lines();
            let first = lines.next().unwrap_or("");
            let mut rendered = format!("{marker}{first}");
            for line in lines {
                rendered.push('\n');
                rendered.push_str(&continuation);
                rendered.push_str(line);
            }
            items.push(rendered);
        }
        block(&items.join("\n"))
    }

    fn footnotes(&self, node: NodeRef<'_>, context: Context) -> String {
        let mut definitions = Vec::new();
        for item in node.children_it(false).filter(|n| node_tag(*n) == "li") {
            let id = item.attr("id").unwrap_or_default();
            let id = id.strip_prefix("fn:").unwrap_or(&id);
            let content = self.children(item, context);
            let content = content.trim().trim_end_matches('↩').trim();
            let mut lines = content.lines();
            let mut definition = format!("[^{}]: {}", footnote_id(id), lines.next().unwrap_or(""));
            // Continuation indentation is required for paragraph/list/code blocks
            // to remain inside the footnote in CommonMark extensions.
            for line in lines {
                definition.push_str("\n    ");
                definition.push_str(line);
            }
            definitions.push(definition);
        }
        block(&definitions.join("\n\n"))
    }

    fn callout(&self, node: NodeRef<'_>, context: Context) -> String {
        let raw_type = node.attr("data-callout").unwrap_or_else(|| "note".into());
        let kind = if !raw_type.is_empty()
            && raw_type
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            raw_type.as_ref()
        } else {
            "note"
        };
        let fold = node.attr("data-callout-fold").unwrap_or_default();
        let fold = if fold.as_ref() == "+" || fold.as_ref() == "-" {
            fold.as_ref()
        } else {
            ""
        };
        let default_title = format!("{}{}", kind[..1].to_ascii_uppercase(), &kind[1..]);
        let heading = first(node, |n| n.has_class("callout-title-inner"))
            .map(|n| collapse_spaces(&n.text()))
            .unwrap_or(default_title);
        let mut content = String::new();
        if let Some(body) = first(node, |n| n.has_class("callout-content")) {
            content = self.children(body, context);
        } else {
            for child in node
                .children_it(false)
                .filter(|n| !n.has_class("callout-title"))
            {
                append(&mut content, &self.render(child, context));
            }
        }
        let mut result = format!("> [!{kind}]{fold} {}", escape_text(heading.trim()));
        for line in content.trim().lines() {
            result.push_str("\n> ");
            result.push_str(line);
        }
        block(&result)
    }

    fn table(&self, node: NodeRef<'_>, context: Context) -> String {
        let rows: Vec<_> = node
            .descendants_it()
            .filter(|n| {
                node_tag(*n) == "tr" && nearest_table(*n).is_some_and(|table| table.id == node.id)
            })
            .collect();
        let cells: Vec<Vec<_>> = rows
            .iter()
            .map(|row| {
                row.children_it(false)
                    .filter(|n| matches!(node_tag(*n).as_str(), "td" | "th"))
                    .collect()
            })
            .collect();
        let caption = || {
            node.children_it(false)
                .find(|n| node_tag(*n) == "caption")
                .map(|n| self.children(n, context))
                .unwrap_or_default()
        };
        let nested = node.descendants_it().any(|n| node_tag(n) == "table");
        let single_column = !cells.is_empty() && cells.iter().all(|row| row.len() <= 1);
        if nested || single_column {
            let caption = caption();
            let mut result = block(caption.trim());
            for row in &cells {
                for cell in row {
                    append(&mut result, &block(self.children(*cell, context).trim()));
                }
            }
            return block(result.trim());
        }
        let width = cells.iter().map(Vec::len).max().unwrap_or(0);
        let alignment: Vec<_> = (0..width)
            .map(|column| {
                cells
                    .iter()
                    .find_map(|row| row.get(column).and_then(|cell| cell_alignment(*cell)))
            })
            .collect();
        let mixed_alignment = cells.iter().any(|row| {
            row.iter().enumerate().any(|(column, cell)| {
                alignment[column].is_some() && cell_alignment(*cell) != alignment[column]
            })
        });
        let rich_cells = cells.iter().flatten().any(|cell| {
            cell.descendants_it().any(|child| {
                matches!(
                    node_tag(child).as_str(),
                    "pre"
                        | "ul"
                        | "ol"
                        | "blockquote"
                        | "hr"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                )
            }) || cell
                .children_it(false)
                .filter(|child| node_tag(*child) == "p")
                .count()
                > 1
        });
        if mixed_alignment
            || rich_cells
            || cells
                .iter()
                .flatten()
                .any(|n| n.has_attr("colspan") || n.has_attr("rowspan"))
        {
            // Source attributes have already passed through unconditional
            // sanitization. Preserve meaningful nested-list, task and code attrs;
            // stripping an inert script's MIME type could make it executable.
            return retained_markdown(node, true);
        }
        if width == 0 {
            return self.children(node, context);
        }
        let mut result = Vec::new();
        for (index, row) in cells.iter().enumerate() {
            let mut values: Vec<_> = row
                .iter()
                .map(|cell| {
                    self.children(
                        *cell,
                        Context {
                            in_table: true,
                            ..context
                        },
                    )
                    .trim()
                    .replace('\n', " ")
                    .replace('|', "\\|")
                })
                .collect();
            values.resize(width, String::new());
            result.push(format!("| {} |", values.join(" | ")));
            if index == 0 {
                let separators: Vec<_> = alignment
                    .iter()
                    .map(|align| match align.as_deref() {
                        Some("left") => ":---",
                        Some("center") => ":---:",
                        Some("right") => "---:",
                        _ => "---",
                    })
                    .collect();
                result.push(format!("| {} |", separators.join(" | ")));
            }
        }
        let caption = caption();
        if caption.trim().is_empty() {
            block(&result.join("\n"))
        } else {
            block(&format!("{}\n\n{}", caption.trim(), result.join("\n")))
        }
    }
}

fn first<'a>(node: NodeRef<'a>, predicate: impl Fn(NodeRef<'a>) -> bool) -> Option<NodeRef<'a>> {
    node.descendants_it().find(|n| predicate(*n))
}
fn nearest_table(node: NodeRef<'_>) -> Option<NodeRef<'_>> {
    node.ancestors_it(None).find(|n| node_tag(*n) == "table")
}
fn block(content: &str) -> String {
    if content.is_empty() {
        String::new()
    } else {
        format!("\n\n{content}\n\n")
    }
}

/// Join at block boundaries without normalizing the interior of code or kept HTML.
fn append(output: &mut String, content: &str) {
    if content.is_empty() {
        return;
    }
    if content.starts_with('\n') {
        output.truncate(output.trim_end_matches([' ', '\t']).len());
    }
    let trailing = output.len() - output.trim_end_matches('\n').len();
    let leading = content.len() - content.trim_start_matches('\n').len();
    let mut content = content;
    if trailing + leading > 2 {
        content = &content[(trailing + leading - 2).min(leading)..];
    }
    if output.ends_with('\n') && content.starts_with(' ') {
        content = content.trim_start_matches(' ');
    }
    if output.ends_with(' ') && content.starts_with(' ') {
        content = &content[1..];
    }
    // A literal exclamation immediately before an image changes Markdown parsing.
    if output.ends_with('!') && (content.starts_with("![") || content.starts_with("[![")) {
        output.push(' ');
    }
    output.push_str(content);
}

fn collapse_spaces(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut previous_space = false;
    for ch in value.chars() {
        if matches!(ch, ' ' | '\t' | '\r' | '\n' | '\x0c' | '\u{a0}') {
            if !previous_space {
                output.push(' ');
            }
            previous_space = true;
        } else {
            output.push(ch);
            previous_space = false;
        }
    }
    output
}

fn escape_entities(value: &str) -> String {
    ENTITY
        .replace_all(value, |caps: &regex::Captures<'_>| {
            format!("&amp;{}", &caps[0][1..])
        })
        .into_owned()
}

fn escape_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let first_content = value.len() - value.trim_start_matches(char::is_whitespace).len();
    let mut entities = ENTITY.find_iter(value).peekable();
    let mut chars = value.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        while entities.peek().is_some_and(|entity| entity.start() < index) {
            entities.next();
        }
        match ch {
            '&' if entities
                .peek()
                .is_some_and(|entity| entity.start() == index) =>
            {
                output.push_str("&amp;");
                entities.next();
            }
            '\\' | '*' | '_' | '`' | '[' | ']' => {
                output.push('\\');
                output.push(ch);
            }
            '<' => {
                let rest = &value[index + 1..];
                let target = rest
                    .char_indices()
                    .find(|(_, ch)| {
                        matches!(*ch, '<' | '>') || ch.is_whitespace() || ch.is_control()
                    })
                    .filter(|(_, ch)| *ch == '>')
                    .map(|(end, _)| &rest[..end]);
                if let Some((target, href)) = target
                    .and_then(|target| autolink_destination(target).map(|href| (target, href)))
                {
                    // Explicit links preserve safe autolink behavior without
                    // leaving a page-derived '<' in any Markdown HTML context.
                    output.push('[');
                    output.push_str(&escape_text(target));
                    output.push_str("](");
                    output.push_str(&destination(&href));
                    output.push(')');
                    let end = index + target.len() + 2;
                    while chars.peek().is_some_and(|(index, _)| *index < end) {
                        chars.next();
                    }
                } else {
                    // Backslash escapes are ignored in CommonMark HTML blocks.
                    // An entity stays literal even if a surrounding raw block
                    // unexpectedly reaches this text.
                    output.push_str("&lt;");
                }
            }
            '#' | '+' | '-' | '=' | '~' | '>' if index == first_content => {
                output.push('\\');
                output.push(ch);
            }
            _ => output.push(ch),
        }
    }
    if let Some(caps) = ORDERED.captures(&output) {
        let index = caps.get(1).unwrap().end();
        output.insert(index, '\\');
    }
    output
}

fn autolink_destination(target: &str) -> Option<String> {
    if target
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"))
        || target
            .get(..8)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
    {
        return Some(target.to_owned());
    }
    // CommonMark email autolinks have an ASCII local part and domain labels
    // of at most 63 bytes, without leading/trailing hyphens. Merely containing
    // '@' also admits HTML comments, processing instructions and malformed tags.
    let (local, domain) = target.split_once('@')?;
    if local.is_empty()
        || !local.bytes().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    b'.' | b'!'
                        | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'/'
                        | b'='
                        | b'?'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'{'
                        | b'|'
                        | b'}'
                        | b'~'
                        | b'-'
                )
        })
        || !domain.split('.').all(|label| {
            label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-')
        })
    {
        return None;
    }
    Some(format!("mailto:{target}"))
}

fn delimit(content: &str, delimiter: &str) -> String {
    if content.trim().is_empty() {
        return content.to_owned();
    }
    let leading = &content[..content.len() - content.trim_start().len()];
    let trailing = &content[content.trim_end().len()..];
    format!(
        "{leading}{delimiter}{}{delimiter}{trailing}",
        content.trim()
    )
}

fn longest_backticks(value: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for byte in value.bytes() {
        if byte == b'`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn code_span(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let value = value.replace(['\n', '\r'], " ");
    let fence = "`".repeat(longest_backticks(&value) + 1);
    let pad = value.starts_with('`')
        || value.ends_with('`')
        || (value.starts_with(' ') && value.ends_with(' ') && !value.trim().is_empty());
    if pad {
        format!("{fence} {value} {fence}")
    } else {
        format!("{fence}{value}{fence}")
    }
}

fn dangerous_url(value: &str, image: bool) -> bool {
    crate::safety::is_dangerous_url(value, image)
}

/// Retained HTML must not change CommonMark's parsing mode for the prose after
/// it. Type 1/6 HTML blocks need blank lines; inline void roots need a closing
/// wrapper so a hard break cannot turn a lone opening tag into a type 7 block.
fn retained_markdown(node: NodeRef<'_>, force_block: bool) -> String {
    let html = retained_html(node);
    let tag = node_tag(node);
    if force_block || commonmark_block_tag(&tag) {
        block(&html)
    } else if crate::dom::is_html_element(node, &tag) && html_void_tag(&tag) {
        format!("<span>{html}</span>")
    } else {
        html
    }
}

fn html_void_tag(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// CommonMark HTML-block type 6's complete tag list, plus type 1 raw-text tags.
/// This is a Markdown boundary rule, including such HTML-inline tags as iframe
/// and title; it is not inferred from CSS display or a subset of article tags.
fn commonmark_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "address"
            | "article"
            | "aside"
            | "base"
            | "basefont"
            | "blockquote"
            | "body"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hr"
            | "html"
            | "iframe"
            | "legend"
            | "li"
            | "link"
            | "main"
            | "menu"
            | "menuitem"
            | "nav"
            | "noframes"
            | "ol"
            | "optgroup"
            | "option"
            | "p"
            | "param"
            | "search"
            | "section"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "track"
            | "ul"
            | "pre"
            | "script"
            | "style"
            | "textarea"
    )
}

/// Serialize a sanitized subtree as literal HTML in CommonMark. HTML parsers
/// accept names which CommonMark rejects, and inline HTML does not protect text
/// from Markdown parsing. Validate names and escape each DOM value where it is
/// emitted, rather than inferring text/attribute boundaries from an HTML string.
fn retained_html(root: NodeRef<'_>) -> String {
    enum Op<'a> {
        Open(NodeRef<'a>),
        Close(String),
    }

    let mut output = String::new();
    let mut pending = vec![Op::Open(root)];
    while let Some(op) = pending.pop() {
        let node = match op {
            Op::Close(tag) => {
                output.push_str("</");
                output.push_str(&tag);
                output.push('>');
                continue;
            }
            Op::Open(node) => node,
        };
        if node.is_text() {
            retained_text(&node.text(), &mut output);
            continue;
        }
        let Some(element) = node.element_ref() else {
            if node.is_document() || node.is_fragment() {
                pending.extend(node.children_it(true).map(Op::Open));
            }
            continue;
        };
        let original_tag = element.name.local.as_ref();
        // Scripts have already been restricted to inert math source. Rendering
        // their literal text in code also makes later MIME-attribute filtering
        // safe. Other legacy raw-text holders likewise need a text-decoding tag.
        let code_source = matches!(
            original_tag,
            "script" | "xmp" | "noembed" | "noframes" | "plaintext"
        );
        let tag = if code_source { "code" } else { original_tag };
        let mut name = tag.bytes();
        if !name.next().is_some_and(|ch| ch.is_ascii_alphabetic())
            || !name.all(|ch| ch.is_ascii_alphanumeric() || ch == b'-')
        {
            // Invalid element names cannot be emitted as CommonMark HTML.
            // Unwrap their children rather than exposing literal Markdown text.
            pending.extend(node.children_it(true).map(Op::Open));
            continue;
        }
        output.push('<');
        output.push_str(tag);
        if !code_source {
            for attribute in &element.attrs {
                let local = attribute.name.local.as_ref();
                let prefix = match attribute.name.ns.as_ref() {
                    "http://www.w3.org/XML/1998/namespace" => Some("xml"),
                    "http://www.w3.org/2000/xmlns/" if local != "xmlns" => Some("xmlns"),
                    "http://www.w3.org/1999/xlink" => Some("xlink"),
                    _ => attribute.name.prefix.as_ref().map(|prefix| prefix.as_ref()),
                };
                let name =
                    prefix.map_or_else(|| local.to_owned(), |prefix| format!("{prefix}:{local}"));
                let mut bytes = name.bytes();
                if !bytes
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphabetic() || matches!(ch, b'_' | b':'))
                    || !bytes.all(|ch| {
                        ch.is_ascii_alphanumeric() || matches!(ch, b'_' | b'.' | b':' | b'-')
                    })
                {
                    continue;
                }
                output.push(' ');
                output.push_str(&name);
                output.push_str("=\"");
                for ch in attribute.value.chars() {
                    match ch {
                        '&' => output.push_str("&amp;"),
                        '<' => output.push_str("&lt;"),
                        '>' => output.push_str("&gt;"),
                        '"' => output.push_str("&quot;"),
                        '\'' => output.push_str("&#39;"),
                        '\n' => output.push_str("&#10;"),
                        '\r' => output.push_str("&#13;"),
                        '\t' => output.push_str("&#9;"),
                        _ => output.push(ch),
                    }
                }
                output.push('"');
            }
        }
        output.push('>');
        let html = element.name.ns.as_ref() == "http://www.w3.org/1999/xhtml";
        if html && html_void_tag(tag) {
            continue;
        }
        // HTML parsing drops the first LF after pre/textarea/listing. Preserve
        // an already-parsed leading LF with one additional character reference.
        if html
            && matches!(tag, "pre" | "textarea" | "listing")
            && node
                .first_child()
                .is_some_and(|child| child.is_text() && child.text().starts_with('\n'))
        {
            output.push_str("&#10;");
        }
        pending.push(Op::Close(tag.to_owned()));
        pending.extend(node.children_it(true).map(Op::Open));
    }
    output
}

/// Numeric references decode only after Markdown has recognized its syntax.
/// Protect all punctuation and physical line breaks while preserving parsed
/// text exactly, including whitespace inside retained pre/code and MathML.
fn retained_text(value: &str, output: &mut String) {
    for ch in value.chars() {
        if ch.is_ascii_punctuation() || matches!(ch, '\n' | '\r' | '\t') {
            write!(output, "&#{};", ch as u32).expect("writing to String cannot fail");
        } else {
            output.push(ch);
        }
    }
}

fn destination(value: &str) -> String {
    let value = escape_entities(value);
    let spaces = value.chars().any(char::is_whitespace);
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '<' | '>' | '\x00'..='\x1f' | '\x7f' => {
                for byte in ch.to_string().bytes() {
                    output.push_str(&format!("%{byte:02X}"));
                }
            }
            '(' | ')' if !spaces => {
                output.push('\\');
                output.push(ch);
            }
            _ => output.push(ch),
        }
    }
    if spaces {
        format!("<{output}>")
    } else {
        output
    }
}

fn title(value: Option<&str>) -> String {
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return String::new();
    };
    let value = value
        .replace(['\r', '\n'], " ")
        .replace('\\', "\\\\")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    format!(" \"{value}\"")
}

fn best_image_src(node: NodeRef<'_>) -> String {
    let mut best = None;
    if let Some(srcset) = node.attr("srcset") {
        for (url, descriptor) in crate::dom::srcset_candidates(&srcset) {
            if let Some(width) = descriptor
                .strip_suffix('w')
                .and_then(|v| v.parse::<u32>().ok())
                && !url.is_empty()
                && !dangerous_url(&url, true)
                && best
                    .as_ref()
                    .is_none_or(|(best_width, _)| width > *best_width)
            {
                best = Some((width, url));
            }
        }
    }
    best.map(|(_, value)| value)
        .unwrap_or_else(|| node.attr("src").unwrap_or_default().to_string())
}

fn footnote_id(value: &str) -> String {
    let mut output = String::new();
    for ch in value.to_lowercase().encode_utf16() {
        if (ch as u32 <= 127)
            && (char::from_u32(ch as u32).unwrap().is_ascii_alphanumeric()
                || ch == b'_' as u16
                || ch == b'-' as u16)
        {
            output.push(char::from_u32(ch as u32).unwrap());
        } else {
            output.push_str(&format!("%{ch:04x}"));
        }
    }
    output
}

fn decode_fragment(value: &str) -> String {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return value.to_owned();
            }
            let (Some(hi), Some(lo)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            ) else {
                return value.to_owned();
            };
            let byte = (hi * 16 + lo) as u8;
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_owned())
}

/// Escape a literal paragraph prefix after inline wrappers and edge whitespace
/// have been rendered. This also catches ordered markers split across spans.
fn escape_block_prefix(value: &str) -> String {
    if value.starts_with(['#', '+', '-', '>']) || value.starts_with("~~~") {
        return format!("\\{value}");
    }
    if let Some(captures) = ORDERED.captures(value) {
        let index = captures.get(1).unwrap().end();
        let mut value = value.to_owned();
        value.insert(index, '\\');
        return value;
    }
    value.to_owned()
}

fn pre_text(node: NodeRef<'_>, output: &mut String, depth: usize) {
    if node.is_text() {
        output.push_str(&node.text());
        return;
    }
    if depth > 512 {
        output.push_str(&node.text());
        return;
    }
    let tag = node_tag(node);
    if matches!(
        tag.as_str(),
        "button" | "script" | "style" | "template" | "noscript"
    ) {
        return;
    }
    if tag == "br" {
        output.push('\n');
        return;
    }
    let block = matches!(tag.as_str(), "div" | "p" | "li" | "tr");
    if block && !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    for child in node.children_it(false) {
        pre_text(child, output, depth + 1);
    }
    if block && !output.ends_with('\n') {
        output.push('\n');
    }
}

fn cell_alignment(node: NodeRef<'_>) -> Option<String> {
    let mut alignment = node
        .attr("align")
        .map(|value| value.trim().to_ascii_lowercase());
    if let Some(style) = node.attr("style") {
        for (name, value) in style.split(';').filter_map(|entry| entry.split_once(':')) {
            if name.trim().eq_ignore_ascii_case("text-align") {
                alignment = Some(
                    value
                        .trim()
                        .to_ascii_lowercase()
                        .trim_end_matches("!important")
                        .trim()
                        .to_owned(),
                );
            }
        }
    }
    alignment.filter(|value| matches!(value.as_str(), "left" | "center" | "right"))
}

// Track authored block boundaries before rendering so literal marker fragments
// split across inline nodes cannot become generated Markdown structure.
fn block_containment(root: NodeRef<'_>) -> (HashSet<NodeId>, HashSet<NodeId>) {
    let nodes: Vec<_> = std::iter::once(root).chain(root.descendants_it()).collect();
    let mut prose = HashSet::new();
    let mut markdown = HashSet::new();
    // Descendants are in document order. Visiting them in reverse means every
    // child's containment is known before its parent, with one pass per edge.
    // Keep prose and CommonMark blocks separate: an inert math script can live
    // inside a source holder without turning that holder into a prose section.
    for node in nodes.into_iter().rev().filter(|node| node.is_element()) {
        let tag = node_tag(node);
        let native_prose = matches!(
            tag.as_str(),
            "p" | "div"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "article"
                | "section"
                | "main"
                | "header"
                | "footer"
                | "aside"
                | "figcaption"
                | "figure"
                | "address"
                | "details"
                | "summary"
                | "dl"
                | "dt"
                | "dd"
                | "center"
                | "fieldset"
                | "form"
                | "pre"
                | "blockquote"
                | "ul"
                | "ol"
                | "li"
                | "table"
                | "thead"
                | "tbody"
                | "tfoot"
                | "tr"
                | "td"
                | "th"
                | "hr"
                | "menu"
                | "search"
                | "legend"
                | "nav"
                | "dialog"
                | "dir"
        );
        if native_prose
            || node
                .children_it(false)
                .any(|child| prose.contains(&child.id))
        {
            prose.insert(node.id);
        }
        if commonmark_block_tag(&tag)
            || node
                .children_it(false)
                .any(|child| markdown.contains(&child.id))
        {
            markdown.insert(node.id);
        }
    }
    (prose, markdown)
}

fn flush_inline(output: &mut String, inline: &mut String) {
    let start = inline.len() - inline.trim_start_matches([' ', '\t']).len();
    let escaped = escape_block_prefix(&inline[start..]);
    let mut content = String::with_capacity(inline.len() + 1);
    content.push_str(&inline[..start]);
    content.push_str(&escaped);
    append(output, &content);
    inline.clear();
}

fn inline_kind(node: NodeRef<'_>) -> Option<&'static str> {
    if crate::math::is_math_source(node)
        || node.has_class("footnote-backref")
        || (node.has_class("callout") && node.has_attr("data-callout"))
        || node
            .attr("href")
            .is_some_and(|href| href.contains("#fnref"))
    {
        return None;
    }
    match node_tag(node).as_str() {
        "code" | "tt" => Some("code"),
        "strong" | "b" => Some("**"),
        "em" | "i" => Some("*"),
        "del" | "s" | "strike" => Some("~~"),
        "mark" => Some("=="),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{convert, escape_text};

    #[test]
    fn page_text_stays_literal_even_inside_an_unexpected_html_block() {
        let escaped = escape_text(
            "<u data-probe>marker</u> <!--@x--> <?p@q?> <![CDATA[@x]]> <u/data-probe=1@x> <https://example.com>",
        );
        assert!(!escaped.contains('<'), "{escaped}");
        let mut rendered = String::new();
        pulldown_cmark::html::push_html(
            &mut rendered,
            pulldown_cmark::Parser::new(&format!("<div>\n{escaped}\n</div>")),
        );
        let document = dom_query::Document::from(rendered);
        assert!(document.select("u").is_empty());
        assert!(document.text().contains("<u data-probe>marker</u>"));
    }

    #[test]
    fn zero_row_table_renders_its_caption_once() {
        assert_eq!(
            convert(
                "<table><caption>Only <em>caption</em>.</caption></table>",
                None
            )
            .unwrap()
            .markdown,
            "Only *caption*."
        );
    }

    #[test]
    fn nested_lookahead_that_starts_an_inline_merge_renders_once() {
        let mut source = "center".to_owned();
        for _ in 0..200 {
            source = format!("<em>prefix(</em><strong>{source}</strong><b>tail</b>");
        }
        // The test-only per-node assertion fails immediately on repeated work,
        // even if a future change reintroduces an exponential lookahead.
        let markdown = convert(&format!("<p>{source}</p>"), None).unwrap().markdown;
        assert_eq!(markdown.matches("prefix(").count(), 200);
        assert_eq!(markdown.matches("tail").count(), 200);
        assert_eq!(markdown.matches("center").count(), 1);
    }
}
