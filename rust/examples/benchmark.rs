//! Paired-run benchmark worker. Imports, input IO, validation and output walks
//! stay outside timers. The coordinator selects mode order and cold/warm runs.
use defuddle::{Extraction, Options, extract};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, hint::black_box, path::PathBuf, time::Instant};

type AnyError = Box<dyn std::error::Error>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InputSpec {
    id: String,
    file: String,
    sha256: String,
    input_sha256: String,
    url: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selection {
    schema_version: usize,
    kind: String,
    pass: usize,
    rounds: usize,
    warmups: usize,
    manifest_sha256: String,
    input_set_sha256: String,
    math_exclusions_sha256: String,
    excluded_math_case_ids: Vec<String>,
    case_ids: Vec<String>,
    inputs: Vec<InputSpec>,
    mode_order: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    id: String,
    url: String,
    input_html: String,
    input_sha256: String,
}
#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputProof {
    sha256: String,
    content_bytes: usize,
    markdown_bytes: usize,
    word_count: usize,
}
fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn load_inputs(selection: &Selection) -> Result<Vec<Input>, AnyError> {
    let warm = selection.kind == "warm";
    if selection.schema_version != 2
        || (!warm && selection.kind != "cold")
        || selection.rounds == 0
        || (warm
            && (selection.warmups == 0 || selection.rounds < 3 || selection.case_ids.len() != 208))
        || (!warm
            && (selection.rounds != 1 || selection.warmups != 0 || selection.case_ids.len() != 1))
        || selection.case_ids.len() != selection.inputs.len()
        || selection.mode_order.is_empty()
        || (warm && selection.mode_order.len() != 2)
        || (!warm && selection.mode_order.len() != 1)
        || selection
            .mode_order
            .iter()
            .any(|mode| mode != "html" && mode != "html+markdown")
        || (warm && selection.mode_order[0] == selection.mode_order[1])
    {
        return Err("invalid benchmark selection dimensions".into());
    }
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/upstream");
    let manifest_bytes = fs::read(directory.join("manifest.json"))?;
    if digest(&manifest_bytes) != selection.manifest_sha256 {
        return Err("upstream manifest hash differs from selection".into());
    }
    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
    let entries = manifest["cases"]
        .as_array()
        .ok_or("missing manifest cases")?;
    let math_bytes =
        fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/quality/math.json"))?;
    if digest(&math_bytes) != selection.math_exclusions_sha256 {
        return Err("math exclusion hash differs from selection".into());
    }
    let math: Value = serde_json::from_slice(&math_bytes)?;
    let mut math_ids: Vec<_> = math
        .as_object()
        .ok_or("invalid math exclusions")?
        .keys()
        .cloned()
        .collect();
    math_ids.sort();
    if math_ids != selection.excluded_math_case_ids {
        return Err("math exclusion IDs differ from selection".into());
    }
    let expected_ids: Vec<_> = entries
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .filter(|id| !math_ids.iter().any(|excluded| excluded == id))
        .collect();
    if (warm && expected_ids != selection.case_ids)
        || selection.case_ids.iter().any(|id| math_ids.contains(id))
    {
        return Err("selected cases must retain the complete ordered non-math corpus".into());
    }
    let mut cases = Vec::with_capacity(selection.inputs.len());
    for (index, spec) in selection.inputs.iter().enumerate() {
        let entry = entries
            .iter()
            .find(|entry| entry["id"].as_str() == Some(&spec.id))
            .ok_or("unknown selected case")?;
        if selection.case_ids[index] != spec.id
            || entry["file"].as_str() != Some(&spec.file)
            || entry["sha256"].as_str() != Some(&spec.sha256)
        {
            return Err(format!("case provenance differs: {}", spec.id).into());
        }
        let bytes = fs::read(directory.join(&spec.file))?;
        if digest(&bytes) != spec.sha256 {
            return Err(format!("case file hash differs: {}", spec.id).into());
        }
        let input: Input = serde_json::from_slice(&bytes)?;
        if input.id != spec.id
            || input.url != spec.url
            || input.url.is_empty()
            || input.input_sha256 != spec.input_sha256
            || digest(&input.input_html) != spec.input_sha256
        {
            return Err(format!("input HTML/URL differs: {}", spec.id).into());
        }
        cases.push(input);
    }
    let identity: Vec<_> = cases
        .iter()
        .map(|input| {
            serde_json::json!({
                "id":input.id,"url":input.url,"inputSha256":input.input_sha256
            })
        })
        .collect();
    if digest(serde_json::to_vec(&identity)?) != selection.input_set_sha256 {
        return Err("selected HTML/URL identity hash differs".into());
    }
    Ok(cases)
}

fn validate_output(result: &Extraction, markdown: bool) -> Result<OutputProof, AnyError> {
    if result.content.is_empty()
        || !result.warnings.is_empty()
        || (markdown && result.content_markdown.is_none())
        || (!markdown && result.content_markdown.is_some())
    {
        return Err(
            "empty output, math warning, or incorrect Markdown mode in non-math benchmark".into(),
        );
    }
    // Shared projection field order matches scripts/porting/shared.ts.
    // The full output is consumed only after the entire round timer stops.
    let projected = serde_json::json!({
        "content": result.content,
        "contentMarkdown": result.content_markdown.as_deref().unwrap_or(""),
        "title": result.metadata.title,
        "author": result.metadata.author,
        "site": result.metadata.site,
        "published": result.metadata.published,
        "wordCount": result.word_count,
        "extractorType": result.extractor_type,
    });
    Ok(OutputProof {
        sha256: digest(serde_json::to_vec(&projected)?),
        content_bytes: result.content.len(),
        markdown_bytes: result.content_markdown.as_ref().map_or(0, String::len),
        word_count: result.word_count,
    })
}

fn main() -> Result<(), AnyError> {
    let args: Vec<_> = std::env::args().collect();
    let selection_path = PathBuf::from(args.get(1).ok_or("benchmark requires selection JSON")?);
    let output = PathBuf::from(args.get(2).ok_or("benchmark requires output path")?);
    let config: Selection = serde_json::from_slice(&fs::read(selection_path)?)?;
    let cases = load_inputs(&config)?;
    if args.get(3).is_some_and(|arg| arg == "--validate-only") {
        println!(
            "Validated {} pinned input strings and URLs; no extraction/timers run.",
            cases.len()
        );
        return Ok(());
    }
    let mut modes = Vec::new();
    for mode in &config.mode_order {
        let markdown = mode == "html+markdown";
        let options = Options {
            markdown,
            ..Default::default()
        };
        let mut samples = Vec::with_capacity(config.rounds);
        let mut per_case = vec![Vec::with_capacity(config.rounds); cases.len()];
        let mut proofs: Vec<Option<OutputProof>> = vec![None; cases.len()];
        let mut raw_rounds = Vec::with_capacity(config.warmups + config.rounds);
        for round in 0..config.warmups + config.rounds {
            let mut outputs = Vec::with_capacity(cases.len());
            let mut case_ms = vec![0.0; cases.len()];
            let start = Instant::now();
            for offset in 0..cases.len() {
                let index = (offset + round) % cases.len();
                let case = &cases[index];
                let before = Instant::now();
                let result = black_box(extract(
                    black_box(&case.input_html),
                    Some(black_box(&case.url)),
                    black_box(&options),
                )?);
                case_ms[index] = before.elapsed().as_secs_f64() * 1000.0;
                outputs.push((index, result));
            }
            let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
            // Validation, hashing, serialization and result destruction are outside
            // both the full-corpus timer and all per-page timers.
            for (index, result) in outputs {
                let proof = validate_output(&result, markdown)?;
                match &proofs[index] {
                    Some(previous) if *previous != proof => {
                        return Err(format!(
                            "nondeterministic {mode} output for {}",
                            cases[index].id
                        )
                        .into());
                    }
                    None => proofs[index] = Some(proof),
                    _ => {}
                }
            }
            let measured = round >= config.warmups;
            if measured {
                samples.push(wall_ms);
                for (index, ms) in case_ms.iter().enumerate() {
                    per_case[index].push(*ms);
                }
            }
            raw_rounds.push(
                serde_json::json!({"round":round,"phase":if measured {"measured"} else {"warmup"},
                "orderStart":round % cases.len(),"wallMs":wall_ms,"caseMs":case_ms}),
            );
        }
        modes.push(
            serde_json::json!({"mode":mode,"roundTotalMs":samples,"rawRounds":raw_rounds,
            "perCase":cases.iter().zip(per_case).zip(proofs).map(|((case,ms),output)|
                serde_json::json!({"id":case.id,"ms":ms,"output":output})).collect::<Vec<_>>()}),
        );
    }
    let result = serde_json::json!({"schemaVersion":2,"implementation":"rust","kind":config.kind,
        "pass":config.pass,"caseCount":cases.len(),"rounds":config.rounds,"warmups":config.warmups,
        "manifestSha256":config.manifest_sha256,"inputSetSha256":config.input_set_sha256,
        "modeOrder":config.mode_order,"modes":modes});
    fs::write(&output, serde_json::to_vec_pretty(&result)?)?;
    println!("Wrote {}", output.display());
    Ok(())
}
