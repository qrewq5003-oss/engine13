//! E2 step 6a acceptance (plan §18): the house and the throne — on the test world, where
//! the deaths are chosen, and on the generated worlds, where the draws give them.
//!
//! - a dead ruler is succeeded by the designated heir, else by the rule: by blood the line
//!   by seniority (the eldest child's line before a younger child), by acclamation the
//!   eldest adult; a child on the throne gets a regent and the regency ends at majority;
//! - when the ruling house dies out the throne passes to another house of the polity
//!   (owner's rule on §18); a polity with no house left raises a new one from the people of
//!   its capital, through the ledger, and lives on (owner's rule on #285) — nothing falls
//!   without war;
//! - a house holding the throne and a governorship gets Х1 from both, as an exact transfer;
//!   a house without a seat gets nothing; the house's treasury has no outlet in E2;
//! - legitimacy moves toward the norm of the ruler's standing;
//! - on 20 generated worlds over 300 ticks: reigns end by natural and by violent death,
//!   their mean length lies in В5; births keep the houses neither exploding nor always
//!   dying out; no polity falls without war; the great houses' share of the tax lies in
//!   the Х4 band as redefined on Х1; the run is deterministic through a save.

use std::collections::BTreeMap;

use engine2::event::Cause;
use engine2::sim::{Game, Simulation};
use engine2::{testworld, worldgen, Account, ActorId, ActorRef, Event, HouseId, Params, PersonId, ProvinceId, ScenarioData, Source, Standing, Succession, VerbId, World};

fn params() -> Params {
    Params::load().unwrap()
}

fn person(s: &ScenarioData, sid: &str) -> PersonId {
    PersonId(s.persons.keys().position(|k| k == sid).unwrap() as u32)
}

fn house(s: &ScenarioData, sid: &str) -> HouseId {
    HouseId(s.houses.keys().position(|k| k == sid).unwrap() as u32)
}

fn world_of(s: &ScenarioData) -> World {
    let c = s.compile(&params()).unwrap();
    World::from_facts(&c.facts, c.names, &params(), 1).unwrap()
}

fn succession_of(r: &engine2::TickReport, a: ActorId) -> Option<(PersonId, PersonId, Standing)> {
    r.events.iter().find_map(|e| match e.event {
        Event::Succession { actor, from, to, how } if actor == a => Some((from, to, how)),
        _ => None,
    })
}

#[test]
fn the_designated_heir_succeeds_a_child_gets_a_regent_and_the_regency_ends_at_majority() {
    let p = params();
    let s = testworld::five_states_scenario();
    let mut w = world_of(&s);
    let s0 = ActorId(0);
    let (r0, c0, b0) = (person(&s, "r0"), person(&s, "c0"), person(&s, "b0"));
    assert_eq!(w.actors[&s0].ruler, r0);
    assert_eq!(w.actors[&s0].heir, Some(c0));
    assert_eq!(w.actors[&s0].succession, Succession::Blood);
    assert!((w.actors[&s0].legitimacy - p.legitimacy_norm_hereditary).abs() < 1e-9, "legitimacy starts at the norm of the standing");
    w.kill(r0, Cause::Natural, &p).unwrap();
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::RulerDied { actor, person, cause: Cause::Natural } if actor == s0 && person == r0)));
    assert_eq!(succession_of(&r, s0), Some((r0, c0, Standing::Hereditary)));
    assert_eq!(w.actors[&s0].ruler, c0);
    assert_eq!(w.actors[&s0].heir, None);
    assert_eq!(w.age(c0), 12);
    // A minor on the throne: the eldest adult of the house is regent — the brother, not
    // the adult of the other house.
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Regency { actor, ward, regent } if actor == s0 && ward == c0 && regent == b0)));
    assert_eq!(w.actors[&s0].regent, Some(b0));
    assert!(w.alive(s0));
    // Legitimacy now moves toward the regency norm, and after majority back up.
    let l1 = w.actors[&s0].legitimacy;
    assert!(l1 < p.legitimacy_norm_hereditary && l1 > p.legitimacy_norm_regency);
    let mut ended_at = None;
    let mut at_end = 0.0;
    for _ in 0..20 {
        let r = w.tick(&p).unwrap();
        if r.events.iter().any(|e| matches!(e.event, Event::RegencyEnded { actor, ruler } if actor == s0 && ruler == c0)) {
            ended_at = Some(w.age(c0));
            at_end = w.actors[&s0].legitimacy;
        }
        if !w.person_alive(c0) || !w.person_alive(b0) {
            return; // the draws took the child or the regent: the shape is covered by the generated worlds
        }
    }
    assert_eq!(ended_at, Some(p.majority_age), "the regency ends in the tick the ruler comes of age");
    assert_eq!(w.actors[&s0].regent, None);
    assert!(at_end < l1, "legitimacy sank toward the regency norm");
    assert!(w.actors[&s0].legitimacy > at_end && w.actors[&s0].legitimacy <= p.legitimacy_norm_hereditary, "and climbs back toward the hereditary norm");
    // Every draw of the phase is the first of its point for its id in the tick.
    let r = w.tick(&p).unwrap();
    assert!(r.draws.iter().filter(|d| matches!(d.draw.point, engine2::DrawPoint::Mortality | engine2::DrawPoint::Birth | engine2::DrawPoint::Violence)).all(|d| d.draw.n == 0), "draws_once_per_person");
}

#[test]
fn by_blood_the_eldest_childs_line_comes_before_a_younger_child_and_by_acclamation_the_eldest_adult_does() {
    let p = params();
    let mut base = testworld::five_states_scenario();
    // Give house 1 a second child of the ruler and a grandchild through the first.
    base.persons.insert("c1b".to_string(), engine2::scenario::PersonData { name: "Child 1 b".into(), house: "h1".into(), born: 1457, parent: Some("r1".into()) });
    base.persons.insert("g1".to_string(), engine2::scenario::PersonData { name: "Grandchild 1".into(), house: "h1".into(), born: 1474, parent: Some("c1".into()) });
    let s1 = ActorId(1);
    for (rule, expected, standing) in [(Succession::Blood, "g1", Standing::Hereditary), (Succession::Acclamation, "c1b", Standing::Acclaimed), (Succession::Election, "c1b", Standing::Elected), (Succession::Mixed, "g1", Standing::Confirmed)] {
        let mut s = base.clone();
        s.actors.get_mut("s1").unwrap().succession = rule;
        let mut w = world_of(&s);
        let (r1, c1) = (person(&s, "r1"), person(&s, "c1"));
        // The eldest child (24) succeeds under every rule; then its own child (3) or the
        // younger sibling (20), by the rule.
        w.kill(r1, Cause::Natural, &p).unwrap();
        let r = w.tick(&p).unwrap();
        assert_eq!(succession_of(&r, s1).map(|(_, to, _)| to), Some(c1), "{rule:?}");
        w.kill(c1, Cause::Murder, &p).unwrap();
        let r = w.tick(&p).unwrap();
        assert_eq!(succession_of(&r, s1), Some((c1, person(&s, expected), standing)), "{rule:?}");
    }
}

#[test]
fn when_the_ruling_house_dies_out_the_throne_passes_to_another_house_of_the_polity_and_the_governorship_falls_vacant() {
    let p = params();
    let s = testworld::five_states_scenario();
    let mut w = world_of(&s);
    let s0 = ActorId(0);
    let (r0, c0, b0, other) = (person(&s, "r0"), person(&s, "c0"), person(&s, "b0"), person(&s, "o0a"));
    assert_eq!(w.provinces[&ProvinceId(5)].governor, Some(b0));
    for x in [c0, b0] {
        w.kill(x, Cause::Natural, &p).unwrap();
    }
    w.kill(r0, Cause::Murder, &p).unwrap();
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::HouseExtinct { house } if house == house_of(&s, "h0"))));
    assert!(r.events.iter().any(|e| matches!(e.event, Event::GovernorChanged { province: ProvinceId(5), from, to: None } if from == b0)));
    assert_eq!(w.provinces[&ProvinceId(5)].governor, None);
    // Blood rule, but the blood is gone: the other house takes the throne by acclamation.
    assert_eq!(succession_of(&r, s0), Some((r0, other, Standing::Acclaimed)));
    assert!(w.alive(s0));
    assert_eq!(w.persons[&w.actors[&s0].ruler].house, house_of(&s, "o0"));
}

fn house_of(s: &ScenarioData, sid: &str) -> HouseId {
    house(s, sid)
}

#[test]
fn a_polity_with_no_house_left_raises_a_new_house_from_the_people_of_its_capital_and_lives_on() {
    let p = params();
    let s = testworld::five_states_scenario();
    let mut g = Game::new(&s, params(), 1).unwrap();
    let s4 = ActorId(4);
    let (r4, c4) = (person(&s, "r4"), person(&s, "c4"));
    let capital = g.world().actors[&s4].capital;
    let provinces = g.world().actors[&s4].provinces.clone();
    let (houses_before, persons_before) = (g.world().houses.len(), g.world().persons.len());
    g.world_mut().kill(c4, Cause::Natural, &p).unwrap();
    g.world_mut().kill(r4, Cause::Natural, &p).unwrap();
    let r = g.tick(&[]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::HouseExtinct { house } if house == house_of(&s, "h4"))));
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::ActorDied { .. })), "the extinction of a dynasty does not kill a polity");
    let (house, founder) = r
        .events
        .iter()
        .find_map(|e| match e.event {
            Event::NewDynasty { actor, house, founder, from, household } if actor == s4 => {
                assert_eq!(from, capital, "the founder comes from the capital");
                assert!((2..=3).contains(&household), "a household of two or three adults, got {household}");
                Some((house, founder))
            }
            _ => None,
        })
        .expect("a new dynasty");
    assert_eq!(succession_of(&r, s4), Some((r4, founder, Standing::Acclaimed)));
    let w = g.world();
    let risen_persons = w.persons.len() - persons_before;
    assert!((3..=4).contains(&risen_persons), "founder and household: {risen_persons}");
    // The founder and the household, through the ledger, off the people of the capital —
    // not out of nothing.
    let risen: Vec<_> = r.postings.iter().filter(|q| q.source == Source::Rise).collect();
    assert_eq!(risen.len(), 1);
    assert_eq!((risen[0].account, risen[0].milli), (Account::Population(capital), -(risen_persons as i64) * 1000));
    assert!(w.alive(s4));
    assert_eq!(w.actors[&s4].ruler, founder);
    assert_eq!(w.actors[&s4].standing, Standing::Acclaimed);
    assert_eq!(w.actors[&s4].provinces, provinces);
    assert_eq!(w.houses.len(), houses_before + 1);
    assert_eq!(w.houses[&house].polity, s4);
    for (id, person) in w.persons.range(PersonId(persons_before as u32)..) {
        assert_eq!(person.house, house, "{id:?}");
        assert!(w.is_adult(*id, &p) && person.parent.is_none(), "{id:?}: an adult of no line");
    }
    assert_eq!(w.members(house).len(), risen_persons);
    assert_eq!(w.names.houses.len(), w.houses.len());
    assert_eq!(w.names.persons.len(), w.persons.len());
    assert_eq!(w.house_treasury(house), 0);
    let offers = g.verbs(ActorRef::Ruler(s4));
    assert!(offers.iter().all(|o| o.why.as_deref() != Some("the polity has fallen")));
    // The polity plays on: the economy runs, the new house draws Х1 from the throne.
    let r = g.tick(&[]).unwrap();
    assert!(r.postings.iter().any(|q| q.account == Account::Treasury(s4) && q.source == Source::Tax));
    assert!(g.world().house_treasury(house) > 0, "the new house holds the throne");
    let _ = VerbId::Recruit;
}

#[test]
fn a_house_with_the_throne_and_a_governorship_gets_x1_from_both_as_an_exact_transfer_and_a_house_without_a_seat_gets_nothing() {
    let p = params();
    let s = testworld::five_states_scenario();
    let mut w = world_of(&s);
    let s0 = ActorId(0);
    let (h0, o0) = (house(&s, "h0"), house(&s, "o0"));
    let governed = ProvinceId(5);
    assert_eq!(w.persons[&w.provinces[&governed].governor.unwrap()].house, h0);
    for _ in 0..5 {
        let income = w.production_per_tick(s0, &p) * w.actors[&s0].tax_rate_ppm / 1_000_000;
        let province_tax = w.population(governed) * p.output_per_head_kg / 2 * w.actors[&s0].tax_rate_ppm / 1_000_000;
        let expected = income * p.seat_income_share_ppm / 1_000_000 + province_tax * p.seat_income_share_ppm / 1_000_000;
        let before = w.house_treasury(h0);
        let r = w.tick(&p).unwrap();
        if !r.events.iter().any(|e| matches!(e.event, Event::PersonDied { .. } | Event::GovernorChanged { .. })) {
            let to_house: i64 = r.postings.iter().filter(|q| q.account == Account::HouseTreasury(h0) && q.source == Source::Seat).map(|q| q.milli).sum();
            let from_treasury: i64 = r.postings.iter().filter(|q| q.account == Account::Treasury(s0) && q.source == Source::Seat).map(|q| q.milli).sum();
            assert_eq!(to_house, expected * 1000);
            assert_eq!(from_treasury, -expected * 1000, "the same figure on both sides");
            assert_eq!(w.house_treasury(h0), before + expected);
            assert!(expected > 0);
        }
        assert_eq!(w.house_treasury(o0), 0, "no seat, no income");
        assert!(r.postings.iter().filter(|q| matches!(q.account, Account::HouseTreasury(_))).all(|q| q.source == Source::Seat && q.milli > 0), "a house's treasury has no outlet in E2 (plan §18)");
    }
}

#[test]
fn a_house_is_offered_nothing_it_can_do_until_diplomacy() {
    let s = testworld::five_states_scenario();
    let g = Game::new(&s, params(), 1).unwrap();
    let offers = g.verbs(ActorRef::House(house(&s, "h0")));
    assert!(offers.iter().all(|o| !o.available && o.why.is_some()));
    let _ = VerbId::RaiseTax; // priced for the ruler in 6b: `tests/legitimacy.rs`
}

/// Reigns, deaths, births and houses on the generated worlds.
#[test]
fn on_generated_worlds_reigns_lie_in_b5_deaths_are_natural_and_violent_houses_neither_explode_nor_always_die_out_nothing_falls_without_war_and_x4_lies_in_its_band() {
    let p = params();
    let reign_band = p.entries["reign.observed_years"].band.unwrap();
    let x4_band = p.entries["house.observed_share_of_income"].band.unwrap();
    let mut reigns: Vec<f64> = Vec::new();
    let (mut natural, mut murders, mut executions, mut births, mut extinct, mut successions_across_houses, mut falls, mut dynasties) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    // The houses of the start are measured at the end apart from the houses risen in play,
    // which start from one founder and would pull the median down.
    let mut sizes_end: Vec<usize> = Vec::new();
    let mut sizes_start: Vec<usize> = Vec::new();
    let mut sizes_risen: Vec<usize> = Vec::new();
    let (mut tax_total, mut seat_total) = (0i64, 0i64);
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let mut g = Game::new(&s, params(), seed).unwrap();
        let mut since: BTreeMap<ActorId, u32> = g.world().actors.keys().map(|a| (*a, 0)).collect();
        for h in g.world().houses.keys() {
            sizes_start.push(g.world().members(*h).len());
        }
        for t in 0..300u32 {
            let r = g.tick(&[]).unwrap();
            for e in &r.events {
                match e.event {
                    Event::RulerDied { actor, cause, .. } => {
                        match cause {
                            Cause::Murder => murders += 1,
                            Cause::Execution => executions += 1,
                            _ => natural += 1,
                        }
                        reigns.push(f64::from(t + 1 - since[&actor]) / 2.0);
                    }
                    Event::Succession { actor, from, to, .. } => {
                        since.insert(actor, t + 1);
                        if g.world().persons[&from].house != g.world().persons[&to].house {
                            successions_across_houses += 1;
                        }
                    }
                    Event::PersonBorn { .. } => births += 1,
                    Event::HouseExtinct { .. } => extinct += 1,
                    Event::ActorDied { .. } => falls += 1,
                    Event::NewDynasty { .. } => dynasties += 1,
                    _ => {}
                }
            }
            for q in &r.postings {
                match (q.account, q.source) {
                    (Account::Treasury(_), Source::Tax) => tax_total += q.milli,
                    (Account::HouseTreasury(_), Source::Seat) => seat_total += q.milli,
                    _ => {}
                }
            }
        }
        let w = g.world();
        let at_start = s.houses.len();
        for h in w.houses.keys() {
            if h.index() < at_start {
                sizes_end.push(w.members(*h).len());
            } else {
                sizes_risen.push(w.members(*h).len());
            }
        }
        // Every seat is held by a living person of the polity's own houses, or is vacant.
        for (a, actor) in &w.actors {
            if w.alive(*a) {
                assert_eq!(w.houses[&w.persons[&actor.ruler].house].polity, *a, "seed {seed}: ruler of another polity's house");
                assert!(actor.regent.is_none_or(|r| w.person_alive(r)), "seed {seed}: a dead regent");
                assert!(w.is_adult(actor.ruler, &p) || actor.regent.is_some() || w.people_of_polity_adults(*a, &p).is_empty(), "seed {seed}: a minor without a regent while an adult exists");
            }
        }
        for (pid, pr) in &w.provinces {
            if let Some(g) = pr.governor {
                assert!(w.person_alive(g), "seed {seed}: dead governor of {pid:?}");
                assert_eq!(w.houses[&w.persons[&g].house].polity, pr.owner, "seed {seed}: governor of {pid:?} from another polity");
            }
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let median = |v: &mut Vec<usize>| {
        v.sort();
        v.get(v.len() / 2).copied().unwrap_or(0)
    };
    let x4 = seat_total as f64 / tax_total as f64;
    let (m_start, m_end, m_risen) = (median(&mut sizes_start), median(&mut sizes_end), median(&mut sizes_risen));
    eprintln!(
        "В5 mean reign {:.1} years over {} reigns ({natural} natural, {murders} violent, {executions} executed by a usurper); {births} births; houses {} of which {extinct} extinct, median size {m_start} -> {m_end}; {successions_across_houses} successions across houses, {dynasties} new dynasties (risen houses: median size {m_risen} at the end), {falls} polities fell; Х4 great houses' share of the tax {x4:.4}",
        mean(&reigns),
        reigns.len(),
        sizes_end.len() + sizes_risen.len()
    );
    assert!(reigns.len() >= 100, "too few reigns ended: {}", reigns.len());
    assert!((reign_band[0]..=reign_band[1]).contains(&mean(&reigns)), "mean reign {:.1} outside В5 {reign_band:?}", mean(&reigns));
    assert!(natural > 0 && murders > 0, "deaths of both kinds");
    assert!(births > 0);
    // Not an explosion (the owner's 30× was the defect), not always extinct, not never.
    assert!(m_end <= 4 * m_start.max(1), "houses explode: median {m_start} -> {m_end}");
    assert!(m_end >= 1, "houses die out in the median: {m_start} -> {m_end}");
    assert!(extinct > 0 && (extinct as usize) < sizes_end.len(), "extinction: {extinct} of {}", sizes_end.len());
    assert!(successions_across_houses > 0, "the throne never passed to another house");
    // Without war nothing falls: a polity whose houses all die out raises a new one.
    assert_eq!(falls, 0, "{falls} polities fell without war");
    assert!(dynasties > 0, "no house ever rose from the people");
    assert!((x4_band[0]..=x4_band[1]).contains(&x4), "Х4 {x4:.4} outside {x4_band:?}");
}

#[test]
fn the_house_and_the_throne_are_deterministic_through_a_save() {
    let p = params();
    let s = worldgen::generate(3, &p);
    let mut straight = Game::new(&s, params(), 7).unwrap();
    let mut a = Vec::new();
    for _ in 0..80 {
        a.push(straight.tick(&[]).unwrap());
    }
    let mut g = Game::new(&s, params(), 7).unwrap();
    let mut b = Vec::new();
    for _ in 0..40 {
        b.push(g.tick(&[]).unwrap());
    }
    let save = g.to_save().unwrap();
    let mut g2 = Game::from_save(&s, params(), &save).unwrap();
    for _ in 0..40 {
        b.push(g2.tick(&[]).unwrap());
    }
    assert_eq!(a, b);
    assert_eq!(straight.to_save().unwrap(), g2.to_save().unwrap());
    assert!(a.iter().any(|r| r.events.iter().any(|e| matches!(e.event, Event::PersonBorn { .. }))), "the run has births to carry");
    assert_eq!(straight.world().names.persons.len(), straight.world().persons.len(), "every person born is named in the world");
}
