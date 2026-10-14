//! E2 step 6b acceptance (plan §18.3): legitimacy as a price.
//!
//! - the tax verbs move the rate by one step inside the band of Э4, the offer carries the
//!   price in legitimacy, raising costs it and lowering gives some back, the take follows
//!   the rate, and at the edge of the band the verb is closed with its reason;
//! - a lasting decision shifts the norm (review of #286): every step of the rate above the
//!   tithe lowers the norm while the rate holds, every step below raises it; a default
//!   lowers the norm for the term of the closed credit and the interest stays at the top of
//!   Э6 afterwards;
//! - the tax collected is the assigned tax times a share that falls with legitimacy below
//!   full collection (review of #286: legitimacy costs the polity, not only the ruler);
//!   paired runs are judged by the outcome of brief §1 — income, provinces, the houses'
//!   seats and members — never by legitimacy as a number: a greedy ruler (tax to the top,
//!   order whenever below the norm) gains at most a quarter over the plain one in net
//!   income, on the test world and on generated worlds, where the greedy take stays inside
//!   Э8; a borrower who spends the loan ends with fewer resources;
//! - `enforce_order` takes its share of a year's revenue from the treasury for its gain in
//!   legitimacy, as one posting signed `Verb`; a treasury that cannot pay closes it;
//! - a debt past the ceiling whose interest the treasury cannot meet is a default: written
//!   off on the debt alone, paid in legitimacy, the credit closed for the term — no loan for
//!   the pay, none to the verb — and open again when the term ends;
//! - a usurper is the regent, else a governor, else the eldest adult of any house of the
//!   polity (owner's rule on §18); the ruler is executed, the usurper holds the throne as
//!   `Usurper` with the lowest norm; no candidate — no usurpation; on 20 generated worlds
//!   over 300 ticks usurpations per century per polity lie in В6, neither never nor always.

use std::collections::BTreeMap;

use engine2::event::Cause;
use engine2::sim::{Game, Simulation};
use engine2::{testworld, worldgen, ActorId, ActorRef, Event, Params, PersonId, ProvinceId, ScenarioData, Source, Standing, Succession, Target, VerbCall, VerbId, World};

fn person(s: &ScenarioData, sid: &str) -> PersonId {
    PersonId(s.persons.keys().position(|k| k == sid).unwrap() as u32)
}

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
    // The take follows the rate — through the share collected, which the price in
    // legitimacy has just lowered.
    let production = w.production_per_tick(a, &p);
    w.apply_verb(&raise, &p).unwrap();
    let share = w.collection_share_ppm(a, &p);
    assert!(share < 1_000_000, "the blow to legitimacy costs collection at once");
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::TaxRateChanged { actor, from_ppm, to_ppm } if actor == a && from_ppm == rate0 && to_ppm == rate0 + p.tax_step_ppm)));
    let tax: i64 = r.postings.iter().filter(|q| q.account == engine2::Account::Treasury(a) && q.source == Source::Tax).map(|q| q.milli).sum();
    assert_eq!(tax / 1000, production * (rate0 + p.tax_step_ppm) / 1_000_000 * share / 1_000_000);
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

#[test]
fn a_debt_past_the_ceiling_the_treasury_cannot_serve_is_a_default_written_off_paid_in_legitimacy_with_the_credit_closed_for_the_term() {
    let p = params();
    let mut s = testworld::five_states_scenario();
    s.verbs.insert("take_loan".to_string(), "Take a loan".to_string());
    let mut g = Game::new(&s, params(), 1).unwrap();
    let a = ActorId(0);
    let who = ActorRef::Ruler(a);
    let loan = VerbCall { actor: who, verb: VerbId::TakeLoan, target: Target::None };
    g.world_mut().actors.get_mut(&a).unwrap().institutions.insert(engine2::Institution::Bank);
    // A debt of twice the ceiling, as the ledger of the test sets it, and an empty treasury:
    // the interest of the tick cannot be met.
    let w = g.world_mut();
    let ceiling = w.debt_ceiling(a, &p);
    assert!(ceiling > 0);
    w.post(engine2::Account::Debt(a), Source::Test, 2 * ceiling * 1000).unwrap();
    let t = w.treasury(a);
    w.post(engine2::Account::Treasury(a), Source::Test, -t * 1000).unwrap();
    let debt = w.debt(a);
    let l0 = w.actors[&a].legitimacy;
    let r = g.tick(&[]).unwrap();
    let written_off = r.events.iter().find_map(|e| match e.event {
        Event::Default { actor, written_off_kg } if actor == a => Some(written_off_kg),
        _ => None,
    });
    assert!(written_off.is_some(), "no default on a debt past the ceiling with its interest unpaid");
    let w = g.world();
    assert_eq!(w.debt(a), 0, "the debt is written off");
    let off: Vec<_> = r.postings.iter().filter(|q| q.source == Source::Default).collect();
    assert_eq!(off.len(), 1, "one entry, on the debt alone");
    assert_eq!(off[0].account, engine2::Account::Debt(a));
    assert_eq!(-off[0].milli / 1000, written_off.unwrap() as i64);
    assert!(-off[0].milli / 1000 >= debt, "the interest of the tick was on the debt when it was written off");
    // The price: legitimacy, with the tick's drift toward the norm on top.
    let norm = w.legitimacy_norm(a, &p);
    let after_price = (l0 - p.legitimacy_cost_default).clamp(0.0, 100.0);
    let expected = after_price + (norm - after_price) * p.legitimacy_rate_per_tick;
    assert!((w.actors[&a].legitimacy - expected).abs() < 1e-9, "{} vs {expected}", w.actors[&a].legitimacy);
    // No credit: the verb is closed with its reason, and the pay borrows nothing.
    let offer = |g: &Game| g.verbs(who).into_iter().find(|o| o.verb == VerbId::TakeLoan).unwrap();
    assert_eq!(offer(&g).why.as_deref(), Some("in default: no credit"));
    assert!(matches!(g.world_mut().apply_verb(&loan, &p), Err(engine2::economy::VerbError::NotAvailable(VerbId::TakeLoan, "in default: no credit"))));
    let unit = g.world().units_of(a)[0];
    let limit = g.world().sustainable_soldiers(a, &p);
    let soldiers = g.world().soldiers(unit);
    g.world_mut().post(engine2::Account::Soldiers(unit), Source::Recruit, (limit * 3 - soldiers).max(0) * 1000).unwrap();
    let until = g.world().actors[&a].credit_closed_until.expect("the term is set");
    assert_eq!(until.0, g.world().tick.0 - 1 + p.default_ticks_without_credit);
    let mut restored_at = None;
    let (mut norm_closed, mut ruler_closed, mut standing_closed) = (0.0, g.world().actors[&a].ruler, g.world().actors[&a].standing);
    for i in 1..=(p.default_ticks_without_credit + 2) {
        let r = g.tick(&[]).unwrap();
        if r.events.iter().any(|e| matches!(e.event, Event::CreditRestored { actor } if actor == a)) {
            restored_at = Some(i);
            break;
        }
        assert!(!r.postings.iter().any(|q| q.source == Source::Loan), "a loan while the credit is closed");
        assert_eq!(g.world().debt(a), 0);
        if g.world().credit_closed(a) {
            let w = g.world();
            if w.actors[&a].standing == Standing::Hereditary && w.is_adult(w.actors[&a].ruler, &p) {
                assert!((w.legitimacy_norm(a, &p) - (p.legitimacy_norm_hereditary - p.legitimacy_norm_penalty_in_default)).abs() < 1e-9, "the norm sits lower for the whole term");
            }
            norm_closed = w.legitimacy_norm(a, &p);
            ruler_closed = w.actors[&a].ruler;
            standing_closed = w.actors[&a].standing;
        }
        // Closed up to the tick of the term; open from it, before its economy says so.
        assert_eq!(g.world().credit_closed(a), g.world().tick < until);
        assert_eq!(offer(&g).available, g.world().tick >= until);
    }
    assert_eq!(restored_at, Some(p.default_ticks_without_credit), "the credit opens exactly when the term ends");
    assert!(!g.world().credit_closed(a));
    assert!(offer(&g).available, "with the credit open the loan is on offer again");
    // The price lasts: the norm was lower for the whole term and is back now; the interest
    // stays at the top of Э6.
    let w = g.world();
    if w.actors[&a].ruler == ruler_closed && w.actors[&a].standing == standing_closed {
        assert!((w.legitimacy_norm(a, &p) - (norm_closed + p.legitimacy_norm_penalty_in_default)).abs() < 1e-9, "the norm is back after the term");
    }
    assert!(w.actors[&a].defaulted);
    assert_eq!(w.interest_ppm_per_tick(a, &p), p.loan_interest_after_default_ppm_per_tick);
    assert!(p.loan_interest_after_default_ppm_per_tick > p.loan_interest_ppm_per_tick);
    g.tick(&[loan]).unwrap();
    let debt = g.world().debt(a);
    assert!(debt > 0);
    let r = g.tick(&[]).unwrap();
    let interest: i64 = r.postings.iter().filter(|q| q.source == Source::Interest).map(|q| q.milli.abs()).sum();
    assert_eq!(interest / 1000, debt * p.loan_interest_after_default_ppm_per_tick / 1_000_000, "interest at the top of the band");
}

#[test]
fn the_usurper_is_the_regent_else_a_governor_else_the_eldest_adult_of_any_house_and_nobody_when_there_is_no_candidate() {
    let p = params();
    let s = testworld::five_states_scenario();
    let s0 = ActorId(0);
    let (r0, c0, b0, other) = (person(&s, "r0"), person(&s, "c0"), person(&s, "b0"), person(&s, "o0a"));
    // A governor before an elder adult of another house.
    let mut w = testworld::world(1);
    assert_eq!(w.provinces[&ProvinceId(5)].governor, Some(b0));
    assert!(w.age(other) > w.age(b0), "the other house's adult is the elder");
    assert_eq!(w.usurper_candidate(s0, &p), Some(b0));
    let l0 = w.actors[&s0].legitimacy;
    assert_eq!(w.usurp_now(s0, &p).unwrap(), Some(b0));
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::RulerDied { actor, person, cause: Cause::Execution } if actor == s0 && person == r0)));
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Succession { actor, from, to, how: Standing::Usurper } if actor == s0 && from == r0 && to == b0)));
    assert_eq!(w.actors[&s0].ruler, b0);
    assert_eq!(w.actors[&s0].standing, Standing::Usurper);
    assert_eq!(w.actors[&s0].heir, None, "the designated heir of the executed ruler is set aside");
    assert_eq!(w.provinces[&ProvinceId(5)].governor, None, "one seat per person: the governorship fell vacant (the house has no adult without a seat)");
    assert!(l0 > p.legitimacy_norm_usurper && w.actors[&s0].legitimacy <= p.legitimacy_norm_usurper + 1e-9, "legitimacy fell to the usurper's norm");
    assert!((w.legitimacy_norm(s0, &p) - p.legitimacy_norm_usurper).abs() < 1e-9);
    // The regent first: a child on the throne with the governor as regent.
    let mut w = testworld::world(1);
    w.kill(r0, Cause::Natural, &p).unwrap();
    w.tick(&p).unwrap();
    assert_eq!((w.actors[&s0].ruler, w.actors[&s0].regent), (c0, Some(b0)));
    assert_eq!(w.usurper_candidate(s0, &p), Some(b0));
    assert_eq!(w.usurp_now(s0, &p).unwrap(), Some(b0));
    assert_eq!(w.actors[&s0].ruler, b0);
    assert!(!w.person_alive(c0));
    assert_eq!(w.actors[&s0].regent, None);
    // No candidate — no usurpation: a house of one, the polity's only house.
    let s4 = ActorId(4);
    let (r4, c4) = (person(&s, "r4"), person(&s, "c4"));
    let mut w = testworld::world(1);
    assert_eq!(w.usurper_candidate(s4, &p), Some(c4), "the eldest adult of the house");
    w.kill(c4, Cause::Natural, &p).unwrap();
    assert_eq!(w.usurper_candidate(s4, &p), None);
    assert_eq!(w.usurp_now(s4, &p).unwrap(), None);
    assert_eq!(w.actors[&s4].ruler, r4);
    assert!(w.person_alive(r4));
    let _ = Succession::Blood;
}

/// В6 on the generated worlds: usurpations per century per polity.
#[test]
fn on_generated_worlds_usurpations_per_century_per_polity_lie_in_b6_neither_never_nor_always() {
    let p = params();
    let band = p.entries["usurpation.observed_per_century"].band.unwrap();
    let mut by_rule: BTreeMap<Succession, (u64, u64)> = BTreeMap::new();
    let (mut usurpations, mut polity_centuries) = (0u64, 0.0f64);
    // «Not always»: a polity whose first century saw no usurpation.
    let mut quiet_centuries = 0u64;
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let mut g = Game::new(&s, params(), seed).unwrap();
        let mut per_actor: BTreeMap<ActorId, u64> = g.world().actors.keys().map(|a| (*a, 0)).collect();
        let mut first_century: BTreeMap<ActorId, u64> = per_actor.clone();
        for t in 0..300u32 {
            let r = g.tick(&[]).unwrap();
            for e in &r.events {
                if let Event::Succession { actor, how: Standing::Usurper, .. } = e.event {
                    *per_actor.get_mut(&actor).unwrap() += 1;
                    if t < 200 {
                        *first_century.get_mut(&actor).unwrap() += 1;
                    }
                }
            }
        }
        let w = g.world();
        for (a, n) in &per_actor {
            let rule = w.actors[a].succession;
            let e = by_rule.entry(rule).or_insert((0, 0));
            e.0 += n;
            e.1 += 1;
            usurpations += n;
            polity_centuries += 1.5;
            if first_century[a] == 0 {
                quiet_centuries += 1;
            }
            // Every seat is held by a living person of the polity's own houses.
            if w.alive(*a) {
                assert_eq!(w.houses[&w.persons[&w.actors[a].ruler].house].polity, *a, "seed {seed}: a usurper from another polity");
            }
        }
    }
    let per_century = usurpations as f64 / polity_centuries;
    let rules: Vec<String> = by_rule.iter().map(|(r, (n, k))| format!("{r:?} {:.1}", *n as f64 / (*k as f64 * 1.5))).collect();
    eprintln!("В6 usurpations per century per polity {per_century:.1} ({usurpations} over {polity_centuries:.0} polity-centuries; {quiet_centuries} polities with none in their first century); by rule: {}", rules.join(", "));
    assert!(usurpations > 0, "never");
    assert!(quiet_centuries > 0, "always: every polity saw a usurpation in its first century");
    assert!((band[0]..=band[1]).contains(&per_century), "В6 {per_century:.1} outside {band:?}");
}

#[test]
fn every_step_of_the_tax_above_the_tithe_lowers_the_norm_while_the_rate_holds_and_every_step_below_raises_it() {
    let p = params();
    let mut w = world();
    let a = ActorId(0);
    let raise = VerbCall { actor: ActorRef::Ruler(a), verb: VerbId::RaiseTax, target: Target::None };
    let lower = VerbCall { actor: ActorRef::Ruler(a), verb: VerbId::LowerTax, target: Target::None };
    let base = w.legitimacy_norm(a, &p);
    assert!((base - p.legitimacy_norm_hereditary).abs() < 1e-9);
    w.apply_verb(&raise, &p).unwrap();
    assert!((w.legitimacy_norm(a, &p) - (base - p.legitimacy_norm_per_tax_step)).abs() < 1e-9, "one step up: the norm one step lower");
    w.apply_verb(&raise, &p).unwrap();
    assert!((w.legitimacy_norm(a, &p) - (base - 2.0 * p.legitimacy_norm_per_tax_step)).abs() < 1e-9);
    // Legitimacy drifts to the shifted norm and stays there as long as the rate holds: the
    // one-off blow is never recovered above it.
    for _ in 0..60 {
        w.tick(&p).unwrap();
        if w.actors[&a].standing != Standing::Hereditary || !w.is_adult(w.actors[&a].ruler, &p) {
            return; // the draws changed the throne: the shape is covered by the paired run
        }
    }
    let shifted = base - 2.0 * p.legitimacy_norm_per_tax_step;
    assert!((w.actors[&a].legitimacy - shifted).abs() < 1.0, "{} vs {shifted}", w.actors[&a].legitimacy);
    // Down below the tithe: the norm rises above the standing's.
    for _ in 0..4 {
        w.apply_verb(&lower, &p).unwrap();
    }
    assert!((w.legitimacy_norm(a, &p) - (base + 2.0 * p.legitimacy_norm_per_tax_step)).abs() < 1e-9, "two steps below the tithe: the norm two steps higher");
    assert!(w.legitimacy_norm(a, &p) <= 100.0);
}

/// The greedy ruler may gain at most this over the plain one in net income: the arithmetic
/// of the two (в) coefficients gives +22 % at the best rate inside Э4 (16 %: the norm four
/// steps lower, the share 0.68, the take 10.9 % against the tithe's 10 % … and at the top,
/// 20 %: the share 0.6, the take 12 %); a quarter is that gain with its margin, never a
/// half — a hard hand pays, but it is not free and not nothing (owner on #286).
const GREEDY_GAIN_CEILING: f64 = 0.25;

/// The outcome of brief §1 for one polity: net income over the run (tax collected less kg
/// spent through verbs), provinces at the end, seats held by its houses (throne and
/// governorships) and their living members, kg spent through verbs, net worth at the end.
#[derive(Debug, Clone, Copy)]
struct Outcome {
    net_income: i64,
    provinces: usize,
    seats: usize,
    members: usize,
    spent: i64,
    net_worth: i64,
    mean_legitimacy: f64,
}

fn outcome_of(w: &World, a: ActorId, income: i64, spent: i64, legit_sum: f64, ticks: u32) -> Outcome {
    let houses: Vec<_> = w.houses_of(a);
    let seats = houses.iter().map(|h| {
        let mut n = 0;
        for p in w.members(*h) {
            if w.actors[&a].ruler == p {
                n += 1;
            }
            n += w.provinces.values().filter(|pr| pr.governor == Some(p)).count();
        }
        n
    }).sum();
    Outcome {
        net_income: income - spent,
        provinces: w.actors[&a].provinces.len(),
        seats,
        members: houses.iter().map(|h| w.members(*h).len()).sum(),
        spent,
        net_worth: w.treasury(a) - w.debt(a),
        mean_legitimacy: legit_sum / f64::from(ticks),
    }
}

/// One tick of a strategy for one ruler, before the world's tick. Returns kg spent.
fn act(w: &mut World, a: ActorId, strategy: &str, p: &Params) -> i64 {
    let who = ActorRef::Ruler(a);
    let call = |v: VerbId| VerbCall { actor: who, verb: v, target: Target::None };
    let mut spent = 0;
    if !w.alive(a) {
        return 0;
    }
    match strategy {
        "plain" => {}
        "greedy" => {
            if w.tax_room(a, VerbId::RaiseTax, p).is_ok() {
                w.apply_verb(&call(VerbId::RaiseTax), p).unwrap();
            }
            if w.actors[&a].legitimacy < w.legitimacy_norm(a, p) && w.treasury(a) >= w.order_cost(a, p) && w.order_cost(a, p) > 0 {
                spent += w.order_cost(a, p);
                w.apply_verb(&call(VerbId::EnforceOrder), p).unwrap();
            }
        }
        "order every tick" => {
            if w.tax_room(a, VerbId::RaiseTax, p).is_ok() {
                w.apply_verb(&call(VerbId::RaiseTax), p).unwrap();
            }
            if w.treasury(a) >= w.order_cost(a, p) && w.order_cost(a, p) > 0 {
                spent += w.order_cost(a, p);
                w.apply_verb(&call(VerbId::EnforceOrder), p).unwrap();
            }
        }
        "borrow and default" => {
            let _ = w.apply_verb(&call(VerbId::TakeLoan), p);
            let mut n = 0;
            while w.order_cost(a, p) > 0 && w.treasury(a) >= w.order_cost(a, p) && n < 50 {
                spent += w.order_cost(a, p);
                w.apply_verb(&call(VerbId::EnforceOrder), p).unwrap();
                n += 1;
            }
        }
        _ => unreachable!(),
    }
    spent
}

/// One ruler plays a strategy over 100 ticks on the test world with the same draws.
fn play_strategy(strategy: &str, p: &Params) -> Outcome {
    let mut w = world();
    let a = ActorId(0);
    w.actors.get_mut(&a).unwrap().institutions.insert(engine2::Institution::Bank);
    let (mut income, mut legit_sum, mut spent) = (0i64, 0.0f64, 0i64);
    for _ in 0..100 {
        spent += act(&mut w, a, strategy, p);
        let r = w.tick(p).unwrap();
        income += r.postings.iter().filter(|q| q.account == engine2::Account::Treasury(a) && q.source == Source::Tax).map(|q| q.milli / 1000).sum::<i64>();
        legit_sum += w.actors[&a].legitimacy;
    }
    outcome_of(&w, a, income, spent, legit_sum, 100)
}

#[test]
fn the_tax_collected_falls_with_legitimacy_below_full_collection() {
    let p = params();
    let mut w = world();
    let a = ActorId(0);
    let full = p.tax_full_collection_at_legitimacy;
    for (legitimacy, expected_ppm) in [(full, 1_000_000), (full + 20.0, 1_000_000), (full - 25.0, 1_000_000 - 25 * p.tax_evasion_ppm_per_legitimacy_point), (0.0, 1_000_000 - (full as i64) * p.tax_evasion_ppm_per_legitimacy_point)] {
        w.actors.get_mut(&a).unwrap().legitimacy = legitimacy.min(100.0);
        assert_eq!(w.collection_share_ppm(a, &p), expected_ppm, "at legitimacy {legitimacy}");
        let production = w.production_per_tick(a, &p);
        let rate = w.actors[&a].tax_rate_ppm;
        let r = w.tick(&p).unwrap();
        let tax: i64 = r.postings.iter().filter(|q| q.account == engine2::Account::Treasury(a) && q.source == Source::Tax).map(|q| q.milli).sum();
        assert_eq!(tax / 1000, production * rate / 1_000_000 * expected_ppm / 1_000_000, "at legitimacy {legitimacy}");
    }
    assert!(w.collection_share_ppm(a, &p) > 0, "even at zero legitimacy something is collected");
}

#[test]
fn on_generated_worlds_the_greedy_take_stays_inside_e8_and_gains_at_most_the_ceiling_over_the_plain_ruler() {
    let p = params();
    let band = p.entries["tax.observed_share_of_production"].band.unwrap();
    let mut totals: BTreeMap<&str, (i64, i64, i64)> = BTreeMap::new(); // (tax, production, spent)
    let mut worst_gain = f64::MIN;
    let mut shares: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for seed in 1..=10u64 {
        let s = worldgen::generate(seed, &p);
        let mut per_world: BTreeMap<&str, i64> = BTreeMap::new();
        for strategy in ["plain", "greedy"] {
            let mut g = Game::new(&s, params(), seed).unwrap();
            let ids: Vec<ActorId> = g.world().actors.keys().copied().collect();
            let (mut tax, mut production, mut spent) = (0i64, 0i64, 0i64);
            for _ in 0..100 {
                for a in &ids {
                    spent += act(g.world_mut(), *a, strategy, &p);
                }
                for a in &ids {
                    if g.world().alive(*a) {
                        production += g.world().production_per_tick(*a, &p);
                    }
                }
                let r = g.tick(&[]).unwrap();
                tax += r.postings.iter().filter(|q| matches!(q.account, engine2::Account::Treasury(_)) && q.source == Source::Tax).map(|q| q.milli / 1000).sum::<i64>();
            }
            let share = tax as f64 / production as f64;
            assert!((band[0]..=band[1]).contains(&share), "seed {seed} {strategy}: tax share of production {share:.3} outside Э8 {band:?}");
            shares.entry(strategy).or_default().push(share);
            let t = totals.entry(strategy).or_insert((0, 0, 0));
            *t = (t.0 + tax, t.1 + production, t.2 + spent);
            per_world.insert(strategy, tax - spent);
        }
        let gain = per_world["greedy"] as f64 / per_world["plain"] as f64 - 1.0;
        worst_gain = worst_gain.max(gain);
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let (plain, greedy) = (totals["plain"], totals["greedy"]);
    let gain = (greedy.0 - greedy.2) as f64 / (plain.0 - plain.2) as f64 - 1.0;
    eprintln!(
        "Э8 tax share of production: plain {:.3}, greedy {:.3}; greedy net income over plain: {:+.1}% in all, worst world {:+.1}%",
        mean(&shares["plain"]),
        mean(&shares["greedy"]),
        gain * 100.0,
        worst_gain * 100.0
    );
    assert!(gain <= GREEDY_GAIN_CEILING, "the greedy ruler gains {:+.1}% over the plain one: above the ceiling", gain * 100.0);
}
#[test]
fn paired_runs_by_the_outcome_of_brief_1_a_greedy_ruler_gains_at_most_the_ceiling_and_a_borrower_who_spends_the_loan_ends_poorer() {
    let p = params();
    let plain = play_strategy("plain", &p);
    let greedy = play_strategy("greedy", &p);
    let every = play_strategy("order every tick", &p);
    let defaulter = play_strategy("borrow and default", &p);
    for (name, o) in [("plain", &plain), ("greedy: tax to the top, order when below the norm", &greedy), ("order whenever affordable at the top tax", &every), ("borrow to the ceiling and spend", &defaulter)] {
        eprintln!("{name}: net income {} provinces {} seats {} members {} spent {} net worth {} (mean legitimacy {:.1})", o.net_income, o.provinces, o.seats, o.members, o.spent, o.net_worth, o.mean_legitimacy);
    }
    assert_eq!(plain.provinces, greedy.provinces, "without war no land moves: the price is in the take");
    for (name, o) in [("greedy", &greedy), ("order whenever affordable", &every)] {
        let gain = o.net_income as f64 / plain.net_income as f64 - 1.0;
        assert!(gain <= GREEDY_GAIN_CEILING, "{name}: {:+.1}% net income over the plain ruler, above the ceiling", gain * 100.0);
    }
    // Resources the borrower commanded: what it spent plus what it holds net of debt.
    let (plain_resources, defaulter_resources) = (plain.spent + plain.net_worth, defaulter.spent + defaulter.net_worth);
    assert!(defaulter_resources < plain_resources, "borrowing to spend on order is not a source of resources: {defaulter_resources} vs {plain_resources}");
}
