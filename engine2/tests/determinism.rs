//! Acceptance of E1a (plan §3): two **processes** play the test world from one seed and
//! produce the same bytes — the saved state and every tick report, bit for bit.
//!
//! The test spawns its own binary twice in child mode (`ENGINE2_DETERMINISM_CHILD=1`) and
//! compares what the children print. One process comparing two of its own runs would not
//! catch a per-process hash seed (lesson "порядок журнала событий").

use std::process::Command;

use engine2::{testworld, to_save, Params};

const TICKS: u32 = 300;
const SEED: u64 = 0x5EED_0001;

fn play() -> String {
    let mut w = testworld::world(SEED);
    let params = Params::load().unwrap();
    let mut out = String::new();
    for _ in 0..TICKS {
        let r = w.tick(&params).unwrap();
        out.push_str(&serde_json::to_string(&r).unwrap());
        out.push('\n');
    }
    out.push_str(&to_save(&w).unwrap());
    out
}

#[test]
fn two_processes_agree_bit_for_bit() {
    if std::env::var_os("ENGINE2_DETERMINISM_CHILD").is_some() {
        // The harness prints `test name ... ` without a newline before the test's output and
        // `ok` after it; surround the payload with newlines so every payload line is whole.
        print!("\n{}\n", play());
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let run = || {
        let out = Command::new(&exe)
            .args(["two_processes_agree_bit_for_bit", "--exact", "--nocapture", "--test-threads=1"])
            .env("ENGINE2_DETERMINISM_CHILD", "1")
            .output()
            .expect("spawn child test process");
        assert!(out.status.success(), "child failed: {}", String::from_utf8_lossy(&out.stderr));
        let text = String::from_utf8(out.stdout).unwrap();
        // The harness prints its own lines around the test's output; keep only ours:
        // tick reports are JSON objects starting with `{"tick"`, the save starts with `{"format"`.
        text.lines().filter(|l| l.starts_with("{\"tick\"") || l.starts_with("{\"format\"")).collect::<Vec<_>>().join("\n")
    };
    let a = run();
    let b = run();
    assert_eq!(a.lines().count(), TICKS as usize + 1, "child output incomplete");
    assert_eq!(a, b);
    // And the parent agrees with the children.
    let mine = play();
    assert_eq!(mine.trim_end(), a);
}
