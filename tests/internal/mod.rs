//! Crate-local tests exercise private pipeline primitives without exporting them.
mod corpus;
mod extractors;
mod foundation;
mod metadata;
mod pipeline;
mod pipeline_review;
mod rich_elements;
mod standardize;

/// Parsed input documents of every pinned upstream case.
pub(crate) fn upstream_documents() -> Vec<dom_query::Document> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/upstream");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let case: serde_json::Value = serde_json::from_slice(
                &std::fs::read(directory.join(entry["file"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            dom_query::Document::from(case["inputHtml"].as_str().unwrap())
        })
        .collect()
}
