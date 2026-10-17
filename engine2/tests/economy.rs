//! E2 step 4 acceptance (plan §16): the treasury as a flow, the budget as the army's limit.
//!
//! - **А11 at the budget's limit** (the measure the owner set): on the generated worlds the
//!   army a polity can keep paid — tax × (1 − court) ÷ pay — lies inside А11 as a share of
//!   its people; an army above the limit melts down to it.
//! - **No treasury ever goes below zero**; without a bank the unpaid share melts. With a
//!   bank the polity borrows up to the ceiling and pays interest on the debt.
//! - **Debt is explicit (review of PR #283):** a loan is the same figure on the treasury and
//!   on the debt, so the net worth (treasury − debt) never grows from borrowing; repeated
//!   loans stop at the ceiling; and no source but the tax puts wheat into a treasury
//!   without a matching entry elsewhere.
//! - **Verbs:** the tax verbs are closed (no price yet), `take_loan` needs a bank.
//! - **Wheat accounting:** every treasury posting is signed by source and the ledger
//!   balances every tick; Э8 and Э9 are measured and reported, not asserted (plan §16).

use engine2::sim::{Game, Simulation};
use engine2::{testworld, worldgen, Account, ActorId, ActorRef, Event, Institution, Params, Source, Target, UnitId, VerbCall, VerbId, World};

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

fn people_of(w: &World, a: ActorId) -> i64 {
    w.actors[&a].provinces.iter().map(|p| w.population(*p)).sum()
}

#[test]
fn army_at_the_limit_of_the_budget_lies_inside_a11_on_generated_worlds() {
    let p = params();
    let band = p.entries["army.observed_share_of_population"].band.unwrap();
    let mut shares = Vec::new();
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let c = s.compile(&p).unwrap();
        let w = World::from_facts(&c.facts, c.names, &p, 1).unwrap();
        for a in w.actors.keys() {
            let limit = w.sustainable_soldiers(*a, &p);
            let share = limit as f64 / people_of(&w, *a) as f64;
            assert!((band[0]..=band[1]).contains(&share), "seed {seed} actor {a:?}: army at the budget limit is {share:.4} of its people, outside А11 {band:?}");
            shares.push(share);
            // And the starting army the generator gave is within the budget.
            let soldiers: i64 = w.units_of(*a).iter().map(|u| w.soldiers(*u)).sum();
            assert!(soldiers <= limit, "seed {seed} actor {a:?}: starts with {soldiers} soldiers over a limit of {limit}");
        }
    }
    let mean = shares.iter().sum::<f64>() / shares.len() as f64;
    eprintln!("army at the budget limit: mean {mean:.4} of people, min {:.4}, max {:.4}", shares.iter().cloned().fold(f64::MAX, f64::min), shares.iter().cloned().fold(f64::MIN, f64::max));
}

#[test]
fn an_army_above_the_budget_melts_to_the_limit_and_the_treasury_never_goes_below_zero_without_a_bank() {
    let p = params();
    let mut w = testworld::world(2);
    let a = ActorId(0);
    assert!(!w.actors[&a].institutions.contains(&Institution::Bank));
    let unit = w.units_of(a)[0];
    let home = w.units[&unit].position;
    let limit = w.sustainable_soldiers(a, &p);
    w.post(engine2::Account::Soldiers(unit), Source::Recruit, (limit * 3 - w.soldiers(unit)) * 1000).unwrap();
    // Spend the treasury down so the budget, not the reserve, is the limit from the start.
    let t = w.treasury(a);
    w.post(engine2::Account::Treasury(a), Source::Verb, -t * 1000).unwrap();
    let mut shortfalls = 0;
    for _ in 0..80 {
        let r = w.tick(&p).unwrap();
        assert!(w.treasury(a) >= 0, "treasury {} below zero without a bank", w.treasury(a));
        shortfalls += r.events.iter().filter(|e| matches!(e.event, Event::PayShortfall { actor, .. } if actor == a)).count();
        // An unpaid army at home deserts; it does not live off its own people (plan §17).
        assert!(!r.events.iter().any(|e| matches!(e.event, Event::Ruin { province, .. } if province == home)));
        assert!(r.postings.iter().all(|q| q.source != Source::Ruin));
    }
    let soldiers = w.soldiers(unit);
    let limit_now = w.sustainable_soldiers(a, &p);
    assert!(shortfalls > 0);
    assert!(soldiers <= limit_now * 105 / 100, "{soldiers} soldiers, limit {limit_now}");
    assert!(soldiers >= limit_now * 80 / 100, "{soldiers} soldiers melted far below the limit {limit_now}");
}

#[test]
fn with_a_bank_the_polity_borrows_to_the_ceiling_pays_interest_and_its_treasury_stays_at_or_above_zero() {
    let p = params();
    let mut w = testworld::world(2);
    let a = ActorId(0);
    w.actors.get_mut(&a).unwrap().institutions.insert(Institution::Bank);
    let unit = w.units_of(a)[0];
    let limit = w.sustainable_soldiers(a, &p);
    w.post(Account::Soldiers(unit), Source::Recruit, (limit * 3 - w.soldiers(unit)) * 1000).unwrap();
    let t = w.treasury(a);
    w.post(Account::Treasury(a), Source::Verb, -t * 1000).unwrap();
    let ceiling = w.debt_ceiling(a, &p);
    assert!(ceiling > 0);
    let mut borrowed = false;
    let mut interest = 0i64;
    for _ in 0..80 {
        let (debt_before, room_before) = (w.debt(a), w.loan_room(a, &p));
        let r = w.tick(&p).unwrap();
        assert!(w.treasury(a) >= 0, "treasury {} below zero", w.treasury(a));
        borrowed |= w.debt(a) > 0;
        let tick_interest = r.postings.iter().filter(|q| q.source == Source::Interest).map(|q| q.milli.abs()).sum::<i64>();
        interest += tick_interest;
        // Nothing but interest carries the debt past the room the ceiling left: no loan
        // beyond it. (The ceiling itself moves with the revenue — with the people, and
        // since 6b with legitimacy through the share collected — so the debt is compared
        // with the room of the tick, not with the ceiling of the next.)
        assert!(w.debt(a) - debt_before <= room_before + tick_interest / 1000 + 1, "debt {} from {debt_before}: borrowed past the room {room_before}", w.debt(a));
    }
    assert!(borrowed, "a bank lets the polity borrow");
    assert!(interest > 0, "debt costs interest");
    assert!(w.soldiers(unit) < limit * 3, "the army still melts once the ceiling is reached");
}

#[test]
fn a_loan_does_not_change_the_net_worth_and_repeated_loans_stop_at_the_ceiling() {
    // The ceiling is years of revenue: a province in revolt (E2 step 9) would pull it from
    // under the debt by another path than the loan — the test is about the loan, so the
    // base of the rising is at nothing here.
    let mut p = params();
    p.revolt_per_tick = 0.0;
    let mut w = testworld::world(2);
    let a = ActorId(0);
    w.actors.get_mut(&a).unwrap().institutions.insert(Institution::Bank);
    // An army well over the budget, so that no surplus repays the debt between loans.
    let unit = w.units_of(a)[0];
    let limit = w.sustainable_soldiers(a, &p);
    w.post(Account::Soldiers(unit), Source::Recruit, (limit * 3 - w.soldiers(unit)) * 1000).unwrap();
    let call = VerbCall { actor: ActorRef::Ruler(a), verb: VerbId::TakeLoan, target: Target::None };
    let before = (w.treasury(a), w.debt(a));
    w.apply_verb(&call, &p).unwrap();
    let after = (w.treasury(a), w.debt(a));
    assert!(after.0 > before.0 && after.1 > before.1, "the loan lands on both accounts: {before:?} → {after:?}");
    assert_eq!(after.0 - after.1, before.0 - before.1, "the net worth does not grow from borrowing");
    assert_eq!(after.0 - before.0, after.1 - before.1, "the same figure on both accounts");
    // Borrow every tick: the debt climbs to the ceiling and stops there. The ceiling itself
    // creeps up with the people (years of revenue), so at the ceiling the verb either refuses
    // or hands out only the room that creep opened — never a year of revenue again.
    let mut at_ceiling_ticks = 0;
    for _ in 0..40 {
        let room_before = w.loan_room(a, &p);
        let debt_before = w.debt(a);
        match w.apply_verb(&call, &p) {
            Ok(()) => assert!(w.debt(a) - debt_before <= room_before, "a loan past the room"),
            Err(engine2::economy::VerbError::NotAvailable(VerbId::TakeLoan, why)) => assert_eq!(why, "at the debt ceiling"),
            Err(e) => panic!("{e}"),
        }
        if room_before <= w.debt_ceiling(a, &p) / 100 {
            at_ceiling_ticks += 1;
        }
        w.tick(&p).unwrap();
        let c = w.debt_ceiling(a, &p);
        assert!(w.debt(a) <= c + c * p.loan_interest_ppm_per_tick / 1_000_000 + 1, "debt {} past the ceiling {c}", w.debt(a));
        assert!(w.treasury(a) >= 0);
    }
    assert!(at_ceiling_ticks >= 20, "the debt reached the ceiling and stayed: {at_ceiling_ticks} ticks");
}

#[test]
fn no_source_but_the_tax_puts_wheat_into_a_treasury_without_a_matching_entry() {
    let p = params();
    let mut loans = 0;
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let c = s.compile(&p).unwrap();
        let mut w = World::from_facts(&c.facts, c.names, &p, 1).unwrap();
        // Push every polity over its budget: the ones with a bank borrow, the others melt.
        let ids: Vec<ActorId> = w.actors.keys().copied().collect();
        for a in &ids {
            let unit = w.units_of(*a)[0];
            let limit = w.sustainable_soldiers(*a, &p);
            w.post(Account::Soldiers(unit), Source::Recruit, (limit * 2 - w.soldiers(unit)).max(0) * 1000).unwrap();
        }
        for _ in 0..60 {
            // A house secedes at the start of a tick (E3 (в): its chance, opened the tick
            // before, is settled first), before the seats are paid: the seat's income is
            // read against the polity the house belongs to once the secessions are made.
            let mut house_polity: std::collections::BTreeMap<engine2::HouseId, ActorId> = w.houses.iter().map(|(h, x)| (*h, x.polity)).collect();
            let r = w.tick(&p).unwrap();
            for e in &r.events {
                if let engine2::Event::ActorBorn { actor, house, .. } = e.event {
                    house_polity.insert(house, actor);
                }
            }
            for a in &ids {
                let sum = |account: Account, src: Source| r.postings.iter().filter(|q| q.account == account && q.source == src).map(|q| q.milli).sum::<i64>();
                let (t, d) = (Account::Treasury(*a), Account::Debt(*a));
                for q in r.postings.iter().filter(|q| q.account == t) {
                    match q.source {
                        Source::Tax => assert!(q.milli >= 0),
                        Source::Court | Source::Upkeep | Source::Interest | Source::Repayment | Source::Seat => assert!(q.milli <= 0, "seed {seed}: {:?} put wheat into a treasury", q.source),
                        Source::Loan => assert!(q.milli >= 0),
                        // A vassal's tribute (E2 step 7): a transfer, either sign.
                        Source::Tribute => {}
                        // A polity born of a rising (E2 step 9): its treasury opened at
                        // nothing, then its share of the parent's — a transfer.
                        Source::Opening => assert_eq!(q.milli, 0, "seed {seed}: a treasury opened with wheat in it"),
                        Source::Secession => {}
                        other => panic!("seed {seed}: treasury moved by {other:?}"),
                    }
                }
                // The counter-entries: the same figure on the debt.
                assert_eq!(sum(t, Source::Loan), sum(d, Source::Loan), "seed {seed} actor {a:?}: a loan without its debt");
                // Tribute moves between treasuries: over all of them it sums to nothing.
                assert_eq!(r.postings.iter().filter(|q| matches!(q.account, Account::Treasury(_)) && q.source == Source::Tribute).map(|q| q.milli).sum::<i64>(), 0, "seed {seed}: tribute without its counter-entry");
                assert_eq!(r.postings.iter().filter(|q| matches!(q.account, Account::Treasury(_)) && q.source == Source::Secession).map(|q| q.milli).sum::<i64>(), 0, "seed {seed}: a secession's share without its counter-entry");
                assert_eq!(sum(t, Source::Repayment), sum(d, Source::Repayment), "seed {seed} actor {a:?}: a repayment without its debt");
                // A seat's income (E0 Х1) is a transfer: what left the treasury arrived in
                // the houses' treasuries of the same polity.
                let to_houses: i64 = r.postings.iter().filter(|q| q.source == Source::Seat && matches!(q.account, Account::HouseTreasury(h) if house_polity[&h] == *a)).map(|q| q.milli).sum();
                assert_eq!(-sum(t, Source::Seat), to_houses, "seed {seed} actor {a:?}: a seat's income without its house");
                assert!(sum(d, Source::Interest) >= 0);
                assert!(w.treasury(*a) >= 0 && w.debt(*a) >= 0);
                if !w.actors[a].institutions.contains(&Institution::Bank) {
                    assert_eq!(w.debt(*a), 0, "seed {seed}: debt without a bank");
                }
                loans += (sum(t, Source::Loan) > 0) as u32;
            }
        }
    }
    assert!(loans > 0, "no polity on any world ever borrowed: the test proves nothing");
}

#[test]
fn a_loan_needs_a_bank() {
    let p = params();
    let mut s = testworld::five_states_scenario();
    s.verbs.insert("take_loan".to_string(), "Take a loan".to_string());
    let mut g = bare(&s, p, 1).unwrap();
    let who = ActorRef::Ruler(ActorId(0));
    // No bank in the test world: the loan is refused.
    assert!(g.verbs(who).iter().all(|o| o.verb != VerbId::TakeLoan || !o.available));
    g.world_mut().actors.get_mut(&ActorId(0)).unwrap().institutions.insert(Institution::Bank);
    let loan = g.verbs(who).into_iter().find(|o| o.verb == VerbId::TakeLoan).unwrap();
    assert!(loan.available, "with a bank the loan is on offer");
    let r = g.tick(&[VerbCall { actor: who, verb: VerbId::TakeLoan, target: Target::None }]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Loan { actor, .. } if actor == ActorId(0))));
    let on = |account: Account| r.postings.iter().filter(|q| q.account == account && q.source == Source::Loan).map(|q| q.milli).sum::<i64>();
    assert!(on(Account::Treasury(ActorId(0))) > 0, "the loan landed in the treasury");
    assert_eq!(on(Account::Treasury(ActorId(0))), on(Account::Debt(ActorId(0))), "and the same figure on the debt");
}

#[test]
fn wheat_is_accounted_by_source_and_the_observed_shares_are_reported() {
    let p = params();
    let (mut tax_shares, mut military_shares) = (Vec::new(), Vec::new());
    for seed in 1..=10u64 {
        let s = worldgen::generate(seed, &p);
        let mut g = bare(&s, params(), 1).unwrap();
        let mut by_source: std::collections::BTreeMap<Source, i64> = std::collections::BTreeMap::new();
        let mut production = 0i64;
        for _ in 0..40 {
            for a in g.world().actors.keys() {
                production += g.world().production_per_tick(*a, &p);
            }
            let r = g.tick(&[]).unwrap();
            for q in r.postings.iter().filter(|q| matches!(q.account, engine2::Account::Treasury(_))) {
                *by_source.entry(q.source).or_insert(0) += q.milli;
            }
        }
        let tax = by_source.get(&Source::Tax).copied().unwrap_or(0) / 1000;
        let court = -by_source.get(&Source::Court).copied().unwrap_or(0) / 1000;
        let pay = -by_source.get(&Source::Upkeep).copied().unwrap_or(0) / 1000;
        assert!(tax > 0 && production > 0);
        tax_shares.push(tax as f64 / production as f64);
        military_shares.push(pay as f64 / (pay + court) as f64);
        // Every treasury movement is one of the economy's sources — or the birth of a
        // polity (E2 step 9: its treasury opened at nothing and the secession's share).
        for src in by_source.keys() {
            assert!(matches!(src, Source::Tax | Source::Seat | Source::Court | Source::Upkeep | Source::Interest | Source::Loan | Source::Repayment | Source::Tribute | Source::Opening | Source::Secession), "seed {seed}: treasury moved by {src:?}");
        }
    }
    let mean = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len() as f64;
    eprintln!("Э8 observed tax share of production: mean {:.4}; Э9 observed military share of spending: mean {:.4}", mean(&tax_shares), mean(&military_shares));
}

#[test]
fn a_shortfall_is_shared_pro_rata_among_the_polity_s_units() {
    let p = params();
    let mut w = testworld::world(8);
    let a = ActorId(0);
    let first = w.units_of(a)[0];
    let home = w.units[&first].position;
    let second = UnitId(w.next_unit);
    w.next_unit += 1;
    w.units.insert(second, engine2::world::Unit { owner: a, position: home, quality: 50.0, destination: None, mercenary: false, house: None });
    w.ledger.open(w.tick, engine2::Account::Soldiers(second), 0).unwrap();
    let limit = w.sustainable_soldiers(a, &p);
    let (big, small) = (limit * 3 / 2, limit / 2);
    w.post(engine2::Account::Soldiers(first), Source::Recruit, (big - w.soldiers(first)) * 1000).unwrap();
    w.post(engine2::Account::Soldiers(second), Source::Recruit, small * 1000).unwrap();
    let t = w.treasury(a);
    w.post(engine2::Account::Treasury(a), Source::Verb, -t * 1000).unwrap();
    w.tick(&p).unwrap();
    let lost_big = big - w.soldiers(first);
    let lost_small = small - w.soldiers(second);
    assert!(lost_big > 0 && lost_small > 0);
    // Same share, whatever the id: the ratio of losses equals the ratio of sizes (to the
    // carried thousandth).
    let ratio_diff = (lost_big * small - lost_small * big).abs();
    assert!(ratio_diff <= big + small, "{lost_big}:{big} vs {lost_small}:{small}");
}

/// Deserters go home (owner's rule after #297): an unpaid levy melts by А7 and what it loses
/// arrives, through the ledger, in the people of a province of its own — the sum of people
/// and levies is the same before and after, to the thousandth; mercenaries leave the world,
/// as when disbanded.
#[test]
fn an_unpaid_levy_deserts_home_and_no_one_leaves_the_world_but_mercenaries() {
    let p = params();
    let mut w = testworld::world(8);
    let a = ActorId(0);
    let levy = w.units_of(a)[0];
    let home = w.units[&levy].position;
    let hired = UnitId(w.next_unit);
    w.next_unit += 1;
    w.units.insert(hired, engine2::world::Unit { owner: a, position: home, quality: 50.0, destination: None, mercenary: true, house: None });
    w.ledger.open(w.tick, engine2::Account::Soldiers(hired), 0).unwrap();
    let limit = w.sustainable_soldiers(a, &p);
    w.post(engine2::Account::Soldiers(levy), Source::Recruit, (limit * 2 - w.soldiers(levy)) * 1000).unwrap();
    w.post(engine2::Account::Soldiers(hired), Source::Hire, limit * 1000).unwrap();
    let t = w.treasury(a);
    w.post(engine2::Account::Treasury(a), Source::Verb, -t * 1000).unwrap();
    let r = w.tick(&p).unwrap();
    let deserted = |account: &dyn Fn(&engine2::Account) -> bool| r.postings.iter().filter(|q| q.source == Source::Desertion && account(&q.account)).map(|q| q.milli).sum::<i64>();
    let from_levy = deserted(&|x| *x == engine2::Account::Soldiers(levy));
    let from_hired = deserted(&|x| *x == engine2::Account::Soldiers(hired));
    let to_people = deserted(&|x| matches!(x, engine2::Account::Population(_)));
    assert!(from_levy < 0 && from_hired < 0, "both units were short of pay and melted: {from_levy}, {from_hired}");
    assert_eq!(to_people, -from_levy, "every deserter of the levy is back among the people, to the thousandth");
    assert!(r.postings.iter().filter(|q| q.source == Source::Desertion && matches!(q.account, engine2::Account::Population(_))).all(|q| q.account == engine2::Account::Population(home)), "at home: the levy stands in its own province");
}

/// The pay of a campaign (plan §27.2, owner's correction on #297): a polity at war with its
/// garrisons at home pays what it pays at peace; a unit outside its own land — here besieging
/// the enemy's capital, which holds for `siege.capital_ticks` — is paid
/// `army.campaign_pay_factor` times as much, through the ledger like any pay.
#[test]
fn at_war_the_garrisons_at_home_are_paid_as_at_peace_and_a_campaign_twice() {
    let p = params();
    let mut w = testworld::world(8);
    let (s0, s1) = (ActorId(0), ActorId(1));
    w.post(engine2::Account::Treasury(s0), Source::Test, 1_000_000_000 * 1000).unwrap();
    let polity_paid = |w: &engine2::World| w.units_of(s0).into_iter().filter(|u| w.house_pays(*u).is_none()).collect::<Vec<UnitId>>();
    let upkeep = |r: &engine2::TickReport| -r.postings.iter().filter(|q| q.account == engine2::Account::Treasury(s0) && q.source == Source::Upkeep).map(|q| q.milli).sum::<i64>() / 1000;
    let peace_due: i64 = polity_paid(&w).iter().map(|u| engine2::economy::pay_per_tick(w.soldiers(*u), &p)).sum();
    // At war, every unit at home: the pay of peace.
    w.declare_war(s0, s1).unwrap();
    let r = w.tick(&p).unwrap();
    assert!(polity_paid(&w).iter().all(|u| !w.on_campaign(*u)), "the garrisons stay at home");
    assert_eq!(upkeep(&r), peace_due, "a war does not raise the pay of the garrisons");
    // A unit sent to besiege the enemy's capital, the enemy's men elsewhere: on campaign.
    let capital = w.actors[&s1].capital;
    for u in w.units_of(s1) {
        let elsewhere = w.actors[&s1].provinces.iter().copied().find(|q| *q != capital).unwrap_or(capital);
        w.units.get_mut(&u).unwrap().position = elsewhere;
    }
    let sent = polity_paid(&w)[0];
    w.units.get_mut(&sent).unwrap().position = capital;
    let due: i64 = polity_paid(&w).iter().map(|u| if *u == sent { engine2::economy::campaign_pay_per_tick(w.soldiers(*u), &p) } else { engine2::economy::pay_per_tick(w.soldiers(*u), &p) }).sum();
    let r = w.tick(&p).unwrap();
    assert!(w.on_campaign(sent) && w.provinces[&capital].owner == s1, "still besieging, on the enemy's land");
    assert_eq!(upkeep(&r), due, "the campaign is paid at the campaign's pay");
    assert_eq!(engine2::economy::campaign_pay_per_tick(1000, &p), 2 * engine2::economy::pay_per_tick(1000, &p), "twice the pay at the shipped factor");
}
