use crate::{code, images};
// Direct standardizer tests keep failures independent of main-content scoring.
use defuddle::safety;
use dom_query::Document;

fn standardize(source: &str) -> Document {
    let document = Document::from(source);
    code::standardize(document.root());
    images::standardize(document.root());
    safety::sanitize(document.root());
    document
}

#[test]
fn strips_gutters_and_ui_without_changing_code_tokens() {
    let document = standardize(
        "<div class=\"highlight language-cpp\"><table><tr><td class=\"gutter\"><pre>1\n2</pre></td><td class=\"code\"><pre><code><div class=\"code-header\">C++<button>Copy</button></div><span class=\"line\"><span class=\"lineno\">1</span>auto f = [](int x) {</span><br><span class=\"line\">    return x;</span><br><span class=\"line\">};</span></code></pre></td></tr></table></div>",
    );
    assert_eq!(document.select("pre").length(), 1);
    assert_eq!(
        document.select("pre code").text().as_ref(),
        "auto f = [](int x) {\n    return x;\n};"
    );
    assert_eq!(
        document.select("code").attr("data-lang").as_deref(),
        Some("cpp")
    );
    assert!(document.select("table,button,span").is_empty());
}

#[test]
fn preserves_blank_lines_tabs_and_authored_code_indentation() {
    let document =
        standardize("<pre><code class=\"language-rust\">  first\n\n\n\tsecond  </code></pre>");
    assert_eq!(
        document.select("code").text().as_ref(),
        "  first\n\n\n    second  "
    );
}

#[test]
fn structured_gutters_and_inline_spans_do_not_double_linebreaks() {
    let document = standardize(
        "<pre><code data-lang=\"python\"><span style=\"display:flex\"><span>1</span><span><b>x</b> = 1\n</span></span><span style=\"display:flex\"><span>2</span><span>\n</span></span><span style=\"display:flex\"><span>3</span><span>print(x)</span></span></code></pre>",
    );
    assert_eq!(document.select("code").text().as_ref(), "x = 1\n\nprint(x)");
}

#[test]
fn code_mirror_keeps_only_the_editor_and_its_language() {
    let document = standardize(
        "<pre><div><div>Python<button>Run code</button></div><div class=\"cm-content\"><span>def f():</span><br><span>    return 1</span><br><br><span>f()</span></div></div></pre>",
    );
    assert_eq!(
        document.select("code").attr("data-lang").as_deref(),
        Some("python")
    );
    assert_eq!(
        document.select("code").text().as_ref(),
        "def f():\n    return 1\n\nf()"
    );
}

#[test]
fn verso_blocks_merge_without_hover_tooltip_duplication() {
    let document = standardize(
        "<article><code class=\"hl lean block\">def x : <a href=\"#nat\">Nat</a><span class=\"hover-info\">duplicate type</span> := 1</code>\n<code class=\"hl lean block\">def y := 2</code><p>Explanation</p><code class=\"hl lean block\">def z := 3</code></article>",
    );
    assert_eq!(document.select("pre").length(), 2);
    assert_eq!(
        document.select("pre").nodes()[0].text().as_ref(),
        "def x : Nat := 1\ndef y := 2"
    );
    assert!(document.select("p").text().contains("Explanation"));
}

#[test]
fn invalid_inline_code_wrappers_do_not_surround_preformatted_blocks() {
    let document = standardize(
        "<article><code class=\"highlight-wrap language-typescript\"><pre><code>const x = 1;\nconsole.log(x);</code></pre></code></article>",
    );
    assert!(document.select("code pre").is_empty());
    assert_eq!(
        document.select("pre code").attr("data-lang").as_deref(),
        Some("typescript")
    );
    assert_eq!(
        document.select("pre code").text().as_ref(),
        "const x = 1;\nconsole.log(x);"
    );
}

#[test]
fn source_and_labels_remain_inert_after_node_construction() {
    let document = standardize(
        "<pre><code data-lang=\"js&amp;quot;&gt;&lt;img src=x onerror=alert(1)&gt;\">&lt;img src=x onerror=alert(1)&gt;\n&lt;script&gt;alert(1)&lt;/script&gt;</code></pre>",
    );
    assert!(document.select("img,script").is_empty());
    assert!(
        document
            .select("code")
            .text()
            .contains("<img src=x onerror=alert(1)>")
    );
}

#[test]
fn explicit_lazy_sources_take_precedence_over_metadata() {
    let document = standardize(
        "<article><img src=\"https://example.com/blank.png\" data-src=\"https://example.com/full.jpg\" data-original=\"https://example.com/other.jpg\" resource=\"https://example.com/wiki/File:metadata.png\" alt=\"Example\"><img src=\"https://example.com/working.jpg\" data-src=\"javascript:alert(1)\" data-lazy-src=\"https://example.com/fallback.jpg\"></article>",
    );
    assert_eq!(
        document.select("img").nodes()[0].attr("src").as_deref(),
        Some("https://example.com/full.jpg")
    );
    assert_eq!(
        document.select("img").nodes()[1].attr("src").as_deref(),
        Some("https://example.com/fallback.jpg")
    );
    assert!(
        document
            .select("[data-src],[data-original],[data-lazy-src]")
            .is_empty()
    );
}

#[test]
fn lazy_srcsets_keep_safe_candidates_and_cdn_commas() {
    let document = standardize(
        "<img src=\"data:image/gif;base64,AAAA\" data-srcset=\"https://cdn.example.com/w_600,c_limit/p.png 600w, javascript:alert(1) 2000w, https://cdn.example.com/w_1200,c_limit/p.png 1200w\">",
    );
    let source_set = document.select("img").attr("srcset").unwrap();
    assert!(source_set.contains("w_600,c_limit/p.png 600w"));
    assert!(source_set.contains("w_1200,c_limit/p.png 1200w"));
    assert!(!source_set.contains("javascript:"));
    assert_eq!(
        document.select("img").attr("src").as_deref(),
        Some("https://cdn.example.com/w_600,c_limit/p.png")
    );
}

#[test]
fn pictures_use_default_sources_and_can_create_missing_images() {
    let document = standardize(
        "<picture><source media=\"(min-width: 1000px)\" srcset=\"/wide.png 2000w\"><source srcset=\"/default.png 1000w\"><img src=\"data:image/gif;base64,AAAA\" alt=\"Photo\"></picture><picture><source srcset=\"/only.png 1200w\"></picture>",
    );
    assert_eq!(document.select("source").length(), 0);
    assert_eq!(document.select("img").length(), 2);
    assert_eq!(
        document.select("img").nodes()[0].attr("src").as_deref(),
        Some("/default.png")
    );
    assert_eq!(
        document.select("img").nodes()[1].attr("src").as_deref(),
        Some("/only.png")
    );
}

#[test]
fn a_loader_does_not_replace_a_loaded_image() {
    let document = standardize(
        "<img src=\"https://example.com/large.png\" data-image-loader=\"https://example.com/small.png\"><img src=\"data:image/svg+xml,%3Csvg/%3E\" data-image-loader=\"https://example.com/real.png\">",
    );
    assert_eq!(
        document.select("img").nodes()[0].attr("src").as_deref(),
        Some("https://example.com/large.png")
    );
    assert_eq!(
        document.select("img").nodes()[1].attr("src").as_deref(),
        Some("https://example.com/real.png")
    );
}

#[test]
fn removes_tiny_placeholders_but_preserves_safe_svg_and_real_images() {
    let document = standardize(
        "<img src=\"data:image/gif;base64,AAAA\"><img src=\"data:image/svg+xml,%3Csvg/%3E\"><img src=\"https://example.com/image.png\">",
    );
    assert_eq!(document.select("img").length(), 2);
    assert!(
        document.select("img").nodes()[0]
            .attr("src")
            .unwrap()
            .starts_with("data:image/svg+xml")
    );
}

#[test]
fn meaningful_captions_become_figcaptions_without_losing_links_or_wrapper_prose() {
    let document = standardize(
        "<span><img src=\"/image.png\"><em>A caption with <a href=\"/source\">attribution</a>.</em></span><figure><p>Before image</p><img src=\"/other.png\"><p>After image</p></figure>",
    );
    assert_eq!(document.select("figcaption").length(), 1);
    assert_eq!(
        document.select("figcaption a").attr("href").as_deref(),
        Some("/source")
    );
    assert!(document.select("p").text().contains("Before image"));
    assert!(document.select("p").text().contains("After image"));
}

/// Inspect immutable extraction oracles without coupling highlighter tests to scoring.
#[test]
#[ignore]
fn code_fixture_comparison() {
    let directory =
        std::env::var("DEFUDDLE_MARKDOWN_ORACLE").expect("set DEFUDDLE_MARKDOWN_ORACLE");
    let mut checked = 0;
    let mut matching = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_none_or(|e| e != "json") {
            continue;
        }
        let case: serde_json::Value =
            serde_json::from_slice(&std::fs::read(entry.path()).unwrap()).unwrap();
        let id = case["id"].as_str().unwrap_or("");
        if !id.starts_with("codeblock") && !id.starts_with("code-block") {
            continue;
        }
        let document = standardize(case["inputHtml"].as_str().unwrap());
        let expected = Document::from(case["expected"]["content"].as_str().unwrap());
        let actual: Vec<_> = document
            .select("pre code")
            .nodes()
            .iter()
            .map(|n| n.text().trim().to_owned())
            .collect();
        let reference: Vec<_> = expected
            .select("pre code")
            .nodes()
            .iter()
            .map(|n| n.text().trim().to_owned())
            .collect();
        checked += reference.len();
        for text in reference {
            if actual.contains(&text) {
                matching += 1;
            } else {
                eprintln!("Mismatch {id}: {text:?}\nActual: {actual:?}");
            }
        }
    }
    eprintln!("Code oracle: {matching}/{checked} expected blocks matched.");
    assert!(checked > 0);
}

#[test]
fn image_only_spans_use_meaningful_alt_as_a_safe_caption() {
    let document = standardize(
        "<span><img src=\"/diagram.png\" alt=\"Architecture diagram.\"></span><span><img src=\"/literal.png\" alt=\"&lt;img src=x onerror=alert(1)&gt;\"></span>",
    );
    assert_eq!(document.select("figure").length(), 2);
    assert_eq!(
        document.select("figcaption").nodes()[0].text().as_ref(),
        "Architecture diagram."
    );
    assert_eq!(
        document.select("figcaption").nodes()[1].text().as_ref(),
        "<img src=x onerror=alert(1)>"
    );
    assert_eq!(document.select("img").length(), 2);
}

#[test]
fn github_line_tables_become_code_without_line_number_cells() {
    let document = standardize(
        "<table class=\"highlight js-file-line-container\"><tbody><tr><td class=\"blob-num\" data-line-number=\"12\"></td><td class=\"blob-code\">const x = 1;</td></tr><tr><td class=\"blob-num\">13</td><td class=\"blob-code\">  return x;</td></tr></tbody></table>",
    );
    assert!(document.select("table,td").is_empty());
    assert_eq!(
        document.select("pre code").text().as_ref(),
        "const x = 1;\n  return x;"
    );
}

#[test]
fn verso_merging_preserves_authored_section_gaps() {
    let document = standardize(
        "<div><code class=\"hl lean block\">def x := 1\n\n</code>\n<code class=\"hl lean block\">/- Next section. -/\n\n#check x</code><pre class=\"hl lean lean-output\">x : Nat</pre></div>",
    );
    assert_eq!(document.select("pre").length(), 1);
    assert_eq!(
        document.select("pre code").text().as_ref(),
        "def x := 1\n\n/- Next section. -/\n\n#check x\nx : Nat"
    );
}

#[test]
fn empty_verso_fragments_remain_blank_lines_when_merged() {
    let document = standardize(
        "<div><code class=\"hl lean block\">#check true</code><pre class=\"hl lean lean-output\">Bool.true : Bool</pre><code class=\"hl lean block\">\n</code><code class=\"hl lean block\">/- Evaluate -/\n\n#eval 5 * 4</code><pre class=\"hl lean lean-output\">20</pre></div>",
    );
    assert_eq!(
        document.select("pre code").text().as_ref(),
        "#check true\nBool.true : Bool\n\n/- Evaluate -/\n\n#eval 5 * 4\n20"
    );
}

#[test]
fn figures_use_alt_captions_only_when_no_caption_exists() {
    let document = standardize(
        "<figure><div class=\"image-wrapper\"><img src=\"/diagram.png\" alt=\"A useful architecture diagram.\"></div></figure><figure><img src=\"/other.png\" alt=\"Another architecture diagram.\"><figcaption>Existing caption.</figcaption></figure>",
    );
    assert_eq!(document.select("figcaption").length(), 2);
    assert_eq!(
        document.select("figcaption").nodes()[0].text().as_ref(),
        "A useful architecture diagram."
    );
    assert_eq!(
        document.select("figcaption").nodes()[1].text().as_ref(),
        "Existing caption."
    );
}
