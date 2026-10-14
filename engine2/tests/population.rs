//! E2 step 3 acceptance (plan §15): the population system on the generated worlds.
//!
//! - **П7:** the average growth over 100 ticks of every plague-free generated world lies in
//!   the observed band (−0.2 … +0.5 % a year); a world outside it fails with its seed.
//! - **Conservation:** migration moves people and never changes their sum — checked by the
//!   ledger invariant every tick and here by the sum over all provinces net of births and
//!   deaths.
//! - **The first randomness:** plague worlds are played with several game seeds; the matrix
//!   flags are read (plan §13). Plague occurs in some run of every plague world (no `Never`),
//!   and the stocks differ across game seeds — randomness reaches the world. The outcome
//!   (who is alive, who owns what) was seed-independent until E2 step 6a: the house and the
//!   throne is the first system that changes who is alive without anyone's order (a polity
//!   whose houses die out falls), so that flag is no longer asserted here.
//! - **Plague and П7:** one band for every world (owner's decision on PR #282): a plague
//!   world's growth over 100 ticks lies in the same band, and the mean over plague worlds is
//!   reported.

use engine2::sim::{Game, Simulation};
use engine2::{worldgen, Disease, Event, Params, ScenarioData, Source};
use sentinel::{Flag, Matrix, Observed, Thresholds};

fn params() -> Params {
    Params::load().unwrap()
}

fn total_people(g: &Game) -> i64 {
    g.world().provinces.keys().map(|p| g.world().population(*p)).sum()
}

/// Yearly growth over `ticks` ticks from `start` to `end`.
fn yearly_growth(start: i64, end: i64, ticks: u32) -> f64 {
    let years = f64::from(ticks) / 2.0;
    (end as f64 / start as f64).powf(1.0 / years) - 1.0
}

fn play(scenario: &ScenarioData, seed: u64, ticks: u32) -> (Game, Vec<engine2::TickReport>) {
    let mut g = Game::new(scenario, params(), seed).unwrap();
    let mut reports = Vec::new();
    for _ in 0..ticks {
        reports.push(g.tick(&[]).unwrap());
    }
    (g, reports)
}

#[test]
fn growth_of_plague_free_worlds_lies_in_the_observed_band_p7() {
    let p = params();
    let band = p.entries["population.observed_growth_per_year"].band.unwrap();
    for seed in 1..=20u64 {
        let mut s = worldgen::generate(seed, &p);
        s.diseases.clear();
        let g0 = Game::new(&s, params(), 1).unwrap();
        let start = total_people(&g0);
        let (g, _) = play(&s, 1, 100);
        let growth = yearly_growth(start, total_people(&g), 100);
        assert!((band[0]..=band[1]).contains(&growth), "seed {seed}: growth {growth:.4}/year outside П7 {band:?}");
    }
}

#[test]
fn migration_conserves_people_exactly_and_moves_toward_emptier_neighbours() {
    let p = params();
    let mut moved_any = false;
    for seed in 1..=20u64 {
        let mut s = worldgen::generate(seed, &p);
        s.diseases.clear();
        let g0 = Game::new(&s, params(), 1).unwrap();
        let before = total_people(&g0);
        let (g, reports) = play(&s, 1, 50);
        let after = total_people(&g);
        let people_postings = || reports.iter().flat_map(|r| &r.postings).filter(|q| matches!(q.account, engine2::Account::Population(_)));
        let migration_net: i64 = people_postings().filter(|q| matches!(q.source, Source::MigrationIn | Source::MigrationOut)).map(|q| q.milli).sum();
        assert_eq!(migration_net, 0, "seed {seed}: migration created or destroyed thousandths of people");
        // The sum moves only by births and deaths, never by moves: the postings explain the
        // change to the carried thousandths (at most one person per province).
        let explained: i64 = people_postings().map(|q| q.milli).sum::<i64>() / 1000;
        assert!((after - before - explained).abs() <= g.world().provinces.len() as i64, "seed {seed}: {before} + {explained} ≠ {after}");
        let famine_deaths: i64 = people_postings().filter(|q| q.source == Source::Famine).map(|q| -q.milli).sum::<i64>() / 1000;
        assert!(famine_deaths <= g.world().provinces.len() as i64, "seed {seed}: {famine_deaths} famine deaths in a world nothing pushes over capacity");
        if reports.iter().any(|r| r.events.iter().any(|e| matches!(e.event, Event::Migration { .. }))) {
            moved_any = true;
        }
    }
    assert!(moved_any, "no generated world saw any migration in 50 ticks");
}

#[test]
fn plague_worlds_are_played_with_several_game_seeds_and_the_matrix_is_read() {
    let p = params();
    let band = p.entries["population.observed_growth_per_year"].band.unwrap();
    let mut plague_worlds = 0;
    let mut growths = Vec::new();
    for world_seed in 1..=20u64 {
        let s = worldgen::generate(world_seed, &p);
        if !s.diseases.contains(&Disease::Plague) {
            continue;
        }
        plague_worlds += 1;
        let mut m = Matrix::new();
        let mut stocks = Vec::new();
        let mut possible = Vec::new();
        for game_seed in [1u64, 2, 3] {
            let mut g = Game::new(&s, params(), game_seed).unwrap();
            let start = total_people(&g);
            let (sen, summary) = sentinel::run(&mut g, 100, game_seed, "none", Thresholds::default(), |g| {
                g.tick(&[]).unwrap();
            });
            assert!(sen.flags().is_empty(), "world {world_seed} game seed {game_seed}: {:?}", sen.flags());
            possible = g.possible_kinds();
            stocks.push(g.observe().stocks);
            growths.push(yearly_growth(start, total_people(&g), 100));
            m.add(summary);
        }
        let flags = m.flags(&Thresholds::default(), &possible, None);
        assert!(possible.contains(&"Plague".to_string()));
        for (i, growth) in growths.iter().rev().take(3).enumerate() {
            assert!((band[0]..=band[1]).contains(growth), "world {world_seed} game seed {}: growth {growth:.4}/year outside П7 {band:?}", 3 - i);
        }
        assert!(!flags.iter().any(|f| matches!(f, Flag::Never { .. })), "world {world_seed}: plague declared but never struck in 3 × 100 ticks");
        assert!(stocks[0] != stocks[1] || stocks[1] != stocks[2], "world {world_seed}: three game seeds, identical stocks — randomness did not reach the world");
        // Since E2 step 6a the outcome may differ by seed: a polity whose houses die out falls
        // (plan §18). Before that step the `SeedIndependent` flag was asserted here.
    }
    assert!(plague_worlds >= 3, "only {plague_worlds} plague worlds among 20");
    let mean = growths.iter().sum::<f64>() / growths.len() as f64;
    let (lo, hi) = growths.iter().fold((f64::MAX, f64::MIN), |(lo, hi), g| (lo.min(*g), hi.max(*g)));
    eprintln!("plague worlds: {plague_worlds}; growth per year over 100 ticks: mean {mean:.4}, min {lo:.4}, max {hi:.4}");
    assert!((band[0]..=band[1]).contains(&mean), "plague worlds: mean growth {mean:.4}/year outside П7 {band:?}");
}

#[test]
fn famine_strikes_a_province_above_its_capacity_until_it_fits() {
    let p = params();
    let mut s = worldgen::generate(3, &p);
    s.diseases.clear();
    let mut g = Game::new(&s, params(), 1).unwrap();
    let prov = *g.world().provinces.keys().next().unwrap();
    let cap = g.world().provinces[&prov].capacity as i64;
    let now = g.world().population(prov);
    g.world_mut().post(engine2::Account::Population(prov), Source::MigrationIn, (cap * 2 - now) * 1000).unwrap();
    let mut famines = 0;
    for _ in 0..40 {
        let r = g.tick(&[]).unwrap();
        famines += r.events.iter().filter(|e| matches!(e.event, Event::Famine { province, .. } if province == prov)).count();
        if g.world().population(prov) <= cap {
            break;
        }
    }
    assert!(famines > 0);
    assert!(g.world().population(prov) <= cap, "famine must bring the province back under its capacity");
}
