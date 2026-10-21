//! Adversarial checks written independently of the renderer and quality comparator.
#[path = "support/quality.rs"]
#[allow(dead_code)]
mod quality;
use defuddle::html_to_markdown;
use dom_query::Document;
use pulldown_cmark::{Options, Parser, html};
fn markdown(source: &str) -> String {
    html_to_markdown(source, None).unwrap().markdown
}
fn rendered(source: &str) -> Document {
    let markdown = markdown(source);
    let mut html = String::new();
    html::push_html(
        &mut html,
        Parser::new_ext(
            &markdown,
            Options::ENABLE_TABLES
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_FOOTNOTES
                | Options::ENABLE_STRIKETHROUGH,
        ),
    );
    Document::from(html)
}

#[test]
fn paragraph_leading_whitespace_must_not_create_markdown_blocks_or_drop_text() {
    for source in [
        "<p> # ordinary prose</p>",
        "<p> - ordinary prose</p>",
        "<p> &gt; ordinary prose</p>",
        "<p> 1. ordinary prose</p>",
    ] {
        let document = rendered(source);
        assert_eq!(
            document.select("p").length(),
            1,
            "{source}: {}",
            markdown(source)
        );
        assert!(document.text().contains("ordinary prose"), "{source}");
        assert!(
            document.select("h1,ul,ol,blockquote").is_empty(),
            "{source}"
        );
    }
}
#[test]
fn pre_keeps_code_siblings_and_surrounding_payload() {
    let document =
        rendered("<pre>prefix <code>first</code> middle <code>second</code> suffix</pre>");
    let payload = document.select("pre code").text();
    assert!(payload.contains("prefix"));
    assert!(payload.contains("first"));
    assert!(payload.contains("middle"));
    assert!(payload.contains("second"));
    assert!(payload.contains("suffix"));
}
#[test]
fn comparator_detects_code_outside_the_first_code_child() {
    assert_ne!(
        quality::html_semantics("<pre><code>first</code><code>second</code></pre>"),
        quality::html_semantics("<pre><code>first</code></pre>")
    );
}
#[test]
fn reversed_lists_and_explicit_item_values_keep_their_numbering() {
    let document = rendered(
        "<ol reversed start='5'><li>First</li><li value='2'>Second</li><li>Third</li></ol>",
    );
    assert!(document.select("ol").attr("reversed").is_some());
    assert_eq!(
        document.select("ol > li[value]").attr("value").as_deref(),
        Some("2")
    );
}
#[test]
fn table_caption_and_alignment_survive_markdown_rendering() {
    let document = rendered(
        "<table><caption>Important caption</caption><tr><th style='text-align:right'>Value</th><th>Meaning</th></tr><tr><td style='text-align:right'>1</td><td>One</td></tr></table>",
    );
    assert!(document.text().contains("Important caption"));
    let header = document.select("th");
    assert!(
        header.attr("align").is_some_and(|a| a.as_ref() == "right")
            || header
                .attr("style")
                .is_some_and(|a| a.contains("text-align: right"))
    );
}
#[test]
fn comparator_detects_responsive_image_source_loss() {
    assert_ne!(
        quality::html_semantics("<img src='/fallback.png' srcset='/small.png 1x, /large.png 2x'>"),
        quality::html_semantics("<img src='/fallback.png'>")
    );
}
#[test]
fn comparator_detects_svg_preserved_whitespace_loss() {
    assert_ne!(
        quality::html_semantics("<svg><text xml:space='preserve'>A  B</text></svg>"),
        quality::html_semantics("<svg><text xml:space='preserve'>A B</text></svg>")
    );
}
#[test]
fn literal_html_like_email_text_cannot_create_executable_attributes() {
    for literal in [
        "&lt;svg/onload=alert(1)//@x&gt;",
        "&lt;img/src=x/onerror=alert(1)//@x&gt;",
        "&lt;iframe/srcdoc=evil()//@x&gt;",
    ] {
        let doc = rendered(&format!("<p>{literal}</p>"));
        assert!(
            doc.select("[onload],[onerror],[srcdoc],svg,img,iframe")
                .is_empty(),
            "{literal}"
        );
    }
}
#[test]
fn comparator_footnote_definition_order_cannot_swap_reference_targets() {
    assert_ne!(
        quality::markdown_semantics("A[^x] B[^y]\n\n[^x]: First\n\n[^y]: Second"),
        quality::markdown_semantics("A[^x] B[^y]\n\n[^y]: First\n\n[^x]: Second")
    );
}

#[test]
fn split_text_nodes_do_not_create_ordered_list_markers() {
    let document = rendered("<p><span>1</span>. ordinary prose</p>");
    assert!(document.select("ol").is_empty());
    assert_eq!(document.select("p").text().as_ref(), "1. ordinary prose");
}
#[test]
fn nested_task_checkbox_does_not_change_parent_item_state() {
    let document =
        rendered("<ul><li>Parent<ul><li><input type='checkbox' checked>Child</li></ul></li></ul>");
    let parent = document.select("body > ul > li").nodes()[0];
    let owned_checkboxes = document
        .select("input[type=checkbox]")
        .nodes()
        .iter()
        .filter(|input| {
            input
                .ancestors_it(None)
                .find(|n| n.is("li"))
                .is_some_and(|owner| owner.id == parent.id)
        })
        .count();
    assert_eq!(
        owned_checkboxes, 0,
        "nested task state leaked into parent item"
    );
    assert_eq!(document.select("input[checked]").length(), 1);
}
#[test]
fn rich_table_cells_preserve_code_and_list_structure() {
    let document = rendered(
        "<table><tr><th>Code</th><th>Instructions</th></tr><tr><td><pre><code>a\nb</code></pre></td><td><ul><li>First</li><li>Second</li></ul></td></tr></table>",
    );
    assert_eq!(document.select("td pre code").text().as_ref(), "a\nb");
    assert_eq!(document.select("td ul > li").length(), 2);
}
#[test]
fn linked_figure_keeps_destination_and_non_caption_text() {
    let document = rendered(
        "<figure><a href='/full-image.jpg'><img src='/thumb.jpg' alt='Image'></a><span>License details</span><figcaption>A caption.</figcaption></figure>",
    );
    assert_eq!(document.select("a[href='/full-image.jpg'] img").length(), 1);
    assert!(document.text().contains("License details"));
}
#[test]
fn comparator_detects_raw_footnote_target_swaps() {
    let first = "<p><sup><a href='#fn:1'>1</a></sup></p><ol><li id='fn:1'>First</li><li id='fn:2'>Second</li></ol>";
    let second = "<p><sup><a href='#fn:1'>1</a></sup></p><ol><li id='fn:2'>First</li><li id='fn:1'>Second</li></ol>";
    assert_ne!(
        quality::html_semantics(first),
        quality::html_semantics(second)
    );
}

#[test]
fn retained_rich_table_keeps_nested_list_and_checkbox_attributes() {
    let document = rendered(
        "<table><tr><th>Task</th><th>Priority</th></tr><tr><td><ol reversed start='5'><li value='2'><input type='checkbox' checked>Ready</li></ol></td><td>High</td></tr></table>",
    );
    assert!(document.select("td ol[reversed][start='5']").length() == 1);
    assert_eq!(document.select("td li[value='2']").length(), 1);
    assert_eq!(
        document.select("td input[type=checkbox][checked]").length(),
        1
    );
}

#[test]
fn bare_block_split_text_does_not_turn_prose_into_a_list() {
    let document = rendered("<div><span>1</span>. ordinary prose</div>");
    assert!(document.select("ol").is_empty());
    assert!(document.text().contains("1. ordinary prose"));
}

#[test]
fn image_attribute_quotes_and_markdown_delimiters_remain_inert() {
    let source = "<p><img src='https://example.test/a.png' alt='close ](https://evil.test) &quot; onerror=&quot;evil()' title='title &quot; onerror=&quot;evil()'></p>";
    let document = rendered(source);
    assert_eq!(document.select("img").length(), 1);
    assert!(document.select("[onerror],a").is_empty());
    assert_eq!(
        document.select("img").attr("src").as_deref(),
        Some("https://example.test/a.png")
    );
    assert_eq!(
        document.select("img").attr("alt").as_deref(),
        Some("close ](https://evil.test) \" onerror=\"evil()")
    );
}

#[test]
fn adjacent_code_spans_keep_their_visible_text_without_extra_backticks() {
    let document = rendered("<p><code>first</code><code>second</code></p>");
    assert_eq!(document.select("p").text().as_ref(), "firstsecond");
    assert_eq!(document.select("p code").text().as_ref(), "firstsecond");
}

#[test]
fn adjacent_emphasis_delimiters_do_not_leak_into_visible_text() {
    for tag in ["strong", "em", "del"] {
        let source = format!("<p><{tag}>first</{tag}><{tag}>second</{tag}></p>");
        let document = rendered(&source);
        assert_eq!(
            document.select("p").text().as_ref(),
            "firstsecond",
            "{source}: {}",
            markdown(&source)
        );
        assert_eq!(document.select(tag).text().as_ref(), "firstsecond");
    }
}
