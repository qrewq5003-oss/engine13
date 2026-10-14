//! Acceptance ground of E2 (brief §0.3 item 3, plan §12 item 11, §13): generated worlds and
//! the extreme worlds all compile through the schema, build, and play without single-run
//! sentinel flags; and the family of generated maps contains every shape movement and
//! supply must survive (owner's review of PR #280). In CI: 20 fixed seeds and the extreme
//! worlds, 50 ticks. Before merging a PR that changes the world: 200 seeds, 300 ticks —
//! `cargo test -p engine2 --all-features --test worldgen -- --ignored`. A failure names its
//! seed.
//!
//! Matrix flags are not in this acceptance yet: every world plays with one game seed, since
//! no system draws randomness. The first system that does must play each world with several
//! game seeds and check the matrix (plan §13).

use std::collections::{BTreeMap, BTreeSet};

use engine2::sim::{Game, Simulation};
use engine2::{worldgen, Params, ScenarioData};
use sentinel::{Observed, Thresholds};

fn play(label: &str, scenario: &ScenarioData, ticks: u32) {
    let params = Params::load().unwrap();
    let mut g = match Game::new(scenario, params, 1) {
        Ok(g) => g,
        Err(e) => panic!("{label}: cannot build: {e}"),
    };
    let mut died = 0usize;
    let (s, _) = sentinel::run(&mut g, ticks, 1, "none", Thresholds::default(), |g| {
        match g.tick(&[]) {
            Ok(r) => died += r.events.iter().filter(|e| matches!(e.event, engine2::Event::ActorDied { .. })).count(),
            Err(e) => panic!("{label}: tick failed: {e}"),
        }
    });
    let flags = s.flags();
    assert!(flags.is_empty(), "{label}: sentinel flags {flags:?}");
    let o = g.observe();
    // Without war nothing conquers; a polity leaves only when its last house dies out, and
    // that is an `ActorDied` on the record (plan §18), never a silent vanishing.
    assert_eq!(o.alive.len() + died, scenario.actors.len(), "{label}: an actor vanished without a system to kill it");
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

// ---------- shapes of the land graph ----------

struct Graph<'a> {
    s: &'a ScenarioData,
}

impl Graph<'_> {
    fn degree(&self, p: &str) -> usize {
        self.s.provinces[p].neighbours.len()
    }

    /// Components of the land graph restricted to `keep`.
    fn components(&self, keep: &dyn Fn(&str) -> bool) -> usize {
        let nodes: Vec<&String> = self.s.provinces.keys().filter(|k| keep(k)).collect();
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut count = 0;
        for start in &nodes {
            if !seen.insert(start.as_str()) {
                continue;
            }
            count += 1;
            let mut stack = vec![start.as_str()];
            while let Some(c) = stack.pop() {
                for n in &self.s.provinces[c].neighbours {
                    if keep(n) && seen.insert(n.as_str()) {
                        stack.push(n.as_str());
                    }
                }
            }
        }
        count
    }

    /// A province whose removal splits its component: a chokepoint.
    fn has_chokepoint(&self) -> bool {
        let base = self.components(&|_| true);
        self.s.provinces.keys().any(|cut| self.components(&|k| k != cut) > base)
    }

    /// A dead end reached through a chain of at-most-two-neighbour provinces: a peninsula.
    fn has_peninsula(&self) -> bool {
        self.s.provinces.iter().any(|(k, p)| {
            p.neighbours.len() == 1 && {
                let next = &p.neighbours[0];
                self.degree(next) <= 2 && *next != *k
            }
        })
    }

    /// A polity whose provinces are not land-connected: an overseas holding or an enclave.
    fn has_split_polity(&self) -> bool {
        let mut by_owner: BTreeMap<&str, usize> = BTreeMap::new();
        for a in self.s.actors.keys() {
            by_owner.insert(a, self.components(&|k| self.s.provinces[k].owner == *a));
        }
        by_owner.values().any(|c| *c >= 2)
    }
}

#[test]
fn the_family_of_maps_contains_every_shape_movement_must_survive() {
    let params = Params::load().unwrap();
    let (mut deg1, mut deg6, mut choke, mut peninsula, mut split, mut islands) = (0, 0, 0, 0, 0, 0);
    const N: u64 = 200;
    for seed in 1..=N {
        let s = worldgen::generate(seed, &params);
        let g = Graph { s: &s };
        let degrees: Vec<usize> = s.provinces.values().map(|p| p.neighbours.len()).collect();
        if degrees.contains(&1) {
            deg1 += 1;
        }
        if degrees.iter().any(|d| *d >= 6) {
            deg6 += 1;
        }
        if g.has_chokepoint() {
            choke += 1;
        }
        if g.has_peninsula() {
            peninsula += 1;
        }
        if g.has_split_polity() {
            split += 1;
        }
        if g.components(&|_| true) >= 2 {
            islands += 1;
        }
    }
    let at_least = |what: &str, n: u64, min: u64| assert!(n >= min, "{what}: {n} of {N} worlds, need ≥ {min}");
    at_least("degree 1", deg1, N / 4);
    at_least("degree ≥ 6", deg6, N / 4);
    at_least("chokepoint", choke, N / 4);
    at_least("peninsula", peninsula, N / 10);
    at_least("polity without a land link (overseas or enclave)", split, N / 5);
    at_least("more than one landmass", islands, N / 5);
}

#[test]
fn extreme_worlds_have_their_shapes() {
    let params = Params::load().unwrap();
    let worlds = worldgen::extreme_worlds(&params);
    let get = |name: &str| worlds.iter().find(|(n, _)| *n == name).map(|(_, s)| s).unwrap();
    assert!(Graph { s: get("chokepoint") }.has_chokepoint());
    assert!(Graph { s: get("chokepoint") }.has_peninsula());
    assert!(Graph { s: get("colony_and_enclave") }.has_split_polity());
    assert!(Graph { s: get("isolated_island") }.components(&|_| true) >= 2);
}
