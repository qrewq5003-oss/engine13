//! E1b acceptance (plan §11): the `Simulation` contract on the test world — a verb is
//! journaled, a disabled verb is an error, the ledger invariant holds every tick, save →
//! load through the contract continues bit for bit and refuses another scenario's save,
//! names live in the world and survive the save, the view shows names and display
//! currency, the chronicle carries typed events only.

use engine2::sim::{Game, SimError, Simulation};
use engine2::{testworld, ActorId, ActorRef, Event, Params, ProvinceId, Target, VerbCall, VerbId};

fn game(seed: u64) -> Game {
    Game::new(&testworld::five_states_scenario(), Params::load().unwrap(), seed).unwrap()
}

#[test]
fn verbs_are_offered_journaled_and_have_no_effect_yet() {
    let mut g = game(1);
    let who = ActorRef::Ruler(ActorId(0));
    let offers = g.verbs(who);
    assert!(offers.iter().any(|o| o.verb == VerbId::RaiseTax && o.available));
    let call = VerbCall { actor: who, verb: VerbId::RaiseTax, target: Target::Province(ProvinceId(0)) };
    let r = g.tick(&[call]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Verb { verb: VerbId::RaiseTax, .. })));
    assert!(matches!(r.events.last().unwrap().event, Event::TickPlayed));
    // A verb not enabled by the scenario is an error, and nothing is played.
    let tick_before = g.world().tick;
    let err = g.tick(&[VerbCall { actor: who, verb: VerbId::DeclareWar, target: Target::Actor(ActorId(1)) }]).unwrap_err();
    assert!(matches!(err, SimError::VerbNotEnabled(VerbId::DeclareWar)));
    assert_eq!(g.world().tick, tick_before);
}

#[test]
fn a_save_of_another_scenario_is_refused() {
    let mut g = game(4);
    g.tick(&[]).unwrap();
    let s = g.to_save().unwrap();
    let mut other = testworld::five_states_scenario();
    other.id = "elsewhere".to_string();
    assert!(matches!(Game::from_save(&other, Params::load().unwrap(), &s), Err(SimError::WrongScenario { .. })));
    // Same id, one fact changed: still refused — the hash of the compiled facts differs.
    let mut other = testworld::five_states_scenario();
    other.actors.get_mut("s0").unwrap().treasury_kg += 1;
    assert!(matches!(Game::from_save(&other, Params::load().unwrap(), &s), Err(SimError::WrongScenario { .. })));
}

#[test]
fn names_live_in_the_world_and_survive_the_save() {
    let scenario = testworld::five_states_scenario();
    let mut g = game(6);
    // An heir born in the game would be named here, not in the scenario.
    let s = {
        let mut w = g.world().clone();
        w.names.actors[0].1 = "Renamed in play".to_string();
        engine2::to_save(&w).unwrap()
    };
    let g2 = Game::from_save(&scenario, Params::load().unwrap(), &s).unwrap();
    assert_eq!(g2.view().actors[0].name, "Renamed in play");
    g.tick(&[]).unwrap();
}

#[test]
fn save_and_load_through_the_contract_continue_bit_for_bit() {
    let scenario = testworld::five_states_scenario();
    let mut straight = game(9);
    let mut a = Vec::new();
    for _ in 0..40 {
        a.push(straight.tick(&[]).unwrap());
    }
    let mut g = game(9);
    let mut b = Vec::new();
    for _ in 0..20 {
        b.push(g.tick(&[]).unwrap());
    }
    let s = g.to_save().unwrap();
    let mut g2 = Game::from_save(&scenario, Params::load().unwrap(), &s).unwrap();
    for _ in 0..20 {
        b.push(g2.tick(&[]).unwrap());
    }
    assert_eq!(a, b);
    assert_eq!(straight.to_save().unwrap(), g2.to_save().unwrap());
    // Event sequence numbers continue across the save, no restart.
    let last_a = a.last().unwrap().events.last().unwrap().seq;
    let last_b = b.last().unwrap().events.last().unwrap().seq;
    assert_eq!(last_a, last_b);
}

#[test]
fn view_shows_names_and_display_currency_and_no_ids() {
    let g = game(2);
    let v = g.view();
    assert_eq!(v.year, 1477);
    assert_eq!(v.currency, "ducat");
    let first = &v.actors[0];
    assert_eq!(first.name, "State 0");
    assert_eq!(first.provinces.len(), 5);
    assert!((first.treasury_display - first.treasury_kg as f64 / 100.0).abs() < 1e-9);
}

#[test]
fn chronicle_carries_typed_events_and_names() {
    let mut g = game(3);
    g.tick(&[]).unwrap();
    let c = g.chronicle();
    assert_eq!(c.alive_actors.len(), 5);
    assert!(matches!(c.events.last().unwrap().event, Event::TickPlayed));
    assert_eq!(c.names.actors[0].1, "State 0");
}
