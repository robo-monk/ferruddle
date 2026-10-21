use defuddle::{Options, extract};
use std::{
    fs,
    io::{self, Read},
    process::ExitCode,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("defuddle: {error}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut path = None;
    let mut url = None;
    let mut markdown = false;
    let mut json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "Usage: defuddle [FILE|-] [--url URL] [--markdown] [--json]\nReads HTML from FILE or standard input. Performs no network requests."
                );
                return Ok(());
            }
            "--url" => url = Some(args.next().ok_or("--url requires a URL")?),
            "--markdown" => markdown = true,
            "--json" => json = true,
            "-" => path = None,
            flag if flag.starts_with('-') => return Err(format!("unknown option: {flag}").into()),
            _ if path.is_none() => path = Some(arg),
            _ => return Err("only one input file is supported".into()),
        }
    }
    let source = if let Some(path) = path {
        fs::read_to_string(path)?
    } else {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        s
    };
    let result = extract(
        &source,
        url.as_deref(),
        &Options {
            markdown,
            ..Default::default()
        },
    )?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?)
    } else {
        for warning in &result.warnings {
            eprintln!(
                "warning [{}]: {}",
                serde_json::to_value(warning.code)?
                    .as_str()
                    .unwrap_or("warning"),
                warning.message
            )
        }
        println!(
            "{}",
            result
                .content_markdown
                .as_deref()
                .unwrap_or(&result.content)
        );
    }
    Ok(())
}
