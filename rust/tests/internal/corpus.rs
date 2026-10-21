//! End-to-end quality contract for the immutable upstream HTML instances.
#[path = "../support/mod.rs"]
mod support;
use defuddle::{Options, extract};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, path::PathBuf};
use support::quality::{
    html_semantics, markdown_prose_words, markdown_semantics, markdown_text, missing_prose,
    prose_words,
};

fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/upstream")
}
fn load() -> Vec<Value> {
    let directory = directory();
    let manifest: Value =
        serde_json::from_slice(&fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let cases = manifest["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 226);
    let mut ids = HashSet::new();
    cases
        .iter()
        .map(|entry| {
            assert!(ids.insert(entry["id"].as_str().unwrap()));
            let bytes = fs::read(directory.join(entry["file"].as_str().unwrap())).unwrap();
            assert_eq!(digest(&bytes), entry["sha256"].as_str().unwrap());
            let case: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(case["id"], entry["id"]);
            assert_eq!(
                digest(case["inputHtml"].as_str().unwrap().as_bytes()),
                case["inputSha256"].as_str().unwrap()
            );
            case
        })
        .collect()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn safe_content(source: &str, id: &str) {
    let doc = dom_query::Document::from(source);
    assert!(doc.select("style,noscript,meta,link,object,embed,applet,template,frame,frameset,base,animate,set,animateMotion,animateTransform,animateColor,discard").is_empty(),"unsafe element in {id}");
    for node in defuddle::dom::subtree(doc.root()) {
        if node.has_name("script") {
            assert!(
                defuddle::dom::is_html_element(node, "script")
                    && node
                        .attr_or("type", "")
                        .to_ascii_lowercase()
                        .starts_with("math/"),
                "script in {id}"
            )
        }
        for attr in node.attrs() {
            let name = attr.name.local.to_ascii_lowercase();
            let name = name.rsplit(':').next().unwrap();
            assert!(
                !name.starts_with("on") && name != "srcdoc",
                "active attribute in {id}"
            );
            if matches!(
                name,
                "href"
                    | "src"
                    | "action"
                    | "formaction"
                    | "poster"
                    | "background"
                    | "cite"
                    | "longdesc"
                    | "manifest"
                    | "data"
                    | "codebase"
                    | "profile"
            ) {
                let value: String = attr
                    .value
                    .chars()
                    .filter(|c| !c.is_whitespace() && !c.is_control())
                    .flat_map(char::to_lowercase)
                    .take(32)
                    .collect();
                assert!(
                    !value.starts_with("javascript:")
                        && !value.starts_with("vbscript:")
                        && !value.starts_with("blob:")
                        && (!value.starts_with("data:")
                            || (value.starts_with("data:image/")
                                && !(name == "src" && node.has_name("iframe")))),
                    "unsafe URL in {id}"
                );
            }
            if matches!(name, "srcset" | "imagesrcset") {
                for (url, descriptor) in defuddle::dom::srcset_candidates(&attr.value) {
                    assert!(
                        !defuddle::safety::is_dangerous_url(&format!("{url}{descriptor}"), true),
                        "unsafe srcset in {id}"
                    );
                }
            }
        }
    }
}

#[test]
fn reference_prose_survives_in_html_and_markdown() {
    let math: Value = serde_json::from_slice(
        &fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/quality/math.json"))
            .unwrap(),
    )
    .unwrap();
    let mut failures = Vec::new();
    for case in load() {
        let id = case["id"].as_str().unwrap();
        let actual = extract(
            case["inputHtml"].as_str().unwrap(),
            case["url"].as_str(),
            &Options {
                markdown: true,
                ..Default::default()
            },
        )
        .unwrap();
        let reference_html = case["expected"]["content"].as_str().unwrap();
        let expected_html = prose_words(reference_html, false);
        let actual_html = prose_words(&actual.content, false);
        let expected_md = if math.get(id).is_some() {
            prose_words(reference_html, true)
        } else {
            markdown_prose_words(case["expected"]["contentMarkdown"].as_str().unwrap())
        };
        let actual_md = markdown_prose_words(actual.content_markdown.as_deref().unwrap());
        for (kind, missing) in [
            ("HTML", missing_prose(&expected_html, &actual_html)),
            ("Markdown", missing_prose(&expected_md, &actual_md)),
        ] {
            if !missing.is_empty() {
                failures.push(format!("{id} {kind}: {missing:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Reference prose lost:\n{}",
        failures.join("\n")
    );
}

#[test]
fn pinned_upstream_corpus_preserves_reviewed_quality_and_security() {
    let contracts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/quality");
    let reviewed: Value =
        serde_json::from_slice(&fs::read(contracts.join("reviewed.json")).unwrap()).unwrap();
    let math: Value =
        serde_json::from_slice(&fs::read(contracts.join("math.json")).unwrap()).unwrap();
    let html_contract: Value =
        serde_json::from_slice(&fs::read(contracts.join("html.json")).unwrap()).unwrap();
    let mut used = HashSet::new();
    let mut math_used = HashSet::new();
    let mut failures = Vec::new();
    for case in load() {
        let id = case["id"].as_str().unwrap();
        let actual = extract(
            case["inputHtml"].as_str().unwrap(),
            case["url"].as_str(),
            &Options {
                markdown: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!actual.content.is_empty(), "empty HTML in {id}");
        let markdown = actual.content_markdown.as_deref().unwrap();
        assert!(!markdown.is_empty(), "empty Markdown in {id}");
        let actual_json = serde_json::to_value(&actual).unwrap();
        for field in ["title", "author", "site", "published", "extractorType"] {
            assert_eq!(actual_json[field], case["expected"][field], "{id}: {field}");
        }
        safe_content(&actual.content, id);
        let html_hash = digest(&serde_json::to_vec(&html_semantics(&actual.content)).unwrap());
        let reference_html_hash = digest(
            &serde_json::to_vec(&html_semantics(
                case["expected"]["content"].as_str().unwrap(),
            ))
            .unwrap(),
        );
        assert_eq!(
            html_contract[id]["actualSemanticsHash"].as_str(),
            Some(html_hash.as_str()),
            "{id}: cleaned HTML semantic regression"
        );
        assert_eq!(
            html_contract[id]["expectedSemanticsHash"].as_str(),
            Some(reference_html_hash.as_str()),
            "{id}: reference HTML contract changed"
        );
        if html_hash != reference_html_hash {
            assert!(
                html_contract[id]["reason"].as_str().unwrap().len() > 40,
                "{id}: HTML difference needs a source-backed reason"
            );
        }
        let mut rendered = String::new();
        pulldown_cmark::html::push_html(
            &mut rendered,
            pulldown_cmark::Parser::new_ext(
                markdown,
                pulldown_cmark::Options::ENABLE_TABLES
                    | pulldown_cmark::Options::ENABLE_FOOTNOTES
                    | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
                    | pulldown_cmark::Options::ENABLE_TASKLISTS,
            ),
        );
        safe_content(&rendered, id);
        let expected = markdown_semantics(case["expected"]["contentMarkdown"].as_str().unwrap());
        let actual = markdown_semantics(markdown);
        let snapshot_path = contracts.join("native").join(format!("{id}.json"));
        let snapshot: Value =
            serde_json::from_slice(&fs::read(&snapshot_path).unwrap_or_else(|error| {
                panic!(
                    "Missing readable snapshot {}: {error}",
                    snapshot_path.display()
                )
            }))
            .unwrap();
        assert_eq!(
            snapshot,
            native_snapshot(&actual_json),
            "{id}: readable native semantic snapshot changed; review the visible diff"
        );
        let expected_hash = digest(&serde_json::to_vec(&expected).unwrap());
        let actual_hash = digest(&serde_json::to_vec(&actual).unwrap());
        if let Some(contract) = math.get(id) {
            math_used.insert(id.to_string());
            assert!(
                contract["reason"].as_str().unwrap().len() > 40,
                "{id}: math scope reason"
            );
            assert_eq!(
                actual_json["warnings"],
                serde_json::json!([{"code":"math_not_supported","message":"Math support is not yet included."}]),
                "{id}: warning"
            );
            assert_eq!(
                contract["actualSemanticsHash"].as_str(),
                Some(actual_hash.as_str()),
                "{id}: unsupported math/source content regression"
            );
        } else {
            assert!(
                actual_json["warnings"].as_array().unwrap().is_empty(),
                "unexpected warning in {id}"
            );
            if actual != expected {
                if let Some(contract) = reviewed.get(id) {
                    used.insert(id.to_string());
                    assert!(contract["reason"].as_str().unwrap().len() > 40);
                    assert_eq!(
                        contract["expectedSemanticsHash"].as_str(),
                        Some(expected_hash.as_str()),
                        "{id}: reference contract changed"
                    );
                    assert_eq!(
                        contract["actualSemanticsHash"].as_str(),
                        Some(actual_hash.as_str()),
                        "{id}: reviewed quality regression"
                    );
                } else {
                    failures.push(id.to_string())
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Unreviewed semantic differences: {}",
        failures.join(", ")
    );
    assert_eq!(
        used.len(),
        reviewed.as_object().unwrap().len(),
        "Remove obsolete reviewed differences"
    );
    assert_eq!(
        math_used.len(),
        math.as_object().unwrap().len(),
        "Unknown math case"
    );
    assert_eq!(
        html_contract.as_object().unwrap().len(),
        226,
        "Missing or unknown HTML contracts"
    );
}

fn native_snapshot(actual: &Value) -> Value {
    let mut metadata = actual.clone();
    for key in [
        "content",
        "contentMarkdown",
        "warnings",
        "debug",
        "wordCount",
        "extractorType",
    ] {
        metadata.as_object_mut().unwrap().remove(key);
    }
    serde_json::json!({
        "html": html_semantics(actual["content"].as_str().unwrap()),
        "markdown": markdown_semantics(actual["contentMarkdown"].as_str().unwrap()),
        "metadata": metadata,
        "warnings": actual["warnings"],
        "wordCount": actual["wordCount"],
        "extractorType": actual["extractorType"],
    })
}

#[test]
#[ignore = "explicit snapshot export for review; never used by normal quality checks"]
fn export_readable_snapshots() {
    let output = PathBuf::from(
        std::env::var("DEFUDDLE_SNAPSHOT_WRITE_DIR")
            .expect("Set DEFUDDLE_SNAPSHOT_WRITE_DIR to an explicit review directory"),
    );
    fs::create_dir_all(&output).unwrap();
    // An immutable pre-change diagnostic can seed readable snapshots without
    // running an old crate in the current build directory.
    let baseline: Option<Vec<Value>> = std::env::var("DEFUDDLE_SNAPSHOT_BASELINE")
        .ok()
        .map(|path| serde_json::from_slice(&fs::read(path).unwrap()).unwrap());
    for case in load() {
        let id = case["id"].as_str().unwrap();
        let actual = if let Some(baseline) = &baseline {
            baseline.iter().find(|record| record["id"] == id).unwrap()["actual"].clone()
        } else {
            serde_json::to_value(
                extract(
                    case["inputHtml"].as_str().unwrap(),
                    case["url"].as_str(),
                    &Options {
                        markdown: true,
                        ..Default::default()
                    },
                )
                .unwrap(),
            )
            .unwrap()
        };
        fs::write(
            output.join(format!("{id}.json")),
            serde_json::to_vec_pretty(&native_snapshot(&actual)).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "diagnostic report; the default corpus test enforces the quality contracts"]
fn report_full_pipeline() {
    let mut report = Vec::new();
    for case in load() {
        let actual = extract(
            case["inputHtml"].as_str().unwrap(),
            case["url"].as_str(),
            &Options {
                markdown: true,
                ..Default::default()
            },
        )
        .unwrap();
        let markdown = actual.content_markdown.as_deref().unwrap();
        let expected = case["expected"]["contentMarkdown"].as_str().unwrap();
        let semantics = markdown_semantics(markdown);
        let expected_semantics = markdown_semantics(expected);
        let actual_html_semantics = html_semantics(&actual.content);
        let expected_html_semantics = html_semantics(case["expected"]["content"].as_str().unwrap());
        report.push(serde_json::json!({"id":case["id"],"math":!actual.warnings.is_empty(),"exact":markdown==expected,"semanticsEqual":semantics==expected_semantics,"textEqual":markdown_text(markdown)==markdown_text(expected),"htmlSemanticsEqual":actual_html_semantics==expected_html_semantics,"actualHtmlSemanticsHash":digest(&serde_json::to_vec(&actual_html_semantics).unwrap()),"expectedHtmlSemanticsHash":digest(&serde_json::to_vec(&expected_html_semantics).unwrap()),"expectedSemanticsHash":digest(&serde_json::to_vec(&expected_semantics).unwrap()),"actualSemanticsHash":digest(&serde_json::to_vec(&semantics).unwrap()),"actualSemantics":semantics,"expectedSemantics":expected_semantics,"actual":actual,"expected":case["expected"]}));
    }
    let output = std::env::var("DEFUDDLE_QUALITY_REPORT")
        .unwrap_or_else(|_| "/tmp/defuddle-rust-quality.json".into());
    fs::write(&output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    eprintln!(
        "{} pages: {} exact, {} equivalent semantics, {} equivalent text; {} math warnings. Report: {}",
        report.len(),
        report.iter().filter(|r| r["exact"] == true).count(),
        report
            .iter()
            .filter(|r| r["semanticsEqual"] == true)
            .count(),
        report.iter().filter(|r| r["textEqual"] == true).count(),
        report.iter().filter(|r| r["math"] == true).count(),
        output
    );
}
