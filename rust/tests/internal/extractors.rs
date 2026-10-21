use defuddle::{IncludeReplies, Options, extractors, metadata};
use dom_query::Document;
fn adapter_html(content: extractors::Content) -> String {
    let extractors::Content::Html(html) = content else {
        panic!("expected HTML adapter")
    };
    html
}
fn selector(content: &extractors::Content) -> Option<&str> {
    match content {
        extractors::Content::Selector(selector) => Some(selector),
        _ => None,
    }
}
fn run(html: &str, url: &str, options: &Options) -> Option<extractors::Extracted> {
    let mut doc = Document::from(html);
    let meta = metadata::extract(&doc, Some(url));
    extractors::extract(&mut doc, Some(url), options, &meta)
}
#[test]
fn gmail_local_dates_and_reply_control() {
    let html = r#"<title>Subject - account@example.org - Gmail</title><div class="adn ads"><span class="gD" name="One Person"></span><span class="g3" title="May 13, 2026, 11:02 PM"></span><div class="a3s"><p>First message.</p><div class="gmail_quote">Quoted duplicate</div><div>&gt; quoted plain text<br>Reply content<br><br></div></div></div><div class="adn ads"><span class="gD">Two People</span><div class="a3s">Second message.</div></div>"#;
    let result = run(
        html,
        "https://mail.google.com/mail/u/0/#inbox/thread",
        &Options::default(),
    )
    .unwrap();
    assert_eq!(result.metadata.title, "Subject");
    assert_eq!(result.metadata.author, "One Person");
    assert_eq!(result.metadata.published, "2026-05-13");
    let content = adapter_html(result.content);
    assert!(content.contains("Second message"));
    assert!(content.contains("Reply content"));
    assert!(!content.contains("Quoted duplicate"));
    assert!(!content.contains("quoted plain"));
    let result = run(
        html,
        "https://mail.google.com/mail/u/0/#inbox/thread",
        &Options {
            include_replies: IncludeReplies::None,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(!adapter_html(result.content).contains("Second message"));
    for (input, expected) in [
        ("Jan 5, 2026, 9:00 PM", "2026-01-05"),
        ("2026年5月13日 23:02", ""),
    ] {
        assert_eq!(
            run(
                &html.replace("May 13, 2026, 11:02 PM", input),
                "https://mail.google.com/mail/u/0/#inbox/thread",
                &Options::default()
            )
            .unwrap()
            .metadata
            .published,
            expected
        );
    }
}
#[test]
fn x_article_draft_code_and_poisoned_attributes() {
    let html = r#"<div data-testid="twitter-article-title">Article title</div><div data-testid="twitterArticleRichTextView"><div class="longform-unstyled"><span style="font-weight: bold">Strong</span> text <a href="javascript:evil()">link</a></div><div data-testid="markdown-code-block"><span>rust</span><pre><code>fn main() {}</code></pre></div></div>"#;
    let out = run(
        html,
        "https://x.com/person/article/12345",
        &Options::default(),
    )
    .unwrap();
    assert_eq!(out.extractor_type, "xarticle");
    assert_eq!(out.metadata.author, "@person");
    let html = adapter_html(out.content);
    assert!(html.contains("<p><strong>Strong</strong> text"));
    assert!(html.contains("language-rust"));
    let fragment = Document::fragment(html);
    defuddle::safety::sanitize(fragment.root());
    assert!(!fragment.html().contains("javascript:"));
}
#[test]
fn chatgpt_merges_fragments_and_deduplicates_citations() {
    let html = r#"<title>Conversation</title><div data-testid="conversation-turn-1"><h4 class="sr-only">Assistant:</h4><div data-message-author-role="assistant"><div class="markdown"><p>Before <span data-state="closed"><span><a href="https://example.org/source" target="_blank" rel="noopener">Source</a></span></span></p></div></div><button>Thought</button><div data-message-author-role="assistant"><div class="markdown"><p>After <span><a href="https://example.org/source" target="_blank" rel="noopener">Source</a></span></p></div></div></div>"#;
    let out = run(html, "https://chatgpt.com/share/id", &Options::default()).unwrap();
    assert_eq!(out.metadata.site, "ChatGPT");
    let html = adapter_html(out.content);
    assert!(html.contains("Before"));
    assert!(html.contains("After"));
    assert!(!html.contains("Thought"));
    assert_eq!(html.matches("<li id=\"fn:").count(), 1);
}
#[test]
fn substack_preloads_are_parsed_without_javascript_execution() {
    let post = serde_json::json!({"feedData":{"initialPost":{"post":{"body_html":"<p>Embedded <em>article</em></p>","title":"Embedded title","post_date":"2026-01-02","publishedBylines":[{"name":"One Person"}]}}}});
    let raw = serde_json::to_string(&post.to_string()).unwrap();
    let html = format!(
        "<body><script>window._preloads = JSON.parse({raw}); throw new Error('must not run')</script></body>"
    );
    let mut doc = Document::from(html);
    let meta = metadata::extract(&doc, Some("https://test.substack.com/p/article"));
    let out = extractors::extract(
        &mut doc,
        Some("https://test.substack.com/p/article"),
        &Options::default(),
        &meta,
    )
    .unwrap();
    assert_eq!(out.metadata.title, "Embedded title");
    assert_eq!(out.metadata.author, "One Person");
    assert_eq!(
        selector(&out.content),
        Some("[data-defuddle-substack-post]")
    );
    assert!(
        doc.select("[data-defuddle-substack-post]")
            .html()
            .contains("Embedded")
    );
}
#[test]
fn bbcode_escapes_source_and_attribute_parameters() {
    let event = serde_json::json!({"announcement_body":{"headline":"Patch title","body":"[p]A <script>evil()</script> [url=javascript:evil()]bad[/url] [url=\"https://example.org/?a=1&b=2\"]safe[/url][/p][img]https://example.org/x\" onerror=evil()[/img]","posttime":1736942400}});
    let html = format!(
        "<div id=\"application_config\" data-partnereventstore='{}'></div>",
        event
            .to_string()
            .replace('&', "&amp;")
            .replace('\'', "&#39;")
    );
    let out = run(&html, "https://store.example.org/post", &Options::default()).unwrap();
    assert_eq!(out.metadata.published, "2025-01-15T12:00:00.000Z");
    let html = adapter_html(out.content);
    let doc = Document::fragment(html.as_str());
    assert!(doc.select("script").is_empty());
    assert!(doc.select("[onerror]").is_empty());
    assert!(!html.contains("href=\"javascript:"));
    assert!(html.contains("&lt;script&gt;"));
}
#[test]
fn hn_listing_and_comments_use_safe_metadata() {
    let html = r#"<title>News | Hacker News</title><table><tr class="athing" id="1"><td><span class="titleline"><a href="javascript:evil()">One story</a></span></td></tr><tr><td class="subtext"><span class="score">4 points</span><a class="hnuser">One Person</a><a>2 comments</a></td></tr><tr class="athing" id="2"><td><span class="titleline"><a href="https://example.org">Two story</a></span></td></tr></table>"#;
    let out = run(
        html,
        "https://news.ycombinator.com/news",
        &Options::default(),
    )
    .unwrap();
    assert_eq!(out.metadata.title, "News");
    assert_eq!(out.metadata.site, "Hacker News");
    let html = adapter_html(out.content);
    assert!(html.contains("<ol>"));
    assert!(!html.contains("javascript:"));
    assert!(html.contains("2 comments"));
}
#[test]
#[ignore]
fn adapter_corpus() {
    let path = std::env::var("DEFUDDLE_METADATA_CORPUS").expect("set DEFUDDLE_METADATA_CORPUS");
    let mut paths: Vec<_> = std::fs::read_dir(path)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json") && p.file_stem().unwrap() != "manifest"
        })
        .collect();
    paths.sort();
    let (mut total, mut detected, mut matches) = (0, 0, 0);
    for path in paths {
        let case: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut doc = Document::from(case["inputHtml"].as_str().unwrap());
        let meta = metadata::extract(&doc, case["url"].as_str());
        let out = extractors::extract(&mut doc, case["url"].as_str(), &Options::default(), &meta);
        let expected = case["expected"]["extractorType"].as_str();
        let actual = out.as_ref().map(|o| o.extractor_type.as_str());
        if expected.is_some() {
            total += 1;
        }
        if actual.is_some() {
            detected += 1;
        }
        if expected != actual {
            println!(
                "{} type actual={actual:?} expected={expected:?}",
                case["id"]
            );
        }
        if let Some(out) = out {
            let fields = [
                ("title", out.metadata.title),
                ("author", out.metadata.author),
                ("site", out.metadata.site),
                ("published", out.metadata.published),
            ];
            let mut clean = true;
            for (key, val) in fields {
                if case["expected"][key].as_str().unwrap_or_default() != val {
                    println!(
                        "{} {key}: actual={val:?} expected={}",
                        case["id"], case["expected"][key]
                    );
                    clean = false;
                }
            }
            matches += usize::from(clean);
        }
    }
    println!("adapters expected={total} detected={detected} all metadata matches={matches}");
}

#[test]
fn twitter_keeps_self_thread_and_stops_at_trailing_discovery() {
    let tweet = |handle, text| {
        format!(
            r#"<div data-testid="cellInnerDiv"><article data-testid="tweet"><div data-testid="User-Name"><a href="/{handle}">Display name</a><a>@{handle}</a></div><div data-testid="tweetText">{text}</div></article></div>"#
        )
    };
    let html = format!(
        "<h2>Post</h2><section>{}{}{}</section><section><h2>Discover more</h2>{}</section>",
        tweet("person", "Main text"),
        tweet("person", "Self reply"),
        tweet("other", "Other reply"),
        tweet("promoted", "Discovery content")
    );
    let out = run(
        &html,
        "https://x.com/person/status/123",
        &Options::default(),
    )
    .unwrap();
    assert_eq!(out.metadata.author, "@person");
    let content = adapter_html(out.content);
    assert!(content.contains("Self reply"));
    assert!(content.contains("Other reply"));
    assert!(!content.contains("Discovery content"));
    let out = run(
        &html,
        "https://x.com/person/status/123",
        &Options {
            include_replies: IncludeReplies::None,
            ..Options::default()
        },
    )
    .unwrap();
    let html = adapter_html(out.content);
    assert!(html.contains("Self reply"));
    assert!(!html.contains("Other reply"));
}

#[test]
fn reddit_author_comes_from_post_and_reply_options_are_honored() {
    let html = r#"<h1>Post title</h1><shreddit-post author="Original Author"><div slot="text-body"><p>Original text.</p></div></shreddit-post><shreddit-comment author="Commenter" depth="0" score="7"><div slot="comment">Reply text.</div></shreddit-comment><span class="author">Logged in User</span>"#;
    for mode in [
        IncludeReplies::All,
        IncludeReplies::Extractors,
        IncludeReplies::None,
    ] {
        let out = run(
            html,
            "https://www.reddit.com/r/test/comments/123/title/",
            &Options {
                include_replies: mode,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(out.metadata.author, "Original Author");
        assert_eq!(out.metadata.site, "r/test");
        assert_eq!(
            adapter_html(out.content).contains("Reply text"),
            mode != IncludeReplies::None
        );
    }
}

#[test]
fn nyt_embedded_values_are_escaped_before_dom_injection() {
    let article = serde_json::json!({"initialData":{"data":{"article":{"headline":{"default":"Article title"},"bylines":[{"creators":[{"displayName":"One Person"}]}],"body":{"content":[{"__typename":"ParagraphBlock","content":[{"text":"<img src=x onerror=evil()>","formats":[{"__typename":"BoldFormat"},{"__typename":"LinkFormat","url":"https://example.org/\" onmouseover=\"evil()"}]}]},{"__typename":"ImageBlock","media":{"altText":"\" onerror=\"evil()","crops":[{"renditions":[{"name":"superJumbo","url":"https://example.org/image.jpg"}]}]}}]}}}}});
    let mut doc = Document::from(format!(
        "<script>window.__preloadedData = {article};</script>"
    ));
    let meta = metadata::extract(&doc, Some("https://www.nytimes.com/article"));
    let out = extractors::extract(
        &mut doc,
        Some("https://www.nytimes.com/article"),
        &Options::default(),
        &meta,
    )
    .unwrap();
    assert_eq!(out.metadata.title, "Article title");
    assert_eq!(out.metadata.author, "One Person");
    let body = doc.select("[data-defuddle-nyt]");
    assert!(body.select("[onerror],[onmouseover]").is_empty());
    assert_eq!(body.select("img").length(), 1);
    assert!(body.html().contains("&lt;img"));
}

#[test]
fn medium_selector_retains_article_images_and_removes_ui() {
    let mut doc = Document::from(
        r#"<meta property="og:site_name" content="Medium"><article class="meteredContent"><h1>Article title</h1><a data-testid="authorName">One Person</a><figure><div role="button"><img src="https://example.org/image.jpg"></div></figure><div data-testid="post-preview">Related content</div><p>Main article paragraph.</p><p>4 min read</p></article>"#,
    );
    let meta = metadata::extract(&doc, Some("https://medium.com/@person/article"));
    let out = extractors::extract(
        &mut doc,
        Some("https://medium.com/@person/article"),
        &Options::default(),
        &meta,
    )
    .unwrap();
    assert_eq!(selector(&out.content), Some("article"));
    assert_eq!(out.metadata.author, "One Person");
    assert_eq!(doc.select("article img").length(), 1);
    assert!(doc.select("article [role=button]").is_empty());
    assert!(!doc.select("article").text().contains("Related content"));
    assert!(!doc.select("article").text().contains("min read"));
}

#[test]
fn youtube_prefers_matching_schema_description_and_keeps_dom_transcript() {
    let comment = serde_json::json!({"@type":"VideoObject","@id":"https://www.youtube.com/watch?v=video123","name":"Comment title","comment":[]});
    let real = serde_json::json!({"@type":"VideoObject","@id":"https://www.youtube.com/watch?v=video123","name":"Real title","description":"<img src=x onerror=evil()> Description.","author":"Channel Name","uploadDate":"2026-01-02"});
    let html = format!(
        r#"<script type="application/ld+json">{comment}</script><script type="application/ld+json">{real}</script><ytd-engagement-panel-section-list-renderer target-id="engagement-panel-searchable-transcript"><div id="segments-container"><ytd-transcript-segment-renderer><div class="segment-timestamp">0:00</div><div class="segment-text">First sentence</div></ytd-transcript-segment-renderer><ytd-transcript-segment-renderer><div class="segment-timestamp">0:04</div><div class="segment-text">continues here.</div></ytd-transcript-segment-renderer></div></ytd-engagement-panel-section-list-renderer>"#
    );
    let out = run(
        &html,
        "https://www.youtube.com/watch?v=video123",
        &Options::default(),
    )
    .unwrap();
    assert_eq!(out.metadata.title, "Real title");
    assert_eq!(out.metadata.author, "Channel Name");
    assert_eq!(out.metadata.published, "2026-01-02");
    let html = adapter_html(out.content);
    assert!(html.contains("First sentence continues here."));
    let doc = Document::fragment(html);
    assert!(doc.select("img[onerror]").is_empty());
    assert!(
        doc.select("iframe")
            .attr("src")
            .unwrap()
            .contains("video123")
    );
}

#[test]
fn forum_and_rendered_social_adapters_build_safe_metadata() {
    let cases = [
        (
            "https://forum.example.org/t/title/123",
            r#"<meta name="generator" content="Discourse 3"><h1 data-topic-id="123">Topic title</h1><div class="topic-post topic-owner"><div class="names"><a data-user-card="One Person"></a></div><div class="cooked"><p>Post content</p></div></div>"#,
            "discourse",
        ),
        (
            "https://lwn.net/Articles/123/",
            r#"<div class="PageHeadline"><h1>Title</h1></div><div class="Byline">Posted May 13, 2026 by person</div><div class="ArticleText"><main><p>Post content</p></main></div>"#,
            "lwn",
        ),
        (
            "https://www.linkedin.com/posts/id",
            r#"<article role="article" class="feed-shared-update-v2"><div class="update-components-actor__title">One Person</div><div class="update-components-text update-components-update-v2__commentary">Post content</div></article>"#,
            "linkedin",
        ),
        (
            "https://bsky.app/profile/person.example/post/123",
            r#"<div data-testid="postThreadScreen"><div data-testid="postThreadItem-by-person.example"><a aria-label="One Person's avatar"></a><div data-word-wrap="1">Post content</div></div></div>"#,
            "bluesky",
        ),
        (
            "https://www.threads.com/@person/post/123",
            r#"<div role="region"><a href="/@person" role="link">person</a><span dir="auto">Post content</span><time datetime="2026-01-02T23:00:00-05:00"></time></div>"#,
            "threads",
        ),
        (
            "https://claude.ai/chat/123",
            r#"<div data-testid="user-message">Question</div><div class="font-claude-response"><div class="standard-markdown"><p>Post content</p></div></div>"#,
            "claude",
        ),
        (
            "https://gemini.google.com/app/123",
            r#"<div class="conversation-container"><user-query><div class="query-text">Question</div></user-query><model-response><div class="model-response-text"><div class="markdown"><p>Post content</p></div></div></model-response></div>"#,
            "gemini",
        ),
        (
            "https://grok.com/chat/123",
            r#"<div class="relative group flex flex-col justify-center w-full items-start"><div class="message-bubble"><p>Post content</p></div></div>"#,
            "grok",
        ),
    ];
    for (url, html, kind) in cases {
        let out = run(html, url, &Options::default()).unwrap_or_else(|| panic!("missing {kind}"));
        assert_eq!(out.extractor_type, kind);
        assert!(adapter_html(out.content).contains("Post content"), "{kind}");
    }
}

#[test]
fn threads_embedded_replies_are_plain_text_not_markup() {
    let data = serde_json::json!({"posts":[{"user":{"username":"person"},"text_fragments":{"fragments":[{"plaintext":"Main post"}]}},{"user":{"username":"other"},"text_fragments":{"fragments":[{"plaintext":"<img src=x onerror=evil()>"}]}}]});
    let html = format!(
        r#"<div role="region"><a href="/@person" role="link">person</a><span dir="auto">Main post</span></div><script type="application/json">{data}</script>"#
    );
    let out = run(
        &html,
        "https://threads.com/@person/post/123",
        &Options::default(),
    )
    .unwrap();
    let html = adapter_html(out.content);
    assert!(html.contains("@other"));
    assert!(html.contains("&lt;img"));
    assert!(Document::fragment(html).select("[onerror]").is_empty());
}

#[test]
fn github_table_cell_comment_preserves_all_body_children() {
    let html = r#"<meta name="expected-hostname" content="github.com"><h1 class="gh-header-title">Title</h1><div class="timeline-comment" id="pullrequest-123"><a class="author">One Person</a><div class="comment-body markdown-body"><p>Original description.</p></div></div><div class="timeline-comment"><a class="author">Reviewer</a><table><tbody><tr><td class="comment-body markdown-body"><p>Review comment paragraph.</p><ul><li>Preserve first change</li><li>Preserve second change</li></ul></td></tr></tbody></table></div>"#;
    let out = run(
        html,
        "https://github.com/owner/repo/pull/123",
        &Options::default(),
    )
    .unwrap();
    let html = adapter_html(out.content);
    assert!(html.contains("<p>Review comment paragraph.</p>"));
    assert!(html.contains("Preserve first change"));
    assert!(html.contains("Preserve second change"));
}

#[test]
fn nested_upstream_youtube_fixture_is_verified_and_does_not_invent_remote_captions() {
    use sha2::{Digest, Sha256};
    let source = include_bytes!("../data/youtube/issues--410-default-audio.html");
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../data/youtube/manifest.json")).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(source)), manifest["sha256"]);
    let html = std::str::from_utf8(source).unwrap();
    let out = run(html, manifest["url"].as_str().unwrap(), &Options::default()).unwrap();
    assert_eq!(out.extractor_type, "youtube");
    assert_eq!(out.metadata.title, "Example video");
    assert!(out.metadata.language.is_empty());
    let content = adapter_html(out.content);
    let doc = Document::fragment(content);
    assert_eq!(
        doc.select("iframe").attr("src").unwrap().to_string(),
        "https://www.youtube.com/embed/example123"
    );
    assert!(!doc.text().contains("Transcript"));
    assert!(!doc.text().contains("これは元の音声です。"));
}

#[test]
fn poisoned_adapter_metadata_and_raw_content_are_safe_with_cleanup_disabled() {
    // The encoded quotes stay inside data values; each adapter must escape new
    // markup before construction, and the public pipeline sanitizes copied HTML.
    let poisoned = "Person &quot; onclick=&quot;evil()";
    let cases=[
        ("https://mail.google.com/mail/u/0/#inbox/id",format!(r#"<div class="adn ads"><span class="gD" name="{poisoned}"></span><div class="a3s"><p onclick="evil()">Message</p><img src="javascript:evil()"></div></div>"#)),
        ("https://reddit.com/r/test/comments/123/title",format!(r#"<div class="thing link" data-author="{poisoned}" data-subreddit="test"><a class="title">Title</a><div class="usertext-body"><div class="md"><p onmouseover="evil()">Message</p></div></div></div><div class="commentarea"><div class="sitetable"><div class="thing comment" data-author="{poisoned}"><div class="entry"><div class="usertext-body"><div class="md">Reply</div></div></div></div></div></div>"#)),
        ("https://news.ycombinator.com/item?id=123",format!(r#"<table class="fatitem"><tr class="athing"><td><span class="titleline"><a href="javascript:evil()">Title</a></span></td></tr><tr><td><a class="hnuser">{poisoned}</a><div class="toptext"><p onclick="evil()">Message</p></div></td></tr></table>"#)),
        ("https://x.com/person/article/123",format!(r#"<div data-testid="twitterArticleReadView"><div data-testid="tweetPhoto"><img src="https://example.org/image.jpg" alt="{poisoned}"></div><div data-testid="twitterArticleRichTextView"><h1 data-testid="twitter-article-title">Title</h1><div class="longform-unstyled">Message</div><iframe srcdoc="&lt;script&gt;evil()&lt;/script&gt;" src="data:text/html,evil"></iframe></div></div>"#)),
        ("https://chatgpt.com/share/id",format!(r#"<div data-testid="conversation-turn-1"><h4 class="sr-only">{poisoned}:</h4><div data-message-author-role="assistant"><div class="markdown"><p onclick="evil()">Message</p><span><a href='https://example.org/&quot; onmouseover=&quot;evil()' target="_blank" rel="noopener">Source</a></span></div></div></div>"#)),
        ("https://leetcode.com/problems/example/",r#"<div data-track-load="description_content"><p onclick="evil()">Message</p><a href="javascript:evil()">Link</a><iframe srcdoc="evil()"></iframe></div>"#.into()),
        ("https://medium.com/@person/article",r#"<article class="meteredContent"><p onclick="evil()">Message</p><img src="blob:evil"></article>"#.into()),
        ("https://test.substack.com/p/article",r#"<div class="body markup"><p onclick="evil()">Message</p><a href="data:text/html,evil">Link</a></div>"#.into()),
    ];
    for (url, html) in cases {
        let out = defuddle::extract(
            &html,
            Some(url),
            &Options {
                remove_exact_selectors: false,
                remove_partial_selectors: false,
                remove_hidden_elements: false,
                remove_low_scoring: false,
                remove_content_patterns: false,
                remove_small_images: false,
                standardize: false,
                ..Options::default()
            },
        )
        .unwrap();
        assert!(out.extractor_type.is_some(), "no adapter for {url}");
        let doc = Document::fragment(out.content);
        for node in defuddle::dom::subtree(doc.root())
            .into_iter()
            .filter(|n| n.is_element())
        {
            for attr in node.attrs() {
                let name = attr.name.local.as_ref();
                assert!(
                    !name.to_ascii_lowercase().starts_with("on"),
                    "{url}: {name}"
                );
                assert_ne!(name, "srcdoc", "{url}");
                if matches!(name, "src" | "href") {
                    assert!(
                        !defuddle::safety::is_dangerous_url(attr.value.as_ref(), node.is("img")),
                        "{url}: unsafe {name}"
                    );
                }
            }
        }
    }
}

#[test]
fn bbcode_image_delimiters_keep_unicode_byte_offsets() {
    for body in [
        "[img]İ[/img]",
        "[img]https://example.org/İ.png[/IMG] tail",
        "[img]https://example.org/K.png[/img]",
    ] {
        let event = serde_json::json!({"announcement_body":{"body":body}});
        let html = format!(
            "<div id=\"application_config\" data-partnereventstore='{}'></div>",
            event
        );
        let out = run(&html, "https://store.example.org/post", &Options::default()).unwrap();
        let html = adapter_html(out.content);
        let image = Document::fragment(html)
            .select("img")
            .attr("src")
            .unwrap()
            .to_string();
        let source = body.split(']').nth(1).unwrap().split('[').next().unwrap();
        assert_eq!(image, source);
    }
}
