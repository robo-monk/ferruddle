use defuddle::metadata;
use dom_query::Document;

fn extract(html: &str) -> defuddle::Metadata {
    metadata::extract(
        &Document::from(html),
        Some("https://www.example.org/articles/current"),
    )
}

#[test]
fn metadata_precedence_and_brand_title_cleanup() {
    let m = extract(
        r#"<html lang="en_US"><head><title>Browser title</title>
      <meta property="og:site_name" content="Example Docs"><meta property="og:title" content="A useful guide - API | Example">
      <meta name="description" content="Preferred description"><meta property="og:description" content="Other description">
      <meta property="og:image" content="/cover.png"><meta name="twitter:image" content="/other.png">
      </head><h1>Heading</h1>"#,
    );
    assert_eq!(m.title, "A useful guide");
    assert_eq!(m.site, "Example Docs");
    assert_eq!(m.domain, "example.org");
    assert_eq!(m.favicon, "https://www.example.org/favicon.ico");
    assert_eq!(m.image, "/cover.png");
    assert_eq!(m.language, "en-US");
    assert_eq!(m.description, "Preferred description");
}

#[test]
fn rejects_placeholders_and_brand_titles() {
    let m = extract(
        r##"<title>Actual article title | Brand</title><meta property="og:title" content="Brand">
      <meta property="og:site_name" content="Brand"><meta name="author" content="#author.fullName}">
      <meta name="description" content=". ."><meta property="og:description" content="A usable description">
      <h1>Actual article title</h1><a rel="author">Person Name</a>"##,
    );
    assert_eq!(m.title, "Actual article title");
    assert_eq!(m.description, "A usable description");
    assert_eq!(m.author, "Person Name");
}

#[test]
fn schema_arrays_graphs_and_safe_entity_decoding() {
    let m = extract(
        r#"<script type="application/ld+json">{ "@graph": [
      {"@type":"Article", "headline":"A &amp; B", "author":[{"name":"One Person"},{"name":"Two People"},{"name":"One Person"}],
       "publisher":{"name":"Test &amp; Site"}, "image":{"url":"/schema.png"}, "datePublished":"2026-01-02",
       "text":"&lt;img src=x onerror=evil()&gt; &amp; safe", "nested":["&quot;quoted&quot;"]}
      ] }</script><script type="application/ld+json">[ {"description":"Schema description"} ]</script>"#,
    );
    assert_eq!(m.title, "A & B");
    assert_eq!(m.author, "One Person, Two People");
    assert_eq!(m.site, "Test & Site");
    assert_eq!(m.image, "/schema.png");
    assert_eq!(m.published, "2026-01-02");
    assert_eq!(m.description, "Schema description");
    assert_eq!(
        m.schema_org_data[0]["text"],
        "<img src=x onerror=evil()> & safe"
    );
    assert_eq!(m.schema_org_data[0]["nested"][0], "\"quoted\"");
}

#[test]
fn poisoned_schema_strings_remain_data_and_invalid_json_is_ignored() {
    let m = extract(
        r#"<script type="application/ld+json">{invalid}</script>
      <script type="application/ld+json">{"headline":"&lt;/textarea&gt;&lt;script&gt;evil()&lt;/script&gt;", "author":{"name":"Person &amp; Name"}}</script>"#,
    );
    assert_eq!(m.title, "</textarea><script>evil()</script>");
    assert_eq!(m.author, "Person & Name");
    assert_eq!(m.schema_org_data.as_array().unwrap().len(), 1);
}

#[test]
fn author_cards_and_citation_names() {
    let m = extract(
        r#"<div class="author"><script>hidden</script><a rel="author">One Person</a><p>Biography of One Person</p></div>"#,
    );
    assert_eq!(m.author, "One Person");
    let m = extract(
        r#"<meta name="citation_author" content="Family, Given"><meta name="citation_author" content="Other Person">"#,
    );
    assert_eq!(m.author, "Given Family, Other Person");
    let m = extract(
        r#"<meta name="author" content="By One Person (https://example.org/a) and Two People —">"#,
    );
    assert_eq!(m.author, "One Person, Two People");
}

#[test]
fn dates_skip_related_cards_but_keep_external_links_and_permalink() {
    let m = extract(
        r#"<a href="/other"><time datetime="2020-01-01">January 1, 2020</time></a>
      <a href="https://external.example/thread"><time datetime="2026-05-04">May 4, 2026</time></a>"#,
    );
    assert_eq!(m.published, "2026-05-04");
    let m = extract(r#"<a href="/articles/current/#date"><time datetime="2026-05-04"></time></a>"#);
    assert_eq!(m.published, "2026-05-04");
}

#[test]
fn heading_adjacent_author_and_dates() {
    let m = extract(
        "<div><p>26 February 2025</p></div><h1>Article title</h1><div><p>By Person Name</p></div>",
    );
    assert_eq!(m.author, "Person Name");
    assert_eq!(m.published, "2025-02-26T00:00:00+00:00");
    let m = extract("<h1>Article title</h1><div><p>Person Name</p><p>June 5, 2023</p></div>");
    assert_eq!(m.author, "Person Name");
    assert_eq!(m.published, "2023-06-05T00:00:00+00:00");
}

#[test]
fn canonical_url_fallback_and_language_precedence() {
    let doc = Document::from(
        r#"<link rel="canonical" href="https://www.news.example/story"><meta http-equiv="Content-Language" content="pt_BR">"#,
    );
    let m = metadata::extract(&doc, Some("about:blank"));
    assert_eq!(m.domain, "news.example");
    assert_eq!(m.favicon, "https://www.news.example/favicon.ico");
    assert_eq!(m.language, "pt-BR");
}

/// Opt-in parity report against the independently generated upstream bundle.
/// DEFUDDLE_METADATA_CORPUS=/path/to/oracle cargo test --test metadata metadata_corpus -- --ignored --nocapture
#[test]
#[ignore]
fn metadata_corpus() {
    let path = std::env::var("DEFUDDLE_METADATA_CORPUS").expect("set DEFUDDLE_METADATA_CORPUS");
    let mut paths: Vec<_> = std::fs::read_dir(path)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json") && p.file_stem().unwrap() != "manifest"
        })
        .collect();
    paths.sort();
    let mut matched = [0; 4];
    let mut all = 0;
    for path in &paths {
        let case: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let doc = Document::from(case["inputHtml"].as_str().unwrap());
        let m = metadata::extract(&doc, case["url"].as_str());
        let values = [&m.title, &m.author, &m.site, &m.published];
        let mut clean = true;
        for (i, field) in ["title", "author", "site", "published"].iter().enumerate() {
            if case["expected"][field].as_str().unwrap_or_default() == values[i] {
                matched[i] += 1;
            } else {
                clean = false;
                println!(
                    "{} {field}: actual={:?} expected={}",
                    case["id"], values[i], case["expected"][field]
                );
            }
        }
        all += usize::from(clean);
    }
    println!(
        "{} cases: title={} author={} site={} published={} all={all}",
        paths.len(),
        matched[0],
        matched[1],
        matched[2],
        matched[3]
    );
    assert!(!paths.is_empty());
}

#[test]
fn schema_wrappers_and_nested_exact_path_precedence() {
    let m = extract(
        r#"<script type="application/ld+json"><![CDATA[
      {"nested":{"headline":"Nested fallback", "datePublished":"2020-01-01"}, "headline":"Exact headline",
       "datePublished":"2026-02-03", "author":{"name":"One Person"}}
      ]]></script><script type="application/ld+json">
      // A comment before valid JSON
      {"publisher":{"name":"Example Site"}} /* comment */
      </script>"#,
    );
    assert_eq!(m.title, "Exact headline");
    assert_eq!(m.site, "Example Site");
    assert_eq!(m.published, "2026-02-03");
    assert_eq!(m.author, "One Person");
}

#[test]
fn ambiguous_author_matches_and_category_lists_do_not_become_authors() {
    let m = extract(
        r#"<h1>Article title</h1><div><time>June 5, 2023</time><a>Category One</a><a>Category Two</a></div>
      <div class="author">Reviewer One</div><div class="author">Reviewer Two</div><div class="author">Reviewer Three</div><div class="author">Reviewer Four</div>"#,
    );
    assert_eq!(m.author, "");
    assert_eq!(m.published, "June 5, 2023");
}

#[test]
fn metadata_extraction_does_not_mutate_input() {
    let doc = Document::from(
        r#"<article><h1>Title</h1><div class="author"><script>ignored()</script><span>Person Name</span><p>Writer role</p></div></article>"#,
    );
    let before = doc.html().to_string();
    let m = metadata::extract(&doc, None);
    assert_eq!(m.author, "Person Name");
    assert_eq!(before, doc.html().to_string());
}
