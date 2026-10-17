//! Convention guards of E1a (plan §9, items 2 and 5). They read the crate's own sources.
//!
//! - no `HashMap`/`HashSet` in `engine2/src`: iteration order must not depend on a hash;
//! - no `default` or `skip` inside any `serde(...)` attribute in `engine2/src`: an
//!   incompatible save bumps `SAVE_FORMAT`, nothing is filled in silently;
//! - every struct that derives `Deserialize` carries `deny_unknown_fields` (or is
//!   `transparent`): a save with a field this build does not know is refused;
//! - the display currency is read only where it is defined (`scenario.rs`, which moves it
//!   into `Display`), in `view.rs`, and built in `testworld.rs` and `worldgen.rs` (scenario
//!   data builders) (plan §9 item 3);
//! - `params.toml` validates: every (a)/(b) inside its band, every (c) names its (b) row
//!   (plan §9 item 4) — the loader's own validation, run over the shipped file.

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
fn display_currency_is_read_only_by_the_view() {
    let mut hits = Vec::new();
    for (path, text) in sources() {
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        // scenario.rs defines it, view.rs reads it, testworld.rs and worldgen.rs *build*
        // scenario data (the same role as a scenario file).
        if file == "scenario.rs" || file == "view.rs" || file == "testworld.rs" || file == "worldgen.rs" {
            continue;
        }
        for (n, line) in code_lines(&text) {
            if line.contains("display_currency") || line.contains(".currency") || line.contains("DisplayCurrency") {
                hits.push(format!("{}:{}: {}", path.display(), n, line.trim()));
            }
        }
    }
    assert!(hits.is_empty(), "display_currency read outside the view:\n{}", hits.join("\n"));
}

#[test]
fn params_in_range() {
    // The loader refuses an out-of-band (a), a (b) with a value, a (c) without its (b) row;
    // the shipped file must pass it.
    engine2::Params::load().expect("engine2/params.toml");
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

/// Line endings are LF in every source, document and config of the repository (plan §28.8):
/// on Windows a python `write_text` rewrote whole files to CRLF and three commits' diffs
/// became the whole file (f630ba9, 9d9567f, bb63f69 — restored by separate commits). The
/// chain catches it here instead of a reviewer's eye.
#[test]
fn no_crlf_in_the_repositorys_sources_documents_and_configs() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if !matches!(name, "target" | "node_modules" | ".git" | "dist") {
                    walk(&path, out);
                }
            } else if matches!(path.extension().and_then(|e| e.to_str()), Some("rs" | "md" | "toml")) {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(files.len() > 50, "the walk found only {} files", files.len());
    let crlf: Vec<String> = files.iter().filter(|f| std::fs::read(f).unwrap().contains(&13u8)).map(|f| f.strip_prefix(&root).unwrap_or(f).display().to_string()).collect();
    assert!(crlf.is_empty(), "CRLF line endings in: {crlf:?}");
}
