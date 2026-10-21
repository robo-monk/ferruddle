//! Independent orchestration probes: use public APIs and inspect reparsed output.
use defuddle::{Options, WarningCode, dom, extract, safety, utils};
use dom_query::Document;

fn untouched() -> Options {
    Options {
        markdown: true,
        content_selector: Some("article".to_owned()),
        remove_exact_selectors: false,
        remove_partial_selectors: false,
        remove_hidden_elements: false,
        remove_low_scoring: false,
        remove_small_images: false,
        remove_content_patterns: false,
        standardize: false,
        ..Options::default()
    }
}

fn assert_safe(html: &str) {
    let document = Document::from(html);
    for node in dom::subtree(document.root()) {
        assert!(!safety::is_unsafe_element(node), "{}", node.html());
        for attribute in node.attrs() {
            let name = attribute.name.local.as_ref();
            assert!(!name.to_ascii_lowercase().starts_with("on") && name != "srcdoc");
            if matches!(name, "href" | "src" | "action" | "formaction" | "poster") {
                assert!(
                    !safety::is_dangerous_url(
                        &attribute.value,
                        !(node.has_name("iframe") && name == "src")
                    ),
                    "{}",
                    node.html()
                );
            }
        }
    }
}

#[test]
fn unsafe_selected_roots_cannot_be_laundered_by_optional_standardizers() {
    for standardize in [false, true] {
        for tag in ["script", "style", "object", "template"] {
            let html = format!(
                "<html><head><title>Safe title</title></head><body><{tag} id=payload class=code-block data-lang=js>poisoned_selected_content</{tag}><article><p>Other article prose.</p></article></body></html>"
            );
            let result = extract(
                &html,
                Some("https://example.test/page"),
                &Options {
                    content_selector: Some("#payload".to_owned()),
                    standardize,
                    ..untouched()
                },
            )
            .unwrap();
            assert_eq!(result.content, "", "{tag}, standardize={standardize}");
            assert_eq!(result.word_count, 0);
        }
    }
}

#[test]
fn explicit_unsafe_root_stays_empty_when_schema_text_is_available() {
    let schema = serde_json::json!({"@type":"Article", "articleBody":
        "<p>A separate schema article has many useful words but was not the selected root.</p>"});
    let html = format!(
        "<script type='application/ld+json'>{schema}</script><script id=payload>unsafe_selected_content</script><article><p>Other.</p></article>"
    );
    let result = extract(
        &html,
        Some("https://example.test/page"),
        &Options {
            content_selector: Some("#payload".to_owned()),
            ..untouched()
        },
    )
    .unwrap();
    assert_eq!(result.content, "");
    assert_eq!(result.content_markdown.as_deref(), Some(""));
}

#[test]
fn metadata_cover_upgrade_never_selects_an_unsafe_srcset_candidate() {
    for poison in [
        "javascript:alert(1)",
        "blob:https://example.test/id",
        "data:text/html,bad",
    ] {
        let html = format!(
            r#"<head><meta property=og:image content="https://example.test/cover.png"></head>
          <article><p>Article prose remains available.</p><img src="/cover.png" srcset="/safe-large.png 1000w, {poison} 9000w"></article>"#
        );
        let result = extract(&html, Some("https://example.test/page"), &untouched()).unwrap();
        assert!(
            !safety::is_dangerous_url(&result.metadata.image, true),
            "{}",
            result.metadata.image
        );
        assert_eq!(result.metadata.image, "https://example.test/safe-large.png");
        assert_safe(&result.content);
    }
}

#[test]
fn schema_math_fallback_reports_the_explicit_unsupported_warning() {
    let schema = serde_json::json!({"@type":"Article", "articleBody":
        "<p>This article has enough words for schema content recovery and a native formula.</p><math><mi>x</mi></math>"});
    let html = format!("<script type='application/ld+json'>{schema}</script><p>Short.</p>");
    let result = extract(
        &html,
        Some("https://example.test/page"),
        &Options {
            markdown: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(result.content.contains("<math>"));
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].code, WarningCode::MathNotSupported);
}

#[test]
fn gallery_views_with_generic_or_empty_alts_are_distinct_content() {
    for alt in ["", "image"] {
        let html = format!(
            r#"<article><p>Two different views of this object.</p><figure>
          <img src="/front.jpg" alt="{alt}"><img src="/back.jpg" alt="{alt}"><figcaption>Front and back views.</figcaption>
          </figure></article>"#
        );
        let result = extract(&html, Some("https://example.test/page"), &untouched()).unwrap();
        let document = Document::from(result.content);
        assert_eq!(dom::select(document.root(), "img").len(), 2, "alt={alt}");
    }
}

#[test]
fn query_parameters_that_identify_images_are_preserved_in_cover_matching() {
    let html = r#"<head><meta property=og:image content="https://images.example.test/render?id=cover"></head>
      <article><p>The article explains a different diagram.</p><img src="https://images.example.test/render?id=diagram" alt="Diagram"></article>"#;
    let result = extract(html, Some("https://example.test/page"), &untouched()).unwrap();
    assert!(result.content.contains("id=diagram"));
    assert_eq!(
        result.metadata.image,
        "https://images.example.test/render?id=cover"
    );
}

#[test]
fn base_context_and_fragments_preserve_safe_urls_when_cleanup_is_disabled() {
    let html = r##"<head><base href="../assets/"></head><article><p>Useful text.</p>
      <a href="references.html?q=a&amp;b=c">Reference</a><a href="#note">Note</a>
      <img src="image.png" srcset="/cdn/w_400,q_auto/a.png 400w, /cdn/w_800,q_auto/a.png 800w">
      <iframe src="data:image/svg+xml,bad" srcdoc="bad"></iframe></article>"##;
    let result = extract(
        html,
        Some("https://example.test/posts/current"),
        &untouched(),
    )
    .unwrap();
    assert!(
        result
            .content
            .contains("https://example.test/assets/references.html?q=a&amp;b=c")
    );
    assert!(result.content.contains("href=\"#note\""));
    assert!(
        result
            .content
            .contains("https://example.test/assets/image.png")
    );
    assert!(
        result
            .content
            .contains("https://example.test/cdn/w_800,q_auto/a.png 800w")
    );
    assert_safe(&result.content);
}

#[test]
fn schema_fallback_keeps_the_original_documents_base_url_context() {
    let schema = serde_json::json!({"@type":"Article", "articleBody":
        "<p>This recovered article has enough useful words for schema fallback and a relative reference.</p><a href='reference.html'>Source</a>"});
    let html = format!(
        "<head><base href='/assets/'><script type='application/ld+json'>{schema}</script></head><p>Short.</p>"
    );
    let result = extract(
        &html,
        Some("https://example.test/posts/current"),
        &Options {
            markdown: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(
        result
            .content
            .contains("https://example.test/assets/reference.html"),
        "{}",
        result.content
    );
}

#[test]
fn remove_images_applies_to_dom_schema_and_embedded_adapter_output() {
    let options = Options {
        remove_images: true,
        ..untouched()
    };
    let dom_case = "<article><p>Article text survives.</p><picture><source srcset=/large.png><img src=/photo.png></picture></article>";
    let schema = serde_json::json!({"@type":"Article", "articleBody":
        "<p>Recovered schema text has sufficient words for fallback extraction.</p><img src=/photo.png>"});
    let schema_case = format!("<script type='application/ld+json'>{schema}</script><p>Short.</p>");
    let embedded = serde_json::json!({"feedData":{"initialPost":{"post":{"body_html":"<p>Embedded article.</p><img src=/photo.png>","title":"Embedded title"}}}});
    let raw = serde_json::to_string(&embedded.to_string()).unwrap();
    let embedded_case = format!("<script>window._preloads=JSON.parse({raw});</script>");
    for (html, url) in [
        (dom_case, "https://example.test/page"),
        (schema_case.as_str(), "https://example.test/page"),
        (
            embedded_case.as_str(),
            "https://sample.substack.com/p/example",
        ),
    ] {
        let result = extract(html, Some(url), &options).unwrap();
        assert!(
            !result.content.contains("<img")
                && !result.content.contains("<picture")
                && !result.content.contains("<source"),
            "url={url}, content={}",
            result.content
        );
        assert!(!result.content.is_empty());
        assert_safe(&result.content);
    }
}

#[test]
fn bounded_nesting_malformed_markup_and_unicode_do_not_panic() {
    let nested = format!(
        "<article>{}<p>Пример مرحبا دنیا 你好 αβ 漢字 👩🏽‍💻.</p>{}</article>",
        "<span>".repeat(128),
        "</span>".repeat(128)
    );
    let templates = format!(
        "<article><p>Safe before.</p>{}<img src=x onerror=bad()>{}<p>Safe after.</p></article>",
        "<template>".repeat(32),
        "</template>".repeat(32)
    );
    for html in [
        nested.as_str(),
        templates.as_str(),
        "",
        "\0<math><mtext><table><mglyph><style><!--x",
        "<article><p>नमस्ते\u{200c}جهان &amp; שלום.</p></article>",
    ] {
        for url in [None, Some("not a URL"), Some("https://example.test/привет")] {
            let result = extract(
                html,
                url,
                &Options {
                    markdown: true,
                    ..Options::default()
                },
            )
            .unwrap();
            assert_safe(&result.content);
        }
    }
    let document = Document::from(
        "<p><em>re</em>tain one</p><table><tr><td>two</td><td>three</td></tr></table>",
    );
    assert_eq!(utils::count_content_words(document.body().unwrap()), 4);
}
