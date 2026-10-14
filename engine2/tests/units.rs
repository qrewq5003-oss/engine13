//! E2 step 2 acceptance (plan §14), updated for step 4 (plan §16): units reach islands and
//! colonies by the sea links, an enclave is unreachable without passage but its garrison is
//! paid like the rest of the army, foreign land is closed, an unpaid or foreign unit melts
//! through the ledger, and all of it holds on the generated worlds.

use engine2::sim::{Game, Simulation};
use engine2::{testworld, worldgen, ActorId, Event, Params, ProvinceId, UnitId, World};

fn params() -> Params {
    Params::load().unwrap()
}

fn by_name(s: &engine2::ScenarioData, name: &str) -> ProvinceId {
    ProvinceId(s.provinces.keys().position(|k| k == name).unwrap() as u32)
}

fn actor_by_name(s: &engine2::ScenarioData, name: &str) -> ActorId {
    ActorId(s.actors.keys().position(|k| k == name).unwrap() as u32)
}

fn world_of(s: &engine2::ScenarioData) -> World {
    let c = s.compile(&params()).unwrap();
    World::from_facts(&c.facts, c.names, &params(), 1).unwrap()
}

fn extreme(name: &str) -> engine2::ScenarioData {
    worldgen::extreme_worlds(&params()).into_iter().find(|(n, _)| *n == name).map(|(_, s)| s).unwrap()
}

#[test]
fn a_unit_crosses_the_sea_to_the_colony_and_the_island_is_reachable_by_sea_only() {
    let s = extreme("colony_and_enclave");
    let mut w = world_of(&s);
    let b = actor_by_name(&s, "s01");
    let unit = w.units_of(b)[0];
    let island = s.provinces.iter().filter(|(_, p)| p.owner == "s01" && p.coastal && p.neighbours.len() <= 1).map(|(k, _)| by_name(&s, k)).next().unwrap();
    assert!(w.provinces[&island].sea_neighbours.iter().any(|n| w.provinces[n].owner == b), "the generator must link the island by sea");
    let path = w.path(b, w.units[&unit].position, island).expect("a sea path exists");
    assert!(path.iter().any(|(_, way)| *way == engine2::movement::Way::Sea));
    w.order_move(unit, island).unwrap();
    let mut moved = Vec::new();
    for _ in 0..path.len() {
        let r = w.tick(&params()).unwrap();
        moved.extend(r.events.iter().filter_map(|e| match &e.event {
            Event::UnitMoved { unit: u, way, .. } if *u == unit => Some(*way),
            _ => None,
        }));
    }
    assert_eq!(w.units[&unit].position, island);
    assert!(moved.contains(&engine2::movement::Way::Sea));
    assert!(w.units[&unit].destination.is_none());
}

#[test]
fn the_enclave_is_unreachable_without_passage_but_its_garrison_is_paid_like_the_rest() {
    let s = extreme("colony_and_enclave");
    let mut w = world_of(&s);
    let b = actor_by_name(&s, "s01");
    let enclave = s.provinces.iter().filter(|(_, p)| p.owner == "s01" && p.neighbours.iter().all(|n| s.provinces[n].owner == "s00") && !p.neighbours.is_empty()).map(|(k, _)| by_name(&s, k)).next().unwrap();
    let unit = w.units_of(b)[0];
    assert!(w.path(b, w.units[&unit].position, enclave).is_none(), "no passage through s00's land yet");
    w.order_move(unit, enclave).unwrap();
    let r = w.tick(&params()).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::NoPath { unit: u, to } if u == unit && to == enclave)));
    assert!(w.units[&unit].destination.is_none(), "a dropped order is not retried forever");
    // A garrison inside the enclave is paid from the treasury like any unit: within the
    // budget it keeps its men without any land link home.
    let garrison = UnitId(w.next_unit);
    w.next_unit += 1;
    w.units.insert(garrison, engine2::world::Unit { owner: b, position: enclave, quality: 50.0, destination: None, mercenary: false });
    let room = w.sustainable_soldiers(b, &params()) - w.units_of(b).iter().map(|u| w.soldiers(*u)).sum::<i64>();
    let size = (room / 2).max(1);
    w.ledger.open(w.tick, engine2::Account::Soldiers(garrison), size).unwrap();
    for _ in 0..5 {
        let r = w.tick(&params()).unwrap();
        assert!(!r.events.iter().any(|e| matches!(e.event, Event::Attrition { unit, .. } if unit == garrison)));
    }
    assert_eq!(w.soldiers(garrison), size);
}

#[test]
fn a_foreign_unit_beyond_what_the_land_feeds_melts_at_the_attrition_rate() {
    let p = params();
    let mut w = testworld::world(3);
    let a = ActorId(0);
    let unit = w.units_of(a)[0];
    // On foreign land a unit forages (tests/army.rs); on emptied land nothing feeds it and
    // the whole unit melts, down to zero and a BoundHit.
    let foreign = w.actors[&ActorId(1)].capital;
    let people = w.population(foreign);
    w.post(engine2::Account::Population(foreign), engine2::Source::Test, -people * 1000).unwrap();
    w.units.get_mut(&unit).unwrap().position = foreign;
    let before = w.soldiers(unit);
    let r = w.tick(&p).unwrap();
    let lost = before - w.soldiers(unit);
    let expected = (before * 150 + 999) / 1000; // 0.30 a year → 0.15 a tick, ceiling of the thousandths
    assert_eq!(lost, expected, "lost {lost}, expected {expected}");
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Attrition { unit: u, unsupplied } if u == unit && unsupplied == before as u64)));
    // (Migration refills the emptied province from its neighbours, so the unit forages again
    // after a few ticks; the melt to zero with a BoundHit is the ledger's own test.)
    // At home an army the treasury cannot pay forages its own province (tests/economy.rs).
}

#[test]
fn foreign_land_is_closed_and_own_land_is_open_through_the_chokepoint() {
    let s = extreme("chokepoint");
    let mut w = world_of(&s);
    let a = actor_by_name(&s, "s00");
    let b = actor_by_name(&s, "s01");
    let ua = w.units_of(a)[0];
    let far_a = s.provinces.iter().filter(|(_, p)| p.owner == "s00").map(|(k, _)| by_name(&s, k)).max().unwrap();
    assert!(w.path(a, w.units[&ua].position, far_a).is_some());
    let b_home = w.actors[&b].capital;
    assert!(w.path(a, w.units[&ua].position, b_home).is_none());
    w.order_move(ua, far_a).unwrap();
    for _ in 0..20 {
        w.tick(&params()).unwrap();
        if w.units[&ua].position == far_a {
            break;
        }
    }
    assert_eq!(w.units[&ua].position, far_a);
}

#[test]
fn on_generated_worlds_every_unit_starts_within_the_budget_and_unreachable_provinces_are_enclaves_by_land_and_sea() {
    let p = params();
    let mut overseas_reached = 0;
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let w = world_of(&s);
        for (id, u) in &w.units {
            assert!(w.soldiers(*id) <= w.sustainable_soldiers(u.owner, &p), "seed {seed}: unit {id:?} starts over its polity's budget");
            let mine = &w.actors[&u.owner].provinces;
            let reachable: Vec<ProvinceId> = mine.iter().copied().filter(|t| w.path(u.owner, u.position, *t).is_some()).collect();
            for target in mine {
                if reachable.contains(target) {
                    if w.path(u.owner, u.position, *target).unwrap().iter().any(|(_, way)| *way == engine2::movement::Way::Sea) {
                        overseas_reached += 1;
                    }
                    continue;
                }
                let pr = &w.provinces[target];
                let touches_reachable = pr.neighbours.iter().chain(pr.sea_neighbours.iter()).any(|n| reachable.contains(n));
                assert!(!touches_reachable, "seed {seed}: province {target:?} of {:?} touches reachable land yet is unreachable", u.owner);
            }
        }
    }
    assert!(overseas_reached > 0, "no generated world had a colony reachable by sea");
}

#[test]
fn a_tick_with_units_is_still_deterministic_through_save_and_load() {
    let s = extreme("colony_and_enclave");
    let mut g = Game::new(&s, params(), 4).unwrap();
    let b = actor_by_name(&s, "s01");
    let unit = g.world().units_of(b)[0];
    let island = s.provinces.iter().filter(|(_, p)| p.owner == "s01" && p.coastal && p.neighbours.len() <= 1).map(|(k, _)| by_name(&s, k)).next().unwrap();
    g.world_mut().order_move(unit, island).unwrap();
    let mut a = Vec::new();
    for _ in 0..6 {
        a.push(g.tick(&[]).unwrap());
    }
    let mut g2 = Game::new(&s, params(), 4).unwrap();
    g2.world_mut().order_move(unit, island).unwrap();
    let mut bb = Vec::new();
    for _ in 0..3 {
        bb.push(g2.tick(&[]).unwrap());
    }
    let save = g2.to_save().unwrap();
    let mut g3 = Game::from_save(&s, params(), &save).unwrap();
    for _ in 0..3 {
        bb.push(g3.tick(&[]).unwrap());
    }
    assert_eq!(a, bb);
}
