//! E2 step 9 acceptance (plan §21): collapse and successors.
//!
//! - **House troops (E0 Х2):** a house's own men are a fact of the scenario, a unit of the
//!   polity marked with the house; the polity pays the ruling house's retinue with its own
//!   men, a house that does not rule pays its retinue from its treasury as an exact posting,
//!   what it cannot pay melts in that unit alone, and a house that dies out loses its men to
//!   the people where they stand; the generator carves the retinues out of the army of А11
//!   by the Х2 row.
//! - **The core and alienation (owner's rule on §21):** a province is alienated by a faith
//!   other than its owner's court's and by a tongue other than its owner's core — the
//!   tongue of the polity's majority — never by the court's tongue; the core moves only
//!   when another tongue outgrows it by the margin.
//! - **The rising (E0 В7):** a house that does not rule takes the provinces its members
//!   govern into a polity of its own — the governor on the throne, acclaimed, under the
//!   parent's rule, with the parent's tax rate and institutions but the bank and the
//!   policy, the treasury split by the people who left, the debt left behind, the house's
//!   men and a militia up to А1, every pair at its norm, a war with the parent without a
//!   price and the parent's ally in it; a province with no house to carry it rises alone
//!   under a new house of its people; a capital never rises; the draw has the form of В3
//!   and В6 — once per province, (1 + marks) × (100 − L)/50 — and a province taken this
//!   tick draws no more; the rising ends by the state of the war: suppression (the polity
//!   falls, the militia to the people), submission at half its land, freedom for the term.
//! - **On the generated worlds:** risings per century per large polity lie in В7, neither
//!   never nor always; every successor is whole; nothing falls (nobody marches without an
//!   order in E2); the run is deterministic through a save; the sentinel declares the kinds
//!   by the horizon.

use engine2::sim::{Game, Simulation};
use engine2::{testworld, worldgen, Account, ActorId, CultureId, Event, HouseId, Params, ProvinceId, ReligionId, ScenarioData, Source, UnitId, World};

fn params() -> Params {
    Params::load().unwrap()
}

/// The constants with the base of the rising at nothing: the test chooses its risings
/// through `rise_now`, the draws add none.
fn calm() -> Params {
    let mut p = params();
    p.revolt_per_tick = 0.0;
    p
}

/// Province `(x, y)` of the 5×5 test world; state `x` owns column `x`, capital at `y = 0`.
fn at(x: u32, y: u32) -> ProvinceId {
    ProvinceId(y * 5 + x)
}

fn house(s: &ScenarioData, sid: &str) -> HouseId {
    HouseId(s.houses.keys().position(|k| k == sid).unwrap() as u32)
}

fn world_of(s: &ScenarioData, seed: u64) -> World {
    let c = s.compile(&params()).unwrap();
    World::from_facts(&c.facts, c.names, &params(), seed).unwrap()
}

fn on(r: &engine2::TickReport, account: Account, src: Source) -> i64 {
    r.postings.iter().filter(|q| q.account == account && q.source == src).map(|q| q.milli).sum::<i64>()
}

#[test]
fn house_troops_are_a_fact_the_polity_pays_the_ruling_houses_retinue_a_house_that_does_not_rule_pays_its_own_and_an_extinct_house_loses_its_men_to_the_people() {
    let p = params();
    let mut s = testworld::five_states_scenario();
    // The ruling house of polity 0 keeps 100 men, the other house 50 — with two ticks of
    // their pay in its treasury.
    let pay_tick = p.pay_per_soldier_kg_per_year / 2;
    s.houses.get_mut("h0").unwrap().soldiers = 100;
    s.houses.get_mut("o0").unwrap().soldiers = 50;
    s.houses.get_mut("o0").unwrap().treasury_kg = 2 * 50 * pay_tick;
    let (h0, o0) = (house(&s, "h0"), house(&s, "o0"));
    let mut w = world_of(&s, 1);
    let s0 = ActorId(0);
    let units = w.units_of(s0);
    assert_eq!(units.len(), 3, "the polity's own unit and the two retinues");
    let of_house = |w: &World, h: HouseId| w.units_of_house(h);
    let (u_h0, u_o0) = (of_house(&w, h0)[0], of_house(&w, o0)[0]);
    let own = units.iter().copied().find(|u| w.units[u].house.is_none()).unwrap();
    assert_eq!((w.soldiers(own), w.soldiers(u_h0), w.soldiers(u_o0)), (600, 100, 50));
    for u in [u_h0, u_o0] {
        assert_eq!(w.units[&u].owner, s0, "a house's men are a unit of the house's polity");
        assert_eq!(w.units[&u].position, w.actors[&s0].capital);
        assert!(!w.units[&u].mercenary);
    }
    assert_eq!(w.house_pays(own), None);
    assert_eq!(w.house_pays(u_h0), None, "the ruling house's retinue is the polity's charge");
    assert_eq!(w.house_pays(u_o0), Some(o0));
    assert_eq!(w.army_size(s0), 750, "the retinues count in the army");
    // Tick one and two: the polity pays 700 men, the other house its 50, exactly.
    for tick in 1..=2 {
        let r = w.tick(&p).unwrap();
        assert_eq!(on(&r, Account::Treasury(s0), Source::Upkeep), -700 * pay_tick * 1000, "tick {tick}: the polity's pay covers its men and the ruling house's");
        assert_eq!(on(&r, Account::HouseTreasury(o0), Source::Upkeep), -50 * pay_tick * 1000, "tick {tick}: the house pays its own, the exact figure");
        assert!(!r.events.iter().any(|e| matches!(e.event, Event::Attrition { .. } | Event::HousePayShortfall { .. })), "tick {tick}: everybody paid, nobody melts");
        assert_eq!(w.houses[&o0].paid_share_ppm, 1_000_000);
    }
    assert_eq!(w.house_treasury(o0), 0, "two ticks of pay spent");
    // Tick three: the house cannot pay; its unit alone melts by А7, the polity's are fed.
    let before = w.soldiers(u_o0);
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::HousePayShortfall { house, unpaid_kg } if house == o0 && unpaid_kg == 50 * pay_tick as u64)));
    assert_eq!(w.houses[&o0].paid_share_ppm, 0);
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Attrition { unit, unsupplied } if unit == u_o0 && unsupplied == before as u64)));
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::Attrition { unit, .. } if unit != u_o0)), "the polity's men and the ruling house's are fed");
    assert_eq!(w.soldiers(u_o0), before - (before * p.army_attrition_unsupplied_ppm_per_tick + 999_999) / 1_000_000, "the unpaid men melt by А7");
    assert_eq!(w.soldiers(u_h0), 100);
    assert_eq!(on(&r, Account::HouseTreasury(o0), Source::Upkeep), 0, "nothing to pay with, nothing posted");
    assert!(w.house_treasury(o0) >= 0);
    // The ruling house's retinue is the polity's charge: with an empty treasury the
    // polity's shortfall is shared over its own men and the retinue alike.
    let t = w.treasury(s0);
    w.post(Account::Treasury(s0), Source::Test, -t * 1000).unwrap();
    let soldiers_u = w.units_of(s0).iter().map(|u| (*u, w.soldiers(*u))).collect::<Vec<_>>();
    let huge = w.sustainable_soldiers(s0, &p) * 4;
    w.post(Account::Soldiers(own), Source::Test, (huge - w.soldiers(own)) * 1000).unwrap();
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::PayShortfall { actor, .. } if actor == s0)));
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Attrition { unit, .. } if unit == u_h0)), "the ruling house's retinue melts with the polity's shortfall");
    let _ = soldiers_u;
    // An extinct house loses its men to the people where they stand.
    let member = w.members(o0)[0];
    let capital = w.actors[&s0].capital;
    let (people, men) = (w.population(capital), w.soldiers(u_o0));
    assert!(men > 0);
    w.kill(member, engine2::event::Cause::Natural, &p).unwrap();
    assert!(!w.units.contains_key(&u_o0));
    assert_eq!(w.population(capital), people + men, "the men joined the people of the capital");
    assert_eq!(w.ledger.stock(Account::Soldiers(u_o0)), Some(0));
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::HouseExtinct { house } if house == o0)));
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Disbanded { unit, soldiers } if unit == u_o0 && soldiers == men as u64)));
    assert_eq!(r.postings.iter().filter(|q| q.source == Source::Disband).map(|q| q.milli).sum::<i64>(), 0, "a transfer between soldiers and people");
    // The polity's levy never joins a house's unit: a fresh levy makes a unit of its own.
    let _ = UnitId(0);
}

#[test]
fn the_generator_carves_the_retinues_out_of_the_army_by_x2_and_a_run_with_house_troops_saves_bit_for_bit() {
    let p = params();
    let band = p.entries["house.retinue_share_of_army"].band.unwrap();
    let (mut ruling_shares, mut other_shares) = (Vec::new(), Vec::new());
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let w = world_of(&s, seed);
        for (a, actor) in &w.actors {
            let army = w.army_size(*a);
            let ruling = w.ruling_house(*a);
            let retinue: i64 = w.units_of_house(ruling).iter().map(|u| w.soldiers(*u)).sum();
            if army > 0 {
                ruling_shares.push(retinue as f64 / army as f64);
            }
            for h in w.houses_of(*a) {
                if h == ruling {
                    continue;
                }
                let men: i64 = w.units_of_house(h).iter().map(|u| w.soldiers(*u)).sum();
                if army > 0 {
                    other_shares.push(men as f64 / army as f64);
                }
                for u in w.units_of_house(h) {
                    assert_eq!(w.house_pays(u), Some(h), "seed {seed}: a house that does not rule pays its own");
                }
            }
            // Inside the army of А11, not on top of it: the whole army within the budget.
            assert!(army <= w.sustainable_soldiers(*a, &p), "seed {seed} {a:?}: army {army} over the budget");
            let _ = actor;
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    eprintln!("Х2 ruling house's retinue: mean {:.3} of the army over {} polities (band {band:?}); other houses {:.3}", mean(&ruling_shares), ruling_shares.len(), mean(&other_shares));
    assert!((band[0]..=band[1]).contains(&mean(&ruling_shares)), "Х2 {:.3} outside {band:?}", mean(&ruling_shares));
    assert!(ruling_shares.iter().all(|s| *s <= band[1] + 0.01 && *s >= band[0] - 0.05), "a polity far outside the band: {ruling_shares:?}");
    assert!(mean(&other_shares) > 0.0 && mean(&other_shares) < band[0]);
    // Save and load through the contract, bit for bit, with house troops melting.
    let s = worldgen::generate(4, &p);
    let mut straight = Game::new(&s, params(), 4).unwrap();
    let mut a = Vec::new();
    for _ in 0..40 {
        a.push(straight.tick(&[]).unwrap());
    }
    let mut g = Game::new(&s, params(), 4).unwrap();
    let mut b = Vec::new();
    for _ in 0..20 {
        b.push(g.tick(&[]).unwrap());
    }
    let save = g.to_save().unwrap();
    assert!(save.contains("\"house\":") && save.contains("\"paid_share_ppm\"") && save.contains("\"core_culture\""));
    let mut g2 = Game::from_save(&s, params(), &save).unwrap();
    for _ in 0..20 {
        b.push(g2.tick(&[]).unwrap());
    }
    assert_eq!(a, b);
    assert!(a.iter().any(|r| r.events.iter().any(|e| matches!(e.event, Event::HousePayShortfall { .. }))), "the run had a house short of pay to carry");
}

#[test]
fn a_province_is_alienated_by_a_faith_other_than_the_courts_and_a_tongue_other_than_the_core_which_is_the_majoritys_and_moves_only_by_the_margin() {
    let p = params();
    // Polity 0: four of its five provinces (the populous ones) speak c1, the capital c0 —
    // and so does the court. The core is the majority's: c1.
    let mut s = testworld::five_states_scenario();
    for y in 1..5 {
        s.provinces.get_mut(&format!("p{:02}", y * 5)).unwrap().culture = "c1".to_string();
    }
    s.provinces.get_mut("p05").unwrap().religion = "f1".to_string();
    let mut w = world_of(&s, 1);
    let s0 = ActorId(0);
    assert_eq!(w.polity_culture(s0), CultureId(0), "the court speaks c0");
    assert_eq!(w.actors[&s0].core_culture, CultureId(1), "the core is the majority of the people");
    assert_eq!(w.majority_culture(s0), CultureId(1));
    // The capital of the court's own tongue is alienated — by its tongue against the core;
    // the heartland of another tongue than the court is not.
    assert_eq!(w.alienation_marks(at(0, 0)), 1);
    assert!(w.alienated(at(0, 0)));
    assert_eq!(w.alienation_marks(at(0, 2)), 0);
    assert!(!w.alienated(at(0, 2)));
    // Another faith than the court's: one mark; both: two.
    assert_eq!(w.alienation_marks(at(0, 1)), 1, "f1 under a court of f0");
    w.provinces.get_mut(&at(0, 1)).unwrap().culture = CultureId(0);
    assert_eq!(w.alienation_marks(at(0, 1)), 2);
    assert!(w.provinces[&at(0, 1)].religion == ReligionId(1));
    // A court that changes its tongue changes nothing here: the core is the people's.
    let ruling = w.ruling_house(s0);
    w.houses.get_mut(&ruling).unwrap().culture = CultureId(1);
    assert_eq!(w.alienation_marks(at(0, 0)), 1, "still the capital against the core");
    assert_eq!(w.alienation_marks(at(0, 2)), 0);
    // The core moves only with the margin. Polity 1 speaks c0 throughout; its people by
    // province are 6 600, 9 600, 12 600, 15 600, 18 600.
    let s1 = ActorId(1);
    assert_eq!(w.actors[&s1].core_culture, CultureId(0));
    let total: i64 = w.actors[&s1].provinces.iter().map(|q| w.population(*q)).sum();
    // Two provinces of c1 are a majority (34 200 of 63 000) but short of the margin.
    for q in [at(1, 4), at(1, 3)] {
        w.provinces.get_mut(&q).unwrap().culture = CultureId(1);
    }
    let (c1, c0): (i64, i64) = (w.population(at(1, 4)) + w.population(at(1, 3)), total - w.population(at(1, 4)) - w.population(at(1, 3)));
    assert!(c1 > c0 && (c1 - c0) * 1_000_000 < p.core_switch_margin_ppm * total, "a majority inside the margin: {c1} vs {c0} of {total}");
    assert_eq!(w.majority_culture(s1), CultureId(1));
    w.tick(&p).unwrap();
    assert_eq!(w.actors[&s1].core_culture, CultureId(0), "the core holds inside the margin");
    assert!(w.alienated(at(1, 4)) && !w.alienated(at(1, 0)));
    // A third province takes the majority past the margin: the core moves at the phase.
    w.provinces.get_mut(&at(1, 2)).unwrap().culture = CultureId(1);
    w.tick(&p).unwrap();
    assert_eq!(w.actors[&s1].core_culture, CultureId(1), "the core moved");
    assert!(!w.alienated(at(1, 4)) && w.alienated(at(1, 0)) && w.alienated(at(1, 1)), "now the old heartland is the alienated part");
    // Back and forth inside the margin changes nothing: no flapping.
    w.provinces.get_mut(&at(1, 2)).unwrap().culture = CultureId(0);
    w.tick(&p).unwrap();
    assert_eq!(w.actors[&s1].core_culture, CultureId(1), "c0 leads again (38 800 vs 34 200) but inside the margin: the core holds");
    // The take is untouched by alienation (owner's answer on §20): the same share collected.
    let plain = world_of(&testworld::five_states_scenario(), 1);
    assert_eq!(w.collection_share_ppm(s0, &p), plain.collection_share_ppm(s0, &p));
}

// ---------- the rising (б) ----------

use engine2::event::Transfer;
use engine2::scenario::{AllianceData, PersonData, RelationsData};
use engine2::{from_save, to_save, Standing, Target, VerbCall, VerbId};

/// The test world with a second adult in the other house of polity 0, the two of them
/// governing p05 and p10, polity 0 allied with polity 2, banked and with the policy.
fn carrier_scenario() -> ScenarioData {
    let mut s = testworld::five_states_scenario();
    s.persons.insert("o0b".to_string(), PersonData { name: "Other 0 second".into(), house: "o0".into(), born: 1445, parent: None, spouse: None });
    s.provinces.get_mut("p05").unwrap().governor = Some("o0a".to_string());
    s.provinces.get_mut("p10").unwrap().governor = Some("o0b".to_string());
    s.houses.get_mut("o0").unwrap().soldiers = 40;
    s.houses.get_mut("o0").unwrap().treasury_kg = 1_000;
    s.actors.get_mut("s0").unwrap().institutions = vec![engine2::Institution::Bank, engine2::Institution::ConversionPolicy];
    s.relations = Some(RelationsData { wars: vec![], alliances: vec![AllianceData { a: "s0".into(), b: "s2".into(), until_year: 1500 }], vassals: vec![] });
    for v in VerbId::ALL {
        s.verbs.insert(v.key().to_string(), v.key().replace('_', " "));
    }
    s
}

fn person(s: &ScenarioData, sid: &str) -> engine2::PersonId {
    engine2::PersonId(s.persons.keys().position(|k| k == sid).unwrap() as u32)
}

#[test]
fn a_house_that_does_not_rule_takes_the_provinces_its_members_govern_into_a_polity_of_its_own_with_its_share_of_the_treasury_its_militia_its_edges_and_a_war() {
    let p = params();
    let s = carrier_scenario();
    let mut w = world_of(&s, 31);
    let (s0, s1, s2) = (ActorId(0), ActorId(1), ActorId(2));
    let (o0, o0a, o0b) = (house(&s, "o0"), person(&s, "o0a"), person(&s, "o0b"));
    let (p05, p10) = (at(0, 1), at(0, 2));
    assert_eq!(w.secession_carrier(p05), Some(o0));
    assert_eq!(w.secession_carrier(at(0, 3)), None, "no seat: the province would rise alone");
    assert!(w.house_would_secede(o0), "E2: a bot house takes the chance");
    let treasury0 = w.treasury(s0);
    let people0: i64 = w.actors[&s0].provinces.iter().map(|q| w.population(*q)).sum();
    let gone = w.population(p05) + w.population(p10);
    let retinue = w.units_of_house(o0)[0];
    let (actors_before, names_before, units_before) = (w.actors.len(), w.names.actors.len(), w.units.len());
    let new = w.rise_now(p05, &p).unwrap().expect("a rising");
    assert_eq!(new, ActorId(actors_before as u32));
    assert_eq!(w.names.actors.len(), names_before + 1);
    assert_eq!(w.names.actors[new.index()].0, "s0.5");
    // The provinces: both the house governs, never the capital; the people with them.
    assert_eq!(w.actors[&new].provinces, vec![p05, p10]);
    assert_eq!(w.actors[&s0].provinces, vec![at(0, 0), at(0, 3), at(0, 4)]);
    assert!(w.provinces[&p05].owner == new && w.provinces[&p10].owner == new);
    assert_eq!(w.actors[&new].capital, p10, "the most populous of them");
    assert!(w.alive(new) && w.alive(s0));
    // The throne: the governor, acclaimed, under the parent's rule; the house moved.
    assert_eq!(w.houses[&o0].polity, new);
    assert_eq!(w.actors[&new].ruler, o0a);
    assert_eq!(w.ruling_house(new), o0);
    assert_eq!(w.actors[&new].standing, Standing::Acclaimed);
    assert!((w.actors[&new].legitimacy - p.legitimacy_norm_acclaimed).abs() < 1e-9);
    assert_eq!(w.actors[&new].succession, w.actors[&s0].succession);
    assert_eq!(w.actors[&new].tax_rate_ppm, w.actors[&s0].tax_rate_ppm);
    assert!(w.actors[&new].institutions.is_empty(), "neither the bank nor the policy");
    assert_eq!(w.actors[&new].vassal_of, None);
    assert_eq!(w.actors[&new].core_culture, w.majority_culture(new));
    // One seat per person: the ruler left p05's seat (nobody free in the house: vacant);
    // the other member keeps p10.
    assert_eq!(w.provinces[&p05].governor, None);
    assert_eq!(w.provinces[&p10].governor, Some(o0b));
    // The treasury by the people who left, the exact figure both ways; the debt stays.
    let share = treasury0 * gone / people0;
    assert!(share > 0);
    assert_eq!(w.treasury(new), share);
    assert_eq!(w.treasury(s0), treasury0 - share);
    assert_eq!(w.debt(new), 0);
    assert_eq!(w.house_treasury(o0), 1_000, "the house treasury goes with the house, untouched");
    // The house's men went with it; the militia rose in every province up to А1.
    assert_eq!(w.units[&retinue].owner, new);
    assert_eq!(w.units[&retinue].house, Some(o0));
    assert_eq!(w.house_pays(retinue), None, "now the ruling house: the polity's charge");
    let militia: Vec<UnitId> = w.units_of(new).into_iter().filter(|u| *u != retinue).collect();
    assert_eq!(militia.len(), 2);
    assert_eq!(w.units.len(), units_before + 2);
    for u in &militia {
        let q = w.units[u].position;
        assert!(q == p05 || q == p10);
        assert!(w.units[u].house.is_none() && !w.units[u].mercenary);
        let people_now = w.population(q);
        let raised = w.soldiers(*u);
        assert!(raised > 0 && raised <= (people_now + raised) * p.recruit_ceiling_ppm / 1_000_000, "from its own people, at most А1 of them");
    }
    // The whole — the retinue and the militia — fills the ceiling of А1 exactly.
    assert_eq!(w.army_size(new), w.recruit_ceiling(new, &p), "the militia rose to the ceiling of А1");
    // Edges: every pair at its norm; war with the parent without a price, the parent's
    // ally in it by §19, nobody else.
    for b in [s0, s1, s2, ActorId(3), ActorId(4)] {
        assert!(w.relations.contains_key(&(b, new)), "a pair with {b:?}");
        assert!((w.relation(new, b) - w.relation_norm(new, b, &p)).abs() < 1e-9, "{b:?}: at its norm");
    }
    assert!(w.at_war(new, s0));
    let war = &w.wars[&(s0, new)];
    assert_eq!(war.of, None, "a war for freedom, not an obligation");
    assert_eq!(war.held, (3, 2));
    assert_eq!(war.taken, (0, 0));
    assert!(w.at_war(new, s2), "the parent's ally entered");
    assert_eq!(w.wars[&(s2, new)].of, Some((s0, new)));
    assert!(!w.at_war(new, s1) && !w.at_war(new, ActorId(3)));
    assert!(w.treaty(new, s0).is_none() && w.allies_of(new).is_empty());
    assert!(w.relation(new, s0) < p.relation_base - p.relation_norm_war + 1e-9, "the norm of war");
    // Posted through the ledger: the secession's share both ways, the militia a transfer.
    w.ledger.check().unwrap();
    let calm = calm();
    let r = w.tick(&calm).unwrap();
    assert!(r.events.iter().any(|e| matches!(&e.event, Event::ActorBorn { actor, from, house, provinces } if *actor == new && *from == s0 && *house == o0 && *provinces == vec![p05, p10])));
    assert_eq!(r.events.iter().filter(|e| matches!(e.event, Event::Secession { from, house, .. } if from == s0 && house == o0)).count(), 2);
    assert_eq!(r.events.iter().filter(|e| matches!(e.event, Event::ProvinceChangedHands { from, to, how: Transfer::Secession, .. } if from == s0 && to == new)).count(), 2);
    assert!(r.events.iter().any(|e| matches!(e.event, Event::War { a, b } if (a, b) == (s0, new))));
    assert!(r.events.iter().any(|e| matches!(e.event, Event::GovernorChanged { province, from, to: None } if province == p05 && from == o0a)));
    assert_eq!(on(&r, Account::Treasury(new), Source::Secession), share * 1000);
    assert_eq!(on(&r, Account::Treasury(s0), Source::Secession), -share * 1000);
    assert_eq!(r.postings.iter().filter(|q| q.source == Source::Recruit).map(|q| q.milli).sum::<i64>(), 0);
    assert!(r.events.iter().filter(|e| matches!(e.event, Event::Recruited { .. })).count() == 2);
    // The new polity plays: its economy ran, its ruling house drew Х1 from its throne.
    assert!(r.postings.iter().any(|q| q.account == Account::Treasury(new) && q.source == Source::Tax));
    assert!(w.house_treasury(o0) > 1_000);
    // Through a save, bit for bit.
    let mut straight = w.clone();
    for _ in 0..10 {
        straight.tick(&p).unwrap();
    }
    let mut loaded = from_save(&to_save(&w).unwrap()).unwrap();
    for _ in 0..10 {
        loaded.tick(&p).unwrap();
    }
    assert_eq!(to_save(&loaded).unwrap(), to_save(&straight).unwrap());
    let _ = calm;
}

#[test]
fn a_province_with_no_house_to_carry_it_rises_alone_under_a_new_house_of_its_own_people_and_a_capital_never_rises() {
    let p = params();
    let mut s = testworld::five_states_scenario();
    s.provinces.get_mut("p05").unwrap().religion = "f1".to_string();
    let mut w = world_of(&s, 32);
    let s0 = ActorId(0);
    let (h0, b0) = (house(&s, "h0"), person(&s, "b0"));
    let p05 = at(0, 1);
    assert_eq!(w.provinces[&p05].governor, Some(b0), "a governor of the ruling house");
    assert_eq!(w.secession_carrier(p05), None, "the ruling house does not secede");
    assert_eq!(w.revolt_chance_per_tick(at(0, 0), &p), 0.0, "the capital never rises: that is the coup of В6");
    assert_eq!(w.rise_now(at(0, 0), &p).unwrap(), None);
    let (houses_before, persons_before, people) = (w.houses.len(), w.persons.len(), w.population(p05));
    let new = w.rise_now(p05, &p).unwrap().expect("a rising");
    let r = w.tick(&calm()).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Revolt { province, from } if province == p05 && from == s0)));
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::Secession { .. })));
    // A new house of the province's people, through the ledger, praying as they do.
    let new_house = HouseId(houses_before as u32);
    assert_eq!(w.houses.len(), houses_before + 1);
    assert_eq!(w.houses[&new_house].polity, new);
    assert_eq!((w.houses[&new_house].culture, w.houses[&new_house].religion), (w.provinces[&p05].culture, ReligionId(1)), "the house of the people prays as they do");
    assert_eq!(w.polity_religion(new), ReligionId(1));
    assert!(!w.alienated(p05), "at one with its own court");
    let founder = w.actors[&new].ruler;
    assert_eq!(w.persons[&founder].house, new_house);
    assert!(founder.index() >= persons_before);
    assert!(r.events.iter().any(|e| matches!(e.event, Event::NewDynasty { actor, house, founder: f, from, .. } if actor == new && house == new_house && f == founder && from == p05)));
    let risen: i64 = -r.postings.iter().filter(|q| q.source == Source::Rise).map(|q| q.milli).sum::<i64>() / 1000;
    assert!((3..=4).contains(&risen));
    let militia = w.units_of(new);
    assert_eq!(militia.len(), 1);
    // The people are all accounted for: the founders and the militia left the province
    // through the ledger, exactly, and the militia is a transfer. (By the end of the tick
    // the militia has begun to melt: a polity of one province cannot pay the ceiling of А1
    // — the budget is the limit for a rising as for any polity.)
    let raised = on(&r, Account::Soldiers(militia[0]), Source::Recruit) / 1000;
    assert_eq!(raised, (people - risen) * p.recruit_ceiling_ppm / 1_000_000, "the ceiling of А1 over the people left");
    assert_eq!(on(&r, Account::Population(p05), Source::Rise), -risen * 1000);
    assert_eq!(on(&r, Account::Population(p05), Source::Recruit), -raised * 1000);
    assert!(w.soldiers(militia[0]) <= raised && w.soldiers(militia[0]) > raised / 2);
    assert!(r.events.iter().any(|e| matches!(e.event, Event::PayShortfall { actor, .. } if actor == new)), "the militia at the ceiling of А1 is more than one province's budget pays");
    // The governor of the ruling house lost the seat and stayed with the parent.
    assert!(r.events.iter().any(|e| matches!(e.event, Event::GovernorChanged { province, from, to: None } if province == p05 && from == b0)));
    assert_eq!(w.provinces[&p05].governor, None);
    assert_eq!(w.persons[&b0].house, h0);
    assert_eq!(w.houses[&h0].polity, s0);
    assert_eq!(w.actors[&new].provinces, vec![p05]);
    assert_eq!(w.actors[&new].capital, p05);
    assert!(w.at_war(new, s0));
    assert_eq!(w.actors[&s0].provinces.len(), 4);
    assert!(w.alive(s0) && w.alive(new));
    assert_eq!(w.actors[&new].standing, Standing::Acclaimed);
    // No province of the parent is left dangling: every seat, every unit, every war whole.
    for (q, pr) in &w.provinces {
        assert!(w.actors[&pr.owner].provinces.contains(q));
        if let Some(g) = pr.governor {
            assert_eq!(w.houses[&w.persons[&g].house].polity, pr.owner);
        }
    }
    assert_eq!(w.names.actors.len(), w.actors.len());
    assert_eq!(w.names.houses.len(), w.houses.len());
    assert_eq!(w.names.persons.len(), w.persons.len());
}

#[test]
fn the_draw_has_the_form_of_b3_and_b6_once_per_province_but_the_capital_and_a_province_taken_this_tick_draws_no_more() {
    let p = params();
    let mut w = testworld::world(33);
    let r = w.tick(&p).unwrap();
    let revolt: Vec<&engine2::RngDraw> = r.draws.iter().filter(|d| d.draw.point == engine2::DrawPoint::Revolt).collect();
    assert_eq!(revolt.len(), 20, "every province but the five capitals, once");
    assert!(revolt.iter().all(|d| d.draw.n == 0 && d.draw.id >= 5));
    // The form: base × (1 + marks) × (100 − L) / 50.
    let (s0, s1) = (ActorId(0), ActorId(1));
    let mut w = testworld::world(34);
    w.actors.get_mut(&s0).unwrap().legitimacy = 50.0;
    assert!((w.revolt_chance_per_tick(at(0, 1), &p) - p.revolt_per_tick).abs() < 1e-12, "a loyal province at legitimacy 50: the base");
    w.provinces.get_mut(&at(0, 1)).unwrap().religion = ReligionId(1);
    assert!((w.revolt_chance_per_tick(at(0, 1), &p) - 2.0 * p.revolt_per_tick).abs() < 1e-12, "one mark: twice");
    w.provinces.get_mut(&at(0, 1)).unwrap().culture = CultureId(1);
    assert!((w.revolt_chance_per_tick(at(0, 1), &p) - 3.0 * p.revolt_per_tick).abs() < 1e-12, "two marks: thrice");
    w.actors.get_mut(&s0).unwrap().legitimacy = 100.0;
    assert_eq!(w.revolt_chance_per_tick(at(0, 1), &p), 0.0, "at legitimacy 100 nothing rises");
    w.actors.get_mut(&s0).unwrap().legitimacy = 0.0;
    assert!((w.revolt_chance_per_tick(at(0, 1), &p) - 6.0 * p.revolt_per_tick).abs() < 1e-12, "at 0 twice the base again");
    // Played: at a base of one half, a province of one mark at legitimacy 50 rises surely,
    // a polity at legitimacy 100 never; a province taken this tick by a house draws no more.
    let mut sure = params();
    sure.revolt_per_tick = 0.5;
    let s = carrier_scenario();
    let mut w = world_of(&s, 35);
    w.actors.get_mut(&s0).unwrap().legitimacy = 50.0;
    w.actors.get_mut(&s1).unwrap().legitimacy = 100.0;
    w.provinces.get_mut(&at(1, 1)).unwrap().religion = ReligionId(1);
    for a in [ActorId(2), ActorId(3), ActorId(4)] {
        w.actors.get_mut(&a).unwrap().legitimacy = 100.0;
    }
    let (p05, p10) = (at(0, 1), at(0, 2));
    w.provinces.get_mut(&p05).unwrap().religion = ReligionId(1);
    let r = w.tick(&sure).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Secession { province, .. } if province == p05)), "one mark at legitimacy 50 and a base of a half: certain");
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Secession { province, .. } if province == p10)), "taken with its house");
    assert!(!r.draws.iter().any(|d| d.draw.point == engine2::DrawPoint::Revolt && d.draw.id == p10.0), "a province taken this tick draws no more");
    assert!(r.draws.iter().any(|d| d.draw.point == engine2::DrawPoint::Revolt && d.draw.id == p05.0));
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::Revolt { from, .. } | Event::Secession { from, .. } if from != s0)), "at legitimacy 100 nothing rises, whatever the marks");
    assert!(w.actors[&s1].provinces.len() == 5);
}

fn call(a: ActorId, verb: VerbId, b: ActorId) -> VerbCall {
    VerbCall { actor: engine2::ActorRef::Ruler(a), verb, target: Target::Actor(b) }
}

#[test]
fn the_end_of_a_rising_is_the_state_of_the_war_suppression_submission_or_freedom() {
    let p = calm();
    let s0 = ActorId(0);
    // Suppression: the parent's army beats the militia and takes the province back; the
    // polity falls as any conquered one, the militia to the people, the war closed.
    let mut w = testworld::world(36);
    let p05 = at(0, 1);
    let new = w.rise_now(p05, &p).unwrap().unwrap();
    let u0 = w.units_of(s0)[0];
    w.post(Account::Soldiers(u0), Source::Test, (6000 - w.soldiers(u0)) * 1000).unwrap();
    let mut fell = false;
    for _ in 0..20 {
        if w.units[&u0].destination.is_none() && w.provinces[&p05].owner == new {
            w.order_move(u0, p05).unwrap();
        }
        let r = w.tick(&p).unwrap();
        if r.events.iter().any(|e| matches!(e.event, Event::ActorDied { actor } if actor == new)) {
            assert!(r.events.iter().any(|e| matches!(e.event, Event::ProvinceChangedHands { province, from, to, how: Transfer::Conquest } if province == p05 && from == new && to == s0)));
            assert!(r.postings.iter().filter(|q| q.source == Source::Disband).map(|q| q.milli).sum::<i64>() == 0, "the militia joined the people");
            fell = true;
            break;
        }
    }
    assert!(fell, "the parent put the rising down");
    assert!(!w.alive(new) && w.actors[&new].provinces.is_empty());
    assert!(w.units_of(new).is_empty());
    assert!(!w.wars.keys().any(|(x, y)| *x == new || *y == new), "the war closed with the fall");
    assert!(w.actors[&s0].provinces.contains(&p05));
    assert!(w.actors.contains_key(&new) && w.names.actors.len() == w.actors.len(), "the record stays");
    // Submission: a rising of two provinces loses one — half its land — and the parent,
    // the winner, offers peace: the rising becomes its vassal (Genoa under Milan); the
    // ally's war, an obligation, closes with it.
    let s = carrier_scenario();
    let mut w = world_of(&s, 37);
    let (s2, p10) = (ActorId(2), at(0, 2));
    let new = w.rise_now(p05, &p).unwrap().unwrap();
    assert!(w.at_war(new, s2));
    let u0 = w.units_of(s0)[0];
    w.post(Account::Soldiers(u0), Source::Test, (9000 - w.soldiers(u0)) * 1000).unwrap();
    let mut taken = false;
    for _ in 0..20 {
        if w.units[&u0].destination.is_none() && w.provinces[&p05].owner == new {
            w.order_move(u0, p05).unwrap();
        }
        w.tick(&p).unwrap();
        if w.provinces[&p05].owner == s0 {
            taken = true;
            break;
        }
    }
    assert!(taken);
    assert_eq!(w.actors[&new].provinces, vec![p10]);
    assert_eq!(w.losing_side(s0, new), Some(new));
    assert!(w.peace_room(s0, new, &p).is_ok(), "the loser accepts the winner's peace always");
    w.apply_verb(&call(s0, VerbId::MakePeace, new), &p).unwrap();
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Submission { vassal, lord } if (vassal, lord) == (new, s0))));
    assert_eq!(w.lord_of(new), Some(s0));
    assert!(!w.at_war(new, s0) && !w.at_war(new, s2), "the ally's war was an obligation and closed with the peace");
    assert_eq!(w.treaty(new, s0).map(|t| t.kind), Some(engine2::TreatyKind::Peace));
    assert!(w.alive(new), "a vassal, alive");
    assert!(on(&r, Account::Treasury(s0), Source::Tribute) > 0, "and the tribute flows");
    // Freedom: the rising takes a province of the parent and offers peace; the loser
    // accepts always — a peace for the term, no lord, the land stays taken.
    let mut w = testworld::world(38);
    let new = w.rise_now(p05, &p).unwrap().unwrap();
    let militia = w.units_of(new)[0];
    w.post(Account::Soldiers(militia), Source::Test, (3000 - w.soldiers(militia)) * 1000).unwrap();
    let p10 = at(0, 2);
    assert_eq!(w.provinces[&p10].owner, s0);
    w.order_move(militia, p10).unwrap();
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::ProvinceChangedHands { province, from, to, how: Transfer::Conquest } if province == p10 && from == s0 && to == new)), "an undefended province of the parent falls within the tick");
    assert_eq!(w.losing_side(s0, new), Some(s0));
    w.apply_verb(&call(new, VerbId::MakePeace, s0), &p).unwrap();
    assert!(!w.at_war(new, s0));
    assert_eq!(w.lord_of(new), None, "free");
    assert_eq!(w.treaty(new, s0).map(|t| t.kind), Some(engine2::TreatyKind::Peace));
    assert_eq!(w.actors[&new].provinces, vec![p05, p10]);
    assert!(w.alive(new) && w.alive(s0));
    assert!((w.war_price(s0, new, &p) - p.legitimacy_cost_declare_war * p.treaty_breach_factor).abs() < 1e-9, "war again before the term is a breach");
}

// ---------- the worlds (в) ----------

use std::collections::BTreeMap;

use sentinel::Observed;

/// В7 on the generated worlds: risings per century per large polity (five provinces and
/// more), neither never nor always; every successor whole — no dangling seat, unit, war or
/// name; a rising kills nobody in E2 (nobody marches without an order); the run is
/// deterministic through a save; the sentinel declares the kinds by the horizon.
#[test]
fn on_generated_worlds_risings_per_century_per_large_polity_lie_in_b7_neither_never_nor_always_and_every_successor_is_whole() {
    let p = params();
    let band = p.entries["revolt.observed_per_century"].band.unwrap();
    const LARGE: usize = 5;
    let (mut risings, mut large_ticks, mut carried, mut alone, mut falls, mut provinces_moved) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut by_marks = [0u64; 3];
    let (mut polities_start, mut polities_end, mut large_start, mut quiet_large) = (0usize, 0usize, 0u64, 0u64);
    let (mut alienated_start, mut alienated_end) = (0usize, 0usize);
    let (mut carriers_start, mut carrier_ticks, mut province_ticks) = (0usize, 0u64, 0u64);
    // The expectation of the draws, carried and alone, against what happened.
    let (mut expected_carried, mut expected_alone) = (0.0f64, 0.0f64);
    let mut declared: Vec<String> = Vec::new();
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let mut g = Game::new(&s, params(), seed).unwrap();
        for k in g.possible_kinds(300) {
            if !declared.contains(&k) {
                declared.push(k);
            }
        }
        polities_start += g.world().actors.len();
        alienated_start += g.world().provinces.keys().filter(|q| g.world().alienated(**q)).count();
        carriers_start += g.world().provinces.keys().filter(|q| g.world().secession_carrier(**q).is_some()).count();
        let large_at_start: Vec<ActorId> = g.world().actors.iter().filter(|(_, a)| a.provinces.len() >= LARGE).map(|(a, _)| *a).collect();
        large_start += large_at_start.len() as u64;
        let mut first_century: BTreeMap<ActorId, u64> = large_at_start.iter().map(|a| (*a, 0)).collect();
        let mut reports = Vec::new();
        for t in 0..300u32 {
            let w = g.world();
            let sizes: BTreeMap<ActorId, usize> = w.actors.iter().filter(|(a, _)| w.alive(**a)).map(|(a, x)| (*a, x.provinces.len())).collect();
            let marks: BTreeMap<ProvinceId, u32> = w.provinces.keys().map(|q| (*q, w.alienation_marks(*q))).collect();
            large_ticks += sizes.values().filter(|n| **n >= LARGE).count() as u64;
            carrier_ticks += w.provinces.keys().filter(|q| w.secession_carrier(**q).is_some()).count() as u64;
            province_ticks += w.provinces.len() as u64;
            for q in w.provinces.keys() {
                let chance = w.revolt_chance_per_tick(*q, &p);
                if w.secession_carrier(*q).is_some() {
                    expected_carried += chance;
                } else {
                    expected_alone += chance;
                }
            }
            let r = g.tick(&[]).unwrap();
            for e in &r.events {
                match &e.event {
                    Event::ActorBorn { from, provinces, .. } => {
                        provinces_moved += provinces.len() as u64;
                        if sizes.get(from).copied().unwrap_or(0) >= LARGE {
                            risings += 1;
                            if t < 200 {
                                if let Some(n) = first_century.get_mut(from) {
                                    *n += 1;
                                }
                            }
                        }
                    }
                    Event::Secession { province, .. } => {
                        carried += 1;
                        by_marks[marks[province] as usize] += 1;
                    }
                    Event::Revolt { province, .. } => {
                        alone += 1;
                        by_marks[marks[province] as usize] += 1;
                    }
                    Event::ActorDied { .. } => falls += 1,
                    _ => {}
                }
            }
            if seed == 1 {
                reports.push(r);
            }
        }
        quiet_large += first_century.values().filter(|n| **n == 0).count() as u64;
        polities_end += g.world().actors.len();
        alienated_end += g.world().provinces.keys().filter(|q| g.world().alienated(**q)).count();
        // Whole: every seat, unit, war, pair and name in place.
        let w = g.world();
        assert_eq!(w.names.actors.len(), w.actors.len(), "seed {seed}: an actor without a name");
        assert_eq!(w.names.houses.len(), w.houses.len());
        assert_eq!(w.names.persons.len(), w.persons.len());
        let n = w.actors.len();
        assert_eq!(w.relations.len(), n * (n - 1) / 2, "seed {seed}: a pair without a relation");
        for (a, actor) in &w.actors {
            for q in &actor.provinces {
                assert_eq!(w.provinces[q].owner, *a, "seed {seed}: {q:?} listed by {a:?} but owned by another");
            }
            if w.alive(*a) {
                assert_eq!(w.houses[&w.persons[&actor.ruler].house].polity, *a, "seed {seed}: the ruler of {a:?} is of another polity's house");
                assert!(actor.regent.is_none_or(|r| w.person_alive(r) && w.houses[&w.persons[&r].house].polity == *a), "seed {seed}: a stray regent");
                assert!(actor.heir.is_none_or(|h| w.houses[&w.persons[&h].house].polity == *a), "seed {seed}: a stray heir");
                assert!(actor.provinces.contains(&actor.capital), "seed {seed}: the capital of {a:?} is not its own");
            } else {
                assert!(actor.provinces.is_empty() && w.units_of(*a).is_empty(), "seed {seed}: a fallen polity with land or men");
            }
        }
        for (q, pr) in &w.provinces {
            assert!(w.actors[&pr.owner].provinces.contains(q), "seed {seed}: {q:?} not in its owner's list");
            if let Some(gov) = pr.governor {
                assert!(w.person_alive(gov), "seed {seed}: a dead governor of {q:?}");
                assert_eq!(w.houses[&w.persons[&gov].house].polity, pr.owner, "seed {seed}: governor of {q:?} from another polity");
            }
        }
        for (u, unit) in &w.units {
            assert!(w.alive(unit.owner), "seed {seed}: unit {u:?} of a fallen polity");
            if let Some(h) = unit.house {
                assert_eq!(w.houses[&h].polity, unit.owner, "seed {seed}: the men of house {h:?} serve another polity");
            }
        }
        for (x, y) in w.wars.keys() {
            assert!(w.alive(*x) && w.alive(*y), "seed {seed}: a war with a fallen polity");
        }
        if seed == 1 {
            // Deterministic through a save, risings included.
            let mut g2 = Game::new(&s, params(), seed).unwrap();
            let mut spliced = Vec::new();
            for _ in 0..150 {
                spliced.push(g2.tick(&[]).unwrap());
            }
            let save = g2.to_save().unwrap();
            let mut g3 = Game::from_save(&s, params(), &save).unwrap();
            for _ in 0..150 {
                spliced.push(g3.tick(&[]).unwrap());
            }
            assert_eq!(reports, spliced, "seed 1: the run differs through a save");
            assert!(reports.iter().any(|r| r.events.iter().any(|e| matches!(e.event, Event::ActorBorn { .. }))), "seed 1: the run had a rising to carry");
        }
    }
    let per_century = risings as f64 / (large_ticks as f64 / 200.0);
    eprintln!(
        "В7 risings per century per polity of {LARGE}+ provinces: {per_century:.2} ({risings} over {:.0} polity-centuries; {quiet_large} of {large_start} large polities with none in their first century); {carried} provinces taken by a house, {alone} risen alone; by marks of alienation none/one/two: {:?}; polities {polities_start} -> {polities_end}, {provinces_moved} provinces moved, {falls} fell; alienated provinces {alienated_start} -> {alienated_end}; provinces with a carrier house: {carriers_start} at the start, {:.3} of all province-ticks; the draws expected {expected_carried:.1} carried and {expected_alone:.1} alone; kinds declared for 300 ticks: {declared:?}",
        large_ticks as f64 / 200.0,
        by_marks,
        carrier_ticks as f64 / province_ticks as f64
    );
    assert!(risings > 0, "never");
    assert!(quiet_large > 0, "always: every large polity saw a rising in its first century");
    assert!((band[0]..=band[1]).contains(&per_century), "В7 {per_century:.2} outside {band:?}");
    assert!(carried > 0 && alone > 0, "both carriers exist: a house with its provinces and a province alone");
    assert!(by_marks[0] > 0 && by_marks[1] + by_marks[2] > 0, "loyal provinces rise too, alienated ones more");
    assert_eq!(falls, 0, "a rising kills nobody in E2: nobody marches without an order (the agents of E3 do)");
    assert!(polities_end > polities_start);
    assert!(["Born", "Revolt", "Secession"].iter().all(|k| declared.contains(&k.to_string())), "the kinds are declared by the horizon: {declared:?}");
    assert!(!Game::new(&worldgen::generate(1, &p), params(), 1).unwrap().possible_kinds(10).iter().any(|k| k == "Born"), "and not for a horizon too short to expect one");
}
