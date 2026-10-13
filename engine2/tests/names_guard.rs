//! Guard "the engine knows no names" (brief §0.3 item 2, plan §9 item 1), the engine2 twin
//! of the old crate's `engine_knows_authored_content_only_by_these_names`.
//!
//! The authored vocabulary is every `id = "…"` / `id: "…"` in `src/scenarios/**` of the old
//! engine (the names engine2 will one day receive as data). No string literal in
//! `engine2/src` may equal one of them. The expected set of exceptions is empty and stays so.

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

#[test]
fn engine_sources_contain_no_authored_name() {
    let authored = authored_ids();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    let mut hits = Vec::new();
    for p in files.iter().filter(|p| p.extension().is_some_and(|e| e == "rs")) {
        let text = std::fs::read_to_string(p).unwrap();
        for (n, line) in text.lines().enumerate() {
            for lit in literals(line) {
                if authored.contains(&lit) {
                    hits.push(format!("{}:{}: \"{}\"", p.display(), n + 1, lit));
                }
            }
        }
    }
    assert!(hits.is_empty(), "engine sources name authored content:\n{}", hits.join("\n"));
}
