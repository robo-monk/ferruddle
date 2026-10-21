use dom_query::{Document, NodeRef};
use ferruddle::{WarningCode, dom, math, safety};

fn first<'a>(document: &'a Document, selector: &str) -> NodeRef<'a> {
    dom::select(document.root(), selector)
        .into_iter()
        .next()
        .expect(selector)
}

fn sanitized(html: &str) -> String {
    let document = Document::from(html);
    dom::preprocess(&document);
    safety::sanitize(document.root());
    document.body().unwrap().inner_html().to_string()
}

fn assert_inert(document: &Document) {
    for node in dom::subtree(document.root()) {
        assert!(
            !safety::is_unsafe_element(node),
            "unsafe element: {}",
            node.html()
        );
        for attribute in node.attrs() {
            let name = attribute.name.local.to_ascii_lowercase();
            let name = name.rsplit(':').next().unwrap();
            assert!(
                !name.starts_with("on") && name != "srcdoc",
                "{}",
                node.html()
            );
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
fn url_schemes_are_normalized_without_rejecting_normal_urls() {
    for value in [
        "javascript:alert(1)",
        " JaVa\tsCrIpT:\nalert(1)",
        "blob:https://example.test/id",
        "data:text/html,<script>x</script>",
        "data:\u{2003}text/html,x",
        "vbscript:msgbox(1)",
    ] {
        assert!(safety::is_dangerous_url(value, true), "{value}");
    }
    for value in [
        "https://example.test/image.png",
        "/images/a.png",
        "#footnote",
        "page.html",
        "mailto:author@example.test",
        "data:image/png;base64,AA==",
    ] {
        assert!(!safety::is_dangerous_url(value, true), "{value}");
    }
    assert!(safety::is_dangerous_url("data:image/svg+xml,<svg/>", false));
    assert!(safety::is_dangerous_url(
        "data:image/png;base64,AA==",
        false
    ));
}

#[test]
fn sanitizes_root_descendants_foreign_attributes_and_all_url_contexts() {
    let document = Document::from(
        r#"<article onclick="root()"><p onfocus="run()">Keep text.</p>
      <a href="java&#9;script:run()">A</a><form action="blob:https://example.test/id">
      <button formaction="data:text/html,bad">B</button></form>
      <iframe src="data:image/svg+xml,&lt;svg onload='bad()'&gt;" srcdoc="bad" onload="bad()"></iframe>
      <iframe src="https://example.test/embed" sandbox="allow-scripts"></iframe>
      <img src="data:image/png;base64,AA==" srcset="/good.png 1x, javascript:bad() 2x" onerror="bad()">
      <svg viewBox="0 0 20 20" onload="bad()"><a xlink:href="javascript:bad()"><text>SVG text</text></a>
      <image href="blob:https://example.test/id"/><image xlink:href="/safe.svg"/>
      <style>@import url(https://example.test/leak)</style><animateTransform/><set to="javascript:bad()"/>
      <script type="math/tex">bad()</script></svg><math onclick="bad()"><mi>x</mi></math></article>"#,
    );
    let article = first(&document, "article");
    assert!(!safety::sanitize(article));
    assert_inert(&document);
    let html = article.html();
    assert!(html.contains("Keep text.") && html.contains("SVG text"));
    assert!(
        html.contains("https://example.test/embed") && html.contains("sandbox=\"allow-scripts\"")
    );
    assert!(html.contains("data:image/png;base64,AA=="));
    assert!(!html.contains("javascript:") && !html.contains("srcdoc") && !html.contains("<script"));
    assert_eq!(
        first(&document, "img").attr_or("srcset", "").as_ref(),
        "/good.png 1x"
    );
    assert_eq!(
        first(&document, "svg").qual_name_ref().unwrap().ns.as_ref(),
        "http://www.w3.org/2000/svg"
    );
    assert_eq!(
        first(&document, "math")
            .qual_name_ref()
            .unwrap()
            .ns
            .as_ref(),
        "http://www.w3.org/1998/Math/MathML"
    );
}

#[test]
fn unsafe_selected_roots_are_neutralized_including_their_inert_contents() {
    for root in [
        r#"<template id="payload"><template><img src="x" onerror="bad()"></template><script>bad()</script></template>"#,
        r#"<script id="payload" src="https://example.test/bad.js">bad()</script>"#,
        r#"<style id="payload">body{display:none}</style>"#,
        r#"<object id="payload" data="javascript:bad()"><img onerror="bad()"></object>"#,
        r#"<embed id="payload" src="https://example.test/bad.html">"#,
        r#"<base id="payload" href="https://example.test/">"#,
    ] {
        let document = Document::from(root);
        let selected = first(&document, "#payload");
        assert!(safety::sanitize(selected), "{root}");
        assert!(selected.attrs().is_empty() && selected.children().is_empty());
        if let Some(content) = dom::template_content(selected) {
            assert!(content.children().is_empty());
        }
        assert!(!selected.html().contains("bad") && !selected.html().contains("example.test"));
    }
    let document = Document::from(r#"<script type="Math/TeX">E = mc^2</script>"#);
    let selected = first(&document, "script");
    assert!(!safety::sanitize(selected));
    assert!(selected.html().contains("E = mc^2"));
}

#[test]
fn template_fragments_are_explicit_and_clones_are_independent() {
    let original = Document::from(
        "<article><template><p>Hidden</p><template><b>Nested</b></template></template><p>Visible</p></article>",
    );
    let template = first(&original, "template");
    assert_eq!(dom::select(original.root(), "p").len(), 1);
    assert!(template.children().is_empty());
    let content = dom::template_content(template).unwrap();
    assert_eq!(dom::select(content, "p")[0].text().as_ref(), "Hidden");
    assert_eq!(
        dom::subtree(template)
            .iter()
            .filter(|node| node.is_element())
            .count(),
        4
    );

    let cloned = original.clone();
    dom::template_content(first(&cloned, "template"))
        .unwrap()
        .remove_children();
    assert!(template.html().contains("Nested"));
    assert!(!dom::same_node(
        first(&original, "article"),
        first(&cloned, "article")
    ));

    let target = Document::from("<main></main>");
    let copied = dom::clone_into(template, &target.tree);
    first(&target, "main").append_child(&copied);
    assert_eq!(template.html(), copied.html());
    dom::template_content(copied).unwrap().remove_children();
    assert!(template.html().contains("Nested"));
    let same_tree_copy = dom::clone_into(template, template.tree);
    assert_eq!(template.html(), same_tree_copy.html());
    dom::template_content(same_tree_copy)
        .unwrap()
        .remove_children();
    assert!(template.html().contains("Nested"));
    safety::sanitize(target.root());
    assert!(dom::select(target.root(), "template").is_empty());
}

#[test]
fn scoped_child_queries_use_direct_iteration_and_selection_stays_ordered() {
    let document = Document::from("<article><img id=a><div><img id=b></div><img id=c></article>");
    let article = first(&document, "article");
    // dom_query currently accepts :scope syntactically but doesn't set the
    // scoped node in its matching context. Callers must use element_children.
    assert!(dom::select(article, ":scope > img").is_empty());
    let direct: Vec<_> = article
        .element_children()
        .into_iter()
        .filter(|node| node.has_name("img"))
        .map(|node| node.id_attr().unwrap().to_string())
        .collect();
    assert_eq!(direct, ["a", "c"]);
    let ids: Vec<_> = dom::select(article, "#c, #a, #b")
        .into_iter()
        .map(|node| node.id_attr().unwrap().to_string())
        .collect();
    assert_eq!(ids, ["a", "b", "c"]);
    assert!(dom::select(article, "[").is_empty());
}

#[test]
fn cached_selectors_do_not_capture_document_nodes_or_change_after_eviction() {
    let first_document = Document::from("<article><p class=shared>First.</p></article>");
    let second_document = Document::from("<section><p class=shared>Second.</p></section>");
    assert_eq!(
        dom::select(first_document.root(), ".shared")[0]
            .text()
            .as_ref(),
        "First."
    );
    assert_eq!(
        dom::select(second_document.root(), ".shared")[0]
            .text()
            .as_ref(),
        "Second."
    );
    for index in 0..640 {
        assert!(dom::select(first_document.root(), &format!(".absent-{index}")).is_empty());
    }
    assert_eq!(
        dom::select(first_document.root(), ".shared")[0]
            .text()
            .as_ref(),
        "First."
    );
    assert!(dom::closest(dom::select(second_document.root(), ".shared")[0], "article").is_none());
    let threads: Vec<_> = (0..3)
        .map(|index| {
            std::thread::spawn(move || {
                let document = Document::from(format!("<p class=shared>Thread {index}.</p>"));
                for _ in 0..8 {
                    assert_eq!(
                        dom::select(document.root(), ".shared")[0].text().as_ref(),
                        format!("Thread {index}.")
                    );
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn ordering_and_tree_moves_keep_text_in_place() {
    let document =
        Document::from("<article>before<span>A<b>B</b>C</span>after<div><p>D</p></div></article>");
    let article = first(&document, "article");
    let span = first(&document, "span");
    let bold = first(&document, "b");
    let paragraph = first(&document, "p");
    assert!(dom::is_before(article, bold));
    assert!(dom::is_before(bold, paragraph));
    assert!(!dom::is_before(paragraph, bold));
    assert!(!dom::is_before(bold, bold));
    let detached = document.tree.new_element("p");
    assert!(!dom::is_before(detached, paragraph));
    dom::unwrap(span);
    assert_eq!(
        article.inner_html().as_ref(),
        "beforeA<b>B</b>Cafter<div><p>D</p></div>"
    );
    let div = first(&document, "div");
    dom::transfer_children(article, paragraph); // Refuse a cycle.
    assert!(article.html().contains("<p>D</p>"));
    let section = dom::create_element(&document, "section");
    article.append_child(&section);
    dom::transfer_children(div, section);
    assert!(div.children().is_empty());
    assert_eq!(section.inner_html().as_ref(), "<p>D</p>");
}

#[test]
fn setters_fragments_and_serialization_preserve_literal_poisoned_values() {
    let document = Document::from("<article></article>");
    let article = first(&document, "article");
    let image = dom::create_element(&document, "img");
    image.set_attr("src", "/safe.png");
    image.set_attr("alt", "\"><img src=x onerror=bad()>&'");
    article.append_child(&image);
    dom::append_text(article, "<script>bad()</script> & text");
    let added = dom::append_fragment(article, "<p>A &amp; B</p><svg><circle r=2></circle></svg>");
    assert_eq!(added.len(), 2);
    assert!(!safety::sanitize(article));
    let reparsed = Document::from(article.html());
    assert_inert(&reparsed);
    assert_eq!(dom::select(reparsed.root(), "img").len(), 1);
    assert!(reparsed.text().contains("<script>bad()</script> & text"));
    assert_eq!(
        first(&reparsed, "circle")
            .qual_name_ref()
            .unwrap()
            .ns
            .as_ref(),
        "http://www.w3.org/2000/svg"
    );
}

#[test]
fn declarative_shadow_contents_are_hoisted_in_exact_order_and_sanitized() {
    let html = sanitized(
        r#"<article><p>Before.</p><div>Light
      <template shadowrootmode="open"><p>First.</p><section><template shadowroot="closed"><p>Nested.</p></template></section>
        <p>Last.</p><img src=x onerror="bad()"><script>bad()</script></template>Tail
      <template shadowrootmode="bogus"><p>Must remain inert.</p></template>
      <template shadowrootmode="bogus" shadowroot="open"><p>Standard mode wins.</p></template>
      </div><p>After.</p></article>"#,
    );
    assert!(html.find("Before.").unwrap() < html.find("First.").unwrap());
    assert!(html.find("First.").unwrap() < html.find("Nested.").unwrap());
    assert!(html.find("Nested.").unwrap() < html.find("Last.").unwrap());
    assert!(html.find("Last.").unwrap() < html.find("After.").unwrap());
    assert!(!html.contains("template") && !html.contains("bad()"));
    assert!(!html.contains("Must remain inert") && !html.contains("Standard mode wins"));
    assert_inert(&Document::from(html));
}

#[test]
fn streams_replace_fallbacks_without_running_scripts_or_losing_article_order() {
    let html = sanitized(
        r#"<article><p>Before.</p><!--$?--><template id="B:0"></template><p>Loading.</p>
      <!--$?--><i>Nested fallback.</i><!--/$--><!--/$--><p>After.</p></article>
      <div hidden id="S:0"><h2>Streamed heading.</h2><p>Streamed text.</p><img src=x onerror="bad()"></div>
      <script>throw Error('must never execute');$RC("B:0", "S:0")</script>"#,
    );
    assert!(!html.contains("Loading") && !html.contains("Nested fallback"));
    assert!(html.find("Before.").unwrap() < html.find("Streamed heading.").unwrap());
    assert!(html.find("Streamed text.").unwrap() < html.find("After.").unwrap());
    assert!(!html.contains("hidden") && !html.contains("bad()") && !html.contains("<script"));
    assert_inert(&Document::from(html));
}

#[test]
fn truncated_stream_preserves_fallback_and_following_text() {
    let document = Document::from(
        r#"<article><template id="B:0"></template><p>Fallback.</p><p>Article tail.</p></article>
      <div hidden id="S:0"><p>Unfinished stream.</p></div><script>$RC("B:0","S:0")</script>"#,
    );
    dom::resolve_streamed_content(&document);
    assert!(document.html().contains("Fallback.") && document.html().contains("Article tail."));
    assert!(first(&document, "#S\\:0").has_attr("hidden"));
}

#[test]
fn noscript_images_restore_lazy_sources_without_promoting_tracking_pixels() {
    let html = sanitized(
        r#"<article><figure><img src="data:image/gif;base64,AA==" alt="An image">
      <noscript><img src="/real.png" alt="An image" srcset="/real-2x.png 2x" onerror="bad()"></noscript></figure>
      <div class="photo"><span class="lazy-placeholder"></span><noscript><img src="/promoted.png" alt="Photo"></noscript></div>
      <p>Text.</p><noscript><img src="/tracker.gif" width=1 height=1></noscript>
      <figure><noscript><img src="javascript:bad()" alt="Unsafe"></noscript></figure></article>"#,
    );
    assert!(html.contains("/real.png") && html.contains("/real-2x.png 2x"));
    assert!(html.contains("/promoted.png"));
    assert!(
        !html.contains("base64") && !html.contains("tracker.gif") && !html.contains("javascript:")
    );
    assert!(!html.contains("noscript") && !html.contains("onerror"));
    assert_eq!(dom::select(Document::from(html).root(), "img").len(), 2);

    let encoded = sanitized(
        "<figure><noscript>&lt;img src='/encoded.png' alt='Encoded'&gt;</noscript></figure>",
    );
    assert!(encoded.contains("/encoded.png"));
    let hydrated = sanitized(
        "<figure><img src=/hydrated.png><noscript><img src=/duplicate.png></noscript></figure>",
    );
    assert!(!hydrated.contains("duplicate.png"));
}

#[test]
fn mutation_payloads_remain_inert_after_serialization_and_reparse() {
    for payload in [
        r#"<svg><template><img src=x onerror=bad()></template></svg>"#,
        r#"<math><mtext><table><mglyph><style><!--</style><img title="--><img src=x onerror=bad()>">"#,
        r#"<svg><style><img src=x onerror=bad()></style><foreignObject><iframe srcdoc="bad"></iframe></foreignObject></svg>"#,
        r#"<math><annotation-xml encoding="text/html"><svg><script>bad()</script><a xlink:href="javascript:bad()">x</a></svg></annotation-xml></math>"#,
        r#"<template><template><iframe srcdoc="&lt;script&gt;bad()&lt;/script&gt;"></iframe></template></template>"#,
        r#"<svg><a><animate attributeName=href values="javascript:bad()"/><text>Keep.</text></a></svg>"#,
    ] {
        let once = sanitized(&format!(
            "<article><p>Before.</p>{payload}<p>After.</p></article>"
        ));
        let reparsed = Document::from(once.as_str());
        assert_inert(&reparsed);
        safety::sanitize(reparsed.root());
        let twice = reparsed.body().unwrap().inner_html();
        // Removing a malformed foreign-content style can cause the browser to
        // repair namespace nesting on reparse. The safety property is that the
        // repaired tree is still inert and the article text is preserved.
        assert_inert(&Document::from(twice));
        assert!(
            once.contains("Before.") && once.contains("After."),
            "{payload}"
        );
    }
}

#[test]
fn srcset_filter_retains_safe_candidates_and_url_commas() {
    let html = sanitized(
        r#"<article><img src="/small.png"
      srcset="/cdn/w_500,q_auto/photo.png 500w, javascript:bad() 1000w, /cdn/w_2000,q_auto/photo.png 2000w">
      <img srcset="data:image/png;base64,AAAA 1x, /image-2x.png 2x, blob:https://example.test/id 3x"></article>"#,
    );
    let document = Document::from(html);
    let images = dom::select(document.root(), "img");
    assert_eq!(
        images[0].attr_or("srcset", "").as_ref(),
        "/cdn/w_500,q_auto/photo.png 500w, /cdn/w_2000,q_auto/photo.png 2000w"
    );
    assert_eq!(
        images[1].attr_or("srcset", "").as_ref(),
        "data:image/png;base64,AAAA 1x, /image-2x.png 2x"
    );
    assert_eq!(
        dom::srcset_candidates("/one.png, /two.png 2x"),
        [
            ("/one.png".to_owned(), "".to_owned()),
            ("/two.png".to_owned(), "2x".to_owned())
        ]
    );
    assert_eq!(
        dom::srcset_candidates("data:image/png;base64,AAAA"),
        [("data:image/png;base64,AAAA".to_owned(), "".to_owned())]
    );
}

#[test]
fn math_detection_warns_once_without_conversion_or_currency_false_positives() {
    for source in [
        "<math><mi>x</mi></math>",
        "<mjx-container>x</mjx-container>",
        "<span class='MathJax'>x</span>",
        "<span class='katex'>x</span>",
        "<span class='math'>x2</span>",
        "<span class='math-inline'>x2</span>",
        "<div class='math-display'>x2</div>",
        "<span role='math'>x2</span>",
        "<script type='Math/TeX'>x^2</script>",
        "<div data-latex='x'></div>",
        r"<p>Inline \(x\) and display \[y\].</p>",
        r"<p>Sum $x_{i}+1$.</p>",
        "<template shadowrootmode=open><math><mi>x</mi></math></template>",
        "<script src=/mathjax.js></script><p>Let $x$ be a variable.</p>",
        "<img src='/render?tex=%5Cfrac%7Ba%7D%7Bb%7D'>",
        r"<img src='/equation.svg' alt='\operatorname{fn}(x) = \exp(x)' width=224 height=18>",
    ] {
        let document = Document::from(source);
        let before = document.html();
        let warnings = math::warnings(&document);
        assert_eq!(warnings.len(), 1, "{source}");
        assert_eq!(warnings[0].code, WarningCode::MathNotSupported);
        assert_eq!(warnings[0].message, "Math support is not yet included.");
        assert_eq!(
            before,
            document.html(),
            "detection must not transform input"
        );
    }
    for source in [
        "<p>Cost $100 today and $200 tomorrow.</p>",
        "<p>The plan costs $10 per user_id and $20 per team.</p>",
        "<p>Set the template to ${user_name} and ${team_name} in the config.</p>",
        "<p>Shell variables like $HOME_DIR and $PATH_LIST are common.</p>",
        r"<p>Match it with the regex \[a-z\]+ in your editor.</p>",
        r"<p>Use the Windows path C:\Program Files\(x86\) here.</p>",
        r"<pre>Example $x_{i}$ and \(y\).</pre>",
        r"<code>Example $x^{2}$</code>",
        "<p>Mathematical prose.</p>",
        "<script src=/mathjax.js></script><p>Price $100 or $200.</p>",
    ] {
        assert!(
            math::warnings(&Document::from(source)).is_empty(),
            "{source}"
        );
    }
}
