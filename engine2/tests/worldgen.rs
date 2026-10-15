//! Acceptance ground of E2 (brief §0.3 item 3, plan §12 item 11, §13): generated worlds and
//! the extreme worlds all compile through the schema, build, and play without single-run
//! sentinel flags; and the family of generated maps contains every shape movement and
//! supply must survive (owner's review of PR #280). In the debug run: 20 fixed seeds and
//! the extreme worlds, 50 ticks. The full matrix — 200 seeds, 300 ticks — is `#[ignore]`d
//! and runs in CI as its own line in release (owner on #285: 18 s with the build, so it
//! depends on nobody's memory): `cargo test --release -p engine2 --all-features --test
//! worldgen -- --ignored`. A failure names its seed.
//!
//! Matrix flags are not in this acceptance yet: every world plays with one game seed, since
//! no system draws randomness. The first system that does must play each world with several
//! game seeds and check the matrix (plan §13).

use std::collections::{BTreeMap, BTreeSet};

use engine2::sim::{Game, Simulation};
use engine2::{worldgen, Params, ScenarioData};
use sentinel::{Flag, Matrix, Observed, RunSummary, Thresholds};

fn play(label: &str, scenario: &ScenarioData, ticks: u32) {
    let _ = play_summary(label, scenario, ticks);
}

/// One world through the sentinel: its run summary for the matrix and the kinds it
/// declares possible within `ticks`.
fn play_summary(label: &str, scenario: &ScenarioData, ticks: u32) -> (RunSummary, Vec<String>) {
    let params = Params::load().unwrap();
    let mut g = match Game::new(scenario, params, 1) {
        Ok(g) => g,
        Err(e) => panic!("{label}: cannot build: {e}"),
    };
    // What the world can produce within the run is read at the start: a channel that has
    // done its work by the end (a province converted) is no longer there to be seen.
    let possible = g.possible_kinds(ticks);
    // A polity falls only by conquest (E3 (б): the bots make war): every fall in the tick
    // its last province was taken.
    let (mut died, mut died_without_conquest) = (0usize, 0usize);
    let (s, summary) = sentinel::run(&mut g, ticks, 1, "none", Thresholds::default(), |g| {
        match g.tick(&[]) {
            Ok(r) => {
                let conquered: Vec<engine2::ActorId> = r.events.iter().filter_map(|e| match e.event {
                    engine2::Event::ProvinceChangedHands { from, how: engine2::event::Transfer::Conquest, .. } => Some(from),
                    _ => None,
                }).collect();
                for e in &r.events {
                    if let engine2::Event::ActorDied { actor } = e.event {
                        died += 1;
                        if !conquered.contains(&actor) {
                            died_without_conquest += 1;
                        }
                    }
                }
            }
            Err(e) => panic!("{label}: tick failed: {e}"),
        }
    });
    let flags = s.flags();
    assert!(flags.is_empty(), "{label}: sentinel flags {flags:?}");
    let o = g.observe();
    // A polity whose last house dies out raises a new one from its people (plan §18.5,
    // owner's rule on #285); a rising bears polities and kills none; only a war takes the
    // last province.
    assert_eq!(died_without_conquest, 0, "{label}: {died_without_conquest} polities fell without a conquest");
    let starting_alive = scenario.actors.keys().filter(|k| o.alive.iter().any(|a| a.0 == **k)).count();
    assert!(starting_alive + died >= scenario.actors.len(), "{label}: an actor vanished without a system to kill it");
    (summary, possible)
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

/// The full matrix before a world-changing merge (200 seeds, 300 ticks): no single-run
/// flag in any world, and over the matrix no `Never` — every kind some world declares
/// possible within 150 years (a conversion, an adoption) happens in some world (owner's
/// rule on #288). `Always` is not read here: the generated worlds share their ids, so a
/// pair common to all of them would say nothing. Run in release in CI.
#[test]
#[ignore]
fn two_hundred_seeds_three_hundred_ticks_no_flags() {
    let params = Params::load().unwrap();
    let mut m = Matrix::new();
    let mut possible: Vec<String> = Vec::new();
    for seed in 1..=200u64 {
        let s = worldgen::generate(seed, &params);
        let (summary, kinds) = play_summary(&format!("seed {seed}"), &s, 300);
        for k in kinds {
            if !possible.contains(&k) {
                possible.push(k);
            }
        }
        m.add(summary);
    }
    for (name, s) in worldgen::extreme_worlds(&params) {
        play(name, &s, 300);
    }
    assert!(possible.contains(&"Converted".to_string()) && possible.contains(&"Adopted".to_string()), "the matrix declares conversions and adoptions possible: {possible:?}");
    assert!(["Born", "Revolt", "Secession"].iter().all(|k| possible.contains(&k.to_string())), "the matrix declares risings possible by the horizon: {possible:?}");
    let never: Vec<Flag> = m.flags(&Thresholds::default(), &possible, None).into_iter().filter(|f| matches!(f, Flag::Never { .. })).collect();
    assert!(never.is_empty(), "declared kinds that happened in no world of the matrix: {never:?}");
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
