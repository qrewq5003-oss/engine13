//! Guard "the engine knows no names" (brief §0.3 item 2, plan §9 item 1), the engine2 twin
//! of the old crate's `engine_knows_authored_content_only_by_these_names`.
//!
//! The authored vocabulary is every `id = "…"` / `id: "…"` in `src/scenarios/**` of the old
//! engine (the names engine2 will one day receive as data). No string literal in
//! `engine2/src` may equal one of them. The expected set of exceptions is empty and stays so.
//!
//! Since E4 one scenario is received as data — milan_1477, in `public/geodata2/milan_1477/` —
//! and the same holds of its ids (`engine_sources_name_nothing_of_the_scenario_of_milan`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn authored_ids() -> BTreeSet<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("src").join("scenarios");
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(!files.is_empty(), "old engine scenarios not found at {}", root.display());
    let mut out = BTreeSet::new();
    for p in files {
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        for line in text.lines() {
            let t = line.trim();
            for prefix in ["id = \"", "id: \""] {
                if let Some(rest) = t.strip_prefix(prefix) {
                    if let Some(name) = rest.split('"').next() {
                        out.insert(name.to_string());
                    }
                }
            }
        }
    }
    assert!(out.len() > 50, "authored vocabulary suspiciously small: {}", out.len());
    out
}

/// String literals of a Rust source line, with `//` comments stripped first.
fn literals(line: &str) -> Vec<String> {
    let code = if let Some(i) = line.find("//") { &line[..i] } else { line };
    let mut out = Vec::new();
    let mut rest = code;
    while let Some(i) = rest.find('"') {
        let after = &rest[i + 1..];
        let Some(j) = after.find('"') else { break };
        out.push(after[..j].to_string());
        rest = &after[j + 1..];
    }
    out
}

/// Every string literal of `engine2/src` that equals a name of `vocabulary`, with its place.
fn named_in_engine_sources(vocabulary: &BTreeSet<String>) -> Vec<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    assert!(files.iter().any(|p| p.extension().is_some_and(|e| e == "rs")), "engine sources not found at {}", src.display());
    let mut hits = Vec::new();
    for p in files.iter().filter(|p| p.extension().is_some_and(|e| e == "rs")) {
        let text = std::fs::read_to_string(p).unwrap();
        for (n, line) in text.lines().enumerate() {
            for lit in literals(line) {
                if vocabulary.contains(&lit) {
                    hits.push(format!("{}:{}: \"{}\"", p.display(), n + 1, lit));
                }
            }
        }
    }
    hits
}

#[test]
fn engine_sources_contain_no_authored_name() {
    let hits = named_in_engine_sources(&authored_ids());
    assert!(hits.is_empty(), "engine sources name authored content:\n{}", hits.join("\n"));
}

/// The ids of the scenario of Milan as engine2 receives it (E4, plan §31.51): the scenario's
/// own id and the keys of its polities, houses, persons, provinces and regions, from its two
/// files. The tongues and the faiths are not ids: the engine interns them and never reads
/// which is which, and the old guard does not take them either.
fn scenario_ids() -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("public").join("geodata2").join("milan_1477");
    let table = |file: &str| -> toml::Table {
        let path = dir.join(file);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };
    let (scenario, provinces) = (table("scenario.toml"), table("provinces.toml"));
    let mut out = BTreeSet::new();
    out.insert(scenario["id"].as_str().expect("the scenario's id").to_string());
    for (file, keys) in [(&scenario, ["actors", "houses", "persons"].as_slice()), (&provinces, ["provinces", "regions"].as_slice())] {
        for key in keys {
            let ids = file.get(*key).and_then(|t| t.as_table()).unwrap_or_else(|| panic!("the scenario of Milan has no [{key}]"));
            assert!(!ids.is_empty(), "[{key}] of the scenario of Milan is empty");
            out.extend(ids.keys().cloned());
        }
    }
    out
}

/// The acceptance of E4 (brief §0.3 item 2, plan §31.43.2 p. 3): the engine receives Milan as
/// data and names nothing of it — no polity, house, person, province or region of the
/// scenario is a string literal of `engine2/src`.
#[test]
fn engine_sources_name_nothing_of_the_scenario_of_milan() {
    let ids = scenario_ids();
    assert!(ids.len() > 100, "the scenario's vocabulary is suspiciously small: {}", ids.len());
    let hits = named_in_engine_sources(&ids);
    assert!(hits.is_empty(), "engine sources name the scenario of Milan:\n{}", hits.join("\n"));
}
