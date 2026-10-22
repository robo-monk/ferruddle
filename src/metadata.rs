//! Metadata precedence and heuristics shared by browser and offline extraction.
use crate::Metadata;
use dom_query::{Document, NodeRef, Selection};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;
use url::Url;

macro_rules! re {
    ($pattern:expr) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).unwrap());
        &*RE
    }};
}

struct Meta {
    name: String,
    property: String,
    content: String,
}
struct Sources<'a> {
    doc: &'a Document,
    schema: Value,
    tags: Vec<Meta>,
}

/// Extract metadata without mutating the page or fetching external resources.
pub fn extract(doc: &Document, page_url: Option<&str>) -> Metadata {
    let sources = Sources {
        doc,
        schema: collect_schema(doc),
        tags: doc
            .select("meta")
            .nodes()
            .iter()
            .filter_map(|node| {
                let content = node.attr("content")?;
                if content.is_empty() {
                    return None;
                }
                Some(Meta {
                    name: attr(*node, "name").to_lowercase(),
                    property: attr(*node, "property").to_lowercase(),
                    content: decode_entities(&content).trim().to_string(),
                })
            })
            .collect(),
    };
    let url = page_url
        .filter(|s| !s.is_empty() && *s != "about:blank")
        .map(str::to_string)
        .unwrap_or_else(|| {
            first_nonempty([
                sources.meta("property", "og:url"),
                sources.meta("property", "twitter:url"),
                sources.schema("url"),
                sources.schema("mainEntityOfPage.url"),
                sources.schema("mainEntity.url"),
                sources.schema("WebSite.url"),
                doc.select_single("link[rel=canonical]")
                    .attr("href")
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
            ])
        });
    let domain = Url::parse(&url)
        .ok()
        .and_then(|url| {
            url.host_str()
                .map(|s| s.strip_prefix("www.").unwrap_or(s).to_string())
        })
        .unwrap_or_default();
    let site_name = sources.site_name();
    let (title, detected_site) = clean_title(&sources.best_title(&domain, &site_name), &site_name);
    let author = sources.author();
    let site = first_nonempty([
        site_name,
        detected_site,
        if author.contains(',') {
            String::new()
        } else {
            author.clone()
        },
        domain.clone(),
    ]);
    Metadata {
        title,
        author,
        site,
        domain,
        description: sources.first_valid(&[
            ("name", "description"),
            ("property", "description"),
            ("property", "og:description"),
            ("schema", "description"),
            ("name", "twitter:description"),
            ("name", "sailthru.description"),
        ]),
        image: first_nonempty([
            sources.meta("property", "og:image"),
            sources.meta("name", "twitter:image"),
            sources.schema("image.url"),
            sources.meta("name", "sailthru.image.full"),
        ]),
        favicon: sources.favicon(&url),
        language: sources.language(),
        published: sources.published(&url),
        schema_org_data: sources.schema,
    }
}

fn attr(node: NodeRef<'_>, key: &str) -> String {
    node.attr(key).map(|s| s.to_string()).unwrap_or_default()
}
fn text(node: NodeRef<'_>) -> String {
    node.text().trim().to_string()
}
fn first_nonempty<const N: usize>(values: [String; N]) -> String {
    values
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_default()
}
fn valid(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(['{', '}'])
        && !re!(r"^#[a-zA-Z]").is_match(s)
        && s.chars().any(char::is_alphanumeric)
}
fn unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}
fn normalized(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn words(s: &str) -> usize {
    let mut count = 0;
    let mut in_word = false;
    for c in s.chars() {
        if matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0xac00..=0xd7af)
        {
            count += 1;
            in_word = false;
        } else if c <= ' ' {
            in_word = false;
        } else if !in_word {
            count += 1;
            in_word = true;
        }
    }
    count
}

// Keep '<' as data. Parsing only escaped text in an inert textarea cannot create
// nodes or allow a schema string containing </textarea> to escape its container.
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let escaped = s.replace('<', "&lt;");
    let doc = Document::from(format!("<textarea>{escaped}</textarea>"));
    doc.select_single("textarea").text().to_string()
}
fn decode_value(value: &mut Value) {
    match value {
        Value::String(s) => *s = decode_entities(s),
        Value::Array(items) => items.iter_mut().for_each(decode_value),
        Value::Object(items) => items.values_mut().for_each(decode_value),
        _ => {}
    }
}
fn collect_schema(doc: &Document) -> Value {
    let mut items = Vec::new();
    for node in doc.select("script[type='application/ld+json']").nodes() {
        let raw = node.text();
        let cleaned = re!(r"(?ms)/\*.*?\*/|(?m)^\s*//[^\r\n]*$").replace_all(&raw, "");
        let cleaned = re!(r"(?s)^\s*<!\[CDATA\[(.*?)\]\]>\s*$").replace(&cleaned, "$1");
        let cleaned = re!(r"^\s*(?:\*/|/\*)\s*|\s*(?:\*/|/\*)\s*$").replace_all(&cleaned, "");
        if let Ok(mut value) = serde_json::from_str::<Value>(cleaned.trim()) {
            if let Some(graph) = value.get_mut("@graph").and_then(Value::as_array_mut) {
                items.append(graph);
            } else {
                items.push(value);
            }
        }
    }
    let mut value = Value::Array(items);
    decode_value(&mut value);
    value
}

/// Match exact paths before searching nested schema objects, preserving source order.
fn schema_property(schema: &Value, path: &str) -> String {
    fn search(data: &Value, props: &[&str], exact: bool, results: &mut Vec<String>) {
        match data {
            Value::String(s) if props.is_empty() => results.push(s.clone()),
            Value::Array(items) => {
                if let Some(index) = props
                    .first()
                    .and_then(|p| p.strip_prefix('['))
                    .and_then(|p| p.strip_suffix(']'))
                    .and_then(|p| p.parse::<usize>().ok())
                {
                    if let Some(item) = items.get(index) {
                        search(item, &props[1..], exact, results);
                    }
                } else if props.is_empty() && items.iter().all(|v| v.is_string() || v.is_number()) {
                    results.extend(items.iter().map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string())
                    }));
                } else {
                    for item in items {
                        search(item, props, exact, results);
                    }
                }
            }
            Value::Object(items) => {
                if let Some(prop) = props.first() {
                    if let Some(value) = items.get(*prop) {
                        search(value, &props[1..], true, results);
                    } else if !exact {
                        for value in items.values().filter(|v| v.is_object() || v.is_array()) {
                            search(value, props, false, results);
                        }
                    }
                } else if let Some(name) = items.get("name").and_then(Value::as_str) {
                    results.push(name.to_string());
                }
            }
            _ => {}
        }
    }
    let props: Vec<_> = path.split('.').collect();
    let mut out = Vec::new();
    search(schema, &props, true, &mut out);
    if out.is_empty() {
        search(schema, &props, false, &mut out);
    }
    unique(out.into_iter().filter(|s| !s.is_empty())).join(", ")
}

impl Sources<'_> {
    fn metas(&self, kind: &str, key: &str) -> Vec<String> {
        self.tags
            .iter()
            .filter(|m| if kind == "name" { &m.name } else { &m.property } == &key.to_lowercase())
            .map(|m| m.content.clone())
            .collect()
    }
    fn meta(&self, kind: &str, key: &str) -> String {
        self.metas(kind, key).into_iter().next().unwrap_or_default()
    }
    fn schema(&self, path: &str) -> String {
        schema_property(&self.schema, path)
    }
    fn first_valid(&self, fields: &[(&str, &str)]) -> String {
        fields
            .iter()
            .map(|(kind, key)| {
                if *kind == "schema" {
                    self.schema(key)
                } else {
                    self.meta(kind, key)
                }
            })
            .find(|s| valid(s))
            .unwrap_or_default()
    }
    fn site_name(&self) -> String {
        let site = self.first_valid(&[
            ("schema", "publisher.name"),
            ("property", "og:site_name"),
            ("name", "og:site_name"),
            ("schema", "WebSite.name"),
            ("schema", "sourceOrganization.name"),
            ("name", "copyright"),
            ("schema", "copyrightHolder.name"),
            ("schema", "isPartOf.name"),
            ("name", "application-name"),
        ]);
        if words(&site) > 6 {
            String::new()
        } else {
            site
        }
    }
    fn best_title(&self, domain: &str, site: &str) -> String {
        let candidates = [
            self.meta("property", "og:title"),
            self.meta("name", "twitter:title"),
            self.schema("headline"),
            self.meta("name", "title"),
            self.meta("name", "sailthru.title"),
            self.doc.select_single("title").text().trim().to_string(),
            self.doc.select_single("h1").text().trim().to_string(),
        ];
        let author = first_nonempty([self.meta("property", "author"), self.meta("name", "author")])
            .trim()
            .to_lowercase();
        let site = site.trim().to_lowercase();
        let domain = re!(r"\.[^.]+$").replace(domain, "").to_lowercase();
        let domain = re!(r"[^a-z0-9]").replace_all(&domain, "");
        let candidates: Vec<_> = candidates.into_iter().filter(|s| valid(s)).collect();
        candidates
            .iter()
            .find(|c| {
                let norm = c.trim().to_lowercase();
                !((!author.is_empty() && norm == author)
                    || (!site.is_empty() && norm == site)
                    || (!domain.is_empty() && re!(r"[^a-z0-9]").replace_all(&norm, "") == domain))
            })
            .or_else(|| candidates.first())
            .cloned()
            .unwrap_or_default()
    }
    fn favicon(&self, base: &str) -> String {
        let icon = first_nonempty([
            self.meta("property", "og:image:favicon"),
            self.doc
                .select_single("link[rel=icon]")
                .attr("href")
                .map(|s| s.to_string())
                .unwrap_or_default(),
            self.doc
                .select_single("link[rel='shortcut icon']")
                .attr("href")
                .map(|s| s.to_string())
                .unwrap_or_default(),
        ]);
        if !icon.is_empty() {
            return icon;
        }
        if (base.starts_with("http://") || base.starts_with("https://"))
            && let Ok(url) = Url::parse(base).and_then(|u| u.join("/favicon.ico"))
        {
            return url.to_string();
        }
        String::new()
    }
    fn language(&self) -> String {
        first_nonempty([
            self.doc
                .select_single("html")
                .attr("lang")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
            self.meta("name", "content-language"),
            self.meta("property", "og:locale"),
            self.doc
                .select_single("meta[http-equiv='Content-Language' i]")
                .attr("content")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
            self.schema("inLanguage"),
        ])
        .replace('_', "-")
    }
    fn published(&self, url: &str) -> String {
        let mut date = self.first_valid(&[
            ("schema", "datePublished"),
            ("name", "publishDate"),
            ("property", "article:published_time"),
        ]);
        if !date.is_empty() {
            return date;
        }
        date = self
            .doc
            .select_single("abbr[itemprop=datePublished]")
            .attr("title")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if valid(&date) {
            return date;
        }
        // The first nonempty time wins; placeholders then fall through to Sailthru.
        let time = self
            .doc
            .select("time")
            .nodes()
            .iter()
            .filter(|node| !linked_elsewhere(**node, url))
            .map(|node| {
                let dt = attr(*node, "datetime");
                if !dt.trim().is_empty() {
                    dt.trim().to_string()
                } else {
                    text(*node)
                }
            })
            .find(|s| !s.is_empty())
            .unwrap_or_default();
        if valid(&time) {
            return time;
        }
        date = self.meta("name", "sailthru.date");
        if valid(&date) {
            return date;
        }
        if let Some(h1) = self.doc.select_single("h1").nodes().first().copied() {
            for after in [true, false] {
                let mut sibling = if after {
                    h1.next_element_sibling()
                } else {
                    h1.prev_element_sibling()
                };
                for _ in 0..3 {
                    let Some(node) = sibling else { break };
                    for child in Selection::from(node).select("p, time").nodes() {
                        let parsed = parse_date(&text(*child));
                        if !parsed.is_empty() {
                            return parsed;
                        }
                    }
                    if after {
                        let parsed = parse_date(&text(node));
                        if !parsed.is_empty() {
                            return parsed;
                        }
                    }
                    sibling = if after {
                        node.next_element_sibling()
                    } else {
                        node.prev_element_sibling()
                    };
                }
            }
        }
        String::new()
    }
    fn author(&self) -> String {
        let raw = self.first_valid(&[
            ("name", "sailthru.author"),
            ("property", "article:author"),
            ("property", "author"),
            ("name", "author"),
            ("name", "byl"),
            ("name", "authorList"),
        ]);
        if !raw.is_empty() {
            let cleaned = re!(r"(?i)^by\s+").replace(&raw, "");
            let cleaned = re!(r"(?i)\(?\s*https?://\S+\s*\)?").replace_all(&cleaned, "");
            let cleaned = re!(r"(?i),?\s+and\s+").replace_all(&cleaned, ", ");
            let cleaned = re!(r"\s*[-–—|]\s*$").replace_all(&cleaned, "");
            if !cleaned.trim().is_empty() {
                return cleaned.trim().to_string();
            }
        }
        let mut citation: Vec<_> = self
            .metas("name", "citation_author")
            .into_iter()
            .filter(|s| valid(s))
            .collect();
        if citation.is_empty() {
            citation = self
                .metas("property", "dc.creator")
                .into_iter()
                .filter(|s| valid(s))
                .collect();
        }
        if !citation.is_empty() {
            return citation
                .iter()
                .map(|s| {
                    re!(r"(.*),\s(.*)")
                        .captures(s)
                        .map(|c| format!("{} {}", &c[2], &c[1]))
                        .unwrap_or_else(|| s.trim().to_string())
                })
                .collect::<Vec<_>>()
                .join(", ");
        }
        let schema = first_nonempty([self.schema("author.name"), self.schema("author.[].name")]);
        let names = unique(
            schema
                .split(',')
                .map(|s| s.trim().trim_end_matches(',').trim().to_string())
                .filter(|s| valid(s)),
        );
        if !names.is_empty() {
            return names.into_iter().take(10).collect::<Vec<_>>().join(", ");
        }
        let rel = self.doc.select("a[rel~=author], address[rel~=author]");
        if (1..=3).contains(&rel.length()) {
            let names = unique(
                rel.nodes()
                    .iter()
                    .map(|n| visible_text(*n))
                    .filter(|s| s.chars().count() < 100 && author_valid(s)),
            );
            if !names.is_empty() {
                return names.join(", ");
            }
        }
        let mut names = Vec::new();
        for (selector, max) in [
            ("[itemprop=author]", usize::MAX),
            (".author", 3),
            ("[href*='/author/']", 3),
            (".authors a", 3),
        ] {
            let matches = self.doc.select(selector);
            if matches.length() > max {
                continue;
            }
            for node in matches.nodes() {
                for part in author_name(*node).split(',') {
                    let name = normalized(part).trim_end_matches(',').trim().to_string();
                    if author_valid(&name) {
                        names.push(name);
                    }
                }
            }
        }
        let names = unique(names);
        let names: Vec<_> = names
            .iter()
            .filter(|a| !names.iter().any(|b| b != *a && a.contains(b.as_str())))
            .take(10)
            .cloned()
            .collect();
        if !names.is_empty() {
            return names.join(", ");
        }
        if let Some(h1) = self.doc.select_single("h1").nodes().first().copied() {
            let mut sibling = h1.next_element_sibling();
            for _ in 0..3 {
                let Some(node) = sibling else { break };
                let children = Selection::from(node).select("p, time");
                let date_child = children
                    .nodes()
                    .iter()
                    .any(|n| !parse_date(&text(*n)).is_empty());
                let sibling_text = text(node);
                if date_child || !parse_date(&sibling_text).is_empty() {
                    let links = Selection::from(node).select("a");
                    if links.length() == 1 {
                        let name = links.text().trim().replace('\u{a0}', " ");
                        if !name.is_empty()
                            && name.chars().count() < 100
                            && parse_date(&name).is_empty()
                        {
                            return name;
                        }
                    }
                    if date_child && sibling_text.chars().count() < 300 {
                        for child in children.nodes().iter().filter(|n| n.is("p")) {
                            let name = text(*child).replace('\u{a0}', " ");
                            if !name.is_empty()
                                && name.chars().count() < 150
                                && parse_date(&name).is_empty()
                            {
                                return name;
                            }
                        }
                    }
                }
                sibling = node.next_element_sibling();
            }
            let mut scope = Some(h1);
            for _ in 0..3 {
                let Some(node) = scope else { break };
                for after in [false, true] {
                    let mut sibling = if after {
                        node.next_element_sibling()
                    } else {
                        node.prev_element_sibling()
                    };
                    for _ in 0..3 {
                        let Some(candidate) = sibling else { break };
                        let mut candidates = vec![candidate];
                        candidates.extend(
                            Selection::from(candidate)
                                .select("p, span, address")
                                .nodes(),
                        );
                        for candidate in candidates {
                            let value = text(candidate).replace('\u{a0}', " ");
                            if value.chars().count() < 50
                                && let Some(c) = re!(r"(?i)^By\s+([A-Z].+)$").captures(&value)
                            {
                                return c[1].trim().to_string();
                            }
                        }
                        sibling = if after {
                            candidate.next_element_sibling()
                        } else {
                            candidate.prev_element_sibling()
                        };
                    }
                }
                scope = node.parent().filter(|n| n.is_element());
            }
        }
        String::new()
    }
}
fn author_valid(s: &str) -> bool {
    valid(s) && !matches!(s.to_lowercase().as_str(), "author" | "authors")
}
fn visible_text(node: NodeRef<'_>) -> String {
    fn collect(node: NodeRef<'_>, out: &mut String) {
        if node.is("script, style, noscript") {
            return;
        }
        if node.is_text() {
            out.push_str(&node.text());
        } else {
            for child in node.children() {
                collect(child, out);
            }
        }
    }
    let mut out = String::new();
    collect(node, &mut out);
    normalized(&out)
}
fn author_name(node: NodeRef<'_>) -> String {
    let value = visible_text(node);
    if value.is_empty() {
        return value;
    }
    for child in Selection::from(node).select("span, a, p").nodes() {
        // Ignore children of non-visible containers as if those had been removed.
        if child
            .ancestors(None)
            .iter()
            .any(|n| n.is("script, style, noscript"))
        {
            continue;
        }
        let name = visible_text(*child);
        if (2..=50).contains(&name.chars().count()) && name != value {
            return name;
        }
    }
    if value.chars().count() <= 100 {
        value
    } else {
        String::new()
    }
}
fn linked_elsewhere(node: NodeRef<'_>, base: &str) -> bool {
    let Ok(current) = Url::parse(base) else {
        return false;
    };
    let Some(anchor) = node.ancestors(None).into_iter().find(|n| n.is("a[href]")) else {
        return false;
    };
    let href = attr(anchor, "href");
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') {
        return false;
    }
    let Ok(target) = current.join(href) else {
        return false;
    };
    current.origin() == target.origin()
        && current.path().trim_end_matches('/') != target.path().trim_end_matches('/')
}
fn parse_date(s: &str) -> String {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    // The supported syntax is English with ASCII digits and case folding.
    // Unicode digits and the long s otherwise match Regex's Unicode defaults.
    let dmy = re!(
        r"(?i-u:\b([0-9]{1,2}))\s+(?i-u:(January|February|March|April|May|June|July|August|September|October|November|December))\s+([0-9]{4})(?-u:\b)"
    );
    let mdy = re!(
        r"(?i-u:\b(January|February|March|April|May|June|July|August|September|October|November|December))\s+([0-9]{1,2}),?\s+([0-9]{4})(?-u:\b)"
    );
    let parsed = dmy
        .captures(s)
        .map(|c| (c[1].to_string(), c[2].to_string(), c[3].to_string()))
        .or_else(|| {
            mdy.captures(s)
                .map(|c| (c[2].to_string(), c[1].to_string(), c[3].to_string()))
        });
    let Some((day, month, year)) = parsed else {
        return String::new();
    };
    let Some(month) = MONTHS.iter().position(|s| s.eq_ignore_ascii_case(&month)) else {
        return String::new();
    };
    let (Ok(day), Ok(year)) = (day.parse::<u32>(), year.parse::<i32>()) else {
        return String::new();
    };
    chrono::NaiveDate::from_ymd_opt(year, month as u32 + 1, day)
        .map(|date| format!("{date}T00:00:00+00:00"))
        .unwrap_or_default()
}
const TITLE_SEPARATORS: [char; 6] = ['|', '-', '–', '—', '/', '·'];

fn clean_title(title: &str, site: &str) -> (String, String) {
    if title.is_empty() {
        return (String::new(), String::new());
    }
    if !site.is_empty() && site.to_lowercase() != title.to_lowercase() && words(site) <= 6 {
        // Both patterns require a separator, which has no case variants.
        let separated = title.contains(TITLE_SEPARATORS);
        for pattern in [
            format!(r"(?i)\s*[|\-–—/·]\s*{}\s*$", regex::escape(site)),
            format!(r"(?i)^\s*{}\s*[|\-–—/·]\s*", regex::escape(site)),
        ]
        .into_iter()
        .filter(|_| separated)
        {
            // Page-controlled site names can exceed the regex compilation
            // budget. Metadata cleanup is best-effort; retain the title and
            // continue with separator heuristics instead of panicking.
            let Ok(regex) = Regex::new(&pattern) else {
                continue;
            };
            if regex.is_match(title) {
                return (
                    regex.replace(title, "").trim().to_string(),
                    site.to_string(),
                );
            }
        }
        let positions: Vec<_> = re!(r"\s+[|\-–—/·]\s+").find_iter(title).collect();
        if let (Some(first), Some(last)) = (positions.first(), positions.last()) {
            let suffix = title[last.end()..].trim().to_lowercase();
            if !suffix.is_empty() && site.to_lowercase().contains(&suffix) {
                let mut cut = last.start();
                for pos in positions.iter().rev().skip(1) {
                    if words(title[pos.end()..cut].trim()) > 3 {
                        break;
                    }
                    cut = pos.start();
                }
                return (title[..cut].trim().to_string(), site.to_string());
            }
            let prefix = title[..first.start()].trim().to_lowercase();
            if !prefix.is_empty() && site.to_lowercase().contains(&prefix) {
                let mut cut = first.end();
                for pos in positions.iter().skip(1) {
                    if words(title[cut..pos.start()].trim()) > 3 {
                        break;
                    }
                    cut = pos.end();
                }
                return (title[cut..].trim().to_string(), site.to_string());
            }
        }
    }
    for (regex, suffix_only) in [(re!(r"\s+[|/·]\s+"), false), (re!(r"\s+[-–—]\s+"), true)] {
        let positions: Vec<_> = regex.find_iter(title).collect();
        if let (Some(first), Some(last)) = (positions.first(), positions.last()) {
            let guard = |t, s| {
                if suffix_only {
                    s <= 2 && t >= 2 && t > s
                } else {
                    s <= 3 && t >= 2 && t >= s * 2
                }
            };
            let t = title[..last.start()].trim();
            let s = title[last.end()..].trim();
            if guard(words(t), words(s)) {
                return (t.to_string(), s.to_string());
            }
            if !suffix_only {
                let t = title[first.end()..].trim();
                let s = title[..first.start()].trim();
                if guard(words(t), words(s)) {
                    return (t.to_string(), s.to_string());
                }
            }
        }
    }
    (title.trim().to_string(), String::new())
}

#[cfg(test)]
mod title_tests {
    use super::*;

    #[test]
    fn case_insensitive_separator_class_matches_only_the_listed_characters() {
        let class = Regex::new(r"(?i)^[|\-–—/·]$").unwrap();
        let mut buffer = [0; 4];
        for ch in (0..=u32::from(char::MAX)).filter_map(char::from_u32) {
            assert_eq!(
                class.is_match(ch.encode_utf8(&mut buffer)),
                TITLE_SEPARATORS.contains(&ch),
                "{ch:?}"
            );
        }
    }
}
