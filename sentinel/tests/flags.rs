//! Every flag on synthetic observations: each one fires on the pattern it names and stays
//! quiet otherwise. These are the sentinel's own acceptance; the engines are measured through
//! `Observed` elsewhere.

use sentinel::{EndState, Flag, History, HistoryError, Matrix, ObsEvent, ObsId, Observation, Rating, RunSummary, Sentinel, Stock, Thresholds};

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

/// A run summary straight from events, the way `Sentinel::summary` builds it.
fn summary(seed: u64, strategy: &str, events: &[ObsEvent], end: &Observation) -> RunSummary {
    let mut s = Sentinel::new(th());
    s.watch(end, events);
    s.summary(seed, strategy)
}

fn kinds(list: &[&str]) -> Vec<String> {
    list.iter().map(|k| k.to_string()).collect()
}

#[test]
fn never_is_over_declared_kinds() {
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[ObsEvent::War("a".into(), "b".into())], &obs(10, 50.0, 1)));
    m.add(summary(2, "none", &[ObsEvent::War("b".into(), "a".into())], &obs(10, 50.0, 2)));
    let flags = m.flags(&th(), &kinds(&["TickPlayed", "War", "Battle"]), None);
    assert!(flags.contains(&Flag::Never { kind: "Battle".into() }));
    assert!(!flags.iter().any(|f| matches!(f, Flag::Never { kind } if kind == "War")));
    // An undeclared kind that never appears is not the engine's promise broken.
    let flags = m.flags(&th(), &kinds(&["War"]), None);
    assert!(!flags.iter().any(|f| matches!(f, Flag::Never { .. })));
}

#[test]
fn always_is_by_kind_and_participant_not_by_kind() {
    // Battles in every run, in different places: normal.
    let mut m = Matrix::new();
    let battle = |place: &str| ObsEvent::Battle { place: place.into(), a: "a".into(), b: "b".into(), winner: None };
    m.add(summary(1, "none", &[battle("p1")], &obs(10, 50.0, 1)));
    m.add(summary(2, "none", &[battle("p2")], &obs(10, 50.0, 2)));
    m.add(summary(3, "none", &[battle("p3")], &obs(10, 50.0, 3)));
    assert!(!m.flags(&th(), &[], None).iter().any(|f| matches!(f, Flag::Always { .. })));
    // The same actor dying in every run: degeneration.
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[battle("p1"), ObsEvent::Died("b".into())], &obs(10, 50.0, 1)));
    m.add(summary(2, "none", &[battle("p2"), ObsEvent::Died("b".into())], &obs(10, 50.0, 2)));
    m.add(summary(3, "none", &[ObsEvent::Died("b".into())], &obs(10, 50.0, 3)));
    let flags = m.flags(&th(), &[], None);
    assert_eq!(flags.iter().filter(|f| matches!(f, Flag::Always { .. })).count(), 1);
    assert!(flags.contains(&Flag::Always { kind: "Died".into(), who: "b".into() }));
    // TickPlayed has no participant and never counts (the seed flag here is right: the
    // two outcomes are the same).
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[ObsEvent::TickPlayed], &obs(10, 50.0, 1)));
    m.add(summary(2, "none", &[ObsEvent::TickPlayed], &obs(10, 50.0, 2)));
    assert!(!m.flags(&th(), &[], None).iter().any(|f| matches!(f, Flag::Always { .. })));
}

#[test]
fn seed_and_strategy_independence_compare_outcomes_not_stocks() {
    // Different stocks, same outcome (alive, owners): still independent — the flag fires.
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[], &obs(10, 50.0, 7)));
    m.add(summary(2, "none", &[], &obs(10, 60.0, 9)));
    m.add(summary(1, "greedy", &[], &obs(10, 70.0, 11)));
    let flags = m.flags(&th(), &[], None);
    assert!(flags.contains(&Flag::SeedIndependent { strategy: "none".into(), runs: 2 }));
    assert!(flags.contains(&Flag::StrategyIndependent { seed: 1, runs: 2 }));
    // A different outcome in one run: no flag.
    let fell = Observation { alive: vec!["a".into()], owners: vec![("p1".into(), "a".into()), ("p2".into(), "a".into())], ..obs(10, 50.0, 7) };
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[], &obs(10, 50.0, 7)));
    m.add(summary(2, "none", &[], &fell));
    m.add(summary(1, "greedy", &[], &fell));
    assert!(!m.flags(&th(), &[], None).iter().any(|f| matches!(f, Flag::SeedIndependent { .. } | Flag::StrategyIndependent { .. })));
}

const COURSE: &str = r#"
scenario = "t"
[[outcome]]
kind = "dead"
id = "b"
[[outcome]]
kind = "owns"
province = "p2"
owner = "a"
"#;

#[test]
fn history_lock_counts_player_less_runs_that_repeat_the_course() {
    let h = History::load(COURSE, &obs(0, 50.0, 1)).unwrap();
    let fell = Observation { alive: vec!["a".into()], owners: vec![("p1".into(), "a".into()), ("p2".into(), "a".into())], ..obs(10, 50.0, 1) };
    let stood = obs(10, 50.0, 2);
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[], &fell));
    m.add(summary(2, "none", &[], &fell));
    m.add(summary(3, "none", &[], &stood));
    m.add(summary(1, "greedy", &[], &fell));
    let flags = m.flags(&th(), &[], Some(&h));
    assert!(flags.iter().any(|f| matches!(f, Flag::HistoryLock { runs: 3, .. })));
    let mut m = Matrix::new();
    m.add(summary(1, "none", &[], &fell));
    m.add(summary(2, "none", &[], &stood));
    m.add(summary(3, "none", &[], &stood));
    assert!(!m.flags(&th(), &[], Some(&h)).iter().any(|f| matches!(f, Flag::HistoryLock { .. })));
}

#[test]
fn history_ids_are_checked_against_the_start_and_the_schema_is_closed() {
    let start = obs(0, 50.0, 1);
    // A typo in `dead` is an error, not an outcome that is true by itself.
    let typo = COURSE.replacen("id = \"b\"", "id = \"bb\"", 1);
    assert_eq!(History::load(&typo, &start).unwrap_err(), HistoryError::UnknownId(ObsId("bb".into())));
    let typo = COURSE.replacen("province = \"p2\"", "province = \"p9\"", 1);
    assert_eq!(History::load(&typo, &start).unwrap_err(), HistoryError::UnknownId(ObsId("p9".into())));
    let typo = COURSE.replacen("owner = \"a\"", "owner = \"zz\"", 1);
    assert_eq!(History::load(&typo, &start).unwrap_err(), HistoryError::UnknownId(ObsId("zz".into())));
    assert!(History::load(COURSE, &start).is_ok());
    assert!(History::parse("scenario = \"t\"\noutcome = []\nthreshold = 0.9\n").is_err());
    assert!(History::parse("scenario = \"t\"\n[[outcome]]\nkind = \"won\"\nid = \"a\"\n").is_err());
    let shipped = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/history/testworld.toml")).unwrap();
    let empty = History::load(&shipped, &start).unwrap();
    assert_eq!(empty.scenario, "testworld");
    // No course named — nothing repeats it, so an empty file cannot lock every run.
    assert!(!empty.matches(&EndState { alive: vec![], owners: vec![] }));
    assert!(!empty.matches(&start.outcome()));
}

#[test]
fn no_engine_or_shell_source_refers_to_the_history_files() {
    // The real course is the sentinel's alone (plan §8): neither engine2 nor the shell may
    // know where it lives.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut hits = Vec::new();
    for dir in ["engine2/src", "src-tauri/src"] {
        let mut stack = vec![root.join(dir)];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(text) = std::fs::read_to_string(&p) {
                    if text.contains("sentinel/history") || (text.contains("history/") && text.contains("sentinel")) {
                        hits.push(p.display().to_string());
                    }
                }
            }
        }
    }
    assert!(hits.is_empty(), "sources refer to the sentinel's history files: {hits:?}");
}
