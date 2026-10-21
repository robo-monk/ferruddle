//! Anonymous minimal reproductions derived from the source-backed review probes.
use defuddle::{Options, WarningCode, extract};

#[test]
fn oversized_site_metadata_cannot_panic_during_title_cleanup() {
    let site = "A".repeat(300_000);
    let html = format!(
        "<html><head><title>Article title</title><meta property=\"og:site_name\" content=\"{site}\"></head><body><article><p>Some ordinary article text.</p></article></body></html>"
    );
    let result = extract(
        &html,
        Some("https://example.org/article"),
        &Options::default(),
    )
    .unwrap();
    assert_eq!(result.metadata.title, "Article title");
    assert!(result.content.contains("Some ordinary article text."));
}
use dom_query::Document;

#[test]
fn hidden_retry_searches_body_descendants_without_selecting_body() {
    let result = extract(
        include_str!("fixtures/pipeline/hidden-body.html"),
        Some("https://example.org/article"),
        &Options {
            debug: true,
            markdown: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(result.word_count, 3);
    assert_eq!(
        result.content,
        include_str!("expected/pipeline/hidden-body.html").trim()
    );
    assert_eq!(
        result.content_markdown.as_deref(),
        Some("Short teaser only.")
    );
}

#[test]
fn hidden_retry_keeps_the_first_equal_candidate_in_document_order() {
    let result = extract(
        include_str!("fixtures/pipeline/hidden-equal-candidates.html"),
        Some("https://example.org/article"),
        &Options {
            debug: true,
            markdown: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(result.word_count, 36);
    assert_eq!(
        result.content,
        include_str!("expected/pipeline/hidden-equal-candidates.html").trim()
    );
    assert!(
        result
            .debug
            .unwrap()
            .content_selector
            .ends_with("section:nth-child(2)")
    );
}

fn relaxed() -> Options {
    Options {
        markdown: true,
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

#[test]
fn unicode_english_dates_return_without_panicking_or_publishing_invalid_dates() {
    for html in [
        include_str!("fixtures/pipeline/unicode-digit-date.html"),
        include_str!("fixtures/pipeline/unicode-month-date.html"),
        "<article><h1>Title</h1><p>3 March ٢٠٢٤</p></article>",
        "<article><h1>Title</h1><p>31 February 2024</p></article>",
    ] {
        let result = extract(html, None, &Options::default()).unwrap();
        assert_eq!(result.metadata.published, "", "{html}");
    }
    for date in ["3 March 2024", "March 3, 2024", "3 MARCH 2024"] {
        let html = format!("<article><h1>Title</h1><p>{date}</p></article>");
        assert_eq!(
            extract(&html, None, &Options::default())
                .unwrap()
                .metadata
                .published,
            "2024-03-03T00:00:00+00:00"
        );
    }
}

#[test]
fn schema_recovery_ignores_unsafe_matching_roots_and_their_descendants() {
    let wrapped = include_str!("fixtures/pipeline/schema-body.html")
        .replace("<script", "<div><script")
        .replace("</script>", "</script></div>");
    for html in [
        include_str!("fixtures/pipeline/schema-body.html"),
        include_str!("fixtures/pipeline/schema-noscript.html"),
        wrapped.as_str(),
    ] {
        let result = extract(
            html,
            Some("https://example.test/story"),
            &Options::default(),
        )
        .unwrap();
        assert_eq!(result.word_count, 196, "{}", result.content);
        assert_eq!(
            Document::fragment(result.content.as_str()).text().trim(),
            include_str!("expected/pipeline/schema-article.txt").trim()
        );
        assert!(!result.content.contains("<noscript"));
        assert!(!result.content.contains("<script"));
    }
}

#[test]
fn ordinary_links_with_social_host_suffixes_keep_their_paragraphs() {
    let html = include_str!("fixtures/pipeline/social-link.html");
    for host in [
        "www.dropbox.com",
        "netflix.com",
        "vox.com",
        "notlinkedin.com",
        "unrelated.example.test",
    ] {
        let result = extract(
            &html.replace("www.dropbox.com", host),
            None,
            &Options::default(),
        )
        .unwrap();
        assert!(
            result
                .content
                .contains("Download the complete source files"),
            "{host}: {}",
            result.content
        );
    }
}

#[test]
fn positional_explicit_unsafe_root_is_fixed_after_image_removal() {
    let schema = serde_json::json!({"@type":"Article","articleBody":"<p>This separate schema article contains substantial useful prose which cannot replace an explicitly selected unsafe element.</p>"});
    let html = format!(
        "<body><script type='application/ld+json'>{schema}</script><img src='/photo.png'><noscript>Unsafe selected text.</noscript><article><p>Separate article text.</p></article></body>"
    );
    let result = extract(
        &html,
        None,
        &Options {
            content_selector: Some("body > :nth-child(2)".into()),
            remove_images: true,
            debug: true,
            ..relaxed()
        },
    )
    .unwrap();
    assert_eq!(result.content, "");
    assert_eq!(result.word_count, 0);
    assert_eq!(result.content_markdown.as_deref(), Some(""));
    assert!(
        result
            .debug
            .unwrap()
            .content_selector
            .contains("noscript:nth-child(2)")
    );
}

#[test]
fn conversation_adapters_strip_controls_even_with_optional_cleanup_disabled() {
    for options in [Options::default(), relaxed()] {
        let result = extract(
            include_str!("fixtures/pipeline/conversation-controls.html"),
            Some("https://chatgpt.com/share/example"),
            &options,
        )
        .unwrap();
        let doc = Document::from(result.content.clone());
        assert!(
            doc.select("form,button,select,textarea,input:not([type=checkbox])")
                .is_empty(),
            "{}",
            result.content
        );
        assert!(result.content.contains("Useful response prose."));
        assert!(result.content.contains("Completed task"));
        assert_eq!(doc.select("li input[type=checkbox]").length(), 2);
        assert_eq!(doc.select("input[checked]").length(), 1);
    }
}

#[test]
fn materialized_adapter_math_warns_once_and_stays_source() {
    for formula in [
        "The estimator is $\\hat{\\theta}_{n} = x_{i}+1$ in this useful article.",
        "The estimator is described in this useful article.",
    ] {
        let data = serde_json::json!({"initialData":{"data":{"article":{"headline":{"default":"Title"},"body":{"content":[{"__typename":"ParagraphBlock","content":[{"text":formula}]}]}}}}});
        let html = format!("<script>window.__preloadedData = {data};</script>");
        let result = extract(
            &html,
            Some("https://www.nytimes.com/article"),
            &Options {
                markdown: true,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(result.extractor_type.as_deref(), Some("nytimes"));
        if formula.contains('$') {
            assert_eq!(result.warnings.len(), 1);
            assert_eq!(result.warnings[0].code, WarningCode::MathNotSupported);
            assert_eq!(
                result.warnings[0].message,
                "Math support is not yet included."
            );
            assert!(result.content.contains("x_{i}+1"));
        } else {
            assert!(result.warnings.is_empty());
        }
    }
}

#[test]
fn scoring_uses_ascii_boundaries_next_to_non_ascii_text() {
    let nav = extract(
        include_str!("fixtures/pipeline/nonascii-nav-boundaries.html"),
        None,
        &Options::default(),
    )
    .unwrap();
    assert!(!nav.content.contains("éshare漢"), "{}", nav.content);
    let date = extract(
        include_str!("fixtures/pipeline/nonascii-date-boundaries.html"),
        None,
        &Options::default(),
    )
    .unwrap();
    assert!(
        date.content.contains("éMarch 3, 2024漢"),
        "{}",
        date.content
    );
}

#[test]
fn promoted_noscript_formula_images_keep_their_warning_without_duplicates() {
    for existing in ["", "<p>A separate source is $x_{i}+1$.</p>"] {
        let html = format!(
            r"<article><p>Useful surrounding article prose.</p>{existing}<figure><noscript><img src='/formula.png' alt='\frac{{x}}{{y}}'></noscript></figure></article>"
        );
        let result = extract(
            &html,
            Some("https://example.test/article"),
            &Options::default(),
        )
        .unwrap();
        assert!(result.content.contains("formula.png"), "{}", result.content);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].code, WarningCode::MathNotSupported);
        assert_eq!(
            result.warnings[0].message,
            "Math support is not yet included."
        );
    }
}

#[test]
fn schema_leading_whitespace_is_trimmed_before_selecting_the_search_phrase() {
    let result = extract(
        include_str!("fixtures/pipeline/schema-leading-whitespace.html"),
        Some("https://example.test/article"),
        &Options {
            content_selector: Some("#teaser".into()),
            debug: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(
        result.content,
        include_str!("expected/pipeline/schema-leading-whitespace.html").trim()
    );
    assert!(
        result
            .debug
            .unwrap()
            .content_selector
            .ends_with("p:nth-child(2)")
    );
}
