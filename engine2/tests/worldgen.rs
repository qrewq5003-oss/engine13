//! Acceptance ground of E2 (brief §0.3 item 3, plan §12 item 11): generated worlds and the
//! extreme worlds all compile through the schema, build, and play without single-run
//! sentinel flags. In CI: 20 fixed seeds and the extreme worlds, 50 ticks. Before merging a
//! PR that changes the world: 200 seeds, 300 ticks — `cargo test -p engine2 --features
//! sentinel --test worldgen -- --ignored`. A failure names its seed.

use engine2::sim::{Game, Simulation};
use engine2::{worldgen, Params, ScenarioData};
use sentinel::{Observed, Thresholds};

fn play(label: &str, scenario: &ScenarioData, ticks: u32) {
    let params = Params::load().unwrap();
    let mut g = match Game::new(scenario, params, 1) {
        Ok(g) => g,
        Err(e) => panic!("{label}: cannot build: {e}"),
    };
    let (s, _) = sentinel::run(&mut g, ticks, 1, "none", Thresholds::default(), |g| {
        if let Err(e) = g.tick(&[]) {
            panic!("{label}: tick failed: {e}");
        }
    });
    let flags = s.flags();
    assert!(flags.is_empty(), "{label}: sentinel flags {flags:?}");
    // The world is alive and observable at the end.
    let o = g.observe();
    assert_eq!(o.alive.len(), scenario.actors.len(), "{label}: an actor vanished without a system to kill it");
}

#[test]
fn twenty_seeds_fifty_ticks_no_flags() {
    let params = Params::load().unwrap();
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &params);
        play(&format!("seed {seed}"), &s, 50);
    }
}

#[test]
fn extreme_worlds_fifty_ticks_no_flags() {
    let params = Params::load().unwrap();
    for (name, s) in worldgen::extreme_worlds(&params) {
        play(name, &s, 50);
    }
}

/// The full matrix before a world-changing merge (200 seeds, 300 ticks). Ignored in CI.
#[test]
#[ignore]
fn two_hundred_seeds_three_hundred_ticks_no_flags() {
    let params = Params::load().unwrap();
    for seed in 1..=200u64 {
        let s = worldgen::generate(seed, &params);
        play(&format!("seed {seed}"), &s, 300);
    }
    for (name, s) in worldgen::extreme_worlds(&params) {
        play(name, &s, 300);
    }
}
