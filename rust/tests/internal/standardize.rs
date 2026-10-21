use crate::standardize;
// Structural tests run independently of content scoring.
use defuddle::{Metadata, Options, dom, safety};

use dom_query::{Document, NodeRef};

fn first<'a>(document: &'a Document, selector: &str) -> NodeRef<'a> {
    dom::select(document.root(), selector)
        .into_iter()
        .next()
        .expect(selector)
}

fn clean(source: &str, title: &str) -> Document {
    let document = Document::from(source);
    dom::preprocess(&document);
    let root = document.body().unwrap();
    let metadata = Metadata {
        title: title.to_owned(),
        ..Metadata::default()
    };
    standardize::before_removals(&document, root, &metadata);
    standardize::content(root, &metadata, &Options::default());
    safety::sanitize(root);
    document
}

#[test]
fn fixture_footnote_definitions_and_reference_counts_match_semantically() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/upstream");
    let mut failures = Vec::new();
    let mut checked = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if !name.starts_with("footnotes--")
            && ![
                "issues--120-dhammatalks-footnotes.json",
                "issues--218-footnote-wrapper-text-lost.json",
                "issues--280-texinfo-footnotes.json",
                "issues--296-oreilly-noteref-footnotes.json",
                "issues--footnote-id-with-quote.json",
            ]
            .contains(&name.as_ref())
        {
            continue;
        }
        let case: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let expected = Document::from(case["expected"]["content"].as_str().unwrap());
        let document = clean(case["inputHtml"].as_str().unwrap(), "");
        let notes = dom::select(document.root(), "li[id^=\"fn:\"]").len();
        let expected_notes = dom::select(expected.root(), "li[id^=\"fn:\"]").len();
        let refs = dom::select(document.root(), "sup[id^=\"fnref:\"]").len();
        let expected_refs = dom::select(expected.root(), "sup[id^=\"fnref:\"]").len();
        // This direct standardizer retains the full source body. The complete
        // reference extraction can intentionally omit later paragraphs and
        // their citations; retain every reference it expected, without making
        // standardization discard additional legitimate article references.
        if notes != expected_notes || refs < expected_refs {
            failures.push(format!(
                "{name}: definitions {notes}/{expected_notes}, refs {refs}/{expected_refs}"
            ));
        }
        checked += 1;
    }
    assert!(checked >= 28);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn duplicate_references_have_unique_ids_and_preserve_surrounding_words() {
    let document = clean(
        r##"<article><p>Before<span class=wrap>word.<span><sup><a href="#a">1</a></sup></span></span>After.
      Again<a href="#a">1</a>.</p><section class=footnotes><ol><li id=a>Note with <em>emphasis</em>.</li></ol></section></article>"##,
        "",
    );
    let refs = dom::select(document.root(), "sup[id^=\"fnref:\"]");
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].id_attr().as_deref(), Some("fnref:1"));
    assert_eq!(refs[1].id_attr().as_deref(), Some("fnref:1-2"));
    assert_eq!(dom::select(document.root(), "a.footnote-backref").len(), 2);
    let text = document.text();
    assert!(text.contains("word.") && text.contains("After.") && text.contains("emphasis"));
}

#[test]
fn hidden_css_notes_and_external_definitions_survive_before_removal() {
    let document = Document::from(
        r#"<article><p>First<sup>1</sup> and second<sup>2</sup>.</p></article>
      <div><section class=page-footnotes><h3>Footnotes</h3><p><sup>1</sup>One.</p><p><sup>2</sup>Two.</p></section></div>"#,
    );
    let article = first(&document, "article");
    standardize::before_removals(&document, article, &Metadata::default());
    assert_eq!(dom::select(article, "#footnotes li").len(), 2);
    assert_eq!(dom::select(article, "sup[id^=\"fnref:\"]").len(), 2);
    assert!(article.text().contains("One.") && article.text().contains("Two."));
}

#[test]
fn callout_types_titles_folds_and_poisoned_text_are_preserved_safely() {
    let document = clean(
        r#"<article><div class="markdown-alert markdown-alert-note"><p class=markdown-alert-title>Note</p><p>GitHub body.</p></div>
      <div class="admonition warning"><p class=admonition-title>&lt;img src=x onerror=bad()&gt;</p><div class=admonition-content><p>Warning body.</p></div></div>
      <div class="alert alert-danger"><h4 class=alert-heading>Danger</h4><p>Bootstrap body.</p></div>
      <div class="callout is-collapsed" data-callout=tip><div class=callout-title><div class=callout-title-inner>Tip</div><div class=callout-fold>Fold</div></div>
      <div class=callout-content hidden style="display:none"><p>Previously hidden.</p></div></div></article>"#,
        "",
    );
    assert_eq!(
        dom::select(document.root(), ".callout[data-callout]").len(),
        4
    );
    assert_eq!(
        first(&document, "[data-callout=tip]")
            .attr_or("data-callout-fold", "")
            .as_ref(),
        "-"
    );
    assert!(document.text().contains("Previously hidden."));
    assert!(document.text().contains("<img src=x onerror=bad()>"));
    assert!(dom::select(document.root(), "img").is_empty());
    assert!(!document.html().contains("is-collapsed") && !document.html().contains("display:none"));
}

#[test]
fn heading_cleanup_roles_and_custom_wrappers_keep_structure() {
    let document = clean(
        r#"<article><h1>Page title<a href=#page>¶</a></h1><custom-body><div><span data-as=p>First <strong>paragraph</strong>.</span>
      <span class=block>Second paragraph.</span><div role=list><div role=listitem><div class=label>1)</div><div class=content><div role=paragraph>List item.</div></div></div></div>
      <h2><a href=#section>Section</a></h2><p>Section content.</p><h2>Empty tail</h2></div></custom-body></article>"#,
        "Page title",
    );
    assert!(
        dom::select(
            document.root(),
            "h1,custom-body,[role=list],[role=listitem]"
        )
        .is_empty()
    );
    assert!(
        document.text().contains("First paragraph.")
            && document.text().contains("Second paragraph.")
    );
    assert_eq!(dom::select(document.root(), "ol > li > p").len(), 1);
    assert_eq!(first(&document, "h2").text().as_ref(), "Section");
    assert!(!document.text().contains("Empty tail") && !document.text().contains("Page title"));
}

#[test]
fn inline_wrappers_with_block_children_do_not_create_nested_paragraphs() {
    let document = clean(
        r#"<article><div><span class=content-wrapper><p>First paragraph.</p><ul><li>A list item.</li></ul><p>Last paragraph.</p></span></div></article>"#,
        "",
    );
    assert!(
        dom::select(document.root(), "p p,p ul,p ol,p div").is_empty(),
        "{}",
        document.html()
    );
    assert_eq!(dom::select(document.root(), "p").len(), 2);
    assert_eq!(dom::select(document.root(), "ul > li").len(), 1);
    assert!(
        document.text().contains("First paragraph.") && document.text().contains("Last paragraph.")
    );
}

#[test]
fn footnote_backlinks_use_meaningful_paragraphs_before_continuation_lists() {
    let document = clean(
        r##"<article><p>Claim<sup><a href="#a">1</a></sup>.</p>
      <section class=footnotes><ol><li id=a><p>Note content.</p><p></p><ul><li>Supporting detail.</li></ul></li></ol></section></article>"##,
        "",
    );
    let note = first(&document, "li[id^=\"fn:\"]");
    assert_eq!(dom::select(note, "p").len(), 1, "{}", note.html());
    assert_eq!(dom::select(note, "p a.footnote-backref").len(), 1);
    assert!(
        first(&document, "#footnotes p")
            .text()
            .contains("Note content.")
    );
    assert!(note.text().contains("Supporting detail."));
}

#[test]
fn layout_table_unwrapping_preserves_data_tables_and_nested_content() {
    let document = clean(
        r#"<article><table><tr><td><p>Layout paragraph.</p></td></tr><tr><td><p>Second paragraph.</p></td></tr></table>
      <table><thead><tr><th>Name</th><th>Value</th></tr></thead><tbody><tr><td>A</td><td>1</td></tr></tbody></table>
      <table><tr><td><table><tr><th>Nested</th></tr><tr><td>Data</td></tr></table></td></tr></table></article>"#,
        "",
    );
    assert_eq!(dom::select(document.root(), "table").len(), 2);
    assert!(
        document.text().contains("Layout paragraph.")
            && document.text().contains("Second paragraph.")
    );
    assert_eq!(dom::select(document.root(), "th").len(), 3);
}

#[test]
fn meaningful_captions_survive_layout_and_empty_table_cleanup() {
    let document = clean(
        r#"<article><table><caption>Layout introduction.</caption><tr><td><p>Actual article content.</p></td></tr></table>
      <table><caption>Information from an otherwise empty table.</caption><tr><td></td></tr></table>
      <table><caption>Data measurements.</caption><tr><th>Name</th><th>Value</th></tr><tr><td>A</td><td>1</td></tr></table></article>"#,
        "",
    );
    assert_eq!(dom::select(document.root(), "table").len(), 1);
    assert_eq!(
        first(&document, "table caption").text().as_ref(),
        "Data measurements."
    );
    assert!(
        document.text().contains("Layout introduction.")
            && document.text().contains("otherwise empty table")
    );
    let html = document.html();
    assert!(
        html.find("Layout introduction.").unwrap() < html.find("Actual article content.").unwrap()
    );
}

#[test]
fn reversed_lists_item_values_and_table_alignment_survive_attribute_cleanup() {
    let source = r#"<article><ol reversed start=7><li>Seven.</li><li value=4>Four.</li><li>Three.</li></ol>
      <table><tr><th style="text-align: left">Left</th><th style="text-align: center !important">Center</th><th align=right>Right</th></tr>
      <tr><td style="color:red;text-align:RIGHT">A</td><td style="text-align: bogus">B</td><td>C</td></tr></table></article>"#;
    let document = clean(source, "");
    let list = first(&document, "ol");
    assert!(list.has_attr("reversed"));
    assert_eq!(list.attr_or("start", "").as_ref(), "7");
    assert_eq!(
        first(&document, "li[value]").attr_or("value", "").as_ref(),
        "4"
    );
    let headers = dom::select(document.root(), "th");
    assert_eq!(
        headers
            .iter()
            .map(|node| node.attr_or("align", "").to_string())
            .collect::<Vec<_>>(),
        ["left", "center", "right"]
    );
    assert_eq!(
        first(&document, "td").attr_or("align", "").as_ref(),
        "right"
    );
    assert!(!document.html().contains("style="));

    let extractor_document = Document::from(source);
    standardize::extractor(extractor_document.body().unwrap(), &Options::default());
    assert!(first(&extractor_document, "ol").has_attr("reversed"));
    assert_eq!(
        first(&extractor_document, "li[value]")
            .attr_or("value", "")
            .as_ref(),
        "4"
    );
    assert_eq!(
        first(&extractor_document, "th")
            .attr_or("align", "")
            .as_ref(),
        "left"
    );
}

#[test]
fn whitespace_preserves_tight_inline_code_math_and_footnote_attachment() {
    let document = clean(
        r#"<article><p>H<sub>2</sub>O and 10<sup>n</sup>. <strong> bold </strong>word. Code <code>x &lt; y</code>.</p>
      <p>Claim<sup>1</sup> follows.</p><p class=footnote><sup>1</sup>Definition.</p><pre><code>  a
    b</code></pre><math><mi>x</mi><mo>+</mo><mi>y</mi></math></article>"#,
        "",
    );
    let html = document.html();
    assert!(html.contains("H<sub>2</sub>O") && html.contains("10<sup>n</sup>"));
    assert!(html.contains("Claim<sup id=\"fnref:1\""));
    assert_eq!(first(&document, "pre code").text().as_ref(), "  a\n    b");
    assert_eq!(first(&document, "math").text().as_ref(), "x+y");
}

#[test]
fn extractor_cleanup_keeps_authored_classes_and_drops_empty_spacers() {
    let document = Document::from(
        r#"<article data-defuddle><div class="reddit post page-style" style=bad><p class="post-text site-font">Body.</p>
      <div><br></div><div class="comment comment-author random">Author.</div></div></article>"#,
    );
    let root = first(&document, "article");
    standardize::extractor(root, &Options::default());
    safety::sanitize(root);
    assert!(root.has_attr("data-defuddle"));
    assert_eq!(
        first(&document, ".reddit").class().as_deref(),
        Some("reddit post")
    );
    assert_eq!(
        first(&document, ".comment").class().as_deref(),
        Some("comment comment-author")
    );
    assert!(
        !root.html().contains("page-style")
            && !root.html().contains("site-font")
            && !root.html().contains("<br")
    );
}

#[test]
fn lazy_noscript_gallery_and_following_paragraphs_survive_extraction() {
    let case: serde_json::Value = serde_json::from_str(include_str!(
        "../data/upstream/issues--227-noscript-lazy-images.json"
    ))
    .unwrap();
    for debug in [false, true] {
        let result = defuddle::extract(
            case["inputHtml"].as_str().unwrap(),
            case["url"].as_str(),
            &Options {
                debug,
                ..Options::default()
            },
        )
        .unwrap();
        assert!(
            result.content.contains("gallery-1.jpg") && result.content.contains("gallery-2.jpg"),
            "debug={debug}: {}",
            result.content
        );
        assert!(result.content.contains("Neque porro quisquam"));
    }
}

#[test]
fn svg_palette_functions_and_utilities_survive_without_page_styles() {
    let document = clean(
        r#"<article><p>Diagram.</p><svg viewBox="0 0 200 100">
      <circle fill="var(--background-card)" stroke="var(--color-amber-600)"/>
      <path stroke="light-dark(var(--color-slate-400), var(--color-slate-300))"/>
      <line class="stroke-zinc-400"/><rect class="fill-orange-500/50"/>
      <text class="fill-amber-500 text-[14px] font-semibold">Label</text>
      <ellipse fill="var(--unknown, var(--color-green-600))"/></svg></article>"#,
        "",
    );
    assert_eq!(
        first(&document, "circle").attr_or("fill", "").as_ref(),
        "Canvas"
    );
    assert_eq!(
        first(&document, "circle").attr_or("stroke", "").as_ref(),
        "#d97706"
    );
    assert_eq!(
        first(&document, "path").attr_or("stroke", "").as_ref(),
        "#94a3b8"
    );
    assert_eq!(
        first(&document, "line").attr_or("stroke", "").as_ref(),
        "#a1a1aa"
    );
    assert_eq!(
        first(&document, "rect").attr_or("fill", "").as_ref(),
        "rgba(249,115,22,0.5)"
    );
    assert_eq!(
        first(&document, "text").attr_or("style", "").as_ref(),
        "font-size:14px;font-weight:600"
    );
    assert_eq!(
        first(&document, "ellipse").attr_or("fill", "").as_ref(),
        "#16a34a"
    );
    assert!(!document.html().contains("var(") && !document.html().contains("light-dark("));
}

#[test]
fn citation_groups_drop_presentation_brackets_and_keep_section_headings() {
    let document = clean(
        r##"<article><p>Many<cite class=ltx_cite>[<a href="#bib.bib2">2</a>,<a href="#bib.bib1">1</a>]</cite>.
      Single<cite class=ltx_cite>[<a href="#bib.bib1">1</a>]</cite>.</p>
      <section><h2>References</h2><ul class=ltx_biblist><li id=bib.bib1>First.</li><li id=bib.bib2>Second.</li></ul></section></article>"##,
        "",
    );
    let paragraph = first(&document, "p");
    assert!(!paragraph.text().contains(['[', ']']));
    assert!(paragraph.html().contains("</sup> <sup"));
    assert_eq!(dom::select(document.root(), "sup[id^=\"fnref:\"]").len(), 3);
    assert_eq!(first(&document, "h2").text().as_ref(), "References");
}

#[test]
fn google_definition_labels_and_numeric_note_prefixes_are_removed() {
    let google = clean(
        r##"<article><p>Claim<sup><a href="#ftnt1">1</a></sup>.</p><h3>Footnotes</h3>
      <div><p id=ftnt1><a href="#ftnt_ref1">[1]</a>Definition.</p></div></article>"##,
        "",
    );
    assert!(dom::select(google.root(), "h3").is_empty());
    let generic = clean(
        r##"<article><p>A<a href="#n1">1</a> and B<a href="#n2">2</a>.</p>
      <div class=notes><p id=n1>1. First definition.</p><p id=n2>2. Second definition.</p></div></article>"##,
        "",
    );
    let notes = first(&generic, "#footnotes").text();
    assert!(!notes.contains("1.") && !notes.contains("2."));
    assert!(notes.contains("First definition.") && notes.contains("Second definition."));
}

#[test]
fn inline_boundary_spaces_do_not_separate_punctuation() {
    let document = clean(
        r#"<p>See <a href=/paper>the paper </a>. Compare <em>the book </em>, too.</p>"#,
        "",
    );
    assert_eq!(
        first(&document, "p").text().as_ref(),
        "See the paper. Compare the book, too."
    );
}
