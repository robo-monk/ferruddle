use dom_query::Document;
use ferruddle::{Options as ExtractOptions, extract, html_to_markdown};
use pulldown_cmark::{Options, Parser, html};

fn markdown(source: &str) -> String {
    html_to_markdown(source, Some("https://example.com/article"))
        .unwrap()
        .markdown
}

fn rendered(source: &str) -> Document {
    assert_safe_markdown(&markdown(source))
}

fn assert_safe_markdown(markdown: &str) -> Document {
    let mut html = String::new();
    html::push_html(
        &mut html,
        Parser::new_ext(
            markdown,
            Options::ENABLE_TABLES
                | Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_FOOTNOTES,
        ),
    );
    let document = Document::from(html);
    assert!(
        document
            .select("script, iframe[srcdoc], object, embed")
            .is_empty(),
        "{markdown}"
    );
    for node in document.root().descendants_it().filter(|n| n.is_element()) {
        for attribute in node.attrs() {
            let name = attribute.name.local.as_ref();
            assert!(
                !name.to_ascii_lowercase().starts_with("on"),
                "{}\n{markdown}",
                node.html()
            );
            if matches!(name, "href" | "src") {
                let normalized: String = attribute
                    .value
                    .chars()
                    .filter(|c| !c.is_control() && !c.is_whitespace())
                    .flat_map(char::to_lowercase)
                    .collect();
                assert!(
                    !normalized.starts_with("javascript:")
                        && !normalized.starts_with("blob:")
                        && !(normalized.starts_with("data:")
                            && !normalized.starts_with("data:image/")),
                    "{}\n{markdown}",
                    node.html()
                );
            }
        }
    }
    document
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[test]
fn blocks_emphasis_and_tight_inline_content() {
    assert_eq!(
        markdown(
            "<h1>Title</h1><h2>Section</h2><p>H<sub>2</sub>O, 10<sup>−23</sup> J; foo<code>x</code>bar <b>bold</b> <i>em</i> <mark>marked</mark> <del>gone</del>.</p><hr>"
        ),
        "## Section\n\nH<sub>2</sub>O, 10<sup>−23</sup> J; foo`x`bar **bold** *em* ==marked== ~~gone~~.\n\n---"
    );
    assert_eq!(
        markdown("<p>Super<wbr>cali<wbr>fragilistic</p>"),
        "Supercalifragilistic"
    );
}

#[test]
fn preserves_code_whitespace_and_uses_a_long_enough_fence() {
    let source = "  before\n\n  ```\n`````\n  after  ";
    let markdown = markdown(&format!(
        "<pre><code data-lang=\"shell session\">{}</code></pre><p>Following</p>",
        escape(source)
    ));
    assert_eq!(
        markdown,
        format!("``````shell\n{source}\n``````\n\nFollowing")
    );
}

#[test]
fn code_language_cannot_escape_the_fence() {
    let source = format!(
        "<pre><code data-lang=\"{}\">echo 1</code></pre>",
        escape("js\n```\n\n<img src=x onerror=alert(1)>")
    );
    let document = rendered(&source);
    assert!(document.select("img").is_empty());
    assert_eq!(document.select("code").text().as_ref(), "echo 1\n");
}

#[test]
fn nested_ordered_and_task_lists_keep_their_structure() {
    let source = "<ol start=\"3\"><li>Outer<ul><li>Nested</li><li><input type=\"checkbox\" checked>Done</li></ul></li><li>Last</li></ol>";
    let document = rendered(source);
    assert_eq!(document.select("ol").attr("start").as_deref(), Some("3"));
    assert_eq!(document.select("ol > li").length(), 2);
    assert_eq!(document.select("ol ul > li").length(), 2);
    assert!(markdown(source).contains("[x] Done"));
}

#[test]
fn ragged_tables_pad_to_the_widest_row() {
    let source = "<table><tr><th>A | B</th><th>Header</th></tr><tr><td>1</td><td>2</td><td>3</td></tr><tr><td>4</td></tr></table>";
    let result = markdown(source);
    assert!(result.contains("| A \\| B | Header |  |"));
    assert!(result.contains("| --- | --- | --- |"));
    assert!(result.contains("| 4 |  |  |"));
    assert_eq!(rendered(source).select("tbody tr").length(), 2);
}

#[test]
fn nested_layout_tables_flatten_while_data_tables_survive() {
    let source = "<table><tr><td><p>Introduction</p><table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table></td><td><p>Afterward</p></td></tr></table>";
    let result = markdown(source);
    assert!(result.starts_with("Introduction\n\n| A | B |"));
    assert!(result.ends_with("Afterward"));
    assert_eq!(rendered(source).select("table").length(), 1);
}

#[test]
fn complex_tables_keep_literal_content_as_safe_html() {
    let text = "<img src=x onerror=alert(1)> &lt;literal&gt; A & B";
    let source = format!(
        "<table onclick=\"alert(1)\"><tr><td colspan=\"2\">{}</td><td>Other</td></tr></table>",
        escape(text)
    );
    let document = rendered(&source);
    assert_eq!(document.select("td").nodes()[0].text().as_ref(), text);
    assert!(document.select("img").is_empty());
}

#[test]
fn image_labels_titles_and_destinations_survive_rendering() {
    let alt = "A [label] <tag> and \\ brackets &lt;b&gt; &copy; A & B";
    let title = "A \"title\" with \\ and <tag>";
    let source = format!(
        "<img src=\"https://example.com/p(1).png?x=1&amp;y=2\" alt=\"{}\" title=\"{}\">",
        escape(alt),
        escape(title)
    );
    let document = rendered(&source);
    assert_eq!(document.select("img").length(), 1);
    assert_eq!(document.select("img").attr("alt").as_deref(), Some(alt));
    assert_eq!(document.select("img").attr("title").as_deref(), Some(title));
    assert_eq!(
        document.select("img").attr("src").as_deref(),
        Some("https://example.com/p(1).png?x=1&y=2")
    );
}

#[test]
fn poisoned_labels_and_destinations_do_not_inject_markup() {
    let payload = "x)\n\n<img src=x onerror=alert(1)>\n\n![y](z";
    for source in [
        format!(
            "<img src=\"https://example.com/p.png\" alt=\"{}\">",
            escape(payload)
        ),
        format!(
            "<figure><img src=\"https://example.com/p.png\" alt=\"{}\"><figcaption>Caption</figcaption></figure>",
            escape(payload)
        ),
        format!(
            "<img src=\"{}\" alt=\"photo\">",
            escape(&format!("https://example.com/a.png) {payload}"))
        ),
        format!(
            "<a href=\"{}\" title=\"{}\">Link</a>",
            escape(&format!("https://example.com/a) {payload}")),
            escape(payload)
        ),
        format!(
            "<div class=\"callout\" data-callout=\"{}\"><div class=\"callout-title-inner\">{}</div><div class=\"callout-content\">Body</div></div>",
            escape(payload),
            escape(payload)
        ),
    ] {
        rendered(&source);
    }
}

#[test]
fn split_text_tags_and_dangerous_autolinks_remain_inert() {
    for source in [
        "<p>&lt;<span>img src=x onerror=alert(1)&gt;</span></p>",
        "<p>&lt;img<span> src=x onerror=alert(1)&gt;</span></p>",
        "<p>&lt;javascript:alert(1)<span>&gt;</span></p>",
    ] {
        let document = rendered(source);
        assert!(document.select("img, a").is_empty());
    }
    let document = rendered(
        "<p>&lt;javascript:alert(1)&gt; &lt;https://example.com&gt; &lt;reader@example.com&gt;</p>",
    );
    assert_eq!(document.select("a").length(), 2);
}

#[test]
fn rejects_dangerous_urls_and_prefers_the_largest_safe_srcset_candidate() {
    for url in [
        "javascript:alert(1)",
        "blob:https://example.com/id",
        "data:text/html,test",
        "java&#10;script:alert(1)",
    ] {
        let document = rendered(&format!(
            "<a href=\"{url}\">Link</a><img src=\"{url}\"><iframe src=\"{url}\" srcdoc=\"&lt;script&gt;alert(1)&lt;/script&gt;\"></iframe>"
        ));
        assert!(document.select("a[href], img[src], iframe[src]").is_empty());
    }
    let document = rendered(
        "<img src=\"https://example.com/small.png\" srcset=\"https://example.com/medium.png 500w, javascript:alert(1) 1000w\"><img src=\"data:image/png;base64,AAAA\">",
    );
    assert_eq!(
        document.select("img").nodes()[0].attr("src").as_deref(),
        Some("https://example.com/medium.png")
    );
    assert_eq!(
        document.select("img").nodes()[1].attr("src").as_deref(),
        Some("data:image/png;base64,AAAA")
    );
}

#[test]
fn urls_do_not_decode_a_second_layer_of_entities() {
    let value = "java&#x73;cript:alert(1)";
    let source = format!(
        "<a href=\"{}\">Link</a><img src=\"{}\">",
        escape(value),
        escape(value)
    );
    for (page_url, expected) in [
        (None, value),
        (
            Some("https://example.com/article"),
            "https://example.com/java&#x73;cript:alert(1)",
        ),
    ] {
        let output = html_to_markdown(&source, page_url).unwrap();
        let document = assert_safe_markdown(&output.markdown);
        assert_eq!(document.select("a").attr("href").as_deref(), Some(expected));
        assert_eq!(
            document.select("img").attr("src").as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn links_with_spaces_and_parentheses_are_unambiguous() {
    assert_eq!(
        markdown("<a href=\"myapp:open shared page\" title=\"Shared page\">Open</a>"),
        "[Open](<myapp:open shared page> \"Shared page\")"
    );
    assert_eq!(
        markdown("<a href=\"https://example.com/page(1)\">Open</a>"),
        "[Open](https://example.com/page\\(1\\))"
    );
    rendered("<a href=\"https://example.com/a\\(b)\">Link</a>");
}

#[test]
fn figures_keep_captions_and_wrapper_prose() {
    let source = "<figure><img src=\"https://example.com/image.png\" alt=\"Example\"><figcaption>Caption with <em>emphasis</em>.</figcaption></figure>";
    assert_eq!(
        markdown(source),
        "![Example](https://example.com/image.png)\n\nCaption with *emphasis*."
    );
    let wrapper =
        "<figure><p>Before</p><img src=\"https://example.com/image.png\"><p>After</p></figure>";
    assert!(markdown(wrapper).contains("Before"));
    assert!(markdown(wrapper).contains("After"));
}

#[test]
fn safe_media_embeds_and_svg_remain_available() {
    assert_eq!(
        markdown("<iframe src=\"https://www.youtube-nocookie.com/embed/abc_123\"></iframe>"),
        "![](https://www.youtube.com/watch?v=abc_123)"
    );
    assert_eq!(
        markdown("<iframe src=\"https://x.com/example/status/123\"></iframe>"),
        "![](https://x.com/example/status/123)"
    );
    let document = rendered(
        "<video src=\"https://example.com/movie.mp4\" controls onerror=\"alert(1)\"></video><svg onload=\"alert(1)\"><path d=\"M0 0\"></path></svg>",
    );
    assert_eq!(document.select("video, svg").length(), 2);
}

#[test]
fn callouts_preserve_fold_titles_and_nested_content() {
    let source = "<div class=\"callout\" data-callout=\"warning\" data-callout-fold=\"-\"><div class=\"callout-title\"><div class=\"callout-title-inner\">Custom [title]</div></div><div class=\"callout-content\"><p>First</p><p>Second <strong>paragraph</strong>.</p></div></div>";
    assert_eq!(
        markdown(source),
        "> [!warning]- Custom \\[title\\]\n> First\n> \n> Second **paragraph**."
    );
}

#[test]
fn repeated_and_nested_footnotes_use_the_definition_id() {
    let source = "<div class=\"callout\" data-callout=\"note\"><div class=\"callout-content\"><p>Inside<sup id=\"fnref:note-1\"><a href=\"#fn:note-1\">1</a></sup></p></div></div><p>Outside<sup id=\"fnref:note-1-2\"><a href=\"#fn:note-1\">1</a></sup></p><div id=\"footnotes\"><ol><li id=\"fn:note-1\"><p>First paragraph.</p><p>Second paragraph.<a href=\"#fnref:note-1\" class=\"footnote-backref\">↩</a></p></li></ol></div>";
    let result = markdown(source);
    assert!(result.contains("Inside[^note-1]"));
    assert!(result.contains("Outside[^note-1]"));
    assert!(result.contains("[^note-1]: First paragraph.\n    \n    Second paragraph."));
    let document = rendered(source);
    assert!(
        document.select("p").text().contains("Second paragraph."),
        "{}",
        document.html()
    );
}

#[test]
fn footnote_ids_and_decoded_fragments_cannot_inject_markdown() {
    for id in [
        "note-é",
        "note-%C3%A9",
        "note [one]-2",
        "note-%ZZ",
        "x]\n\n<img src=x onerror=alert(1)>\n\n[^y",
    ] {
        let encoded: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
        let source = format!(
            "<p>Text<sup id=\"fnref:{}\"><a href=\"#fn:{}\">1</a></sup></p><div id=\"footnotes\"><ol><li id=\"fn:{}\">Note</li></ol></div>",
            escape(id),
            encoded.replace('+', "%20"),
            escape(id)
        );
        let result = markdown(&source);
        let reference = result
            .split("Text[^")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap();
        assert!(
            result.contains(&format!("[^{reference}]: Note")),
            "{result}"
        );
        rendered(&source);
    }
}

#[test]
fn math_is_safe_source_with_an_explicit_unsupported_warning() {
    let result = html_to_markdown(
        "<p>Value <math data-latex=\"x^2\"><msup><mi>x</mi><mn>2</mn></msup></math>.</p>",
        None,
    )
    .unwrap();
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        result.warnings[0].message,
        "Math support is not yet included."
    );
    assert!(result.markdown.contains("<math"));
    assert!(result.markdown.contains("<msup>"));
    assert!(!result.markdown.contains("$x"));
    rendered("<math data-latex=\"&lt;img src=x onerror=alert(1)&gt;\"><mi>x</mi></math>");
    let script =
        html_to_markdown("<p>Value <script type=\"math/tex\">x^2</script>.</p>", None).unwrap();
    assert_eq!(script.markdown, "Value `x^2`.");
    assert_eq!(script.warnings.len(), 1);
}

#[test]
fn exclamations_do_not_change_image_parsing() {
    assert!(markdown("<p>Yey!<img src=\"https://example.com/img.png\"></p>").contains("Yey! !["));
    assert!(markdown("<p>Hello!<a href=\"https://example.com\"><img src=\"https://example.com/img.png\"></a></p>").contains("Hello! [!["));
}

/// Optional diagnostic against immutable, externally generated oracle data.
/// Run with DEFUDDLE_MARKDOWN_ORACLE=/path/to/bundle cargo test --test markdown oracle_comparison -- --ignored --nocapture.
#[test]
#[ignore]
fn oracle_comparison() {
    let path = std::env::var("DEFUDDLE_MARKDOWN_ORACLE").expect("set DEFUDDLE_MARKDOWN_ORACLE");
    let output = std::env::var("DEFUDDLE_MARKDOWN_REPORT").ok();
    let mut report = Vec::new();
    let mut exact = 0;
    let mut math = 0;
    let mut total = 0;
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_none_or(|e| e != "json") {
            continue;
        }
        let case: serde_json::Value =
            serde_json::from_slice(&std::fs::read(entry.path()).unwrap()).unwrap();
        let Some(expected) = case["expected"]["contentMarkdown"].as_str() else {
            continue;
        };
        let source = case["expected"]["content"].as_str().unwrap();
        let result = html_to_markdown(source, case["url"].as_str()).unwrap();
        total += 1;
        if result.markdown == expected {
            exact += 1;
        }
        if !result.warnings.is_empty() {
            math += 1;
        }
        let text = |markdown: &str| {
            let mut html = String::new();
            html::push_html(
                &mut html,
                Parser::new_ext(
                    markdown,
                    Options::ENABLE_TABLES
                        | Options::ENABLE_STRIKETHROUGH
                        | Options::ENABLE_TASKLISTS
                        | Options::ENABLE_FOOTNOTES,
                ),
            );
            Document::from(html)
                .text()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };
        let expected_text = text(expected);
        let actual_text = text(&result.markdown);
        report.push(serde_json::json!({"id":case["id"],"exact":result.markdown==expected,"math":!result.warnings.is_empty(),"renderedTextEqual":expected_text==actual_text,"expectedText":expected_text,"actualText":actual_text,"expected":expected,"actual":result.markdown}));
    }
    if let Some(output) = output {
        std::fs::write(output, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
    eprintln!("Markdown oracle: {exact}/{total} exact; {math} cases contain unsupported math.");
    assert!(total > 0);
}

/// Independent check of rendered output from whole-pipeline corpus runs.
#[test]
#[ignore]
fn integrated_corpus_markdown_security() {
    let source =
        std::env::var("DEFUDDLE_INTEGRATED_CORPUS").expect("set DEFUDDLE_INTEGRATED_CORPUS");
    let cases: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(source).unwrap()).unwrap();
    let mut checked = 0;
    for case in cases {
        if let Some(markdown) = case["actual"]["contentMarkdown"].as_str() {
            assert_safe_markdown(markdown);
            checked += 1;
        }
    }
    assert!(checked > 0);
    eprintln!("Rendered Markdown security: {checked} corpus outputs passed.");
}

#[test]
fn headings_keep_line_break_text_in_one_atx_heading() {
    assert_eq!(markdown("<h2>First<br>Second</h2>"), "## First Second");
}

#[test]
fn ordered_lists_keep_large_and_negative_starts_without_losing_items() {
    let document =
        rendered("<ol start=\"100\"><li><p>First</p><p>Continuation</p></li><li>Second</li></ol>");
    assert_eq!(document.select("ol > li").length(), 2);
    assert_eq!(document.select("ol > li:first-child > p").length(), 2);
    let document = rendered("<ol start=\"-2\"><li>First</li><li>Second</li></ol>");
    assert_eq!(document.select("ol").attr("start").as_deref(), Some("-2"));
    assert_eq!(document.select("ol > li").length(), 2);
}

#[test]
fn layout_table_captions_are_not_lost_during_flattening() {
    let source = "<table><caption>Important measurements</caption><tr><td>First item</td></tr><tr><td>Second item</td></tr></table>";
    assert_eq!(
        markdown(source),
        "Important measurements\n\nFirst item\n\nSecond item"
    );
}

#[test]
fn pre_keeps_all_children_and_explicit_display_breaks() {
    let source = "<pre>prefix <code>first<br>  second</code>\n<code>third</code> suffix</pre>";
    let document = rendered(source);
    assert_eq!(
        document.select("pre code").text().as_ref(),
        "prefix first\n  second\nthird suffix\n"
    );
}

#[test]
fn literal_block_markers_stay_prose_after_trimming_and_inline_wrappers() {
    assert_eq!(
        rendered("<p> # ordinary prose</p>")
            .select("p")
            .text()
            .as_ref(),
        "# ordinary prose"
    );
    assert_eq!(
        rendered("<p><span>1</span>. ordinary prose</p>")
            .select("p")
            .text()
            .as_ref(),
        "1. ordinary prose"
    );
    let document = rendered("<p>First<br> - continuation with <em>emphasis</em>.</p>");
    assert!(document.select("ul").is_empty());
    assert_eq!(document.select("br").length(), 1);
    assert_eq!(document.select("em").text().as_ref(), "emphasis");
}

#[test]
fn inline_marker_runs_respect_container_and_hard_break_boundaries() {
    for source in [
        "<div><span>1</span>. ordinary prose</div>",
        "<p>First<br> <span>1</span>. continuation</p>",
    ] {
        let document = rendered(source);
        assert!(document.select("ol").is_empty(), "{}", markdown(source));
        assert!(document.text().contains("1. "));
    }
    assert!(
        markdown("<div><span><h2>Real heading</h2></span><p>Body.</p></div>")
            .contains("## Real heading")
    );
}

#[test]
fn generated_inline_delimiters_survive_paragraph_prefix_protection() {
    assert_eq!(markdown("<p><mark>highlight</mark></p>"), "==highlight==");
    assert_eq!(markdown("<p><del>removed</del></p>"), "~~removed~~");
    assert_eq!(
        markdown("<p><strong>first</strong><strong>second</strong></p>"),
        "**firstsecond**"
    );
    assert_eq!(
        markdown("<p><code>first</code><span></span><code>second</code></p>"),
        "`firstsecond`"
    );
    assert_eq!(markdown("<p>before<code></code>after</p>"), "beforeafter");
}

#[test]
fn inline_runs_skip_only_empty_nonblock_nodes() {
    assert_eq!(
        markdown("<strong>first</strong><span></span><!-- empty --><b>second</b>"),
        "**firstsecond**"
    );
    assert_eq!(
        markdown("<strong>first</strong><span> </span><b>second</b>"),
        "**first** **second**"
    );
    assert_eq!(
        markdown("<strong>first</strong><span><div></div></span><b>second</b>"),
        "**first****second**"
    );
    assert_eq!(
        markdown("<code>first</code><span></span><tt>second</tt>"),
        "`firstsecond`"
    );
}

#[test]
fn deep_inline_lookahead_keeps_each_source_token() {
    let depth = 200;
    let source = format!(
        "<p>{}center{}</p>",
        "<small><b>prefix,</b>".repeat(depth),
        "</small>".repeat(depth)
    );
    let result = markdown(&source);
    assert_eq!(result.matches("prefix,").count(), depth);
    assert_eq!(result.matches("center").count(), 1);

    let punctuation = format!(
        "<article><p>{}center{}</p></article>",
        "<u><em>prefix(</em>".repeat(depth),
        "</u>".repeat(depth)
    );
    let extraction = extract(
        &punctuation,
        None,
        &ExtractOptions {
            markdown: true,
            ..Default::default()
        },
    )
    .unwrap();
    let result = extraction.content_markdown.unwrap();
    assert_eq!(result.matches("prefix(").count(), depth);
    assert_eq!(result.matches("center").count(), 1);
}

#[test]
fn unclosed_legacy_fonts_do_not_repeat_markdown_render_work() {
    let depth = 200;
    let legacy: String = (0..depth)
        .map(|index| format!("<b>Item {index}:</b><font size=2>detail {index}. "))
        .collect();
    let source = format!(
        "<article><h1>Example</h1><p>{}</p><p>{legacy}</p></article>",
        "Plain article context for readers. ".repeat(60)
    );
    for result in [
        markdown(&source),
        extract(
            &source,
            None,
            &ExtractOptions {
                markdown: true,
                ..Default::default()
            },
        )
        .unwrap()
        .content_markdown
        .unwrap(),
    ] {
        assert_eq!(result.matches("Item ").count(), depth);
        assert_eq!(result.matches("detail ").count(), depth);
        for index in 0..depth {
            assert!(
                result.contains(&format!("Item {index}:")),
                "missing item {index}"
            );
            assert!(
                result.contains(&format!("detail {index}.")),
                "missing detail {index}"
            );
        }
    }
}

#[test]
fn inline_code_display_breaks_keep_token_boundaries_in_paragraphs_and_tables() {
    let source = include_str!("data/markdown/inline-code-breaks.html");
    let expected = include_str!("data/markdown/inline-code-breaks.md");
    assert_eq!(markdown(source), expected.trim_end());
    let document = rendered(source);
    let code: Vec<_> = document
        .select("code")
        .nodes()
        .iter()
        .map(|node| node.text().to_string())
        .collect();
    assert_eq!(code, ["make install", "one twothree four", "a b", "c de f"]);
}

#[test]
fn standalone_conversion_exposes_only_sanitized_declarative_shadow_content() {
    let source = "<h1>Article title</h1><div><template shadowrootmode=\"open\"><p>Visible <a href=\"javascript:alert(1)\" onclick=\"alert(1)\">link</a>.</p><script>alert(1)</script><span><template shadowrootmode=\"closed\"><p>Nested content.</p><img src=\"javascript:alert(1)\" onerror=\"alert(1)\"></template></span></template><template><p>Inert content.</p></template><template shadowrootmode=\"invalid\"><p>Invalid content.</p></template></div>";
    let result = markdown(source);
    assert_eq!(result, "Visible link.\n\nNested content.");
    let document = assert_safe_markdown(&result);
    assert!(document.select("a, img, script").is_empty());
    assert_eq!(markdown("<h1>Article title only</h1>"), "");
}

#[test]
fn standalone_conversion_resolves_relative_urls_with_page_and_base_context() {
    let source = "<base href=\"/docs/\"><p><a href=\"guide\">Guide</a> <a href=\"#part\">Part</a></p><img src=\"small.png\" srcset=\"medium.png 500w, large.png 1000w\">";
    assert_eq!(
        markdown(source),
        "[Guide](https://example.com/docs/guide) [Part](#part)\n\n![](https://example.com/docs/large.png)"
    );
    for page_url in [None, Some("invalid url")] {
        assert_eq!(
            html_to_markdown("<a href=\"../guide\">Guide</a>", page_url)
                .unwrap()
                .markdown,
            "[Guide](../guide)"
        );
    }
}

#[test]
fn retained_tables_cannot_activate_inert_math_script_source() {
    let source = "<table><tr><td colspan=\"2\"><script type=\"math/tex\">&lt;img src=x onerror=alert(1)&gt; + x^2</script><ul><li><input type=\"checkbox\" checked disabled>Checked</li></ul></td><td>Other</td></tr></table>";
    let result = html_to_markdown(source, None).unwrap();
    assert_eq!(result.warnings.len(), 1);
    let document = assert_safe_markdown(&result.markdown);
    assert!(document.select("script").is_empty());
    assert!(document.select("td code").text().contains("x^2"));
    assert_eq!(
        document
            .select("input[type=checkbox][checked][disabled]")
            .length(),
        1
    );
}

#[test]
fn retained_math_holders_keep_source_attributes_without_joining_inline_runs() {
    let source = "x^2\n<img src=x onerror=alert(1)> _literal_ `tick` 'single' \"double\"";
    for attribute in ["data-math", "data-tex", "data-mathml", "data-katex"] {
        let output = html_to_markdown(
            &format!(
                "<p>Before <strong {attribute}=\"{}\" onclick=\"alert(1)\"></strong><b>After</b>.</p>",
                escape(source)
            ),
            None,
        )
        .unwrap();
        assert_eq!(output.warnings.len(), 1);
        assert_eq!(
            output.warnings[0].message,
            "Math support is not yet included."
        );
        let document = assert_safe_markdown(&output.markdown);
        assert!(document.select("img").is_empty());
        assert_eq!(
            document.select("strong").nodes()[0]
                .attr(attribute)
                .as_deref(),
            Some(source)
        );
        assert!(output.markdown.contains("**After**"));
    }
}

#[test]
fn retained_math_holders_render_inert_script_source_as_code() {
    let source = "<span data-math=\"x^2\"><script type=\"math/tex\">&lt;img src=x onerror=alert(1)&gt; + x^2</script></span>";
    let output = html_to_markdown(source, None).unwrap();
    assert_eq!(output.warnings.len(), 1);
    let document = assert_safe_markdown(&output.markdown);
    assert!(document.select("script, img").is_empty());
    assert!(
        document
            .select("span[data-math] code")
            .text()
            .contains("x^2")
    );
}

#[test]
fn retained_math_text_survives_markdown_syntax() {
    let source = r"\mathrm{head}_{1},...,\mathrm{head}_{h} + *literal* [label](path) | x";
    let document = rendered(&format!(
        "<p>Before <math><semantics><mi>x</mi><annotation encoding=\"application/x-tex\">{}</annotation></semantics></math> after.</p>",
        escape(source)
    ));
    assert_eq!(document.select("annotation").text().as_ref(), source);
    assert_eq!(document.select("annotation em, annotation a").length(), 0);
}

#[test]
fn multiline_math_source_keeps_following_prose_outside_its_container() {
    let source = "<p>Before <mjx-container>\n    \n    <mjx-assistive-mml display=\"inline\">\n        <math><mi>x</mi></math>\n    </mjx-assistive-mml>\n</mjx-container> after.</p><h2>Following heading</h2><p>Surrounding prose remains outside the math source.</p>";
    let document = rendered(source);
    assert_eq!(document.select("h2").text().as_ref(), "Following heading");
    assert!(
        document
            .select("mjx-container h2, mjx-container p")
            .is_empty()
    );
    assert_eq!(document.select("mjx-container").length(), 1);
    assert!(document.text().contains("Before"));
    assert!(document.text().contains("after."));
}

#[test]
fn malformed_retained_attribute_names_cannot_activate_literal_markdown() {
    for attribute in ["data-a'b", "data-a\"b", "data-invalid?", "9invalid"] {
        let source = format!(
            "<article><p>See <span class=\"math\" data-math=\"x\"><b {attribute}=\"1\" data-valid=\"kept\">x</b> [evil](javascript:alert(1))</span> done.</p></article>"
        );
        let mut outputs = vec![markdown(&source)];
        for (debug, standardize) in [(false, true), (true, true), (false, false)] {
            outputs.push(
                extract(
                    &source,
                    None,
                    &ExtractOptions {
                        markdown: true,
                        debug,
                        standardize,
                        content_selector: Some("article".into()),
                        ..Default::default()
                    },
                )
                .unwrap()
                .content_markdown
                .unwrap(),
            );
        }
        for output in outputs {
            let document = assert_safe_markdown(&output);
            assert!(document.select("a").is_empty(), "{attribute}: {output}");
            assert!(document.text().contains("[evil](javascript:alert(1))"));
            for node in document.select("b").nodes() {
                assert!(!node.has_attr(attribute), "{attribute}: {output}");
            }
        }
    }
}

#[test]
fn malformed_retained_tag_names_are_unwrapped_without_activating_page_text() {
    let literal = "[evil](javascript:alert(1)) *literal*";
    for tag in ["bad'tag", "bad\"tag", "bad:tag", "bad_tag"] {
        let source = format!(
            "<span data-math=\"x\"><{tag}>{}</{tag}></span>",
            escape(literal)
        );
        let output = markdown(&source);
        let document = assert_safe_markdown(&output);
        assert!(document.select("a").is_empty(), "{tag}: {output}");
        assert_eq!(
            document.select("span[data-math]").text().as_ref(),
            literal,
            "{tag}: {output}"
        );
        assert!(!output.contains(&format!("<{tag}")), "{tag}: {output}");
    }
}

#[test]
fn retained_inert_script_preserves_exact_source_as_code() {
    let source = "<span data-math=\"x\"><script type=\"math/tex\">\n\n\\mathrm{head}_{1} &lt;tag&gt; &amp; λ\n\t[evil](javascript:alert(1))\n</script></span>";
    let original = Document::from(source);
    let document = rendered(source);
    assert_eq!(
        document.select("code").text(),
        original.select("script").text()
    );
    assert_eq!(document.select("code").length(), 1);
    assert!(document.select("script, a").is_empty());
}

fn assert_retained_literal_text(source: &str, selector: &str, literal: &str) {
    let output = markdown(source);
    let document = assert_safe_markdown(&output);
    assert!(document.select("a").is_empty(), "{output}");
    assert_eq!(
        document.select(selector).text().as_ref(),
        literal,
        "{output}"
    );
}

#[test]
fn retained_sup_and_sub_keep_markdown_links_literal() {
    let literal = "[click](javascript:alert(1))\n\n*literal*\t[other](https://example.org/)";
    for tag in ["sup", "sub"] {
        assert_retained_literal_text(
            &format!("<p>Before <{tag}>{}</{tag}> after.</p>", escape(literal)),
            tag,
            literal,
        );
    }
}

#[test]
fn retained_svg_keeps_markdown_links_literal_and_namespace_attributes() {
    let literal = "[click](javascript:alert(1))\n\n*literal*\t[other](https://example.org/)";
    let source = format!(
        "<svg viewBox=\"0 0 1 1\"><text>{}</text><use xlink:href=\"#shape\"></use></svg>",
        escape(literal)
    );
    assert_retained_literal_text(&source, "svg text", literal);
    let document = rendered(&source);
    assert_eq!(
        document.select("svg").attr("viewBox").as_deref(),
        Some("0 0 1 1")
    );
    let href = document.select("use").nodes()[0]
        .attrs()
        .into_iter()
        .find(|attribute| {
            attribute.name.local.as_ref() == "href"
                && attribute.name.ns.as_ref() == "http://www.w3.org/1999/xlink"
        })
        .map(|attribute| attribute.value.to_string());
    assert_eq!(href.as_deref(), Some("#shape"));
}

#[test]
fn retained_media_keeps_markdown_links_literal() {
    let literal = "first\n\n[click](javascript:alert(1))\n\t*literal*";
    for tag in ["audio", "video"] {
        assert_retained_literal_text(
            &format!("<{tag} controls><span>{}</span></{tag}>", escape(literal)),
            &format!("{tag} span"),
            literal,
        );
    }
}

#[test]
fn retained_unusual_lists_keep_markdown_links_literal() {
    let literal = "first\n\n[click](javascript:alert(1))\n\t*literal*";
    assert_retained_literal_text(
        &format!("<ol start=\"-2\"><li>{}</li></ol>", escape(literal)),
        "ol li",
        literal,
    );
}

#[test]
fn retained_tables_keep_blank_lines_and_parsed_pre_code_source() {
    let source = include_str!("data/markdown/retained-pre-breaks.html");
    let original = Document::from(source);
    let output = markdown(source);
    let document = assert_safe_markdown(&output);
    assert_eq!(document.select("table").length(), 1, "{output}");
    assert_eq!(document.select("a").length(), 0, "{output}");
    assert_eq!(document.select("pre").length(), 2, "{output}");
    for (expected, actual) in original
        .select("pre")
        .nodes()
        .iter()
        .zip(document.select("pre").nodes())
    {
        assert_eq!(actual.text().as_ref(), expected.text().as_ref(), "{output}");
    }
    assert_eq!(
        document.select("pre code").text(),
        original.select("pre code").text()
    );
    assert_eq!(
        document.select("td:last-child").text(),
        original.select("td:last-child").text()
    );
}

#[test]
fn block_math_keeps_following_markdown_structure_and_literal_prose() {
    for attributes in [
        "data-math=\"a+b\"",
        "data-tex=\"a+b\"",
        "data-latex=\"a+b\"",
        "class=\"math\"",
        "class=\"MathJax_Display\"",
    ] {
        let source = format!(
            "<article><p>Before <em>start</em>.</p><div><div {attributes}></div>Then *literal* star and [bracket] text with <em>emphasis</em> follows here.</div><h2>After heading</h2><ul><li>First item</li><li>Second <strong>item</strong></li></ul></article>"
        );
        let mut outputs = vec![markdown(&source)];
        for (debug, standardize) in [(false, true), (true, true), (false, false)] {
            outputs.push(
                extract(
                    &source,
                    None,
                    &ExtractOptions {
                        markdown: true,
                        debug,
                        standardize,
                        content_selector: Some("article".into()),
                        ..Default::default()
                    },
                )
                .unwrap()
                .content_markdown
                .unwrap(),
            );
        }
        for output in outputs {
            let document = assert_safe_markdown(&output);
            assert_eq!(
                document.select("h2").text().as_ref(),
                "After heading",
                "{output}"
            );
            assert_eq!(document.select("ul > li").length(), 2, "{output}");
            assert_eq!(document.select("em").length(), 2, "{output}");
            assert_eq!(document.select("strong").length(), 1, "{output}");
            assert!(
                document
                    .text()
                    .contains("Then *literal* star and [bracket] text"),
                "{output}"
            );
            assert!(!document.text().contains("\\*"), "{output}");
            assert!(!document.text().contains("\\["), "{output}");
        }
    }
}

fn assert_literal_probe_and_structure(output: &str) -> Document {
    let document = assert_safe_markdown(output);
    assert!(document.select("u[data-probe]").is_empty(), "{output}");
    assert!(
        document.text().contains("<u data-probe>marker</u>"),
        "{output}"
    );
    assert!(
        document.select("em").text().contains("emphasis"),
        "{output}"
    );
    document
}

fn public_markdown_modes(source: &str) -> Vec<String> {
    let mut output = vec![markdown(source)];
    for (debug, standardize) in [(false, true), (true, true), (false, false)] {
        output.push(
            extract(
                source,
                Some("https://example.com/article"),
                &ExtractOptions {
                    markdown: true,
                    debug,
                    standardize,
                    ..Default::default()
                },
            )
            .unwrap()
            .content_markdown
            .unwrap(),
        );
    }
    output
}

#[test]
fn retained_inline_math_image_before_hard_break_stays_inline_and_literal() {
    let source = include_str!("data/markdown/retained-math-image-break.html");
    for output in public_markdown_modes(source) {
        let document = assert_literal_probe_and_structure(&output);
        assert_eq!(document.select("img[data-tex]").length(), 1, "{output}");
        let image = document.select("img[data-tex]").nodes()[0];
        assert_eq!(image.attr("data-tex").as_deref(), Some("x^2"));
        assert_eq!(
            image.attr("src").as_deref(),
            Some("https://example.com/formula.png")
        );
        let paragraph = image
            .ancestors_it(None)
            .find(|node| node.has_name("p"))
            .unwrap_or_else(|| panic!("math image left its inline paragraph: {output}"));
        assert!(
            paragraph.descendants_it().any(|node| node.has_name("em")),
            "{output}"
        );
        assert!(
            paragraph.descendants_it().any(|node| node.has_name("br")),
            "{output}"
        );
    }
}

#[test]
fn commonmark_block_math_tags_do_not_consume_following_markdown() {
    // CommonMark HTML-block type 6 plus the four raw-text type 1 starts. Unsafe
    // and structurally invalid HTML roots may be removed/repositioned by the
    // HTML parser; every public conversion must still preserve following prose.
    let tags = [
        "address",
        "article",
        "aside",
        "base",
        "basefont",
        "blockquote",
        "body",
        "caption",
        "center",
        "col",
        "colgroup",
        "dd",
        "details",
        "dialog",
        "dir",
        "div",
        "dl",
        "dt",
        "fieldset",
        "figcaption",
        "figure",
        "footer",
        "form",
        "frame",
        "frameset",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hr",
        "html",
        "iframe",
        "legend",
        "li",
        "link",
        "main",
        "menu",
        "menuitem",
        "nav",
        "noframes",
        "ol",
        "optgroup",
        "option",
        "p",
        "param",
        "search",
        "section",
        "summary",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "title",
        "tr",
        "track",
        "ul",
        "pre",
        "script",
        "style",
        "textarea",
    ];
    for tag in tags {
        let source = format!(
            "<article><p>Before line<br><{tag} data-math=\"x\"></{tag}> after &lt;u data-probe&gt;marker&lt;/u&gt; <em>emphasis</em>.</p><h2>Following heading</h2><ul><li>First item</li><li>Second item</li></ul></article>"
        );
        let output = markdown(&source);
        let document = assert_literal_probe_and_structure(&output);
        assert_eq!(
            document.select("h2").text().as_ref(),
            "Following heading",
            "{tag}: {output}"
        );
        assert_eq!(document.select("ul > li").length(), 2, "{tag}: {output}");
    }
}

#[test]
fn retained_block_math_boundary_fixture_is_literal_in_every_public_mode() {
    let source = include_str!("data/markdown/retained-math-block-boundaries.html");
    for output in public_markdown_modes(source) {
        let document = assert_literal_probe_and_structure(&output);
        assert_eq!(document.select("em").length(), 4, "{output}");
        assert_eq!(
            document.select("h2").text().as_ref(),
            "Following heading",
            "{output}"
        );
        assert_eq!(document.select("ul > li").length(), 2, "{output}");
    }
}

#[test]
fn text_that_resembles_html_block_openers_cannot_activate_markup() {
    for literal in [
        "<!--@x-->",
        "<?p@q?>",
        "<![CDATA[@x]]>",
        "<u/data-probe=@x>",
        "<u@x data-probe>",
    ] {
        let source = format!(
            "<p>{} after &lt;u data-probe&gt;marker&lt;/u&gt; <em>emphasis</em>.</p>",
            escape(literal)
        );
        let document = assert_literal_probe_and_structure(&markdown(&source));
        assert_eq!(document.select("u").length(), 0, "{literal}");
    }
}

#[test]
fn valid_http_and_email_autolinks_keep_their_destinations() {
    let source = "<p>&lt;https://example.com&gt; &lt;http://example.org/?x=1&amp;y=2&gt; &lt;reader@example.com&gt; &lt;u/data-probe=1@sample.example&gt; &lt;https://example.com/?literal=&amp;copy;&gt;</p>";
    let document = rendered(source);
    let links: Vec<_> = document
        .select("a")
        .nodes()
        .iter()
        .map(|node| node.attr("href").unwrap().to_string())
        .collect();
    assert_eq!(
        links,
        [
            "https://example.com",
            "http://example.org/?x=1&y=2",
            "mailto:reader@example.com",
            "mailto:u/data-probe=1@sample.example",
            "https://example.com/?literal=&copy;"
        ]
    );
    assert!(document.select("u").is_empty());
}

#[test]
fn mixed_math_holder_with_inline_voids_keeps_its_source_and_prose() {
    let source = "<article><p>Before <span data-math=\"x^2\"><img src=\"https://example.com/formula.png\" alt=\"x\"><br></span> after &lt;u data-probe&gt;marker&lt;/u&gt; <em>emphasis</em>.</p></article>";
    for output in public_markdown_modes(source) {
        let document = assert_literal_probe_and_structure(&output);
        assert_eq!(
            document
                .select("span[data-math]")
                .attr("data-math")
                .as_deref(),
            Some("x^2"),
            "{output}"
        );
        assert_eq!(
            document.select("span[data-math] img").length(),
            1,
            "{output}"
        );
        assert_eq!(
            document.select("span[data-math] br").length(),
            1,
            "{output}"
        );
    }
}
