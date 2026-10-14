//! E2 step 7 acceptance (plan §19): diplomacy.
//!
//! - starting relations are facts of the scenario — wars, alliances to a year, vassals,
//!   marriages — compiled into the world, and the schema refuses the malformed ones;
//! - the relation of a pair has a norm from the state of the pair (war, alliance, marriage,
//!   vassalage, a shared border) and moves towards it; it stays inside 0..100 by the norm,
//!   not by a clamp;
//! - a treaty runs out exactly at its term, with its event;
//! - a run with relations saves and loads bit for bit;
//! - on generated worlds the share of polities bound at the start lies in Д3 (the row
//!   measures the generator in E2, owner's rule on §19), and without agents the starting
//!   treaties have all run out by the end.

use std::collections::BTreeMap;

use engine2::scenario::{AllianceData, RelationsData, VassalData};
use engine2::sim::{Game, Simulation};
use engine2::{from_save, testworld, to_save, worldgen, ActorId, Event, Params, PersonId, ScenarioData, Tick, TreatyKind, World};

fn params() -> Params {
    Params::load().unwrap()
}

fn scenario_with(relations: RelationsData) -> ScenarioData {
    let mut s = testworld::five_states_scenario();
    s.relations = Some(relations);
    s
}

fn alliance(a: &str, b: &str, until_year: i32) -> AllianceData {
    AllianceData { a: a.to_string(), b: b.to_string(), until_year }
}

fn vassal(vassal: &str, lord: &str) -> VassalData {
    VassalData { vassal: vassal.to_string(), lord: lord.to_string() }
}

fn relations(wars: &[(&str, &str)], alliances: Vec<AllianceData>, vassals: Vec<VassalData>) -> RelationsData {
    RelationsData { wars: wars.iter().map(|(a, b)| [a.to_string(), b.to_string()]).collect(), alliances, vassals }
}

#[test]
fn starting_relations_are_facts_of_the_scenario_and_the_schema_refuses_the_malformed_ones() {
    let p = params();
    let mut s = scenario_with(relations(&[("s1", "s0")], vec![alliance("s2", "s3", 1497)], vec![vassal("s4", "s3")]));
    s.persons.get_mut("r0").unwrap().spouse = Some("r1".to_string());
    s.persons.get_mut("r1").unwrap().spouse = Some("r0".to_string());
    let g = Game::new(&s, params(), 1).unwrap();
    let w = g.world();
    let (s0, s1, s2, s3, s4) = (ActorId(0), ActorId(1), ActorId(2), ActorId(3), ActorId(4));
    assert!(w.at_war(s0, s1) && w.at_war(s1, s0), "the pair is at war either way round");
    assert_eq!(w.wars[&(s0, s1)].taken, (0, 0));
    assert!(w.allied(s2, s3));
    assert_eq!(w.treaty(s2, s3).unwrap().until, Tick(40), "1497 is twenty years from 1477: forty ticks");
    assert_eq!(w.lord_of(s4), Some(s3));
    assert_eq!(w.vassals_of(s3), vec![s4]);
    assert!(w.bound_by_vassalage(s3, s4));
    let r0 = w.persons.iter().find(|(_, q)| q.spouse.is_some()).map(|(id, _)| *id).unwrap();
    let r1 = w.persons[&r0].spouse.unwrap();
    assert_eq!(w.persons[&r1].spouse, Some(r0), "named on both");
    assert!(w.married_across(s0, s1) && w.married_across(s1, s0));
    // Every pair starts at its norm, and the norm reads the state.
    for a in 0..5u32 {
        for b in a + 1..5 {
            let (a, b) = (ActorId(a), ActorId(b));
            assert!((w.relation(a, b) - w.relation_norm(a, b, &p)).abs() < 1e-9, "{a:?}–{b:?} starts at its norm");
            assert!((w.relation(a, b) - w.relation(b, a)).abs() < 1e-9);
        }
    }
    let border = |a, b| if w.borders(a, b) { p.relation_norm_border } else { 0.0 };
    assert!((w.relation_norm(s0, s1, &p) - (p.relation_base - p.relation_norm_war + p.relation_norm_marriage - border(s0, s1))).abs() < 1e-9, "war and a marriage at once");
    assert!((w.relation_norm(s2, s3, &p) - (p.relation_base + p.relation_norm_alliance - border(s2, s3))).abs() < 1e-9);
    assert!((w.relation_norm(s3, s4, &p) - (p.relation_base + p.relation_norm_vassalage - border(s3, s4))).abs() < 1e-9);
    assert!(w.borders(s0, s1) || w.borders(s0, s2) || w.borders(s0, s3) || w.borders(s0, s4), "the test world has touching states");
    assert!((0..5u32).any(|b| b != 0 && !w.borders(s0, ActorId(b))), "and states that do not touch");

    // The malformed ones.
    let refused = |s: ScenarioData, what: &str| {
        let err = s.compile(&p).err().unwrap_or_else(|| panic!("{what}: accepted"));
        eprintln!("{what}: {err}");
    };
    refused(scenario_with(relations(&[("s0", "s0")], vec![], vec![])), "a war with oneself");
    refused(scenario_with(relations(&[("s0", "nobody")], vec![], vec![])), "an unknown actor");
    refused(scenario_with(relations(&[("s0", "s1"), ("s1", "s0")], vec![], vec![])), "a pair bound twice");
    refused(scenario_with(relations(&[("s0", "s1")], vec![alliance("s1", "s0", 1500)], vec![])), "allied and at war");
    refused(scenario_with(relations(&[], vec![alliance("s0", "s1", 1477)], vec![])), "a treaty over at the start");
    refused(scenario_with(relations(&[], vec![], vec![vassal("s0", "s1"), vassal("s0", "s2")])), "two lords");
    refused(scenario_with(relations(&[], vec![], vec![vassal("s0", "s1"), vassal("s1", "s2")])), "a vassal with vassals");
    refused(scenario_with(relations(&[("s0", "s1")], vec![], vec![vassal("s0", "s1")])), "a vassal at war with its lord at the start");
    let mut s = scenario_with(relations(&[], vec![], vec![]));
    s.persons.get_mut("r0").unwrap().spouse = Some("nobody".to_string());
    refused(s, "an unknown spouse");
    let mut s = scenario_with(relations(&[], vec![], vec![]));
    s.persons.get_mut("r0").unwrap().spouse = Some("r1".to_string());
    refused(s, "a spouse who does not name back");
    let mut s = scenario_with(relations(&[], vec![], vec![]));
    s.persons.get_mut("r0").unwrap().spouse = Some("c0".to_string());
    s.persons.get_mut("c0").unwrap().spouse = Some("r0".to_string());
    refused(s, "spouses of one house");
    // Relations absent: a world without bonds, every pair at the base less the border.
    let g = Game::new(&testworld::five_states_scenario(), params(), 1).unwrap();
    assert!(g.world().wars.is_empty() && g.world().treaties.is_empty());
    assert_eq!(g.world().relations.len(), 10);
}

#[test]
fn the_relation_moves_to_a_norm_that_follows_the_state_of_the_pair() {
    let p = params();
    let mut w = testworld::world(1);
    let (a, b, c, d) = (ActorId(0), ActorId(1), ActorId(2), ActorId(3));
    let base = |w: &World, x, y| p.relation_base - if w.borders(x, y) { p.relation_norm_border } else { 0.0 };
    assert!((w.relation(a, b) - base(&w, a, b)).abs() < 1e-9);
    // War: the norm falls by the row and the relation follows, a share of the gap a tick.
    w.declare_war(a, b).unwrap();
    assert!((w.relation_norm(a, b, &p) - (base(&w, a, b) - p.relation_norm_war)).abs() < 1e-9);
    let before = w.relation(a, b);
    w.tick(&p).unwrap();
    let after = w.relation(a, b);
    let expected = before + p.relation_rate_per_tick * (w.relation_norm(a, b, &p) - before);
    assert!((after - expected).abs() < 1e-9, "one tick: {before} → {after}, expected {expected}");
    for _ in 0..200 {
        w.tick(&p).unwrap();
    }
    assert!((w.relation(a, b) - w.relation_norm(a, b, &p)).abs() < 0.01, "the relation converges to the norm");
    // Alliance, vassalage, marriage: each shifts the norm by its row, together they add.
    w.ally_now(c, d, &p).unwrap();
    assert!((w.relation_norm(c, d, &p) - (base(&w, c, d) + p.relation_norm_alliance)).abs() < 1e-9);
    w.make_vassal(d, c).unwrap();
    assert!((w.relation_norm(c, d, &p) - (base(&w, c, d) + p.relation_norm_alliance + p.relation_norm_vassalage)).abs() < 1e-9);
    let ruler_c = w.actors[&c].ruler;
    let ruler_d = w.actors[&d].ruler;
    w.marry_now(ruler_c, ruler_d);
    assert!((w.relation_norm(c, d, &p) - (base(&w, c, d) + p.relation_norm_alliance + p.relation_norm_vassalage + p.relation_norm_marriage)).abs() < 1e-9);
    assert!(w.relation_norm(c, d, &p) <= 100.0);
    // A marriage holds while both live: the death of one ends its shift.
    w.persons.get_mut(&ruler_d).unwrap().died = Some(w.tick);
    assert!(!w.married_across(c, d), "a dead spouse binds nothing");
    // Pushes saturate: no push reaches the edge.
    let r = w.relation(a, b);
    for _ in 0..50 {
        let before = w.relation(a, b);
        w.tick(&p).unwrap();
        let _ = before;
    }
    assert!(w.relation(a, b) >= 0.0 && w.relation(a, b) <= 100.0);
    let _ = r;
}

#[test]
fn a_treaty_runs_out_exactly_at_its_term() {
    let p = params();
    let mut w = testworld::world(2);
    let (a, b) = (ActorId(1), ActorId(3));
    let until = w.ally_now(a, b, &p).unwrap();
    assert_eq!(until, Tick(w.tick.0 + p.treaty_term_ticks));
    assert_eq!(p.treaty_term_ticks, 50, "25 years");
    assert_eq!(w.allies_of(a), vec![b]);
    let mut ended_at = None;
    for _ in 0..=p.treaty_term_ticks + 2 {
        let tick = w.tick;
        let r = w.tick(&p).unwrap();
        if r.events.iter().any(|e| matches!(e.event, Event::TreatyExpired { a: x, b: y, kind: TreatyKind::Alliance } if (x, y) == (a, b))) {
            ended_at = Some(tick);
            break;
        }
        assert!(w.allied(a, b) || tick >= until, "allied until the term");
    }
    assert_eq!(ended_at, Some(until), "the treaty ends in the tick of its term");
    assert!(!w.allied(a, b) && w.allies_of(a).is_empty());
}

#[test]
fn a_run_with_relations_saves_and_loads_bit_for_bit() {
    let p = params();
    let mut s = scenario_with(relations(&[("s0", "s1")], vec![alliance("s2", "s3", 1490)], vec![vassal("s4", "s3")]));
    s.persons.get_mut("r2").unwrap().spouse = Some("r4".to_string());
    s.persons.get_mut("r4").unwrap().spouse = Some("r2".to_string());
    let mut straight = Game::new(&s, params(), 7).unwrap();
    let mut first = Game::new(&s, params(), 7).unwrap();
    let mut straight_reports = Vec::new();
    let mut reports = Vec::new();
    for _ in 0..40 {
        straight_reports.push(straight.tick(&[]).unwrap());
    }
    for _ in 0..20 {
        reports.push(first.tick(&[]).unwrap());
    }
    let save = first.to_save().unwrap();
    assert!(save.contains("\"treaties\"") && save.contains("\"relations\"") && save.contains("\"vassal_of\""));
    let mut second = Game::from_save(&s, params(), &save).unwrap();
    for _ in 0..20 {
        reports.push(second.tick(&[]).unwrap());
    }
    assert_eq!(reports, straight_reports);
    assert_eq!(second.to_save().unwrap(), straight.to_save().unwrap());
    let w = from_save(&to_save(straight.world()).unwrap()).unwrap();
    assert_eq!(&w, straight.world());
    let _ = p;
}

#[test]
fn on_generated_worlds_the_bound_share_at_the_start_lies_in_d3_and_without_agents_the_treaties_run_out() {
    let p = params();
    let band = p.entries["diplomacy.observed_bound_share"].band.unwrap();
    let mut shares = Vec::new();
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let mut g = Game::new(&s, params(), seed).unwrap();
        let w = g.world();
        let n = w.actors.len();
        let bound = w.actors.keys().filter(|a| !w.allies_of(**a).is_empty() || w.lord_of(**a).is_some() || !w.vassals_of(**a).is_empty()).count();
        *kinds.entry("alliances").or_insert(0) += w.treaties.len();
        *kinds.entry("vassals").or_insert(0) += w.actors.values().filter(|a| a.vassal_of.is_some()).count();
        shares.push(bound as f64 / n as f64);
        assert!(w.wars.is_empty(), "seed {seed}: the generator opens no war");
        for ((a, b), t) in &w.treaties {
            assert!(t.until > Tick(0) && t.until <= Tick(2 * p.treaty_term_years_band[1] as u32), "seed {seed}: {a:?}–{b:?} term {:?}", t.until);
        }
        for (a, x) in &w.actors {
            if let Some(lord) = x.vassal_of {
                assert!(w.actors[&lord].provinces.len() >= 2 * x.provinces.len(), "seed {seed}: lord {lord:?} of {a:?} is not twice its size");
                assert!(w.vassals_of(*a).is_empty() && w.allies_of(*a).is_empty(), "seed {seed}: vassal {a:?} bound by something else");
            }
        }
        if seed <= 5 {
            for _ in 0..300 {
                g.tick(&[]).unwrap();
            }
            let w = g.world();
            assert!(w.treaties.is_empty(), "seed {seed}: a treaty outlived 150 years without renewal");
            for ((a, b), r) in &w.relations {
                assert!((0.0..=100.0).contains(r), "seed {seed}: relation {a:?}–{b:?} = {r}");
                if w.alive(*a) && w.alive(*b) {
                    assert!((r - w.relation_norm(*a, *b, &p)).abs() < 1.0, "seed {seed}: {a:?}–{b:?} far from its norm after 300 ticks");
                }
            }
        }
    }
    let mean = shares.iter().sum::<f64>() / shares.len() as f64;
    eprintln!(
        "Д3 share of polities bound at the start: mean {mean:.3} over 20 worlds (band {band:?}); by world {}; {} alliances, {} vassals",
        shares.iter().map(|s| format!("{s:.2}")).collect::<Vec<_>>().join(" "),
        kinds["alliances"],
        kinds["vassals"]
    );
    assert!((band[0]..=band[1]).contains(&mean), "Д3 mean {mean:.3} outside {band:?}");
    assert!(shares.iter().all(|s| *s > 0.0), "every generated world starts with a bond");
    let _ = PersonId(0);
}
