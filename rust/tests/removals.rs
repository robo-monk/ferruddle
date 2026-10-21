use defuddle::{Options, extract};
fn clean(body: &str) -> String {
    let html = format!(
        "<html><head><title>Article title</title></head><body><main><h1>Article title</h1>{body}</main></body></html>"
    );
    extract(
        &html,
        Some("https://example.org/articles/current"),
        &Options {
            content_selector: Some("main".into()),
            remove_exact_selectors: false,
            remove_partial_selectors: false,
            remove_hidden_elements: false,
            remove_low_scoring: false,
            remove_small_images: false,
            standardize: false,
            ..Options::default()
        },
    )
    .unwrap()
    .content
}
fn prose() -> String {
    "<p>This is a substantial article paragraph describing an ordinary subject with several complete sentences. It provides enough background and supporting discussion to establish that the text before the closing material is real article content. The author explains the evidence carefully, giving concrete examples that readers can evaluate and use in their own work.</p>".repeat(3)
}
#[test]
fn preserves_short_link_paragraph_at_article_end() {
    let html = clean(&(prose() + "<p><a href='/reference'>Reference documentation</a></p>"));
    assert!(html.contains("Reference documentation"));
}
#[test]
fn preserves_content_list_introduced_by_wrapped_heading() {
    let html = clean(
        &(prose()
            + "<div><h2>Useful options</h2></div><ul><li>One choice</li><li>Another choice</li></ul>"),
    );
    assert!(html.contains("One choice"));
    assert!(html.contains("Another choice"));
}
#[test]
fn removes_orphan_related_intro_and_preserves_next_section() {
    let html = clean(
        &(prose()
            + "<p>For more on this topic</p><h2>Next section</h2><p>This paragraph continues the article after an unrelated recommendation was removed. It has substantial prose and useful information for the reader to understand the topic in detail.</p>"),
    );
    assert!(!html.contains("For more on"));
    assert!(html.contains("continues the article"));
}
#[test]
fn removes_trailing_related_paragraph_cards() {
    let html = clean(
        &(prose()
            + "<section><p><a href='/first'>Another article to read</a> <a href='/tag'>Topic</a></p><p><a href='/second'>A different article</a> <a href='/tag'>Topic</a></p></section>"),
    );
    assert!(!html.contains("Another article to read"));
    assert!(!html.contains("A different article"));
}
#[test]
fn removes_contact_block_with_avatar_and_contacts() {
    let html = clean(
        &(prose()
            + "<section><div>Contact</div><p>One Person</p><p><a href='mailto:person@example.org'>person@example.org</a></p><p>555-123-4567</p></section>"),
    );
    assert!(!html.contains("person@example.org"));
    assert!(!html.contains("555-123-4567"));
}
#[test]
fn preserves_direct_related_heading_when_followed_by_real_prose() {
    let html = clean(
        &(prose()
            + "<h2>Further reading</h2><p>This paragraph develops the topic of further reading as part of the article itself. It describes several ways to evaluate and compare resources, providing a substantial explanation with enough context to be useful.</p>"),
    );
    assert!(html.contains("Further reading"));
    assert!(html.contains("develops the topic"));
}
#[test]
fn preserves_read_time_text_in_article_body() {
    let html = clean(&(prose() + "<p>3 min read</p>" + &prose()));
    assert!(html.contains("3 min read"));
}
#[test]
fn code_clutter_names_are_protected_and_output_stays_sanitized() {
    let html = "<article><h1>Article title</h1><p>Actual article content with enough clear information to identify the main prose.</p><pre><code><span class='sidebar subscribe'>let example = 1;</span><a href='javascript:evil()' onclick='evil()'>code link</a></code></pre></article>";
    let out = extract(html, Some("https://example.org/"), &Options::default())
        .unwrap()
        .content;
    assert!(out.contains("let example = 1;"));
    assert!(!out.contains("javascript:"));
    assert!(!out.contains("onclick="));
}

#[test]
fn removes_trailing_external_link_lists_but_preserves_internal_resources() {
    let tail = "<h2>Recommended resources</h2><ul><li><a href='https://other.example/a'>A resource</a></li><li><a href='https://other.example/b'>Another resource</a></li></ul>";
    let html = clean(&(prose() + tail));
    assert!(!html.contains("Recommended resources"));
    assert!(!html.contains("Another resource"));
    let html = clean(&(prose() + &tail.replace("https://other.example/b", "/internal")));
    assert!(html.contains("Recommended resources"));
    assert!(html.contains("Another resource"));
}
#[test]
fn removes_bare_social_counter_anchor() {
    let html = clean(&(prose() + "<a>13 Likes</a>"));
    assert!(!html.contains("13 Likes"));
}
#[test]
fn upstream_image_gallery_keeps_all_following_article_sections() {
    let case: serde_json::Value = serde_json::from_str(include_str!(
        "data/upstream/content-patterns--multi-image-row-midarticle.json"
    ))
    .unwrap();
    let result = extract(
        case["inputHtml"].as_str().unwrap(),
        case["url"].as_str(),
        &Options::default(),
    )
    .unwrap();
    for text in [
        "Methods",
        "Contemporary studies rely",
        "Results",
        "Reception",
        "Legacy",
        "The protocol has since been adopted",
    ] {
        assert!(result.content.contains(text), "lost {text}");
    }
    assert_eq!(
        result.word_count,
        case["expected"]["wordCount"].as_u64().unwrap() as usize
    );
}

#[test]
#[ignore]
fn inspect_removal_fixture() {
    let id = std::env::var("DEFUDDLE_INSPECT_CASE").unwrap();
    let bytes = std::fs::read(format!(
        "{}/tests/data/upstream/{id}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let case: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let out = extract(
        case["inputHtml"].as_str().unwrap(),
        case["url"].as_str(),
        &Options {
            debug: true,
            ..Options::default()
        },
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

#[test]
fn upstream_stripped_related_image_cards_are_removed_without_losing_prose() {
    let case: serde_json::Value = serde_json::from_str(include_str!(
        "data/upstream/related--inline-related-stories-block.json"
    ))
    .unwrap();
    let result = extract(
        case["inputHtml"].as_str().unwrap(),
        case["url"].as_str(),
        &Options::default(),
    )
    .unwrap();
    let actual = dom_query::Document::fragment(result.content);
    let expected = dom_query::Document::fragment(case["expected"]["content"].as_str().unwrap());
    assert_eq!(
        actual.select("img").length(),
        expected.select("img").length()
    );
    assert_eq!(
        result.word_count,
        case["expected"]["wordCount"].as_u64().unwrap() as usize
    );
}

#[test]
fn quoted_newsletter_invitation_is_article_content() {
    let html = clean(
        &(prose()
            + "<section><blockquote><p>Subscribe to our newsletter for regular updates, the original invitation said. This historical quotation is evidence discussed by the article.</p></blockquote></section>"
            + &prose()),
    );
    assert!(html.contains("historical quotation"));
}
