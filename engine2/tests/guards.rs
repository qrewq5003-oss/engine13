//! Convention guards of E1a (plan §9, items 2 and 5). They read the crate's own sources.
//!
//! - no `HashMap`/`HashSet` in `engine2/src`: iteration order must not depend on a hash;
//! - no `default` or `skip` inside any `serde(...)` attribute in `engine2/src`: an
//!   incompatible save bumps `SAVE_FORMAT`, nothing is filled in silently;
//! - every struct that derives `Deserialize` carries `deny_unknown_fields` (or is
//!   `transparent`): a save with a field this build does not know is refused.

use std::path::{Path, PathBuf};

fn sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push((p.clone(), std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    walk(&root, &mut out);
    assert!(!out.is_empty());
    out
}

/// Lines of code, with `//` comments and doc comments stripped, so that a sentence
/// *about* `HashMap` in a comment does not trip the guard.
fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines().enumerate().filter_map(|(i, l)| {
        let t = l.trim_start();
        if t.starts_with("//") {
            None
        } else {
            Some((i + 1, l.split("//").next().unwrap_or(l)))
        }
    })
}

#[test]
fn no_hash_collections_in_engine_sources() {
    let mut hits = Vec::new();
    for (path, text) in sources() {
        for (n, line) in code_lines(&text) {
            if line.contains("HashMap") || line.contains("HashSet") {
                hits.push(format!("{}:{}: {}", path.display(), n, line.trim()));
            }
        }
    }
    assert!(hits.is_empty(), "hash collections in engine sources:\n{}", hits.join("\n"));
}

/// Every `serde(...)` attribute on a line, with its contents.
fn serde_attrs(line: &str) -> impl Iterator<Item = &str> {
    line.match_indices("serde(").filter_map(move |(i, _)| {
        let rest = &line[i + "serde(".len()..];
        rest.find(')').map(|j| &rest[..j])
    })
}

#[test]
fn no_serde_defaults_or_skips_in_engine_sources() {
    let mut hits = Vec::new();
    for (path, text) in sources() {
        for (n, line) in code_lines(&text) {
            for attr in serde_attrs(line) {
                let bad = attr.split(',').map(str::trim).any(|item| {
                    item == "default" || item.starts_with("default ") || item.starts_with("default=") || item.starts_with("skip")
                });
                if bad {
                    hits.push(format!("{}:{}: {}", path.display(), n, line.trim()));
                }
            }
        }
    }
    assert!(hits.is_empty(), "serde default/skip in a saved type:\n{}", hits.join("\n"));
}

#[test]
fn every_deserializable_struct_denies_unknown_fields() {
    let mut hits = Vec::new();
    for (path, text) in sources() {
        let lines: Vec<(usize, &str)> = code_lines(&text).collect();
        for (k, (n, line)) in lines.iter().enumerate() {
            if !(line.contains("derive(") && line.contains("Deserialize")) {
                continue;
            }
            // Attributes between the derive and the item; the item line itself ends the scan.
            let mut ok = false;
            let mut item = None;
            for (_, l) in &lines[k + 1..] {
                let t = l.trim();
                if t.starts_with("#[") {
                    if t.contains("deny_unknown_fields") || t.contains("transparent") {
                        ok = true;
                    }
                    continue;
                }
                item = Some(t.to_string());
                break;
            }
            let is_struct = item.as_deref().is_some_and(|t| t.contains("struct "));
            if is_struct && !ok {
                hits.push(format!("{}:{}: {}", path.display(), n, item.unwrap_or_default()));
            }
        }
    }
    assert!(hits.is_empty(), "Deserialize struct without deny_unknown_fields:\n{}", hits.join("\n"));
}
