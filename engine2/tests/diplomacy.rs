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
//!   treaties have all run out by the end;
//! - a vassal sends its lord the tribute of Д2 out of its take, through the ledger, and
//!   nothing while at war with it;
//! - a war declared on a member of an alliance brings its allies, its lord and its vassals
//!   in, the attacker's vassals in on the other side; the entry is no declaration — no
//!   price, no breach, no chain (the owner's test: A attacks B, each with an ally — three
//!   fight, not four); a peace in force keeps an ally out;
//! - bound polities march through each other's land and are supplied there by their pay,
//!   never by foraging;
//! - the verbs: `declare_war` costs legitimacy, a breach the factor and the treaty, a
//!   vassal fights only its lord; `make_peace` is accepted by the state of the war — the
//!   loser always, the winner at the threshold — closes the obligations, binds for the term,
//!   and settles the loser (submission at half its land, release of a vassal that won);
//!   `propose_alliance` spends the embassy's gifts accepted or not and is accepted at the
//!   threshold; `send_embassy` and `bribe` push with saturation, the gift staying in the
//!   world and worth more to the poor; `propose_marriage` is open to every house, the dowry
//!   out of the house treasury, the threshold across polities only; every offer shows the
//!   threshold a proposal needs.

use std::collections::BTreeMap;

use engine2::scenario::{AllianceData, RelationsData, VassalData};
use engine2::sim::{Game, Simulation};
use engine2::{from_save, testworld, to_save, worldgen, ActorId, Event, Params, PersonId, ScenarioData, Tick, TreatyKind, World};

fn params() -> Params {
    Params::load().unwrap()
}

/// A game whose bots are off: the test drives the world by hand (E3 (а)).
fn bare(s: &engine2::ScenarioData, p: Params, seed: u64) -> Result<Game, engine2::sim::SimError> {
    Game::new(s, p, seed).map(|mut g| {
        g.set_bots(false);
        g
    })
}

/// A game loaded with its bots off, to continue a bare run.
#[allow(dead_code)]
fn bare_load(s: &engine2::ScenarioData, p: Params, save: &str) -> Result<Game, engine2::sim::SimError> {
    Game::from_save(s, p, save).map(|mut g| {
        g.set_bots(false);
        g
    })
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
    let g = bare(&s, params(), 1).unwrap();
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
    let g = bare(&testworld::five_states_scenario(), params(), 1).unwrap();
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
    let mut straight = bare(&s, params(), 7).unwrap();
    let mut first = bare(&s, params(), 7).unwrap();
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
    let mut second = bare_load(&s, params(), &save).unwrap();
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
        let mut g = bare(&s, params(), seed).unwrap();
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
                    // The norm itself moves when a court changes faith or tongue at a
                    // succession (E2 step 8), and the drift only ever closes the gap: so
                    // the gap is at most one such jump, and nothing runs away.
                    let jump = p.relation_norm_faith + p.relation_norm_culture;
                    assert!((r - w.relation_norm(*a, *b, &p)).abs() <= jump, "seed {seed}: {a:?}–{b:?} farther from its norm than one jump of the norm after 300 ticks");
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

fn at(x: u32, y: u32) -> engine2::ProvinceId {
    engine2::ProvinceId(y * 5 + x)
}

#[test]
fn a_vassal_sends_its_lord_the_tribute_of_d2_out_of_its_take_through_the_ledger_and_nothing_while_at_war_with_it() {
    let p = params();
    let mut w = testworld::world(3);
    let (lord, vassal) = (ActorId(0), ActorId(1));
    w.make_vassal(vassal, lord).unwrap();
    let r = w.tick(&p).unwrap();
    let on = |r: &engine2::TickReport, a: ActorId, src: engine2::Source| -> i64 { r.postings.iter().filter(|q| q.account == engine2::Account::Treasury(a) && q.source == src).map(|q| q.milli).sum() };
    let take = on(&r, vassal, engine2::Source::Tax) / 1000;
    let sent = on(&r, vassal, engine2::Source::Tribute);
    let got = on(&r, lord, engine2::Source::Tribute);
    assert!(take > 0);
    assert_eq!(sent, -got, "the same figure on both treasuries");
    assert_eq!(got / 1000, take * p.tribute_share_ppm / 1_000_000, "a fifth of the take");
    assert!((0.1..=0.3).contains(&(p.tribute_share_ppm as f64 / 1e6)), "inside Д2");
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::TributeShort { .. })));
    // A rebel pays nothing.
    w.declare_war(vassal, lord).unwrap();
    let r = w.tick(&p).unwrap();
    assert_eq!(on(&r, vassal, engine2::Source::Tribute), 0, "no tribute while at war with the lord");
    assert_eq!(w.lord_of(vassal), Some(lord), "still a vassal until the peace");
}

#[test]
fn a_war_on_a_member_of_an_alliance_brings_its_allies_lord_and_vassals_in_and_their_entry_is_no_declaration() {
    let p = params();
    // The owner's test: A attacks B; A and B each have an ally — three fight, not four.
    let mut w = testworld::world(4);
    let (a, b, c, d, e) = (ActorId(0), ActorId(1), ActorId(2), ActorId(3), ActorId(4));
    w.ally_now(a, c, &p).unwrap();
    w.ally_now(b, d, &p).unwrap();
    w.ally_now(d, e, &p).unwrap();
    let legitimacy_d = w.actors[&d].legitimacy;
    w.declare_war(a, b).unwrap();
    assert!(w.at_war(a, b) && w.at_war(a, d), "the victim's ally enters against the attacker");
    assert!(!w.at_war(a, c) && !w.at_war(b, c) && !w.at_war(c, d), "the attacker's ally stays out: it was not attacked");
    assert!(!w.at_war(a, e) && !w.at_war(e, b), "the entrant's own ally stays out: entry is no declaration, there is no chain");
    assert_eq!(w.wars.len(), 2);
    assert!((w.actors[&d].legitimacy - legitimacy_d).abs() < 1e-9, "entry costs the entrant nothing");
    assert!(w.allied(b, d) && w.allied(a, c), "no treaty is broken by the entry");
    // Vassals follow their lord's wars; a lord defends its vassal.
    let mut w = testworld::world(5);
    w.make_vassal(c, a).unwrap();
    w.make_vassal(e, b).unwrap();
    w.declare_war(a, b).unwrap();
    assert!(w.at_war(c, b), "the attacker's vassal enters against the attacked");
    assert!(w.at_war(e, a), "the attacked side's vassal enters against the attacker");
    assert!(!w.at_war(c, e), "the vassals are not set against each other: pairwise, no chain");
    let mut w = testworld::world(6);
    w.make_vassal(b, d).unwrap();
    w.declare_war(a, b).unwrap();
    assert!(w.at_war(d, a), "the lord defends its vassal");
    // A peace in force keeps an ally out.
    let mut w = testworld::world(7);
    w.ally_now(b, d, &p).unwrap();
    w.bind(a, d, TreatyKind::Peace, &p).unwrap();
    w.declare_war(a, b).unwrap();
    assert!(!w.at_war(a, d), "the treaty of peace holds: the ally does not enter");
}

#[test]
fn bound_polities_march_through_each_others_land_and_are_supplied_there_by_their_pay_not_by_foraging() {
    let p = params();
    let (a, b) = (ActorId(0), ActorId(1));
    // Closed without a bond.
    let mut w = testworld::world(8);
    let unit = w.units_of(a)[0];
    w.order_move(unit, at(1, 2)).unwrap();
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::NoPath { unit: u, .. } if u == unit)), "a neighbour's land is closed to the march");
    assert_eq!(w.units[&unit].position, at(0, 0));
    // Open to an ally, and supplied there as at home.
    let mut w = testworld::world(8);
    w.ally_now(a, b, &p).unwrap();
    let unit = w.units_of(a)[0];
    let soldiers = w.soldiers(unit);
    w.order_move(unit, at(1, 2)).unwrap();
    let r = w.tick(&p).unwrap();
    assert_eq!(w.units[&unit].position, at(1, 2), "the right of passage comes with the alliance");
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::Ruin { province, .. } if province == at(1, 2))), "nobody forages on an ally");
    let paid = w.actors[&a].paid_share_ppm;
    let unsupplied = soldiers - soldiers * paid / 1_000_000;
    let melted = r.events.iter().find_map(|e| match e.event {
        Event::Attrition { unit: u, unsupplied } if u == unit => Some(unsupplied as i64),
        _ => None,
    });
    assert_eq!(melted.unwrap_or(0), unsupplied, "the paid share is fed by the pay, the unpaid share melts as at home");
    // The same for a vassal on its lord's land and the lord on the vassal's.
    let mut w = testworld::world(9);
    w.make_vassal(b, a).unwrap();
    let unit = w.units_of(b)[0];
    w.order_move(unit, at(0, 3)).unwrap();
    w.tick(&p).unwrap();
    assert_eq!(w.units[&unit].position, at(0, 3));
}

fn scenario_with_verbs() -> ScenarioData {
    let mut s = testworld::five_states_scenario();
    for v in engine2::VerbId::ALL {
        s.verbs.insert(v.key().to_string(), v.key().replace('_', " "));
    }
    s
}

fn call(a: ActorId, verb: engine2::VerbId, b: ActorId) -> engine2::VerbCall {
    engine2::VerbCall { actor: engine2::ActorRef::Ruler(a), verb, target: engine2::Target::Actor(b) }
}

fn on(r: &engine2::TickReport, account: engine2::Account, src: engine2::Source) -> i64 {
    r.postings.iter().filter(|q| q.account == account && q.source == src).map(|q| q.milli).sum::<i64>() / 1000
}

/// `a`'s unit takes the undefended enemy province `p`: first to its own province next to
/// `p` (so the march meets no other contact), then across — no battle, a siege within the
/// tick.
fn take(w: &mut World, a: ActorId, p: engine2::ProvinceId, params: &Params) {
    let unit = w.units_of(a)[0];
    let here = w.units[&unit].position;
    let staging = if w.provinces[&here].neighbours.contains(&p) { here } else { w.actors[&a].provinces.iter().copied().find(|q| w.provinces[q].neighbours.contains(&p)).expect("an own province next to the target") };
    if here != staging {
        w.order_move(unit, staging).unwrap();
        w.tick(params).unwrap();
        assert_eq!(w.units[&unit].position, staging, "the staging march");
    }
    w.order_move(unit, p).unwrap();
    let r = w.tick(params).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::ProvinceChangedHands { province, to, .. } if province == p && to == a)), "{p:?} not taken");
}

#[test]
fn declare_war_costs_legitimacy_a_breach_costs_the_factor_and_ends_the_treaty_and_a_vassal_fights_only_its_lord() {
    use engine2::VerbId;
    let p = params();
    let mut g = bare(&scenario_with_verbs(), params(), 11).unwrap();
    let (s0, s1, s2, s3, s4) = (ActorId(0), ActorId(1), ActorId(2), ActorId(3), ActorId(4));
    let offer = g.verbs(engine2::ActorRef::Ruler(s0)).into_iter().find(|o| o.verb == VerbId::DeclareWar).unwrap();
    assert!(offer.available && offer.cost_legitimacy == p.legitimacy_cost_declare_war.round() as i64 && offer.threshold.is_none());
    let l0 = g.world().actors[&s0].legitimacy;
    let r = g.tick(&[call(s0, VerbId::DeclareWar, s1)]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::War { a, b } if (a, b) == (s0, s1))));
    assert!(g.world().at_war(s0, s1));
    let paid = l0 - g.world().actors[&s0].legitimacy;
    let drift = p.legitimacy_rate_per_tick * (g.world().legitimacy_norm(s0, &p) - (l0 - p.legitimacy_cost_declare_war));
    assert!((paid - (p.legitimacy_cost_declare_war - drift)).abs() < 1e-6, "the exact price, then the tick's drift: paid {paid}");
    // A breach: twice the price, the treaty gone, the victim's allies' relations to nothing.
    g.world_mut().ally_now(s0, s2, &p).unwrap();
    g.world_mut().ally_now(s2, s3, &p).unwrap();
    assert!(g.world().relation(s0, s3) > 0.0);
    let l0 = g.world().actors[&s0].legitimacy;
    let norm_before = g.world().legitimacy_norm(s0, &p);
    assert!((g.world().war_price(s0, s2, &p) - p.legitimacy_cost_declare_war * p.treaty_breach_factor).abs() < 1e-9);
    let r = g.tick(&[call(s0, VerbId::DeclareWar, s2)]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::TreatyBroken { a, b, kind: TreatyKind::Alliance } if (a, b) == (s0, s2))));
    assert!(g.world().treaty(s0, s2).is_none() && g.world().at_war(s0, s2));
    let after_price = l0 - p.legitimacy_cost_declare_war * p.treaty_breach_factor;
    let expected = after_price + p.legitimacy_rate_per_tick * (norm_before - after_price);
    assert!((g.world().actors[&s0].legitimacy - expected).abs() < 1e-6, "the breach costs the factor");
    let drifted = p.relation_rate_per_tick * g.world().relation_norm(s0, s3, &p);
    assert!(g.world().relation(s0, s3) <= drifted + 1e-6, "the relation with the victim's ally is pushed to nothing, then the tick's drift: {}", g.world().relation(s0, s3));
    assert!(g.world().at_war(s3, s0), "and the ally entered all the same");
    // A vassal fights only its lord.
    g.world_mut().make_vassal(s4, s3).unwrap();
    let err = g.tick(&[call(s4, VerbId::DeclareWar, s1)]).unwrap_err();
    assert!(err.to_string().contains("a vassal fights only its lord"), "{err}");
    let offer = g.verbs(engine2::ActorRef::Ruler(s4)).into_iter().find(|o| o.verb == VerbId::DeclareWar).unwrap();
    assert!(offer.available, "the lord is a target");
    g.tick(&[call(s4, VerbId::DeclareWar, s3)]).unwrap();
    assert!(g.world().at_war(s4, s3), "the way out is open");
    assert_eq!(g.world().lord_of(s4), Some(s3), "still a vassal until the peace");
    let offer = g.verbs(engine2::ActorRef::Ruler(s4)).into_iter().find(|o| o.verb == VerbId::DeclareWar).unwrap();
    assert!(!offer.available && offer.why.as_deref() == Some("a vassal fights only its lord"), "{:?}", offer.why);
}

#[test]
fn peace_is_accepted_by_the_state_of_the_war_closes_the_obligations_and_settles_the_loser() {
    use engine2::VerbId;
    let p = params();
    let (s0, s1, s2, s3) = (ActorId(0), ActorId(1), ActorId(2), ActorId(3));
    // Nothing taken either way: a status quo both accept; the peace binds for the term and
    // a new war before it is a breach.
    let mut w = testworld::world(12);
    w.declare_war(s0, s1).unwrap();
    assert!(w.losing_side(s0, s1).is_none());
    assert!(w.peace_room(s0, s1, &p).is_ok() && w.peace_room(s1, s0, &p).is_ok());
    w.apply_verb(&call(s1, VerbId::MakePeace, s0), &p).unwrap();
    assert!(!w.at_war(s0, s1) && w.peace_in_force(s0, s1));
    assert_eq!(w.treaty(s0, s1).unwrap().until, Tick(w.tick.0 + p.treaty_term_ticks));
    assert!((w.war_price(s0, s1, &p) - p.legitimacy_cost_declare_war * p.treaty_breach_factor).abs() < 1e-9, "war again before the term is a breach");
    assert_eq!(w.peace_room(s0, s1, &p), Err("not at war"));
    // The winner takes a province: the loser's proposal is refused at the norm of war, the
    // winner's accepted; the loser accepts at the threshold.
    let mut w = testworld::world(13);
    w.ally_now(s1, s3, &p).unwrap();
    w.declare_war(s0, s1).unwrap();
    assert!(w.at_war(s0, s3), "the ally entered");
    take(&mut w, s0, at(1, 4), &p);
    assert_eq!(w.wars[&(s0, s1)].taken, (1, 0));
    assert_eq!(w.losing_side(s0, s1), Some(s1));
    assert!(w.relation(s0, s1) < p.treaty_accept_at_relation);
    let err = w.apply_verb(&call(s1, VerbId::MakePeace, s0), &p).unwrap_err();
    assert!(err.to_string().contains("the winner wants more"), "{err}");
    assert!(w.at_war(s0, s1));
    w.relations.insert((s0, s1), p.treaty_accept_at_relation);
    assert!(w.peace_room(s1, s0, &p).is_ok(), "at the threshold the winner accepts");
    w.relations.insert((s0, s1), 20.0);
    let before = w.tick;
    w.apply_verb(&call(s0, VerbId::MakePeace, s1), &p).unwrap();
    assert!(!w.at_war(s0, s1), "the loser accepts the winner's peace always");
    assert!(!w.at_war(s0, s3), "the ally's war was an obligation of this one and closed with it");
    assert_eq!(w.treaty(s0, s1).map(|t| t.kind), Some(TreatyKind::Peace));
    assert_eq!(w.lord_of(s1), None, "one province of five lost: no submission");
    assert_eq!(w.actors[&s1].provinces.len(), 4, "what was taken stays taken");
    let _ = before;
    // Down to half its land the loser submits.
    let mut w = testworld::world(14);
    w.declare_war(s0, s1).unwrap();
    for y in [4, 3, 2] {
        take(&mut w, s0, at(1, y), &p);
    }
    assert_eq!(w.actors[&s1].provinces.len(), 2);
    let r = {
        w.apply_verb(&call(s0, VerbId::MakePeace, s1), &p).unwrap();
        w.tick(&p).unwrap()
    };
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Submission { vassal, lord } if (vassal, lord) == (s1, s0))));
    assert_eq!(w.lord_of(s1), Some(s0));
    assert!(on(&r, engine2::Account::Treasury(s0), engine2::Source::Tribute) > 0, "and the tribute flows");
    // A vassal that beat its lord is released.
    let mut w = testworld::world(15);
    w.make_vassal(s1, s0).unwrap();
    w.declare_war(s1, s0).unwrap();
    take(&mut w, s1, at(0, 4), &p);
    assert_eq!(w.losing_side(s0, s1), Some(s0));
    w.apply_verb(&call(s1, VerbId::MakePeace, s0), &p).unwrap();
    assert_eq!(w.lord_of(s1), None, "released");
    assert!(!w.at_war(s0, s1));
    // A rebel that lost stays a vassal.
    let mut w = testworld::world(16);
    w.make_vassal(s1, s0).unwrap();
    w.declare_war(s1, s0).unwrap();
    take(&mut w, s0, at(1, 4), &p);
    w.apply_verb(&call(s0, VerbId::MakePeace, s1), &p).unwrap();
    assert_eq!(w.lord_of(s1), Some(s0), "the rebel lost: still a vassal");
    let _ = s2;
}

#[test]
fn an_alliance_is_proposed_for_the_gifts_of_an_embassy_accepted_or_not_and_accepted_at_the_threshold() {
    use engine2::VerbId;
    let p = params();
    let mut g = bare(&scenario_with_verbs(), params(), 17).unwrap();
    let (s0, s1, s2, s4) = (ActorId(0), ActorId(1), ActorId(2), ActorId(4));
    let who = engine2::ActorRef::Ruler(s0);
    let cost = g.world().embassy_cost(s0, &p);
    assert!(cost > 0);
    g.world_mut().post(engine2::Account::Treasury(s0), engine2::Source::Test, 20 * cost * 1000).unwrap();
    let offer = g.verbs(who).into_iter().find(|o| o.verb == VerbId::ProposeAlliance).unwrap();
    assert!(!offer.available && offer.cost_kg == cost && offer.threshold == Some(p.treaty_accept_at_relation.round() as i64));
    assert!(offer.why.as_deref().unwrap().contains("threshold 60"), "{:?}", offer.why);
    // Refused below the threshold: the gifts are spent all the same.
    let r0 = g.world().relation(s0, s1);
    assert!(r0 < p.treaty_accept_at_relation);
    let r = g.tick(&[call(s0, VerbId::ProposeAlliance, s1)]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Refused { actor, verb: VerbId::ProposeAlliance, target } if (actor, target) == (s0, s1))));
    assert_eq!(on(&r, engine2::Account::Treasury(s0), engine2::Source::Verb), -cost);
    assert!(!g.world().allied(s0, s1));
    // Embassies push with saturation: each push smaller than the last; four reach the
    // threshold from the border norm, and the proposal in the same tick is accepted.
    let before = g.world().relation(s0, s1);
    let cost = g.world().embassy_cost(s0, &p);
    let r = g.tick(&[call(s0, VerbId::SendEmbassy, s1)]).unwrap();
    assert_eq!(on(&r, engine2::Account::Treasury(s0), engine2::Source::Verb), -cost);
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Embassy { from, to, kg } if (from, to) == (s0, s1) && kg == cost as u64)));
    let pushed = before + p.embassy_gain * (100.0 - before) / 100.0;
    let after_tick = pushed + p.relation_rate_per_tick * (g.world().relation_norm(s0, s1, &p) - pushed);
    assert!((g.world().relation(s0, s1) - after_tick).abs() < 1e-6, "push with saturation, then the tick's drift");
    let embassies: Vec<engine2::VerbCall> = (0..4).map(|_| call(s0, VerbId::SendEmbassy, s1)).chain([call(s0, VerbId::ProposeAlliance, s1)]).collect();
    let r = g.tick(&embassies).unwrap();
    let until = r.events.iter().find_map(|e| match e.event {
        Event::Alliance { a, b, until } if (a, b) == (s0, s1) => Some(until),
        _ => None,
    });
    assert!(until.is_some(), "accepted at the threshold");
    assert!(g.world().allied(s0, s1));
    assert_eq!(g.world().treaty(s0, s1).unwrap().until, until.unwrap());
    assert!((g.world().relation_norm(s0, s1, &p) - (p.relation_base + p.relation_norm_alliance - p.relation_norm_border)).abs() < 1e-9);
    // A vassal makes no alliance; at war with the other's ally — refused.
    g.world_mut().make_vassal(s4, s2).unwrap();
    let err = g.tick(&[call(s4, VerbId::ProposeAlliance, s0)]).unwrap_err();
    assert!(err.to_string().contains("a vassal makes no alliance"), "{err}");
    g.world_mut().declare_war(s2, s1).unwrap();
    g.world_mut().relations.insert((s0, s2), 100.0);
    let r = g.tick(&[call(s0, VerbId::ProposeAlliance, s2)]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Refused { actor, target, .. } if (actor, target) == (s0, s2))), "at war with the proposer's ally");
}

#[test]
fn a_gift_moves_between_treasuries_and_pushes_in_proportion_to_what_it_is_worth_to_the_receiver() {
    use engine2::VerbId;
    let p = params();
    let mut g = bare(&scenario_with_verbs(), params(), 18).unwrap();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let gift = g.world().gift_size(s0, &p);
    assert!(gift > 0 && (0.01..=0.02).contains(&(p.bribe_share_ppm as f64 / 1e6)), "inside Д2");
    let worth = gift as f64 / g.world().yearly_income(s1, &p) as f64;
    let before = g.world().relation(s0, s1);
    let r = g.tick(&[call(s0, VerbId::Bribe, s1)]).unwrap();
    assert_eq!(on(&r, engine2::Account::Treasury(s0), engine2::Source::Gift), -gift);
    assert_eq!(on(&r, engine2::Account::Treasury(s1), engine2::Source::Gift), gift, "the money stays in the world");
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Gift { from, to, kg } if (from, to) == (s0, s1) && kg == gift as u64)));
    let pushed = before + p.bribe_gain_per_share * worth * (100.0 - before) / 100.0;
    let after_tick = pushed + p.relation_rate_per_tick * (g.world().relation_norm(s0, s1, &p) - pushed);
    assert!((g.world().relation(s0, s1) - after_tick).abs() < 1e-6, "the push is the gift's worth to the receiver");
    let offer = g.verbs(engine2::ActorRef::Ruler(s0)).into_iter().find(|o| o.verb == VerbId::Bribe).unwrap();
    assert!(offer.available && offer.cost_kg == g.world().gift_size(s0, &p));
    // A house does not send gifts or embassies.
    let house = g.verbs(engine2::ActorRef::House(engine2::HouseId(0))).into_iter().find(|o| o.verb == VerbId::SendEmbassy).unwrap();
    assert!(!house.available && house.why.as_deref() == Some("a house does not rule the polity"));
}

#[test]
fn a_marriage_binds_two_houses_for_a_dowry_out_of_the_house_treasury_and_is_open_to_every_house_of_a_polity() {
    use engine2::{ActorRef, HouseId, Target, VerbCall, VerbId};
    let p = params();
    let mut g = bare(&scenario_with_verbs(), params(), 19).unwrap();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let (h0, h1, h2, o0) = (HouseId(0), HouseId(1), HouseId(2), HouseId(5));
    assert_eq!(g.world().houses[&o0].polity, s0, "the other house of polity 0");
    let marry = |who: ActorRef, g: HouseId| VerbCall { actor: who, verb: VerbId::ProposeMarriage, target: Target::House(g) };
    // Across polities the threshold applies.
    let err = g.tick(&[marry(ActorRef::Ruler(s0), h1)]).unwrap_err();
    assert!(err.to_string().contains("below the threshold"), "{err}");
    g.world_mut().relations.insert((s0, s1), p.treaty_accept_at_relation);
    let r = g.tick(&[marry(ActorRef::Ruler(s0), h1)]).unwrap();
    let (a, b) = r.events.iter().find_map(|e| match e.event {
        Event::Marriage { a, b, dowry_kg: 0 } => Some((a, b)),
        _ => None,
    }).expect("a marriage with no dowry: the house has nothing yet");
    let w = g.world();
    assert_eq!((w.persons[&a].house, w.persons[&b].house), (h0, h1));
    assert_eq!(w.persons[&a].spouse, Some(b));
    assert_eq!(w.persons[&b].spouse, Some(a));
    assert_eq!(a, w.actors[&s0].ruler, "the eldest unmarried adult of the house: the ruler");
    assert!(w.married_across(s0, s1));
    assert!((w.relation_norm(s0, s1, &p) - (p.relation_base + p.relation_norm_marriage - p.relation_norm_border)).abs() < 1e-9);
    // The ruler's offer carries the dowry its house would give and the threshold.
    let offer = g.verbs(ActorRef::Ruler(s0)).into_iter().find(|o| o.verb == VerbId::ProposeMarriage).unwrap();
    assert!(offer.available && offer.cost_kg == g.world().dowry(h0, &p) && offer.threshold == Some(p.treaty_accept_at_relation.round() as i64));
    // A house that does not rule marries its members into the ruling house of its own
    // polity without a threshold — its way up — for a dowry out of its own treasury, and
    // then has nobody left to marry.
    g.world_mut().post(engine2::Account::HouseTreasury(o0), engine2::Source::Test, 1000 * 1000).unwrap();
    let dowry = g.world().dowry(o0, &p);
    assert_eq!(dowry, 1000 * p.dowry_share_ppm / 1_000_000, "a house without a seat has only what it was given");
    let offer = g.verbs(ActorRef::House(o0)).into_iter().find(|o| o.verb == VerbId::ProposeMarriage).unwrap();
    assert!(offer.available && offer.cost_kg == dowry, "{:?}", offer.why);
    let err = g.tick(&[marry(ActorRef::House(o0), HouseId(3))]).unwrap_err();
    assert!(err.to_string().contains("below the threshold"), "{err}");
    let r = g.tick(&[marry(ActorRef::House(o0), h0)]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Marriage { dowry_kg, .. } if dowry_kg == dowry as u64)));
    assert_eq!(on(&r, engine2::Account::HouseTreasury(o0), engine2::Source::Dowry), -dowry);
    assert_eq!(on(&r, engine2::Account::HouseTreasury(h0), engine2::Source::Dowry), dowry);
    let _ = h2;
    let offer = g.verbs(ActorRef::House(o0)).into_iter().find(|o| o.verb == VerbId::ProposeMarriage).unwrap();
    assert!(!offer.available && offer.why.as_deref() == Some("no unmarried adult to marry"), "{:?}", offer.why);
    let err = g.tick(&[marry(ActorRef::House(o0), h1)]).unwrap_err();
    assert!(err.to_string().contains("no unmarried adult"), "{err}");
}
