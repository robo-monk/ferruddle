//! Source preservation, rather than formula conversion, is the unsupported-math contract.
use defuddle::{Options, WarningCode, extract};
use dom_query::{Document, NodeRef};
use std::{collections::BTreeMap, path::Path};

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn markdown_document(markdown: &str) -> Document {
    let mut html = String::new();
    pulldown_cmark::html::push_html(
        &mut html,
        pulldown_cmark::Parser::new_ext(
            markdown,
            pulldown_cmark::Options::ENABLE_TABLES
                | pulldown_cmark::Options::ENABLE_FOOTNOTES
                | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
                | pulldown_cmark::Options::ENABLE_TASKLISTS,
        ),
    );
    Document::from(html)
}

fn assert_warning(result: &defuddle::Extraction) {
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].code, WarningCode::MathNotSupported);
    assert_eq!(
        result.warnings[0].message,
        "Math support is not yet included."
    );
}

#[test]
fn unsupported_data_math_keeps_sources_and_the_following_article_heading() {
    let result = extract(
        include_str!("data/native/math-source-heading.html"),
        Some("https://example.org/article"),
        &Options {
            markdown: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_warning(&result);
    let html = Document::from(result.content.as_str());
    assert_eq!(html.select("h3").text().as_ref(), "Quadratic Formula");
    let sources: Vec<_> = html
        .select("[data-math]")
        .nodes()
        .iter()
        .map(|node| node.attr_or("data-math", "").to_string())
        .collect();
    assert_eq!(sources, ["x", r"x = \frac{-b}{2a}"]);
    let markdown = result.content_markdown.unwrap();
    assert!(markdown.contains("### Quadratic Formula"), "{markdown}");
    let rendered = markdown_document(&markdown);
    assert_eq!(rendered.select("[data-math]").length(), 2, "{markdown}");
    assert!(markdown.contains("For all") && markdown.contains("it holds."));
}

fn signature(node: NodeRef<'_>) -> String {
    if node.has_name("img") {
        format!("{}|{}", node.attr_or("src", ""), node.attr_or("alt", ""))
    } else {
        let source_attributes = ["data-math", "data-latex", "data-tex", "data-mathml"];
        // Source attributes are the formula contract; hidden duplicate visual
        // output may be removed while the complete authored source survives.
        let mut signature = if !node.has_name("math")
            && source_attributes
                .iter()
                .any(|attribute| node.has_attr(attribute))
        {
            String::new()
        } else {
            normalized(&node.text())
        };
        if node.has_name("math") {
            signature.push_str("|MathML:");
            for descendant in std::iter::once(node)
                .chain(node.descendants_it())
                .filter(NodeRef::is_element)
            {
                signature.push_str(&descendant.node_name().unwrap_or_default());
                signature.push(' ');
            }
        }
        for attribute in source_attributes {
            if let Some(value) = node.attr(attribute) {
                signature.push_str(&format!("|{attribute}={value}"));
            }
        }
        signature
    }
}

fn signatures(document: &Document, selector: &str) -> BTreeMap<String, usize> {
    let mut result = BTreeMap::new();
    for node in document.select(selector).nodes() {
        *result.entry(signature(*node)).or_default() += 1;
    }
    result
}

fn assert_formula_contract(
    id: &str,
    original: &Document,
    document: &Document,
    selector: &str,
    prose: &str,
    markdown: bool,
    mode: &str,
) {
    assert!(
        normalized(&document.text()).contains(prose),
        "{id} {mode}: article prose"
    );
    if !selector.is_empty() {
        let output_selector = if markdown && id == "math--mathjax-tex-scripts" {
            "code"
        } else {
            selector
        };
        assert_eq!(
            signatures(document, output_selector),
            signatures(original, selector),
            "{id} {mode}: every original formula source"
        );
    }
    if id == "math--raw-latex" {
        for formula in [
            r"h_{i}^{\ell+1}",
            r"f(x) = \sum_{i=0}^{n}",
            r"g(x) = \int_0^1",
            r"\mathcal{L}(\theta)",
            "$x$",
            "$n$",
            r"\(E = mc^2\)",
            r"\[F = ma\]",
        ] {
            assert!(
                normalized(&document.text()).contains(formula),
                "{id} {mode}: missing original source {formula}"
            );
        }
    }
}

#[test]
fn every_pinned_math_case_keeps_its_formula_sources_and_surrounding_prose() {
    // Contracts apply to default/debug extraction and standalone conversion.
    // They identify authored source, including multiplicities, not rendered TeX.
    let contracts: serde_json::Value =
        serde_json::from_str(include_str!("data/quality/math.json")).unwrap();
    let cases = contracts.as_object().unwrap();
    assert_eq!(cases.len(), 18);
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/upstream");
    for (id, contract) in cases {
        let source_contract = &contract["sourceContract"];
        let selector = source_contract["selector"].as_str().unwrap();
        let count = source_contract["count"].as_u64().unwrap() as usize;
        let prose = source_contract["prose"].as_str().unwrap();
        let case: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join(format!("{id}.json"))).unwrap())
                .unwrap();
        let source = case["inputHtml"].as_str().unwrap();
        let original = Document::from(source);
        if !selector.is_empty() {
            assert_eq!(
                original.select(selector).length(),
                count,
                "{id}: source formula count"
            );
        }
        for debug in [false, true] {
            let mode = if debug { "debug" } else { "default" };
            let result = extract(
                source,
                case["url"].as_str(),
                &Options {
                    markdown: true,
                    debug,
                    ..Options::default()
                },
            )
            .unwrap();
            assert_warning(&result);
            assert_formula_contract(
                id,
                &original,
                &Document::from(result.content.as_str()),
                selector,
                prose,
                false,
                mode,
            );
            assert_formula_contract(
                id,
                &original,
                &markdown_document(result.content_markdown.as_deref().unwrap()),
                selector,
                prose,
                true,
                mode,
            );
        }
        let standalone = defuddle::html_to_markdown(source, case["url"].as_str()).unwrap();
        assert_eq!(
            standalone.warnings.len(),
            1,
            "{id}: standalone warning count"
        );
        assert_eq!(standalone.warnings[0].code, WarningCode::MathNotSupported);
        assert_eq!(
            standalone.warnings[0].message,
            "Math support is not yet included."
        );
        assert_formula_contract(
            id,
            &original,
            &markdown_document(&standalone.markdown),
            selector,
            prose,
            true,
            "standalone",
        );
    }
}

fn relaxed(standardize: bool) -> Options {
    Options {
        markdown: true,
        standardize,
        remove_exact_selectors: false,
        remove_partial_selectors: false,
        remove_hidden_elements: false,
        remove_low_scoring: false,
        remove_small_images: false,
        remove_content_patterns: false,
        ..Options::default()
    }
}

fn assert_no_directives(document: &Document) {
    assert!(
        document.select("meta,link").is_empty(),
        "{}",
        document.html()
    );
    for node in document.select("*").nodes() {
        for attribute in node.attrs() {
            let name = attribute.name.local.to_ascii_lowercase();
            assert!(
                !name.starts_with("on") && name.as_ref() != "srcdoc",
                "{}",
                node.html()
            );
        }
    }
}

#[test]
fn resource_directives_and_poisoned_math_sources_are_sanitized_with_optional_cleanup_disabled() {
    let source = r#"<article><p>Keep the article explanation.</p>
      <meta http-equiv="refresh" content="0;url=https://example.org/redirect">
      <link rel="stylesheet" href="https://example.org/track.css">
      <span data-math="&quot;&gt;&lt;img src=x onerror=bad()&gt;" onclick="bad()"></span>
      <span data-tex="x_{i}" onfocus="bad()"></span><div data-latex="x^2"></div>
      <math onclick="bad()"><semantics><mi>x</mi><annotation encoding="application/x-tex">x</annotation></semantics></math>
      <a href="javascript:bad()">Safe link text.</a></article>"#;
    for standardize in [false, true] {
        let result = extract(
            source,
            Some("https://example.org/article"),
            &relaxed(standardize),
        )
        .unwrap();
        assert_warning(&result);
        let html = Document::from(result.content.as_str());
        let markdown = markdown_document(result.content_markdown.as_deref().unwrap());
        for document in [&html, &markdown] {
            assert_no_directives(document);
            assert!(
                document.select("img").is_empty(),
                "source must remain an inert attribute value"
            );
            assert_eq!(
                document.select("[data-math]").attr("data-math").as_deref(),
                Some("\"><img src=x onerror=bad()>")
            );
            assert_eq!(
                document.select("[data-tex]").attr("data-tex").as_deref(),
                Some("x_{i}")
            );
            assert_eq!(
                document
                    .select("[data-latex]")
                    .attr("data-latex")
                    .as_deref(),
                Some("x^2")
            );
            assert_eq!(document.select("math mi").text().as_ref(), "x");
        }
    }
}

#[test]
fn conversation_adapter_directives_are_removed_with_standardization_disabled() {
    let source = r#"<div data-testid="conversation-turn-1"><div data-message-author-role="assistant"><div class="markdown">
      <p onclick="bad()">Keep the message text.</p><meta http-equiv="refresh" content="0;url=https://example.org/redirect">
      <link rel="stylesheet" href="https://example.org/track.css"><math onload="bad()"><mi>x</mi></math>
      </div></div></div>"#;
    for standardize in [false, true] {
        let result = extract(
            source,
            Some("https://chatgpt.com/share/example"),
            &relaxed(standardize),
        )
        .unwrap();
        assert!(result.extractor_type.is_some());
        assert_warning(&result);
        assert_no_directives(&Document::from(result.content.as_str()));
        assert_no_directives(&markdown_document(
            result.content_markdown.as_deref().unwrap(),
        ));
        assert!(result.content.contains("Keep the message text."));
    }
}

#[test]
fn hidden_explicit_sources_keep_their_section_and_never_restore_active_attributes() {
    let source = r#"<article><p>The explanation introduces the preserved source formulas.</p>
      <h3>Source formulas</h3><p><span hidden data-tex="x_{i}" onclick="bad()"></span>
      <span aria-hidden="true" data-latex="y^2"></span><span class="hidden" data-math="z"></span>
      <math hidden><semantics><mi>w</mi><annotation encoding="application/x-tex">w</annotation></semantics></math></p></article>"#;
    let result = extract(
        source,
        None,
        &Options {
            markdown: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_warning(&result);
    for document in [
        Document::from(result.content.as_str()),
        markdown_document(result.content_markdown.as_deref().unwrap()),
    ] {
        assert_eq!(document.select("h3").text().as_ref(), "Source formulas");
        assert_eq!(
            document.select("[data-tex]").attr("data-tex").as_deref(),
            Some("x_{i}")
        );
        assert_eq!(
            document
                .select("[data-latex]")
                .attr("data-latex")
                .as_deref(),
            Some("y^2")
        );
        assert_eq!(
            document.select("[data-math]").attr("data-math").as_deref(),
            Some("z")
        );
        assert_eq!(
            document.select("annotation").attr("encoding").as_deref(),
            Some("application/x-tex")
        );
        assert_no_directives(&document);
    }
}

#[test]
fn explicitly_selected_resource_directives_are_empty_with_optional_cleanup_disabled() {
    let source = "<article><meta id=refresh http-equiv=refresh content='0;url=https://example.org/'><link id=resource rel=stylesheet href='https://example.org/style.css'><p>Article prose.</p></article>";
    for selector in ["#refresh", "#resource"] {
        let result = extract(
            source,
            None,
            &Options {
                content_selector: Some(selector.into()),
                ..relaxed(false)
            },
        )
        .unwrap();
        assert!(result.content.is_empty());
        assert_eq!(result.word_count, 0);
        assert_eq!(result.content_markdown.as_deref(), Some(""));
    }
}

fn assert_converted_prose(markdown: &str, heading: &str) {
    assert!(markdown.contains(&format!("## {heading}")), "{markdown}");
    assert!(markdown.contains("**linear**"), "{markdown}");
    assert!(markdown.contains("- First *item*"), "{markdown}");
    assert!(
        !markdown.contains("<section"),
        "article wrappers must render their children: {markdown}"
    );
    let rendered = markdown_document(markdown);
    assert_eq!(rendered.select("h2").text().as_ref(), heading);
    assert_eq!(rendered.select("p").length(), 3, "{markdown}");
    assert_eq!(rendered.select("ul > li").length(), 2, "{markdown}");
    assert_eq!(rendered.select("strong").text().as_ref(), "linear");
    assert_eq!(rendered.select("em").text().as_ref(), "item");
    assert_eq!(rendered.select("a").text().as_ref(), "relative link");
}

#[test]
fn mathjax_processing_controls_keep_normal_markdown_without_a_false_warning() {
    let source = include_str!("data/native/math-processing-controls.html");
    let url = Some("https://example.org/notebook");
    for (mode, options) in [
        (
            "default",
            Options {
                markdown: true,
                ..Options::default()
            },
        ),
        (
            "debug",
            Options {
                markdown: true,
                debug: true,
                ..Options::default()
            },
        ),
        (
            "standardize:false",
            Options {
                markdown: true,
                standardize: false,
                ..Options::default()
            },
        ),
    ] {
        let result = extract(source, url, &options).unwrap();
        assert!(result.warnings.is_empty(), "{mode}: {:?}", result.warnings);
        assert_converted_prose(
            result.content_markdown.as_deref().unwrap(),
            "Model overview",
        );
    }
    let standalone = defuddle::html_to_markdown(source, url).unwrap();
    assert!(standalone.warnings.is_empty(), "{:?}", standalone.warnings);
    assert_converted_prose(&standalone.markdown, "Model overview");
}

#[test]
fn actual_source_holder_with_prose_blocks_warns_and_renders_its_children() {
    let source = include_str!("data/native/math-processing-controls.html").replace(
        "class=\"tex2jax_ignore mathjax_ignore\"",
        "data-math=\"x^2\"",
    );
    let url = Some("https://example.org/notebook");
    for options in [
        Options {
            markdown: true,
            ..Options::default()
        },
        Options {
            markdown: true,
            debug: true,
            ..Options::default()
        },
        Options {
            markdown: true,
            standardize: false,
            ..Options::default()
        },
    ] {
        let result = extract(&source, url, &options).unwrap();
        assert_warning(&result);
        assert_converted_prose(
            result.content_markdown.as_deref().unwrap(),
            "Model overview",
        );
    }
    let standalone = defuddle::html_to_markdown(&source, url).unwrap();
    assert_eq!(standalone.warnings.len(), 1);
    assert_eq!(standalone.warnings[0].code, WarningCode::MathNotSupported);
    assert_eq!(
        standalone.warnings[0].message,
        "Math support is not yet included."
    );
    assert_converted_prose(&standalone.markdown, "Model overview");
}
