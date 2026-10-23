//! Math detection only. No math parsing, rendering, or conversion is performed.

use std::sync::OnceLock;

use dom_query::{Document, NodeData, NodeRef};
use regex::Regex;

use crate::{Warning, WarningCode, dom};

/// Formula source roots shared by cleanup and Markdown. MathJax processing
/// controls describe prose containers; they are deliberately absent here.
/// `is_source_candidate` implements this list; tests prove the equivalence.
#[cfg(test)]
pub(crate) const SOURCE_SELECTOR: &str = concat!(
    "math,mjx-container,mjx-math,",
    "[data-math],[data-latex],[data-tex],[data-mathml],[data-katex],",
    "script[type^='math/' i],annotation[encoding='application/x-tex' i],",
    "[class~='katex' i],[class~='katex-display' i],[class~='katex-mathml' i],",
    "[class~='MathJax' i],[class~='MathJax_Display' i],[class~='MathJax_CHTML' i],",
    "[class~='MathJax_SVG' i],[class~='MathJax_SVG_Display' i],",
    "[class~='mwe-math-element' i],[class~='mwe-math-mathml-inline' i],[class~='mwe-math-mathml-display' i],",
    "[class~='math' i],[class~='math-inline' i],[class~='math-block' i],[class~='math-display' i],",
    "[class~='arithmatex' i],[class~='tex-math' i],[class~='hurmet-tex' i]"
);

/// Match exactly the elements `SOURCE_SELECTOR` matches, reading each attribute
/// once instead of evaluating every selector branch. Type selectors match any
/// namespace; unprefixed attribute selectors match only no-namespace attributes.
pub(crate) fn is_source_candidate(node: NodeRef<'_>) -> bool {
    node.query_or(false, |node| {
        let NodeData::Element(element) = &node.data else {
            return false;
        };
        let tag = element.name.local.as_ref();
        if matches!(tag, "math" | "mjx-container" | "mjx-math") {
            return true;
        }
        element.attrs.iter().any(|attribute| {
            if !attribute.name.ns.is_empty() {
                return false;
            }
            match attribute.name.local.as_ref() {
                "data-math" | "data-latex" | "data-tex" | "data-mathml" | "data-katex" => true,
                "type" => tag == "script" && starts_ascii_case(&attribute.value, "math/"),
                "encoding" => {
                    tag == "annotation" && attribute.value.eq_ignore_ascii_case("application/x-tex")
                }
                "class" => attribute.value.split_ascii_whitespace().any(source_class),
                _ => false,
            }
        })
    })
}

/// Test the same source candidates as `select(SOURCE_SELECTOR)`, stopping at
/// the first accepted source and excluding the root and template fragments.
pub(crate) fn has_source_descendant(root: NodeRef<'_>) -> bool {
    root.descendants_it()
        .any(|node| node.is_element() && is_source_candidate(node) && is_math_source(node))
}

pub(crate) const SOURCE_ATTRIBUTES: &[&str] = &[
    "data-math",
    "data-latex",
    "data-tex",
    "data-mathml",
    "data-katex",
];

/// Explicit source must survive hidden-rendering cleanup even when there is no
/// visible fallback. Generated duplicate visual output is not an explicit source.
pub(crate) fn has_explicit_source(node: NodeRef<'_>) -> bool {
    SOURCE_ATTRIBUTES
        .iter()
        .any(|attribute| node.has_attr(attribute))
        || node.has_name("math")
        || (dom::is_html_element(node, "script")
            && node
                .attr_or("type", "")
                .to_ascii_lowercase()
                .starts_with("math/"))
        || (node.has_name("annotation")
            && node
                .attr_or("encoding", "")
                .eq_ignore_ascii_case("application/x-tex"))
}

fn source_class(class: &str) -> bool {
    [
        "katex",
        "katex-display",
        "katex-mathml",
        "mathjax",
        "mathjax_display",
        "mathjax_chtml",
        "mathjax_svg",
        "mathjax_svg_display",
        "mwe-math-element",
        "mwe-math-mathml-inline",
        "mwe-math-mathml-display",
        "math",
        "math-inline",
        "math-block",
        "math-display",
        "arithmatex",
        "tex-math",
        "hurmet-tex",
    ]
    .iter()
    .any(|known| class.eq_ignore_ascii_case(known))
}

fn starts_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .as_bytes()
        .get(..prefix.len())
        .is_some_and(|part| part.eq_ignore_ascii_case(prefix.as_bytes()))
}

fn ends_ascii_case(value: &str, suffix: &str) -> bool {
    value
        .as_bytes()
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|part| part.eq_ignore_ascii_case(suffix.as_bytes()))
}

fn processing_control(class: &str) -> bool {
    ["_ignore", "_process", "-ignore", "-process"]
        .iter()
        .any(|suffix| ends_ascii_case(class, suffix))
}

/// Identify actual source holders and known generated formula roots. Detection
/// also recognizes broader markup; those prose containers must render normally.
/// A source holder with native prose descendants must render its children too.
pub(crate) fn is_math_source(node: NodeRef<'_>) -> bool {
    node.is_element()
        && (has_explicit_source(node)
            || node.has_name("mjx-container")
            || node.has_name("mjx-math")
            || node
                .class()
                .is_some_and(|classes| classes.split_ascii_whitespace().any(source_class)))
}

/// Recognize that a document contains math, without treating broad markers as
/// formula boundaries. Processing-control classes alone are not math evidence.
pub(crate) fn has_math_markup(node: NodeRef<'_>) -> bool {
    if !node.is_element() {
        return false;
    }
    is_math_source(node)
        || node.node_name().is_some_and(|tag| tag.starts_with("mjx-"))
        || node.attr_or("role", "").eq_ignore_ascii_case("math")
        || node.class().is_some_and(|classes| {
            classes.split_ascii_whitespace().any(|class| {
                !processing_control(class)
                    && (starts_ascii_case(class, "mathjax")
                        || starts_ascii_case(class, "katex")
                        || starts_ascii_case(class, "mwe-math")
                        || class.eq_ignore_ascii_case("latex"))
            })
        })
}

fn library_present(nodes: &[NodeRef<'_>]) -> bool {
    static EXTERNAL: OnceLock<Regex> = OnceLock::new();
    static INLINE: OnceLock<Regex> = OnceLock::new();
    let external =
        EXTERNAL.get_or_init(|| Regex::new("(?i)mathjax|katex|temml").expect("literal regex"));
    let inline = INLINE.get_or_init(|| {
        Regex::new(r"(?i)mathjax|katex\.render|temml\.render").expect("literal regex")
    });
    nodes.iter().any(|node| {
        if !dom::is_html_element(*node, "script") {
            return false;
        }
        let src = node.attr_or("src", "");
        if src.is_empty() {
            inline.is_match(&node.text())
        } else {
            external.is_match(&src)
        }
    })
}

fn backslash_delimiter_is_math(text: &str, start: usize, end: usize) -> bool {
    let before = &text[..start];
    if before.ends_with('\\') {
        return false;
    }
    // A drive path may contain spaces and parenthesized directory names. Keep
    // adjacent prose/formula delimiters valid; reject only a drive path context.
    static DRIVE_PATH: OnceLock<Regex> = OnceLock::new();
    let path = DRIVE_PATH
        .get_or_init(|| Regex::new(r#"(?:^|[\s"'=])[A-Za-z]:\\"#).expect("literal regex"));
    let line = before.rsplit('\n').next().unwrap_or(before);
    if before
        .chars()
        .next_back()
        .is_some_and(|ch| !ch.is_whitespace())
        && path.is_match(line)
    {
        return false;
    }
    // A quantified escaped character range is an ordinary regex example.
    let delimited = &text[start..end];
    !(delimited.starts_with(r"\[")
        && text[end..].starts_with(['+', '*', '?', '{'])
        && delimited.as_bytes().windows(3).any(|range| {
            range[1] == b'-' && range[0].is_ascii_alphanumeric() && range[2].is_ascii_alphanumeric()
        }))
}

fn dollar_source_is_math(source: &str) -> bool {
    // `${name}` is a template substitution, including adjacent substitutions.
    let template_identifier = source.strip_prefix('{').and_then(|s| s.strip_suffix('}'));
    if template_identifier.is_some_and(|name| {
        !name.is_empty()
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    }) {
        return false;
    }
    source.contains(['\\', '_', '^', '{', '}'])
}

/// Detect source-bearing formula images, including lazily promoted noscript images.
pub(crate) fn is_math_image(node: NodeRef<'_>) -> bool {
    static TEX_COMMAND: OnceLock<Regex> = OnceLock::new();
    if !dom::is_html_element(node, "img") {
        return false;
    }
    let src = node.attr_or("src", "").to_ascii_lowercase();
    let tex_command =
        TEX_COMMAND.get_or_init(|| Regex::new(r"\\[A-Za-z]{2,}").expect("literal regex"));
    has_math_markup(node)
        || (src.contains("%5c") && (src.contains('?') || src.contains("latex")))
        || tex_command.is_match(&node.attr_or("alt", ""))
}

/// Detect structured math and source LaTeX in the original input document.
/// Template content is included because preprocessing can expose shadow content.
/// Examples inside code/pre and ordinary currency text do not trigger a warning.
pub fn has_math(doc: &Document) -> bool {
    static DELIMITED: OnceLock<Regex> = OnceLock::new();
    let delimited = DELIMITED.get_or_init(|| {
        Regex::new(
            r"(?s)\\\((.+?)\\\)|\\\[(.+?)\\\]|\$\$(.+?)\$\$|\$([^\s$\n](?:[^$\n]*[^\s$\n])?)\$",
        )
        .expect("literal regex")
    });
    let nodes = dom::subtree(doc.root());
    let library = OnceLock::new();
    for node in nodes.iter().copied() {
        if has_math_markup(node) {
            return true;
        }
        if is_math_image(node) {
            return true;
        } else if node.is_text() {
            if node.ancestors_it(None).any(|ancestor| {
                ancestor.node_name().is_some_and(|tag| {
                    matches!(
                        tag.as_ref(),
                        "pre" | "code" | "script" | "style" | "textarea"
                    )
                })
            }) {
                continue;
            }
            let text = node.text();
            for captures in delimited.captures_iter(&text) {
                if captures.get(1).or_else(|| captures.get(2)).is_some() {
                    let whole = captures.get(0).expect("whole delimiter match");
                    if backslash_delimiter_is_math(&text, whole.start(), whole.end()) {
                        return true;
                    }
                    continue;
                }
                let source = captures
                    .get(3)
                    .or_else(|| captures.get(4))
                    .expect("one math delimiter capture")
                    .as_str()
                    .trim();
                if dollar_source_is_math(source)
                    || (!source.is_empty()
                        && !source.contains(char::is_whitespace)
                        && !source.starts_with('{')
                        && source.chars().any(char::is_alphabetic)
                        && *library.get_or_init(|| library_present(&nodes)))
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Return exactly one explicit warning when original input contains math.
pub fn warnings(doc: &Document) -> Vec<Warning> {
    if has_math(doc) {
        vec![Warning {
            code: WarningCode::MathNotSupported,
            message: "Math support is not yet included.".to_owned(),
        }]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_candidate_equals_selector(doc: &Document) {
        let selector = dom_query::Matcher::new(SOURCE_SELECTOR).unwrap();
        for node in dom::subtree(doc.root()) {
            assert_eq!(
                is_source_candidate(node),
                node.is_element() && node.is_match(&selector),
                "{}",
                node.html()
            );
        }
    }

    #[test]
    fn source_candidate_equals_selector_on_every_real_input() {
        for doc in crate::tests::upstream_documents() {
            assert_candidate_equals_selector(&doc);
        }
    }

    #[test]
    fn source_candidate_equals_selector_on_unusual_markup() {
        assert_candidate_equals_selector(&Document::from(concat!(
            "<MATH><mi>x</mi></MATH><Mjx-Container><MJX-MATH>x</MJX-MATH></Mjx-Container>",
            "<span DATA-MATH>x</span><span data-latex=''>x</span><span data-tex>x</span>",
            "<span data-mathml>x</span><span data-katex>x</span><span data-maths>x</span>",
            "<span data-math-x>x</span><span math>x</span><span class>x</span>",
            "<script type='MATH/TeX; mode=display'>x</script><script type=' math/tex'>x</script>",
            "<script type='math'>x</script><script type='text/math/'>x</script>",
            "<div type='math/tex'>x</div><annotation encoding='Application/X-TeX'>x</annotation>",
            "<annotation encoding='application/x-tex '>x</annotation>",
            "<span encoding='application/x-tex'>x</span>",
            "<svg><script type='math/tex'>x</script><a xlink:href='#' xml:lang='en' data-tex>x</a>",
            "<annotation encoding='application/x-tex'>x</annotation><math>x</math></svg>",
            "<math><script type='math/tex'>x</script><semantics><annotation encoding='application/x-tex'>x",
            "</annotation></semantics><mtext class='katex'>x</mtext></math>",
            "<span class='\tKaTeX\n'>x</span><span class='a\u{c}mathjax_svg_display\rb'>x</span>",
            "<span class='katex\u{a0}'>x</span><span class='kaTex-HTML'>x</span>",
            "<span class='mathjax_ignore'>x</span><span class='MATH-display math-blocks'>x</span>",
            "<span class='ma\u{307}th'>x</span><span class='K\u{212a}atex'>x</span>",
            "<span class='tex2jax_process hurmet-tex'>x</span><span class='arithmatex'>x</span>",
            "<span class='tex-math' data-katex onclick='x'>x</span><p class='math-inline'>x</p>",
            "<template><span class='mwe-math-element'>x</span><math>x</math></template>",
        )));
    }

    #[test]
    fn source_descendant_search_equals_collecting_selector_results() {
        let doc = Document::from(
            "<article><p>Prose.</p><span class='mathjax_ignore'>Ignored marker</span><div><span data-math='x'></span><math><mi>x</mi></math></div><span class='MaThJaX'>x</span><template><span data-tex='y'>y</span></template></article>",
        );
        for node in dom::subtree(doc.root()) {
            let expected = dom::select(node, SOURCE_SELECTOR)
                .into_iter()
                .any(is_math_source);
            assert_eq!(has_source_descendant(node), expected, "{}", node.html());
        }
    }

    #[test]
    fn ascii_class_boundaries_equal_lowercasing_without_allocating() {
        for value in [
            "",
            "k",
            "MathJax",
            "MATHJAX_Preview",
            "MathJax_PROCESS",
            "katex-IGNORE",
            "LaTeX",
            "mathjax大",
            "大mathjax",
            "é_PROCESS",
            "İmathjax_ignore",
            "katex_ignöre",
        ] {
            let lowercase = value.to_ascii_lowercase();
            for prefix in ["mathjax", "katex", "mwe-math"] {
                assert_eq!(
                    starts_ascii_case(value, prefix),
                    lowercase.starts_with(prefix),
                    "{value}"
                );
            }
            for suffix in ["_ignore", "_process", "-ignore", "-process"] {
                assert_eq!(
                    ends_ascii_case(value, suffix),
                    lowercase.ends_with(suffix),
                    "{value}"
                );
            }
        }
    }

    #[test]
    fn processing_controls_are_not_math_evidence_or_source_boundaries() {
        for class in [
            "tex2jax_ignore",
            "tex2jax_process",
            "mathjax_ignore",
            "mathjax_process",
            "MathJax_Ignore",
            "MATHJAX_PROCESS",
            "katex_ignore",
        ] {
            let doc = Document::from(format!(
                "<section class='{class}'><p>Ordinary prose.</p></section>"
            ));
            let section = dom::select(doc.root(), "section")[0];
            assert!(!is_math_source(section), "{class}");
            assert!(!has_math_markup(section), "{class}");
            assert!(
                dom::select(doc.root(), SOURCE_SELECTOR).is_empty(),
                "{class}"
            );
            assert!(warnings(&doc).is_empty(), "{class}");
        }
        let doc = Document::from(
            "<section class='tex2jax_ignore mathjax_ignore'><p>Actual formula <math><mi>x</mi></math>.</p></section>",
        );
        assert_eq!(
            warnings(&doc).len(),
            1,
            "actual descendant math remains detectable"
        );
    }

    #[test]
    fn broad_detection_markers_do_not_define_source_boundaries() {
        for class in ["MathJax_Preview", "katex-html", "latex"] {
            let doc = Document::from(format!("<span class='{class}'>x</span>"));
            let span = dom::select(doc.root(), "span")[0];
            assert!(has_math_markup(span), "{class}");
            assert!(!is_math_source(span), "{class}");
            assert!(
                dom::select(doc.root(), SOURCE_SELECTOR).is_empty(),
                "{class}"
            );
            assert_eq!(warnings(&doc).len(), 1, "{class}");
        }
        let doc = Document::from("<span role='math'>x</span>");
        let span = dom::select(doc.root(), "span")[0];
        assert!(has_math_markup(span));
        assert!(!is_math_source(span));
        assert_eq!(warnings(&doc).len(), 1);
    }

    #[test]
    fn known_generated_source_roots_match_the_shared_selector() {
        for class in [
            "MathJax",
            "MathJax_Display",
            "MathJax_CHTML",
            "MathJax_SVG",
            "MathJax_SVG_Display",
            "katex",
            "katex-display",
            "katex-mathml",
            "mwe-math-element",
            "mwe-math-mathml-inline",
            "mwe-math-mathml-display",
            "math",
            "math-inline",
            "math-block",
            "math-display",
            "arithmatex",
            "tex-math",
            "hurmet-tex",
        ] {
            let doc = Document::from(format!("<span class='{class}'>x</span>"));
            let span = dom::select(doc.root(), "span")[0];
            assert!(is_math_source(span), "{class}");
            assert!(has_math_markup(span), "{class}");
            assert_eq!(dom::select(doc.root(), SOURCE_SELECTOR).len(), 1, "{class}");
            assert_eq!(warnings(&doc).len(), 1, "{class}");
        }
    }
}
