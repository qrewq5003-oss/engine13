//! E2 step 6b acceptance (plan §18.3): legitimacy as a price.
//!
//! - the tax verbs move the rate by one step inside the band of Э4, the offer carries the
//!   price in legitimacy, raising costs it and lowering gives some back, the take follows
//!   the rate, and at the edge of the band the verb is closed with its reason;
//! - `enforce_order` takes its share of a year's revenue from the treasury for its gain in
//!   legitimacy, as one posting signed `Verb`; a treasury that cannot pay closes it.

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

#[test]
fn enforce_order_buys_legitimacy_with_a_share_of_a_years_revenue_and_closes_when_the_treasury_cannot_pay() {
    let p = params();
    let mut s = testworld::five_states_scenario();
    s.verbs.insert("enforce_order".to_string(), "Enforce order".to_string());
    let mut g = Game::new(&s, params(), 1).unwrap();
    let a = ActorId(0);
    let who = ActorRef::Ruler(a);
    let offer = |g: &Game| g.verbs(who).into_iter().find(|o| o.verb == VerbId::EnforceOrder).unwrap();
    // The starting treasury of the test world is a few ticks of surplus short of the price;
    // the economy fills it.
    let mut ticks = 0;
    while !offer(&g).available {
        assert_eq!(offer(&g).why.as_deref(), Some("cannot pay"));
        g.tick(&[]).unwrap();
        ticks += 1;
        assert!(ticks < 40, "the treasury never reached the price");
    }
    let cost = g.world().order_cost(a, &p);
    let yearly = g.world().production_per_tick(a, &p) * 2 * g.world().actors[&a].tax_rate_ppm / 1_000_000;
    assert!(cost > 0 && cost == yearly * p.order_cost_share_ppm / 1_000_000);
    let o = offer(&g);
    assert!(o.available && o.why.is_none());
    assert_eq!((o.cost_kg, o.cost_legitimacy), (cost, -(p.order_gain.round() as i64)));
    assert!(g.verbs(ActorRef::House(engine2::HouseId(0))).iter().all(|x| x.verb != VerbId::EnforceOrder || !x.available));
    // Through the world: the exact price on the treasury, the exact gain in legitimacy.
    let w = g.world_mut();
    let (t0, l0) = (w.treasury(a), w.actors[&a].legitimacy);
    let call = VerbCall { actor: who, verb: VerbId::EnforceOrder, target: Target::None };
    w.apply_verb(&call, &p).unwrap();
    assert_eq!(w.treasury(a), t0 - cost);
    assert!((w.actors[&a].legitimacy - (l0 + p.order_gain).min(100.0)).abs() < 1e-9);
    // Through the contract: the posting is one entry signed `Verb`, with its event. The
    // direct purchase left the treasury short; the economy fills it again.
    while !offer(&g).available {
        g.tick(&[]).unwrap();
    }
    let cost_now = g.world().order_cost(a, &p);
    let r = g.tick(&[call]).unwrap();
    let paid: Vec<_> = r.postings.iter().filter(|q| q.source == Source::Verb).collect();
    assert_eq!(paid.len(), 1);
    assert_eq!((paid[0].account, paid[0].milli), (engine2::Account::Treasury(a), -cost_now * 1000));
    assert!(r.events.iter().any(|e| matches!(e.event, Event::OrderEnforced { actor, .. } if actor == a)));
    // Legitimacy cannot pass 100: repeated purchases saturate, the treasury still pays.
    for _ in 0..30 {
        if !offer(&g).available {
            break;
        }
        g.tick(&[call]).unwrap();
        assert!(g.world().actors[&a].legitimacy <= 100.0);
    }
    // An empty treasury closes the verb with its reason.
    let w = g.world_mut();
    let t = w.treasury(a);
    w.post(engine2::Account::Treasury(a), Source::Verb, -t * 1000).unwrap();
    let o = offer(&g);
    assert!(!o.available);
    assert_eq!(o.why.as_deref(), Some("cannot pay"));
    assert!(matches!(g.world_mut().apply_verb(&call, &p), Err(engine2::economy::VerbError::NotAvailable(VerbId::EnforceOrder, "cannot pay"))));
}
