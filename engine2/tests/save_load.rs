//! Acceptance of E1a (plan §4): a run interrupted by save → load continues bit for bit.

use engine2::world::Land;
use engine2::{from_save, testworld, to_save, Params};

#[test]
fn save_then_load_continues_the_same_run() {
    let seed = 0xA11CE;
    let params = Params::load().unwrap();

    let mut straight = testworld::world(seed);
    let mut straight_reports = Vec::new();
    for _ in 0..300 {
        straight_reports.push(straight.tick(&params).unwrap());
    }

    let mut first = testworld::world(seed);
    let mut reports = Vec::new();
    for _ in 0..150 {
        reports.push(first.tick(&params).unwrap());
    }
    // The save is taken in the middle of accumulated remainders: at least one province
    // carries a non-zero fraction, so the carry itself travels through the save.
    assert!(first.ledger.accounts().any(|a| first.ledger.carry(a).unwrap() != 0), "no remainder to save");
    let save = to_save(&first).unwrap();
    drop(first);
    let mut second = from_save(&save).unwrap();
    for _ in 0..150 {
        reports.push(second.tick(&params).unwrap());
    }

    assert_eq!(reports, straight_reports);
    assert_eq!(to_save(&second).unwrap(), to_save(&straight).unwrap());
    // And the run did move stocks: this is not an empty world.
    assert!(straight_reports.iter().any(|r| r.postings.iter().any(|p| p.milli != 0)));
}

#[test]
fn save_is_json_with_a_format_number_and_floats_round_trip() {
    let mut w = testworld::world(1);
    // A legitimacy value that does not print in shortest form without float_roundtrip.
    w.actors.get_mut(&engine2::ActorId(0)).unwrap().legitimacy = 0.1 + 0.2;
    let s = to_save(&w).unwrap();
    assert!(s.starts_with(&format!("{{\"format\":{},", engine2::SAVE_FORMAT)));
    let back = from_save(&s).unwrap();
    assert_eq!(back.actors[&engine2::ActorId(0)].legitimacy.to_bits(), (0.1f64 + 0.2).to_bits());
}

/// A save of format 24 written **before** the land's change (plan §31.33, decision № 7): the
/// generated world of seed 1 after 50 ticks, saved by the engine with four lands and the
/// arable at 40 (commit 50cdc6d). A new land in the closed list is an addition, not a change
/// of the world's shape: the format number stays, and this save must load. When the format
/// number is next raised, this file goes with the format it is of.
const SAVE_BEFORE_THE_LAND_CHANGE: &str = include_str!("data/save_24_before_the_land_change.json");

#[test]
fn a_save_written_before_the_lands_change_loads_and_goes_on() {
    let params = Params::load().unwrap();
    let mut w = from_save(SAVE_BEFORE_THE_LAND_CHANGE).expect("a save of format 24 written before the land's change loads");
    // Nothing is lost or defaulted on the way: the loaded world saves as the file reads.
    assert_eq!(to_save(&w).unwrap(), SAVE_BEFORE_THE_LAND_CHANGE);
    // The capacity of a province is derived once, when the world is made, and travels in the
    // save: an old save goes on with the capacities written into it — the arable at 40 —
    // not with the file's 50. What is read from the parameters every tick is the file's.
    let arable: Vec<_> = w.provinces.values().filter(|p| p.land == Land::Arable).collect();
    assert!(!arable.is_empty());
    assert!(arable.iter().all(|p| p.capacity == p.area_km2 * 40), "an old save keeps the capacities written into it");
    assert!(arable.iter().all(|p| p.capacity < params.capacity(p.area_km2, Land::Arable)));
    for _ in 0..50 {
        w.tick(&params).unwrap();
    }
}

/// A **game's** save written before the share of hills (E4, plan §31.44): `Game::to_save` on
/// main afc34ac — the test world, seed 0xE4, thirty ticks with the bots. This is the path of
/// a real load: `Game::from_save` takes the scenario and refuses a save whose facts hash
/// otherwise (`SimError::WrongScenario`), and the hash is of the facts' JSON — a field added
/// to a province's facts, written even when empty, would change the hash of every scenario
/// and turn every earlier save away. (It did, on the first cut of the share of hills; the
/// test of the world's save above loads past that check and saw nothing.) When the test
/// world's facts are changed on purpose, this file is written again.
const GAME_SAVE_BEFORE_THE_SHARE_OF_HILLS: &str = include_str!("data/game_save_before_the_share_of_hills.json");

#[test]
fn a_games_save_written_before_the_share_of_hills_loads_by_its_scenario_and_goes_on() {
    use engine2::sim::{Game, SimError, Simulation};
    let params = Params::load().unwrap();
    let scenario = testworld::five_states_scenario();
    let mut g = Game::from_save(&scenario, params.clone(), GAME_SAVE_BEFORE_THE_SHARE_OF_HILLS).expect("the scenario's facts hash as they did before the share of hills");
    assert_eq!(g.to_save().unwrap(), GAME_SAVE_BEFORE_THE_SHARE_OF_HILLS, "the loaded game saves byte for byte as it was");
    for _ in 0..10 {
        g.tick(&[]).unwrap();
    }
    // The check the load passed is alive: the same save against a scenario with a share of
    // hills in one province — other facts — is refused.
    let mut hilly = scenario.clone();
    hilly.provinces.values_mut().next().unwrap().hills = Some(0.25);
    assert!(matches!(Game::from_save(&hilly, params, GAME_SAVE_BEFORE_THE_SHARE_OF_HILLS), Err(SimError::WrongScenario { .. })), "a share of hills is another scenario");
}

const FACTS_BEFORE_THE_OPTIONAL_SOLDIERS: &str = include_str!("data/facts_before_the_optional_soldiers.json");

/// E4 (plan §31.50, executor's decision № 21): the starting army of a polity became optional
/// in the scenario and in the facts. A scenario that gives every number must be written as it
/// was — the facts' JSON byte for byte (the fixture is the test world's facts as main eb116e8
/// wrote them, before the change), so its hash stands and every save of it loads
/// (`a_games_save_written_before_the_share_of_hills_loads_by_its_scenario_and_goes_on` is
/// that load). A number left out is another fact — written as `null`, never as zero.
#[test]
fn the_facts_of_a_scenario_that_gives_its_armies_are_written_byte_for_byte_as_before() {
    use engine2::sim::{Game, SimError, Simulation};
    let params = Params::load().unwrap();
    let scenario = testworld::five_states_scenario();
    let facts = scenario.compile(&params).unwrap().facts;
    assert_eq!(serde_json::to_string(&facts).unwrap(), FACTS_BEFORE_THE_OPTIONAL_SOLDIERS);
    assert_eq!(facts.hash(), 17_085_394_115_506_308_705);
    assert!(FACTS_BEFORE_THE_OPTIONAL_SOLDIERS.contains("\"soldiers\":600"), "the fixture carries armies given by number");

    let with = |soldiers: Option<u64>| {
        let mut s = scenario.clone();
        s.actors.get_mut("s1").unwrap().soldiers = soldiers;
        s
    };
    let left_out = serde_json::to_string(&with(None).compile(&params).unwrap().facts).unwrap();
    let zero = serde_json::to_string(&with(Some(0)).compile(&params).unwrap().facts).unwrap();
    assert_eq!(left_out.matches("\"soldiers\":null").count(), 1, "a number left out is written as none");
    assert_eq!(zero.matches("\"soldiers\":null").count(), 0);
    assert_ne!(left_out, zero, "none is not zero: «no number» and «no army» are two facts");
    assert_ne!(with(None).compile(&params).unwrap().facts.hash(), facts.hash());
    // And the old save is of the scenario that gave the number, not of the one that left it out.
    assert!(matches!(Game::from_save(&with(None), params, GAME_SAVE_BEFORE_THE_SHARE_OF_HILLS), Err(SimError::WrongScenario { .. })));
}
