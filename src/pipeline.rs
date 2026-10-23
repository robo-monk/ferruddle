//! Prepare shared input once, clean independent candidates, and finish the winner.
use crate::{
    DebugInfo, Error, Extraction, Metadata, Options, Warning, dom, extractors, finalize, math,
    metadata, patterns, removals, safety, selection, standardize,
    utils::{count_words, tag},
};
use dom_query::{Document, NodeId, NodeRef};
use serde_json::Value;
use std::cell::OnceCell;
use url::Url;

/// Document::clone preserves arena indices. These IDs are selected only after
/// preprocessing, image removal, and adapter mutations have finished.
#[derive(Clone, Copy)]
struct OriginalNode(NodeId);
impl OriginalNode {
    fn in_clone(self, clone: &Document) -> NodeRef<'_> {
        NodeRef::new(self.0, &clone.tree)
    }
}

struct Prepared {
    original: Document,
    metadata: Metadata,
    warnings: Vec<Warning>,
    base: Option<Url>,
    main: OriginalNode,
    schema_root: OnceCell<Option<OriginalNode>>,
    invalid_selector: Option<String>,
}

impl Prepared {
    fn schema_root(&self) -> Option<OriginalNode> {
        *self.schema_root.get_or_init(|| {
            schema_element(
                &self.original,
                schema_text(&self.metadata.schema_org_data, 0),
            )
            .map(|node| OriginalNode(node.id))
        })
    }
}

#[derive(Clone, Copy)]
enum Output {
    Unsafe,
    Outer,
    Inner,
}
struct Candidate {
    doc: Document,
    root: NodeId,
    output: Output,
    word_count: usize,
    metadata: Metadata,
    debug: DebugInfo,
    /// Partial selectors found a removal target. When false, disabling them
    /// reproduces this candidate exactly.
    partial_targets: bool,
}
impl Candidate {
    fn new(
        doc: Document,
        root: NodeId,
        output: Output,
        metadata: Metadata,
        debug: DebugInfo,
    ) -> Self {
        let word_count = if matches!(output, Output::Unsafe) {
            0
        } else {
            crate::utils::count_content_words(NodeRef::new(root, &doc.tree))
        };
        Self {
            doc,
            root,
            output,
            word_count,
            metadata,
            debug,
            partial_targets: false,
        }
    }
    fn html(&self) -> String {
        let root = NodeRef::new(self.root, &self.doc.tree);
        match self.output {
            Output::Unsafe => String::new(),
            Output::Outer => root.html().trim().to_string(),
            Output::Inner => root.inner_html().trim().to_string(),
        }
    }
    fn finish(
        self,
        page_url: Option<&str>,
        options: &Options,
        warnings: Vec<Warning>,
        kind: Option<String>,
    ) -> Extraction {
        let content = self.html();
        let content_markdown = options.markdown.then(|| {
            #[cfg(test)]
            tests::count(2);
            if matches!(self.output, Output::Unsafe) {
                String::new()
            } else {
                crate::markdown::convert_node(NodeRef::new(self.root, &self.doc.tree), page_url)
            }
        });
        Extraction {
            content,
            content_markdown,
            metadata: self.metadata,
            word_count: self.word_count,
            extractor_type: kind,
            warnings,
            debug: options.debug.then_some(self.debug),
        }
    }
}

pub fn extract(html: &str, page_url: Option<&str>, options: &Options) -> Result<Extraction, Error> {
    let mut original = Document::from(html);
    let mut warnings = math_warnings(&original);
    let metadata = metadata::extract(&original, page_url);
    let promoted_images = dom::preprocess(&original);
    remove_images(&original, options);
    // Resolve positional selectors on the same image-free DOM used by attempts.
    // Explicit unsafe roots remain empty even when retries/schema are available.
    if let Some(root) = options
        .content_selector
        .as_deref()
        .and_then(|s| dom::select(original.root(), s).first().copied())
        .filter(|n| safety::is_unsafe_element(*n))
    {
        let root_id = root.id;
        let debug = DebugInfo {
            content_selector: node_path(root),
            ..Default::default()
        };
        safety::sanitize(root);
        return Ok(
            Candidate::new(original, root_id, Output::Unsafe, metadata, debug)
                .finish(page_url, options, warnings, None),
        );
    }
    let adapter = extractors::extract(&mut original, page_url, options, &metadata);
    let base = finalize::base_url(&original, page_url);
    // Only image promotion and selector adapters can expose previously unscanned
    // source here. Generic pages retain their single original scan; detached
    // adapter HTML is checked separately by from_html.
    let source_materialized = matches!(
        adapter.as_ref().map(|a| &a.content),
        Some(extractors::Content::Selector(_))
    ) || (adapter.is_none() && promoted_images);
    if warnings.is_empty() && source_materialized {
        warnings = math_warnings(&original);
    }
    remove_images(&original, options);
    let (metadata, selected, kind) = match adapter {
        Some(adapter) => match adapter.content {
            extractors::Content::Html(html) => {
                let candidate = from_html(
                    &html,
                    base.as_ref(),
                    options,
                    adapter.metadata,
                    &mut warnings,
                );
                return Ok(candidate.finish(
                    page_url,
                    options,
                    warnings,
                    Some(adapter.extractor_type),
                ));
            }
            extractors::Content::Selector(selector) => {
                let mut selected = options.clone();
                selected.content_selector = Some(selector);
                selected.remove_low_scoring = false;
                selected.remove_hidden_elements = false;
                (
                    adapter.metadata,
                    Some(selected),
                    Some(adapter.extractor_type),
                )
            }
        },
        None => (metadata, None, None),
    };
    let active = selected.as_ref().unwrap_or(options);
    let prepared = prepare(original, metadata, warnings, base, active);
    let mut result = attempt(&prepared, active, None, page_url);
    if selected.is_some() {
        return Ok(result.finish(page_url, options, prepared.warnings, kind));
    }
    if result.word_count < 200 && options.remove_partial_selectors {
        let mut retry = options.clone();
        retry.remove_partial_selectors = false;
        if result.partial_targets {
            let candidate = attempt(&prepared, &retry, None, page_url);
            if candidate.word_count > result.word_count * 2 {
                result = candidate
            }
        } else {
            // The retry would reproduce `result`; tests run it to prove that.
            #[cfg(test)]
            tests::assert_same_candidate(&result, &attempt(&prepared, &retry, None, page_url));
        }
    }
    if result.word_count < 50 && options.remove_hidden_elements {
        let mut retry = options.clone();
        retry.remove_hidden_elements = false;
        let candidate = attempt(&prepared, &retry, None, page_url);
        if candidate.word_count > result.word_count * 2 {
            result = candidate
        }
        // Match body.querySelectorAll: the body itself is not a candidate, and
        // equal-sized candidates retain the first root in document order.
        let hidden = prepared.original.tree.body().and_then(|body| {
            let mut best = None;
            let mut best_words = 0;
            for node in dom::select(body, crate::constants::HIDDEN_EXACT_SKIP_SELECTOR) {
                if crate::utils::attribute(node, "class").contains("math") {
                    continue;
                }
                let words = crate::utils::text_words(node);
                if words > best_words {
                    best = Some(node);
                    best_words = words;
                }
            }
            best.filter(|_| best_words >= 30)
        });
        if let Some(hidden) = hidden {
            retry.remove_partial_selectors = false;
            let candidate = attempt(&prepared, &retry, Some(OriginalNode(hidden.id)), page_url);
            // Both candidates have already been sanitized; this comparison uses
            // the exact trimmed serialization length, just as final output does.
            if candidate.word_count > result.word_count
                || (candidate.word_count > 20.max(result.word_count * 7 / 10)
                    && candidate.html().len() < result.html().len())
            {
                result = candidate
            }
        }
    }
    if result.word_count < 50 {
        let mut retry = options.clone();
        retry.remove_low_scoring = false;
        retry.remove_partial_selectors = false;
        retry.remove_content_patterns = false;
        let candidate = attempt(&prepared, &retry, None, page_url);
        if candidate.word_count > result.word_count {
            result = candidate
        }
    }
    let schema = schema_text(&prepared.metadata.schema_org_data, 0);
    let mut warnings = prepared.warnings.clone();
    if !schema.is_empty()
        && count_words(&Document::fragment(schema).text()) as f64 > result.word_count as f64 * 1.5
    {
        result = if let Some(node) = prepared.schema_root() {
            attempt(&prepared, options, Some(node), page_url)
        } else {
            from_html(
                schema,
                prepared.base.as_ref(),
                options,
                prepared.metadata.clone(),
                &mut warnings,
            )
        };
    }
    Ok(result.finish(page_url, options, warnings, None))
}

fn math_warnings(doc: &Document) -> Vec<Warning> {
    #[cfg(test)]
    tests::count(3);
    math::warnings(doc)
}

fn remove_images(doc: &Document, options: &Options) {
    if options.remove_images {
        for node in dom::select(doc.root(), "img,picture,source") {
            node.remove_from_parent()
        }
    }
}

fn prepare(
    original: Document,
    metadata: Metadata,
    warnings: Vec<Warning>,
    base: Option<Url>,
    options: &Options,
) -> Prepared {
    #[cfg(test)]
    tests::count(0);
    let body = original.tree.body().unwrap_or(original.root());
    let main = selection::find_main(&original, options.content_selector.as_deref()).unwrap_or(body);
    let main_is_body = tag(main) == "body";
    let main = OriginalNode(main.id);
    let invalid_selector = options
        .content_selector
        .as_ref()
        .filter(|_| options.debug)
        .filter(|s| dom_query::Matcher::new(s).is_err())
        .map(|s| format!("Invalid contentSelector: {s}"));
    let mut prepared = Prepared {
        original,
        metadata,
        warnings,
        base,
        main,
        schema_root: OnceCell::new(),
        invalid_selector,
    };
    if main_is_body {
        prepared.main = prepared.schema_root().unwrap_or(main);
    }
    prepared
}

fn attempt(
    prepared: &Prepared,
    options: &Options,
    forced: Option<OriginalNode>,
    page_url: Option<&str>,
) -> Candidate {
    #[cfg(test)]
    tests::count(1);
    let clone = prepared.original.clone();
    let body = clone.tree.body().unwrap_or(clone.root());
    let root = forced.unwrap_or(prepared.main).in_clone(&clone);
    let root_id = root.id;
    let mut debug = DebugInfo::default();
    if options.debug {
        debug.content_selector = node_path(root);
        debug.errors.extend(prepared.invalid_selector.clone());
    }
    if safety::is_unsafe_element(root) {
        safety::sanitize(root);
        return Candidate::new(
            clone,
            root_id,
            Output::Unsafe,
            prepared.metadata.clone(),
            debug,
        );
    }
    let mut metadata = prepared.metadata.clone();
    if !metadata.author.is_empty() || !metadata.published.is_empty() {
        removals::remove_metadata_block(root)
    }
    for node in dom::select(root, "wbr") {
        node.remove_from_parent()
    }
    if options.standardize {
        standardize::before_removals(&clone, root, &metadata)
    }
    if options.remove_small_images {
        removals::remove_small_images(body, options.debug, &mut debug.removals)
    }
    if options.remove_hidden_elements {
        removals::remove_hidden(body, options.debug, &mut debug.removals)
    }
    if options.remove_content_patterns {
        patterns::eyebrow(root, options.debug, &mut debug.removals)
    }
    let mut partial_targets = false;
    if options.remove_exact_selectors || options.remove_partial_selectors {
        partial_targets = removals::remove_selectors(body, root, options, &mut debug.removals)
    }
    if options.remove_low_scoring {
        removals::remove_low_scoring(body, root, options.debug, &mut debug.removals)
    }
    if options.remove_content_patterns {
        patterns::remove_patterns(
            root,
            page_url,
            &metadata,
            options.debug,
            &mut debug.removals,
        )
    }
    if options.standardize {
        standardize::content(root, &metadata, options)
    }
    finalize::urls(root, prepared.base.as_ref());
    finalize::images(root, &mut metadata);
    let output = if safety::sanitize(root) {
        Output::Unsafe
    } else {
        Output::Outer
    };
    Candidate {
        partial_targets,
        ..Candidate::new(clone, root_id, output, metadata, debug)
    }
}

fn from_html(
    html: &str,
    base: Option<&Url>,
    options: &Options,
    metadata: Metadata,
    warnings: &mut Vec<Warning>,
) -> Candidate {
    let doc = Document::fragment(html);
    let root = doc.html_root();
    let root_id = root.id;
    if warnings.is_empty() {
        *warnings = math_warnings(&doc)
    }
    safety::sanitize(root);
    remove_images(&doc, options);
    if options.standardize {
        standardize::extractor(root, options)
    }
    finalize::urls(root, base);
    // Required after transforms and resolution, including when cleanup is off.
    let output = if safety::sanitize(root) {
        Output::Unsafe
    } else {
        Output::Inner
    };
    Candidate::new(doc, root_id, output, metadata, DebugInfo::default())
}

fn schema_text(value: &Value, depth: usize) -> &str {
    if depth > 10 {
        return "";
    }
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .map(|v| schema_text(v, depth + 1))
            .find(|s| !s.is_empty())
            .unwrap_or("");
    }
    value
        .get("text")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            value
                .get("articleBody")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| {
            value
                .get("@graph")
                .map(|v| schema_text(v, depth + 1))
                .unwrap_or("")
        })
}
fn schema_element<'a>(doc: &'a Document, text: &str) -> Option<NodeRef<'a>> {
    #[cfg(test)]
    tests::record_schema_search();
    static PARAGRAPH: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\n\s*\n").unwrap());
    let first = PARAGRAPH.split(text).next().unwrap_or("").trim();
    let phrase: String = first.chars().take(100).collect();
    let phrase = phrase.trim();
    if phrase.is_empty() {
        return None;
    }
    // Matching the sanitized view also excludes text contributed by unsafe
    // descendants of an otherwise safe wrapper. Clone-stable IDs map the
    // winning eligible node back into the prepared source document.
    let safe = doc.clone();
    let body = safe.tree.body()?;
    safety::sanitize(body);
    let count = count_words(text);
    let mut pending: Vec<_> = body.element_children().into_iter().rev().collect();
    let mut best = None;
    while let Some(node) = pending.pop() {
        let text = node.text();
        if !text.contains(phrase) {
            continue;
        }
        let words = count_words(&text);
        if (words as f64) < count as f64 * 0.8 {
            continue;
        }
        if best.is_none_or(|(_, best_words)| words < best_words) {
            best = Some((node.id, words));
        }
        pending.extend(node.element_children().into_iter().rev());
    }
    best.map(|(id, _)| NodeRef::new(id, &doc.tree))
}
fn node_path(node: NodeRef<'_>) -> String {
    let mut path = Vec::new();
    for n in std::iter::once(node)
        .chain(node.ancestors_it(None))
        .filter(|n| n.is_element())
    {
        let index = n
            .parent()
            .map(|p| {
                p.element_children()
                    .iter()
                    .position(|s| s.id == n.id)
                    .unwrap_or(0)
                    + 1
            })
            .unwrap_or(1);
        path.push(format!("{}:nth-child({index})", tag(n)));
    }
    path.reverse();
    path.join(" > ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    thread_local! {
        static COUNTERS: Cell<[usize; 4]> = const { Cell::new([0; 4]) };
        static SCHEMA_SEARCHES: Cell<usize> = const { Cell::new(0) };
    }
    pub(super) fn record_schema_search() {
        SCHEMA_SEARCHES.with(|count| count.set(count.get() + 1));
    }
    #[test]
    fn schema_search_is_skipped_when_article_selection_and_length_are_sufficient() {
        SCHEMA_SEARCHES.with(|count| count.set(0));
        let prose = "This synthetic article includes enough useful detail to preserve the complete original source. ".repeat(16);
        let schema = serde_json::json!({"@type":"Article", "articleBody":prose});
        let html = format!(
            "<head><script type='application/ld+json'>{schema}</script></head><article><p>{prose}</p></article>"
        );
        let result = extract(&html, None, &Options::default()).unwrap();
        assert!(result.word_count >= 200);
        assert_eq!(SCHEMA_SEARCHES.with(Cell::get), 0);
    }
    pub(super) fn assert_same_candidate(expected: &Candidate, actual: &Candidate) {
        assert_eq!(actual.word_count, expected.word_count);
        assert_eq!(actual.html(), expected.html());
        assert_eq!(
            format!("{:?}", actual.metadata),
            format!("{:?}", expected.metadata)
        );
        assert_eq!(
            format!("{:?}", actual.debug),
            format!("{:?}", expected.debug)
        );
    }
    pub(super) fn count(index: usize) {
        COUNTERS.with(|counts| {
            let mut value = counts.get();
            value[index] += 1;
            counts.set(value);
        });
    }
    #[test]
    fn deferred_schema_search_caches_both_safe_matches_and_misses() {
        let prose = "The recovered source contains enough useful detail for readers to identify this original article.";
        for matching_dom in [false, true] {
            SCHEMA_SEARCHES.with(|count| count.set(0));
            let schema = serde_json::json!({"@type":"Article", "articleBody":prose});
            let recovered = if matching_dom {
                format!("<section><p>{prose}</p></section>")
            } else {
                String::new()
            };
            let original = Document::from(format!(
                "<head><script type='application/ld+json'>{schema}</script></head><article><p>A short teaser.</p></article>{recovered}"
            ));
            let metadata = Metadata {
                schema_org_data: schema,
                ..Default::default()
            };
            let options = Options {
                content_selector: Some("article".into()),
                ..Default::default()
            };
            let prepared = prepare(original, metadata, Vec::new(), None, &options);
            assert!(prepared.schema_root.get().is_none());
            assert_eq!(SCHEMA_SEARCHES.with(Cell::get), 0);
            let first = prepared.schema_root();
            assert_eq!(first.is_some(), matching_dom);
            assert!(prepared.schema_root.get().is_some());
            assert_eq!(
                prepared.schema_root().map(|node| node.0),
                first.map(|node| node.0)
            );
            assert_eq!(SCHEMA_SEARCHES.with(Cell::get), 1);
        }
    }
    #[test]
    fn body_selection_resolves_and_reuses_the_schema_root_before_attempts() {
        SCHEMA_SEARCHES.with(|count| count.set(0));
        let prose = "The recovered source contains enough useful detail for readers to identify this original article.";
        let original = Document::from(format!("<div id='recovered'><p>{prose}</p></div>"));
        let expected = dom::select(original.root(), "#recovered")[0].id;
        let metadata = Metadata {
            schema_org_data: serde_json::json!({"articleBody":prose}),
            ..Default::default()
        };
        let options = Options {
            content_selector: Some("body".into()),
            debug: true,
            ..Default::default()
        };
        let prepared = prepare(original, metadata, Vec::new(), None, &options);
        assert_eq!(prepared.main.0, expected);
        assert_eq!(prepared.schema_root().map(|node| node.0), Some(expected));
        for _ in 0..2 {
            let candidate = attempt(&prepared, &options, None, None);
            assert_eq!(candidate.root, expected);
            assert!(candidate.html().contains(prose));
        }
        assert_eq!(SCHEMA_SEARCHES.with(Cell::get), 1);
    }
    #[test]
    fn retries_share_preparation_and_render_markdown_only_for_the_winner() {
        COUNTERS.with(|counts| counts.set([0; 4]));
        let result = extract(
            "<article><p>A short useful article remains available.</p></article>",
            None,
            &Options {
                markdown: true,
                ..Options::default()
            },
        )
        .unwrap();
        let [prepares, attempts, renders, scans] = COUNTERS.with(Cell::get);
        assert_eq!(prepares, 1);
        assert!(attempts > 1);
        assert_eq!(renders, 1);
        assert_eq!(scans, 1);
        assert!(result.content_markdown.unwrap().contains("useful article"));
    }
    #[test]
    fn candidate_metrics_exclude_unsafe_text_before_retry_decisions() {
        let original = Document::from(
            "<article><p>Visible café prose.</p><script>many unsafe extra words must not inflate candidate metrics</script><style>more unsafe source words</style></article>",
        );
        let options = Options {
            remove_exact_selectors: false,
            remove_partial_selectors: false,
            remove_content_patterns: false,
            standardize: false,
            ..Options::default()
        };
        let prepared = prepare(original, Metadata::default(), Vec::new(), None, &options);
        let candidate = attempt(&prepared, &options, None, None);
        assert_eq!(candidate.word_count, 3);
        assert_eq!(
            candidate.html(),
            "<article><p>Visible café prose.</p></article>"
        );
        assert_eq!(candidate.html().len(), 46);
    }
    #[test]
    fn forced_root_ids_preserve_the_selected_element_across_attempt_clones() {
        let original = Document::from(
            "<article><p>Initial prose.</p></article><div id='forced'><p>The forced source is independent from the initial article.</p></div>",
        );
        let id = OriginalNode(dom::select(original.root(), "#forced")[0].id);
        let expected_path = node_path(id.in_clone(&original));
        let options = Options {
            debug: true,
            standardize: false,
            remove_low_scoring: false,
            remove_content_patterns: false,
            ..Options::default()
        };
        let prepared = prepare(original, Metadata::default(), Vec::new(), None, &options);
        let candidate = attempt(&prepared, &options, Some(id), None);
        assert!(candidate.html().contains("id=\"forced\""));
        assert!(candidate.html().contains("The forced source"));
        assert_eq!(candidate.debug.content_selector, expected_path);
        assert!(
            id.in_clone(&prepared.original)
                .text()
                .contains("The forced source")
        );
    }
}
