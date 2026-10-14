//! E2 step 6b acceptance (plan §18.3): legitimacy as a price.
//!
//! - the tax verbs move the rate by one step inside the band of Э4, the offer carries the
//!   price in legitimacy, raising costs it and lowering gives some back, the take follows
//!   the rate, and at the edge of the band the verb is closed with its reason.

use engine2::sim::{Game, Simulation};
use engine2::{testworld, ActorId, ActorRef, Event, Params, Source, Target, VerbCall, VerbId, World};

fn params() -> Params {
    Params::load().unwrap()
}

fn world() -> World {
    testworld::world(1)
}

#[test]
fn the_tax_verbs_move_the_rate_one_step_for_a_price_in_legitimacy_and_close_at_the_edges_of_the_band() {
    let p = params();
    let mut w = world();
    let a = ActorId(0);
    let raise = VerbCall { actor: ActorRef::Ruler(a), verb: VerbId::RaiseTax, target: Target::None };
    let lower = VerbCall { actor: ActorRef::Ruler(a), verb: VerbId::LowerTax, target: Target::None };
    let (rate0, l0) = (w.actors[&a].tax_rate_ppm, w.actors[&a].legitimacy);
    assert_eq!(rate0, p.tax_default_ppm);
    w.apply_verb(&raise, &p).unwrap();
    assert_eq!(w.actors[&a].tax_rate_ppm, rate0 + p.tax_step_ppm);
    assert!((w.actors[&a].legitimacy - (l0 - p.legitimacy_cost_raise_tax)).abs() < 1e-9, "the exact price");
    w.apply_verb(&lower, &p).unwrap();
    assert_eq!(w.actors[&a].tax_rate_ppm, rate0);
    assert!((w.actors[&a].legitimacy - (l0 - p.legitimacy_cost_raise_tax + p.legitimacy_gain_lower_tax)).abs() < 1e-9, "lowering gives the gain back");
    // A target the verb does not take is refused and changes nothing.
    let bad = VerbCall { target: Target::Actor(a), ..raise };
    assert!(matches!(w.apply_verb(&bad, &p), Err(engine2::economy::VerbError::BadTarget(VerbId::RaiseTax, _))));
    assert_eq!(w.actors[&a].tax_rate_ppm, rate0);
    // The take follows the rate: a tick at the raised rate collects more than one at the
    // base rate, by the step's share of production.
    let production = w.production_per_tick(a, &p);
    w.apply_verb(&raise, &p).unwrap();
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::TaxRateChanged { actor, from_ppm, to_ppm } if actor == a && from_ppm == rate0 && to_ppm == rate0 + p.tax_step_ppm)));
    let tax: i64 = r.postings.iter().filter(|q| q.account == engine2::Account::Treasury(a) && q.source == Source::Tax).map(|q| q.milli).sum();
    assert_eq!(tax / 1000, production * (rate0 + p.tax_step_ppm) / 1_000_000);
    // Up to the top of the band and no further; down to the bottom and no further.
    let mut raised = 0;
    while w.apply_verb(&raise, &p).is_ok() {
        raised += 1;
        assert!(raised < 100);
    }
    assert_eq!(w.actors[&a].tax_rate_ppm + p.tax_step_ppm > p.tax_band_ppm.1, true);
    assert!(w.actors[&a].tax_rate_ppm <= p.tax_band_ppm.1);
    assert!(matches!(w.apply_verb(&raise, &p), Err(engine2::economy::VerbError::NotAvailable(VerbId::RaiseTax, "at the top of the tax band (Э4)"))));
    assert!(w.actors[&a].legitimacy >= 0.0, "legitimacy stays inside 0–100");
    let mut lowered = 0;
    while w.apply_verb(&lower, &p).is_ok() {
        lowered += 1;
        assert!(lowered < 100);
    }
    assert!(w.actors[&a].tax_rate_ppm >= p.tax_band_ppm.0 && w.actors[&a].tax_rate_ppm - p.tax_step_ppm < p.tax_band_ppm.0);
    assert!(matches!(w.apply_verb(&lower, &p), Err(engine2::economy::VerbError::NotAvailable(VerbId::LowerTax, "at the bottom of the tax band (Э4)"))));
    assert!(w.actors[&a].legitimacy <= 100.0);
}

#[test]
fn the_offer_carries_the_price_in_legitimacy_closes_at_the_edge_with_its_reason_and_a_house_is_offered_nothing() {
    let p = params();
    let s = testworld::five_states_scenario();
    let mut g = Game::new(&s, params(), 1).unwrap();
    let who = ActorRef::Ruler(ActorId(0));
    let offer = |g: &Game, v: VerbId| g.verbs(who).into_iter().find(|o| o.verb == v).unwrap();
    let raise = offer(&g, VerbId::RaiseTax);
    assert!(raise.available && raise.why.is_none());
    assert_eq!((raise.cost_kg, raise.cost_legitimacy), (0, p.legitimacy_cost_raise_tax.round() as i64));
    let lower = offer(&g, VerbId::LowerTax);
    assert!(lower.available);
    assert_eq!(lower.cost_legitimacy, -(p.legitimacy_gain_lower_tax.round() as i64), "lowering gives: a negative price");
    // A house does not rule the polity: nothing on offer.
    let house = g.verbs(ActorRef::House(engine2::HouseId(0)));
    assert!(house.iter().filter(|o| matches!(o.verb, VerbId::RaiseTax | VerbId::LowerTax)).all(|o| !o.available && o.why.as_deref() == Some("a house does not rule the polity")));
    // Raise through the contract until the offer closes with its reason; the journal
    // carries the verb and the change.
    let call = VerbCall { actor: who, verb: VerbId::RaiseTax, target: Target::None };
    let mut ticks = 0;
    while offer(&g, VerbId::RaiseTax).available {
        let r = g.tick(&[call]).unwrap();
        assert!(r.events.iter().any(|e| matches!(e.event, Event::Verb { verb: VerbId::RaiseTax, .. })));
        assert!(r.events.iter().any(|e| matches!(e.event, Event::TaxRateChanged { actor: ActorId(0), .. })));
        ticks += 1;
        assert!(ticks < 20);
    }
    let closed = offer(&g, VerbId::RaiseTax);
    assert_eq!(closed.why.as_deref(), Some("at the top of the tax band (Э4)"));
    assert_eq!(g.world().actors[&ActorId(0)].tax_rate_ppm, p.tax_band_ppm.1);
    assert!(g.tick(&[call]).is_err(), "the contract refuses what the offer closed");
}
