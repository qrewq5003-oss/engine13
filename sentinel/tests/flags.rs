//! Every flag on synthetic observations: each one fires on the pattern it names and stays
//! quiet otherwise. These are the sentinel's own acceptance; the engines are measured through
//! `Observed` elsewhere.

use sentinel::{Flag, History, Matrix, ObsEvent, Observation, Rating, RunSummary, Sentinel, Stock, Thresholds};
use std::collections::BTreeSet;

fn obs(tick: u32, legit: f64, people: i64) -> Observation {
    Observation {
        tick,
        alive: vec!["a".into(), "b".into()],
        owners: vec![("p1".into(), "a".into()), ("p2".into(), "b".into())],
        ratings: vec![Rating { who: "a".into(), name: "legitimacy".into(), value: legit, lo: 0.0, hi: 100.0 }],
        stocks: vec![Stock { who: "p1".into(), name: "population".into(), value: people }],
    }
}

fn th() -> Thresholds {
    Thresholds { pinned_ticks: 3, stagnation_ticks: 4, history_lock_share: 0.5 }
}

#[test]
fn pinned_to_bound_fires_after_the_threshold_and_resets_when_it_moves() {
    let mut s = Sentinel::new(th());
    for t in 0..2 {
        s.watch(&obs(t, 100.0, 10 + t as i64), &[ObsEvent::TickPlayed]);
    }
    assert!(s.flags().is_empty());
    s.watch(&obs(2, 100.0, 12), &[ObsEvent::TickPlayed]);
    assert_eq!(s.flags(), vec![Flag::PinnedToBound { who: "a".into(), rating: "legitimacy".into(), ticks: 3 }]);
    // A rating that touches the bound and leaves never accumulates three.
    let mut quiet = Sentinel::new(th());
    for t in 0..10 {
        let v = if t % 2 == 0 { 100.0 } else { 50.0 };
        quiet.watch(&obs(t, v, 10 + t as i64), &[ObsEvent::TickPlayed]);
    }
    assert!(quiet.flags().is_empty());
}

#[test]
fn bound_hit_and_read_absent_come_from_events() {
    let mut s = Sentinel::new(th());
    s.watch(
        &obs(0, 50.0, 0),
        &[
            ObsEvent::BoundHit { who: "p1".into(), stock: "population".into(), source: "Famine".into() },
            ObsEvent::BoundHit { who: "p1".into(), stock: "population".into(), source: "Famine".into() },
            ObsEvent::ReadAbsent("ghost".into()),
            ObsEvent::TickPlayed,
        ],
    );
    let flags = s.flags();
    assert!(flags.contains(&Flag::BoundHit { who: "p1".into(), stock: "population".into(), count: 2 }));
    assert!(flags.contains(&Flag::ReadAbsent { who: "ghost".into() }));
}

#[test]
fn stagnation_is_nothing_moving_and_nothing_happening() {
    let mut s = Sentinel::new(th());
    for t in 0..6 {
        s.watch(&obs(t, 50.0, 10), &[ObsEvent::TickPlayed]);
    }
    assert_eq!(s.flags(), vec![Flag::Stagnation { from_tick: 1, ticks: 5 }]);
    // An event — even without a stock moving — is not stagnation.
    let mut busy = Sentinel::new(th());
    for t in 0..6 {
        busy.watch(&obs(t, 50.0, 10), &[ObsEvent::War("a".into(), "b".into()), ObsEvent::TickPlayed]);
    }
    assert!(busy.flags().is_empty());
}

fn summary(seed: u64, strategy: &str, kinds: &[&str], end: Observation) -> RunSummary {
    RunSummary { seed, strategy: strategy.into(), kinds: kinds.iter().map(|k| k.to_string()).collect::<BTreeSet<_>>(), end }
}

#[test]
fn never_and_always_are_over_declared_kinds() {
    let mut m = Matrix::new();
    m.add(summary(1, "none", &["TickPlayed", "War"], obs(10, 50.0, 1)));
    m.add(summary(2, "none", &["TickPlayed", "War"], obs(10, 50.0, 2)));
    let possible = ["TickPlayed".to_string(), "War".to_string(), "Battle".to_string()];
    let flags = m.flags(&th(), &possible, None);
    assert!(flags.contains(&Flag::Never { kind: "Battle".into() }));
    assert!(flags.contains(&Flag::Always { kind: "War".into() }));
    assert!(!flags.iter().any(|f| matches!(f, Flag::Always { kind } if kind == "TickPlayed")));
    // An undeclared kind that appears is neither.
    let flags = m.flags(&th(), &["TickPlayed".to_string()], None);
    assert!(!flags.iter().any(|f| matches!(f, Flag::Never { .. } | Flag::Always { .. })));
}

#[test]
fn seed_and_strategy_independence_compare_end_states() {
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[], obs(10, 50.0, 7)));
    m.add(summary(2, "none", &[], obs(10, 50.0, 7)));
    m.add(summary(1, "greedy", &[], obs(10, 50.0, 7)));
    let flags = m.flags(&th(), &[], None);
    assert!(flags.contains(&Flag::SeedIndependent { strategy: "none".into(), runs: 2 }));
    assert!(flags.contains(&Flag::StrategyIndependent { seed: 1, runs: 2 }));
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[], obs(10, 50.0, 7)));
    m.add(summary(2, "none", &[], obs(10, 50.0, 8)));
    m.add(summary(1, "greedy", &[], obs(10, 50.0, 9)));
    assert!(m.flags(&th(), &[], None).is_empty());
}

#[test]
fn history_lock_counts_player_less_runs_that_repeat_the_course() {
    let h = History::parse(
        r#"
scenario = "t"
[[outcome]]
kind = "dead"
id = "b"
[[outcome]]
kind = "owns"
province = "p2"
owner = "a"
"#,
    )
    .unwrap();
    let fell = Observation { alive: vec!["a".into()], owners: vec![("p1".into(), "a".into()), ("p2".into(), "a".into())], ..obs(10, 50.0, 1) };
    let stood = obs(10, 50.0, 2);
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[], fell.clone()));
    m.add(summary(2, "none", &[], fell.clone()));
    m.add(summary(3, "none", &[], stood.clone()));
    m.add(summary(1, "greedy", &[], fell.clone()));
    let flags = m.flags(&th(), &[], Some(&h));
    assert!(flags.iter().any(|f| matches!(f, Flag::HistoryLock { runs: 3, .. })));
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[], fell));
    m.add(summary(2, "none", &[], stood.clone()));
    m.add(summary(3, "none", &[], stood));
    assert!(!m.flags(&th(), &[], Some(&h)).iter().any(|f| matches!(f, Flag::HistoryLock { .. })));
}

#[test]
fn history_file_schema_is_closed() {
    assert!(History::parse("scenario = \"t\"\noutcome = []\n").is_ok());
    assert!(History::parse("scenario = \"t\"\noutcome = []\nthreshold = 0.9\n").is_err());
    assert!(History::parse("scenario = \"t\"\n[[outcome]]\nkind = \"won\"\nid = \"a\"\n").is_err());
    let shipped = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/history/testworld.toml")).unwrap();
    let empty = History::parse(&shipped).unwrap();
    assert_eq!(empty.scenario, "testworld");
    // No course named — nothing repeats it, so an empty file cannot lock every run.
    assert!(!empty.matches(&obs(10, 50.0, 1)));
}
