//! E1c acceptance (plan §11): the sentinel's short run in CI — the test world, 50 ticks,
//! three seeds, no single-run flags. And the honest statement of what E1 is: with no system
//! drawing randomness yet, the end state does not depend on the seed, and the matrix says so.
//! The first system of E2 that uses a draw must make `seed_independence_is_still_true_in_e1`
//! fail, and then it is deleted — that is the point of asserting it now.

use engine2::sim::{Game, Simulation};
use engine2::{testworld, Params};
use sentinel::{Flag, History, Matrix, Observed, Thresholds};

fn play(seed: u64, ticks: u32) -> (Vec<Flag>, sentinel::RunSummary) {
    let mut g = Game::new(&testworld::five_states_scenario(), Params::load().unwrap(), seed).unwrap();
    let (s, summary) = sentinel::run(&mut g, ticks, seed, "none", Thresholds::default(), |g| {
        g.tick(&[]).unwrap();
    });
    (s.flags(), summary)
}

#[test]
fn test_world_fifty_ticks_three_seeds_no_single_run_flags() {
    for seed in [1, 2, 3] {
        let (flags, _) = play(seed, 50);
        assert!(flags.is_empty(), "seed {seed}: {flags:?}");
    }
}

#[test]
fn observation_is_on_the_worlds_string_ids() {
    let g = Game::new(&testworld::five_states_scenario(), Params::load().unwrap(), 1).unwrap();
    let o = g.observe();
    assert_eq!(o.alive[0].0, "s0");
    assert_eq!(o.owners[0], ("p00".into(), "s0".into()));
    assert!(o.stocks.iter().any(|s| s.who.0 == "p00" && s.name == "population" && s.value == 6000));
    assert!(o.ratings.iter().all(|r| r.name == "legitimacy"));
}

#[test]
fn seed_independence_is_still_true_in_e1() {
    let mut m = Matrix::new();
    let mut g = None;
    for seed in [1, 2, 3] {
        let (_, summary) = play(seed, 50);
        m.add(summary);
        g.get_or_insert_with(|| Game::new(&testworld::five_states_scenario(), Params::load().unwrap(), seed).unwrap());
    }
    let g = g.unwrap();
    let possible = g.possible_kinds();
    // The history file is the sentinel's: engine2 does not know where it lives, the test
    // reads it through the sentinel crate's manifest directory.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("sentinel").join("history").join("testworld.toml");
    let history = History::load(&std::fs::read_to_string(path).unwrap(), &g.observe()).unwrap();
    let flags = m.flags(&Thresholds::default(), &possible, Some(&history));
    assert_eq!(flags, vec![Flag::SeedIndependent { strategy: "none".into(), runs: 3 }]);
}
