// Copyright 2026 the Docudis contributors. Licensed under Apache-2.0.

use docudis_core::{
    anonymize, BundledListDetector, Detection, DetectionPipeline, DictionaryDetector, MappingEntry,
    PlaceholderMap, RegexDetector,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    env, fs,
    io::{self, IsTerminal, Read},
    process,
};

#[derive(Serialize)]
struct DetectOutput {
    schema_version: u32,
    detections: Vec<Detection>,
}
#[derive(Serialize)]
struct ProcessOutput {
    schema_version: u32,
    text: String,
    mappings: Vec<MappingEntry>,
    replacements: Vec<docudis_core::Replacement>,
    detections: Vec<Detection>,
}
#[derive(Serialize)]
struct RestoreOutput {
    schema_version: u32,
    text: String,
}

#[derive(Default)]
struct Options {
    text: Option<String>,
    json: bool,
    detect_only: bool,
    restore: bool,
    regions: Option<HashSet<String>>,
    dictionary: Vec<String>,
    never_hide: Vec<String>,
    ner_file: Option<String>,
    map_file: Option<String>,
    bundled_lists: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("docudis: {error}");
        process::exit(2)
    }
}
fn run() -> Result<(), String> {
    let mut options = Options::default();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => options.json = true,
            "--detect-only" => options.detect_only = true,
            "--restore" => options.restore = true,
            "--bundled-lists" => options.bundled_lists = true,
            "--text" => options.text = Some(args.next().ok_or("--text requires a UTF-8 value")?),
            "--regions" => {
                let value = args
                    .next()
                    .ok_or("--regions requires comma-separated region codes")?;
                options.regions = Some(
                    value
                        .split(',')
                        .filter(|v| !v.is_empty())
                        .map(|v| v.to_ascii_lowercase())
                        .collect(),
                )
            }
            "--dictionary" => options
                .dictionary
                .push(args.next().ok_or("--dictionary requires a term")?),
            "--never-hide" => options
                .never_hide
                .push(args.next().ok_or("--never-hide requires a term")?),
            "--ner-detections" => {
                options.ner_file = Some(args.next().ok_or("--ner-detections requires a JSON file")?)
            }
            "--map" => options.map_file = Some(args.next().ok_or("--map requires a JSON file")?),
            "-h" | "--help" => {
                help();
                return Ok(());
            }
            value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
            value => {
                if options.text.is_some() {
                    return Err("provide text once (quote text containing spaces)".into());
                }
                options.text = Some(value.into())
            }
        }
    }
    let text = read_text(options.text.clone())?;
    if options.restore {
        return restore_command(&text, &options);
    }
    let mut candidates = RegexDetector::bundled(options.regions.as_ref(), None)
        .map_err(|e| e.to_string())?
        .detect_sync(&text)
        .map_err(|e| e.to_string())?;
    candidates.extend(DictionaryDetector::new(options.dictionary.clone()).detect_sync(&text));
    if options.bundled_lists {
        candidates.extend(
            BundledListDetector::bundled()
                .map_err(|e| e.to_string())?
                .detect_sync(&text),
        )
    }
    if let Some(path) = options.ner_file.as_deref() {
        candidates.extend(read_detections(path)?)
    }
    let detections = DetectionPipeline::new(options.never_hide.clone()).process(&text, candidates);
    if options.detect_only {
        if options.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&DetectOutput {
                    schema_version: 1,
                    detections
                })
                .map_err(|e| e.to_string())?
            )
        } else {
            for d in detections {
                println!(
                    "{}\t{}\t{}\t{}\t{:.3}\t{}",
                    d.start, d.end, d.entity_type, d.value, d.confidence, d.detector
                )
            }
        }
        return Ok(());
    }
    let previous = options.map_file.as_deref().map(read_map).transpose()?;
    let result = anonymize(&text, &detections, previous.as_ref()).map_err(|e| e.to_string())?;
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&ProcessOutput {
                schema_version: 1,
                text: result.text,
                mappings: result.map.entries(),
                replacements: result.replacements,
                detections
            })
            .map_err(|e| e.to_string())?
        )
    } else {
        print!("{}", result.text)
    }
    Ok(())
}

fn read_text(value: Option<String>) -> Result<String, String> {
    match value {
        Some(v) => Ok(v),
        None => {
            if io::stdin().is_terminal() {
                return Err("provide text as an argument, with --text, or on stdin".into());
            }
            let mut value = String::new();
            io::stdin()
                .read_to_string(&mut value)
                .map_err(|e| format!("could not read stdin: {e}"))?;
            Ok(value)
        }
    }
}
fn read_value(path: &str) -> Result<Value, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;
    serde_json::from_str(&source).map_err(|e| format!("invalid JSON in {path}: {e}"))
}
fn read_detections(path: &str) -> Result<Vec<Detection>, String> {
    let value = read_value(path)?;
    let values = value
        .as_array()
        .cloned()
        .or_else(|| value.get("detections").and_then(Value::as_array).cloned())
        .ok_or("NER JSON must be an array or an object with a detections array")?;
    serde_json::from_value(Value::Array(values))
        .map_err(|e| format!("invalid detections in {path}: {e}"))
}
fn read_map(path: &str) -> Result<PlaceholderMap, String> {
    let value = read_value(path)?;
    let values = value
        .as_array()
        .cloned()
        .or_else(|| value.get("mappings").and_then(Value::as_array).cloned())
        .ok_or("map JSON must be an array or an object with a mappings array")?;
    let entries: Vec<MappingEntry> = serde_json::from_value(Value::Array(values))
        .map_err(|e| format!("invalid mappings in {path}: {e}"))?;
    let mut map = PlaceholderMap::new();
    map.import(entries);
    Ok(map)
}
fn restore_command(text: &str, options: &Options) -> Result<(), String> {
    let path = options
        .map_file
        .as_deref()
        .ok_or("--restore requires --map FILE")?;
    let map = read_map(path)?;
    let restored = map.restore(text);
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&RestoreOutput {
                schema_version: 1,
                text: restored
            })
            .map_err(|e| e.to_string())?
        )
    } else {
        print!("{restored}")
    }
    Ok(())
}
fn help() {
    println!("Docudis Core CLI\n\nUsage:\n  docudis [OPTIONS] [TEXT]\n  printf TEXT | docudis [OPTIONS]\n\nOptions:\n  --regions fr,gb       Load global rules plus selected region packs\n  --dictionary TERM     Always hide a user term (repeatable)\n  --never-hide TERM     Keep a public term visible (repeatable)\n  --bundled-lists       Enable bundled company/place lists\n  --ner-detections FILE Merge external UTF-8-offset NER detection JSON\n  --detect-only         Print detections instead of anonymizing\n  --restore --map FILE  Restore placeholders using mapping JSON\n  --map FILE            Previous map for anonymization\n  --json                Emit schema-versioned JSON\n\nWithout --ner-detections, only rules and requested lists/dictionary run.\nAll offsets are half-open UTF-8 byte offsets.")
}
