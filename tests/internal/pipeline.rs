use dom_query::Document;
use ferruddle::{Options, WarningCode, extract};

fn run(html: &str, options: Options) -> ferruddle::Extraction {
    extract(html, Some("https://example.com/posts/current"), &options).unwrap()
}
fn unsafe_attributes(html: &str) {
    let doc = Document::from(html);
    assert!(
        doc.select("script:not([type^='math/']),style,object,embed,template,iframe[srcdoc]")
            .is_empty(),
        "{html}"
    );
    for n in ferruddle::dom::elements(doc.root()) {
        for a in n.attrs() {
            let name = a.name.local.as_ref();
            assert!(!name.starts_with("on"), "{html}");
            if matches!(name, "href" | "src") {
                assert!(
                    !ferruddle::safety::is_dangerous_url(
                        &a.value,
                        name == "src" && !n.has_name("iframe")
                    ),
                    "{html}"
                );
            }
        }
    }
}
#[test]
fn extracts_article_and_resolves_urls_with_base_context() {
    let result = run(
        r#"<head><base href='/assets/'><title>Example story</title></head><nav>Menu</nav><article><h1>Example story</h1><p>This is a useful paragraph with sufficient words to be real article content.</p><p><a href='../references'>Sources</a> and <a href='#note'>note</a>.</p><img src='large.png' width=600 height=400></article><footer>Footer</footer>"#,
        Options {
            markdown: true,
            ..Default::default()
        },
    );
    assert!(result.content.contains("useful paragraph"));
    assert!(result.content.contains("https://example.com/references"));
    assert!(
        result
            .content
            .contains("https://example.com/assets/large.png")
    );
    assert!(result.content.contains("href=\"#note\""));
    assert!(!result.content.contains("Menu"));
    assert!(result.content_markdown.unwrap().contains("Sources"));
}
#[test]
fn all_optional_passes_disabled_still_sanitize_html_and_markdown() {
    let options = Options {
        markdown: true,
        remove_exact_selectors: false,
        remove_partial_selectors: false,
        remove_hidden_elements: false,
        remove_low_scoring: false,
        remove_small_images: false,
        remove_content_patterns: false,
        standardize: false,
        ..Default::default()
    };
    let result = run(
        r#"<article onclick='evil()'><p>A safe paragraph stays present.</p><a href='java&#x09;script:evil()'>Link</a><iframe src='data:image/svg+xml,x' srcdoc='evil()'></iframe><svg><script type='math/tex'>evil()</script></svg><img src='blob:x' onerror='evil()'><script>evil()</script></article>"#,
        options,
    );
    unsafe_attributes(&result.content);
    let markdown = result.content_markdown.unwrap();
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new(&markdown));
    unsafe_attributes(&html);
    assert!(result.content.contains("A safe paragraph"));
}
#[test]
fn unsafe_selected_root_is_empty_even_without_standardization() {
    for selector in ["script", "template", "object"] {
        let result = run(
            "<script id=bad>evil()</script><template>hidden</template><object>hidden</object><p>Other.</p>",
            Options {
                content_selector: Some(selector.into()),
                standardize: false,
                ..Default::default()
            },
        );
        assert_eq!(result.content, "");
        assert_eq!(result.word_count, 0);
    }
}
#[test]
fn original_math_is_warned_once_in_html_and_markdown_modes() {
    let result = run(
        "<article><h1>Math</h1><p>Some useful prose with an inline formula <math><mi>x</mi></math> in the original document.</p><div class=katex>y</div></article>",
        Options {
            markdown: true,
            ..Default::default()
        },
    );
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].code, WarningCode::MathNotSupported);
    assert_eq!(
        result.warnings[0].message,
        "Math support is not yet included."
    );
    assert!(result.content.contains("<math>"));
}
#[test]
fn debug_preserves_diagnostic_attributes_and_invalid_selector_errors() {
    let result = run(
        "<article id=story class=prose><p>Useful article prose stays here, with a complete sentence.</p></article>",
        Options {
            debug: true,
            content_selector: Some("[".into()),
            ..Default::default()
        },
    );
    assert!(result.content.contains("id=\"story\""));
    let debug = result.debug.unwrap();
    assert!(!debug.content_selector.is_empty());
    assert_eq!(debug.errors.len(), 1);
}
#[test]
fn safe_large_inline_data_images_are_retained() {
    let html = format!(
        "<article><p>A complete paragraph with sufficient words to identify article content.</p><img src=\"data:image/png;base64,{}\" width=400 height=300></article>",
        "A".repeat(160)
    );
    assert!(
        run(&html, Options::default())
            .content
            .contains("data:image/png;base64,")
    );
}
#[test]
fn schema_fallback_sanitizes_content_even_with_no_dom_match() {
    let html = r#"<script type='application/ld+json'>{"@type":"Article","articleBody":"<p>This long useful content is provided in schema data and must be returned safely.</p><img src=x onerror=evil()><script>evil()<\/script>"}</script><p>Short.</p>"#;
    let result = run(
        html,
        Options {
            markdown: true,
            ..Default::default()
        },
    );
    assert!(result.content.contains("schema data"));
    unsafe_attributes(&result.content);
}

#[test]
fn word_counts_separate_blocks_and_cells_but_keep_inline_words_together() {
    let doc = Document::from(
        "<p><em>re</em>tain this</p><p>next paragraph</p><table><tr><td>one</td><td>two</td></tr></table>",
    );
    assert_eq!(
        ferruddle::utils::count_content_words(doc.body().unwrap()),
        6
    );
    assert_eq!(ferruddle::utils::count_words("你好 world"), 3);
}
