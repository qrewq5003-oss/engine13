//! E1c acceptance (plan §11): the sentinel's short run in CI — the test world, 50 ticks,
//! three seeds, no single-run flags. The E1 statement «the end state does not depend on the
//! seed» lived here until the population system brought the first draw (plague); the matrix
//! over game seeds now lives in `tests/population.rs`.

use engine2::sim::{Game, Simulation};
use engine2::{testworld, Params};
use sentinel::{Flag, Observed, Thresholds};

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
