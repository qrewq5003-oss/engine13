//! E4, stage 3 (plan §31.43.2 p. 3, §31.51): the scenario of Milan under the sentinel, with
//! no player.
//!
//! The scenario is its two files as the product keeps them
//! (`ScenarioData::parse_parts`); the real course after 1477 is the sentinel's own data
//! (`sentinel/history/milan_1477.toml`), which neither the engine nor the scenario can read.
//! The horizon is 300 ticks — 150 years, to 1627 — the one the history file is written for.
//!
//! Asserted: no flag of a single run (pinned to a bound, stagnation, a read of an absent
//! actor); every kind the engine declares possible occurs; the real course does not repeat
//! in half the runs or more (`history_lock`, brief §6); more than one polity lives at the end.
//!
//! **Printed, not asserted — and not waived** (executor's decision № 22): the polities that
//! die in every run (`Always Died`). The sentinel calls the death of the same actor in every
//! game a degeneration; it is a finding about the world, which stage 3 — the loading of the
//! scenario — does not close. The acceptance of E4 («the sentinel is clean», brief) stands:
//! at the closing of E4 it is either cured (stages 5–6: the relations in the data, the small
//! polities) or brought to the owner as a question of its own.

use std::collections::BTreeMap;

use engine2::params::Params;
use engine2::scenario::ScenarioData;
use engine2::sim::{Game, Simulation};
use sentinel::{Flag, History, Matrix, Observed, Thresholds};

const SCENARIO: &str = include_str!("../../public/geodata2/milan_1477/scenario.toml");
const PROVINCES: &str = include_str!("../../public/geodata2/milan_1477/provinces.toml");
const HISTORY: &str = include_str!("../../sentinel/history/milan_1477.toml");

/// The horizon the history file is written for (reviewer's word, plan §31.47): 150 years.
const TICKS: u32 = 300;
/// A century of play: where the stop «one polity over the whole peninsula» is read.
const CENTURY: u32 = 200;

#[test]
fn the_scenario_of_milan_stands_under_the_sentinel_without_a_player() {
    let p = Params::load().unwrap();
    let scenario = ScenarioData::parse_parts(SCENARIO, PROVINCES).expect("the two files join into the scenario");
    let start = Game::new(&scenario, p.clone(), 1).unwrap().observe();
    let history = History::load(HISTORY, &start).expect("the history names the scenario's own ids");
    assert_eq!(history.scenario, scenario.id, "the history is this scenario's");
    assert!(!history.outcomes.is_empty(), "a history with no outcome names no course and can lock nothing");

    let started = std::time::Instant::now();
    let seeds: Vec<u64> = (1..=10).collect();
    let runs = engine2::par::map(&seeds, |seed| {
        let mut g = Game::new(&scenario, p.clone(), *seed).unwrap();
        let (mut played, mut at_century) = (0u32, String::new());
        let (sentinel, summary) = sentinel::run(&mut g, TICKS, *seed, "none", Thresholds::default(), |g| {
            g.tick(&[]).unwrap();
            played += 1;
            if played == CENTURY {
                let o = g.observe();
                let mut held: BTreeMap<&str, usize> = BTreeMap::new();
                for (_, owner) in &o.owners {
                    *held.entry(owner.0.as_str()).or_insert(0) += 1;
                }
                let (largest, provinces) = held.iter().max_by_key(|(id, n)| (**n, std::cmp::Reverse(**id))).map(|(id, n)| (id.to_string(), *n)).unwrap_or_default();
                at_century = format!("{} alive, the largest ({largest}) holds {provinces} of {} provinces", o.alive.len(), o.owners.len());
            }
        });
        (sentinel.flags(), summary, at_century, g.observe().alive.len(), g.possible_kinds(TICKS))
    });
    let took = started.elapsed();

    let mut matrix = Matrix::new();
    let mut possible = Vec::new();
    let mut repeats = 0usize;
    let mut ends: Vec<sentinel::EndState> = Vec::new();
    for (seed, (flags, summary, at_century, alive_at_the_end, kinds)) in seeds.iter().zip(runs) {
        let repeated = history.matches(&summary.outcome);
        eprintln!("seed {seed}: at a century {at_century}; at the end {alive_at_the_end} alive; the real course repeated: {repeated}");
        assert!(flags.is_empty(), "seed {seed}: flags of the single run: {flags:?}");
        assert!(alive_at_the_end > 1, "seed {seed}: {alive_at_the_end} polities alive at the end");
        repeats += usize::from(repeated);
        ends.push(summary.outcome.clone());
        matrix.add(summary);
        possible = kinds;
    }
    let flags = matrix.flags(&Thresholds::default(), &possible, Some(&history));
    let never: Vec<&Flag> = flags.iter().filter(|f| matches!(f, Flag::Never { .. })).collect();
    let locked: Vec<&Flag> = flags.iter().filter(|f| matches!(f, Flag::HistoryLock { .. })).collect();
    let mut always_died: Vec<&str> = Vec::new();
    let mut always_else: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &flags {
        if let Flag::Always { kind, who } = f {
            if kind == "Died" {
                always_died.push(who.0.as_str());
            } else {
                *always_else.entry(kind.as_str()).or_insert(0) += 1;
            }
        }
    }
    eprintln!("the scenario of Milan, {} runs × {TICKS} ticks, no player, {:.0} s: the real course repeated in {repeats} of {}", seeds.len(), took.as_secs_f64(), seeds.len());
    eprintln!("Always Died — printed, not waived (decision № 22, plan §31.51): {always_died:?}");
    // The flag takes the death in every run; one run short of it is no flag and the same
    // finding. So the deaths are printed by their count: the polities of the start dead at
    // the end, and in how many runs.
    let dead_in: Vec<String> = start
        .alive
        .iter()
        .map(|id| (ends.iter().filter(|end| !end.alive.contains(id)).count(), id))
        .filter(|(runs, _)| *runs > 0)
        .map(|(runs, id)| format!("{} {runs}", id.0))
        .collect();
    eprintln!("the polities of the start dead at the end, in runs of {}: {}", seeds.len(), dead_in.join(", "));
    eprintln!("the other pairs common to every run, by kind: {always_else:?}");
    assert!(never.is_empty(), "kinds the engine declares possible that never occurred: {never:?}");
    assert!(locked.is_empty(), "the real course repeats too often without a player: {locked:?}");

    // The check is alive: a course every run did take is flagged. Taken off the runs
    // themselves — a polity alive at the end of every run, or dead at the end of every run —
    // so the data may change without this line being rewritten.
    let constant = start
        .alive
        .iter()
        .find_map(|id| {
            let alive = ends.iter().filter(|end| end.alive.contains(id)).count();
            (alive == ends.len()).then(|| ("alive", id)).or_else(|| (alive == 0).then(|| ("dead", id)))
        })
        .expect("of the scenario's polities one lives or dies in every run");
    let a_course_taken = History::load(&format!("scenario = \"{}\"\n[[outcome]]\nkind = \"{}\"\nid = \"{}\"\n", scenario.id, constant.0, constant.1 .0), &start).unwrap();
    assert!(
        matrix.flags(&Thresholds::default(), &possible, Some(&a_course_taken)).iter().any(|f| matches!(f, Flag::HistoryLock { share, runs } if *share == 1.0 && *runs == seeds.len())),
        "a course every run takes ({} {}) is not flagged",
        constant.0,
        constant.1 .0
    );
}
