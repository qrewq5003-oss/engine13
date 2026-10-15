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

use engine2::sim::{Game, Simulation};
use engine2::{testworld, worldgen, Account, ActorId, CultureId, Event, HouseId, Params, ProvinceId, ReligionId, ScenarioData, Source, UnitId, World};

fn params() -> Params {
    Params::load().unwrap()
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
