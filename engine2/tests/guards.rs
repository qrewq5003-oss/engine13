//! Convention guards of E1a (plan §9, items 2 and 5). They read the crate's own sources.
//!
//! - no `HashMap`/`HashSet` in `engine2/src`: iteration order must not depend on a hash;
//! - no `#[serde(default)]` in `engine2/src`: an incompatible save bumps `SAVE_FORMAT`.

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

#[test]
fn no_serde_defaults_in_engine_sources() {
    let mut hits = Vec::new();
    for (path, text) in sources() {
        for (n, line) in code_lines(&text) {
            if line.contains("serde(default") {
                hits.push(format!("{}:{}: {}", path.display(), n, line.trim()));
            }
        }
    }
    assert!(hits.is_empty(), "serde defaults in a saved type:\n{}", hits.join("\n"));
}
