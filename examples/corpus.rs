//! Run every pinned upstream page and write reviewable extraction results.
//! cargo run --release --example corpus -- /tmp/ferruddle-corpus
use ferruddle::{Options, extract};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/upstream");
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "/tmp/ferruddle-corpus".into()),
    );
    fs::create_dir_all(&output)?;
    let manifest: Value = serde_json::from_slice(&fs::read(input.join("manifest.json"))?)?;
    let cases = manifest["cases"]
        .as_array()
        .ok_or("missing manifest cases")?;
    assert_eq!(
        cases.len(),
        manifest["caseCount"].as_u64().unwrap() as usize
    );
    let mut results = Vec::new();
    for entry in cases {
        let bytes = fs::read(input.join(entry["file"].as_str().unwrap()))?;
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            entry["sha256"].as_str().unwrap()
        );
        let case: Value = serde_json::from_slice(&bytes)?;
        let html = case["inputHtml"].as_str().unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(html.as_bytes())),
            case["inputSha256"].as_str().unwrap()
        );
        let result = extract(
            html,
            case["url"].as_str(),
            &Options {
                markdown: true,
                ..Default::default()
            },
        )?;
        let record =
            serde_json::json!({"id":case["id"],"actual":result,"expected":case["expected"]});
        fs::write(
            output.join(entry["file"].as_str().unwrap()),
            serde_json::to_vec_pretty(&record)?,
        )?;
        results.push(record);
    }
    fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&results)?,
    )?;
    println!(
        "Extracted {} verified pages into {}",
        cases.len(),
        output.display()
    );
    Ok(())
}
