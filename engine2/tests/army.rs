//! E2 step 5 acceptance (plan §17): war as the minimal relation, the march from А5 to
//! contact, the battle with a winner and an end, the siege that changes owners, the fall of
//! a landless polity, foraging with its cost, the levy, the disbanding and the hire — on
//! the test world and on the generated worlds with scripted wars.
//!
//! `declare_war` is closed to the player and the agents («awaits diplomacy»); the tests
//! open wars through `World::declare_war` (feature `testing`).

use engine2::event::{Outcome, Transfer};
use engine2::sim::{Game, SimError, Simulation};
use engine2::{testworld, worldgen, Account, ActorId, ActorRef, Event, Institution, Params, ProvinceId, Source, Target, UnitId, VerbCall, VerbId, World};

fn params() -> Params {
    Params::load().unwrap()
}

/// Province `(x, y)` of the 5×5 test world; state `x` owns column `x`, capital at `y = 0`.
fn at(x: u32, y: u32) -> ProvinceId {
    ProvinceId(y * 5 + x)
}

fn people_and_soldiers(w: &World) -> i64 {
    w.provinces.keys().map(|p| w.population(*p)).sum::<i64>() + w.units.keys().map(|u| w.soldiers(*u)).sum::<i64>()
}

fn set_soldiers(w: &mut World, u: UnitId, n: i64) {
    let now = w.soldiers(u);
    w.post(Account::Soldiers(u), Source::Test, (n - now) * 1000).unwrap();
}

#[test]
fn war_is_closed_in_the_offers_and_only_the_tests_open_it() {
    let mut s = testworld::five_states_scenario();
    s.verbs.insert("declare_war".to_string(), "Declare war".to_string());
    let mut g = Game::new(&s, params(), 1).unwrap();
    let who = ActorRef::Ruler(ActorId(0));
    let war = g.verbs(who).into_iter().find(|o| o.verb == VerbId::DeclareWar).unwrap();
    assert!(!war.available);
    assert_eq!(war.why.as_deref(), Some("awaits diplomacy"));
    let err = g.tick(&[VerbCall { actor: who, verb: VerbId::DeclareWar, target: Target::Actor(ActorId(1)) }]).unwrap_err();
    assert!(matches!(err, SimError::Verb(_)), "{err}");
    assert!(g.world().wars.is_empty());
    let mut w = testworld::world(1);
    w.declare_war(ActorId(1), ActorId(0)).unwrap();
    assert!(w.at_war(ActorId(0), ActorId(1)) && w.at_war(ActorId(1), ActorId(0)));
    assert!(!w.at_war(ActorId(0), ActorId(2)));
    let r = w.tick(&params()).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::War { a: ActorId(0), b: ActorId(1) })));
}

#[test]
fn a_unit_marches_its_whole_path_across_its_own_land_in_one_tick_and_stops_at_contact_with_the_enemy() {
    let p = params();
    let mut w = testworld::world(1);
    let s0 = ActorId(0);
    let u0 = w.units_of(s0)[0];
    assert!(w.march_steps_per_tick(&p) >= 5, "А5 gives dozens of provinces a tick, not one");
    w.order_move(u0, at(0, 4)).unwrap();
    let r = w.tick(&p).unwrap();
    assert_eq!(w.units[&u0].position, at(0, 4));
    assert_eq!(r.events.iter().filter(|e| matches!(e.event, Event::UnitMoved { unit, .. } if unit == u0)).count(), 4);
    // Into enemy land the march ends at the first enemy province on the path.
    w.declare_war(s0, ActorId(1)).unwrap();
    w.units.get_mut(&w.units_of(ActorId(1))[0]).unwrap().position = at(1, 0);
    let dest = at(1, 1);
    let path = w.path(s0, at(0, 4), dest).unwrap();
    let first_enemy = path.iter().position(|(q, _)| w.provinces[q].owner == ActorId(1)).unwrap();
    w.order_move(u0, dest).unwrap();
    let r = w.tick(&p).unwrap();
    let stopped = path[first_enemy].0;
    assert_eq!(w.units[&u0].position, stopped, "stopped at contact");
    assert_eq!(r.events.iter().filter(|e| matches!(e.event, Event::UnitMoved { unit, .. } if unit == u0)).count(), first_enemy + 1);
    if stopped != dest {
        assert_eq!(w.units[&u0].destination, Some(dest), "the order stands for after the siege");
    }
    assert!(r.events.iter().any(|e| matches!(e.event, Event::ProvinceChangedHands { province, from: ActorId(1), to: ActorId(0), how: Transfer::Conquest } if province == stopped)));
    // Land of a polity not at war stays closed.
    assert!(w.path(s0, at(0, 0), at(2, 0)).is_none() || w.path(s0, at(0, 0), at(2, 0)).unwrap().iter().all(|(p, _)| w.provinces[p].owner != ActorId(1) || w.at_war(s0, ActorId(1))));
    assert!(!w.passable(s0, at(3, 0)));
}

#[test]
fn a_battle_has_a_winner_losses_inside_a6_and_the_loser_falls_back_to_its_own_land() {
    let p = params();
    let mut w = testworld::world(1);
    let (s0, s1) = (ActorId(0), ActorId(1));
    let (u0, u1) = (w.units_of(s0)[0], w.units_of(s1)[0]);
    w.declare_war(s0, s1).unwrap();
    set_soldiers(&mut w, u0, 2000);
    w.order_move(u0, at(1, 0)).unwrap();
    let r = w.tick(&p).unwrap();
    let battle = r.events.iter().find_map(|e| match &e.event {
        Event::Battle { province, attacker, defender, outcome, soldiers, losses } => Some((*province, *attacker, *defender, *outcome, *soldiers, *losses)),
        _ => None,
    });
    let (province, attacker, defender, outcome, soldiers, losses) = battle.expect("one battle");
    assert_eq!((province, attacker, defender), (at(1, 0), s0, s1), "the defender is the province's owner");
    assert_eq!(soldiers, (2000, 600));
    let (w_band, l_band) = (p.entries["battle.losses_winner"].band.unwrap(), p.entries["battle.losses_loser"].band.unwrap());
    let share = |lost: u64, of: u64| lost as f64 / of as f64;
    let (winner_unit, loser_unit, winner_losses, loser_losses, winner_men, loser_men) = match outcome {
        Outcome::AttackerWon => (u0, u1, losses.0, losses.1, soldiers.0, soldiers.1),
        Outcome::DefenderWon => (u1, u0, losses.1, losses.0, soldiers.1, soldiers.0),
        Outcome::Drawn => panic!("a battle has a winner"),
    };
    assert!((w_band[0] - 0.001..=w_band[1] + 0.001).contains(&share(winner_losses, winner_men)), "winner lost {winner_losses} of {winner_men}");
    assert!((l_band[0] - 0.001..=l_band[1] + 0.001).contains(&share(loser_losses, loser_men)), "loser lost {loser_losses} of {loser_men}");
    assert_eq!(r.postings.iter().filter(|q| q.source == Source::BattleLoss).map(|q| -q.milli).sum::<i64>(), (winner_losses + loser_losses) as i64 * 1000);
    let fell_back_to = r.events.iter().find_map(|e| match e.event {
        Event::Retreat { unit, to } if unit == loser_unit => Some(to),
        _ => None,
    });
    let to = fell_back_to.expect("the loser fell back");
    assert_eq!(w.provinces[&to].owner, w.units[&loser_unit].owner, "to its own land");
    assert_eq!(w.units[&loser_unit].position, to);
    assert_eq!(w.units[&winner_unit].position, at(1, 0), "the winner holds the field");
    assert_eq!(r.events.iter().filter(|e| matches!(e.event, Event::Battle { .. })).count(), 1, "one battle per province per tick");
    // The capital is now besieged, not taken within the tick.
    if outcome == Outcome::AttackerWon {
        assert_eq!(w.provinces[&at(1, 0)].owner, s1);
        assert!(w.provinces[&at(1, 0)].siege.is_some());
    }
}

#[test]
fn a_beaten_unit_with_no_way_home_is_destroyed() {
    let p = params();
    let (mut facts, names) = testworld::five_states();
    for f in &mut facts.provinces {
        f.sea_neighbours.clear();
    }
    let mut w = World::from_facts(&facts, names, &p, 1).unwrap();
    let (s0, s2) = (ActorId(0), ActorId(2));
    let (u0, u2) = (w.units_of(s0)[0], w.units_of(s2)[0]);
    w.declare_war(s0, s2).unwrap();
    // Deep in s2's land, with s1's land (not at war) between it and home, and no sea.
    w.units.get_mut(&u0).unwrap().position = at(2, 2);
    w.units.get_mut(&u2).unwrap().position = at(2, 2);
    set_soldiers(&mut w, u0, 10);
    set_soldiers(&mut w, u2, 6000);
    assert!(w.nearest_own(s0, at(2, 2)).is_none());
    let r = w.tick(&p).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Battle { outcome: Outcome::DefenderWon, .. })));
    assert!(r.events.iter().any(|e| matches!(e.event, Event::UnitDestroyed { unit } if unit == u0)));
    assert!(!w.units.contains_key(&u0));
    assert_eq!(w.ledger.stock(Account::Soldiers(u0)), Some(0), "the men are posted out, not dropped");
}

#[test]
fn an_undefended_province_falls_within_the_tick_and_a_capital_after_siege_capital_ticks_and_the_capital_moves() {
    let p = params();
    let mut w = testworld::world(1);
    let (s0, s1) = (ActorId(0), ActorId(1));
    let (u0, u1) = (w.units_of(s0)[0], w.units_of(s1)[0]);
    w.declare_war(s0, s1).unwrap();
    // s1's army is away at the bottom of its column; its capital stands undefended.
    w.units.get_mut(&u1).unwrap().position = at(1, 4);
    w.order_move(u0, at(1, 1)).unwrap();
    let mut taken_at = None;
    for t in 1..=p.siege_capital_ticks + 1 {
        let r = w.tick(&p).unwrap();
        assert_eq!(w.units[&u0].position, at(1, 0), "besieging the capital, tick {t}");
        if r.events.iter().any(|e| matches!(e.event, Event::ProvinceChangedHands { province, from, to, .. } if province == at(1, 0) && from == s1 && to == s0)) {
            taken_at = Some(t);
            assert!(r.events.iter().any(|e| matches!(e.event, Event::CapitalMoved { actor, to } if actor == s1 && to == at(1, 4))), "the most populous province left is the bottom one");
            assert_eq!(w.actors[&s1].capital, at(1, 4));
            break;
        }
        assert_eq!(w.provinces[&at(1, 0)].siege.map(|s| s.ticks), Some(t));
    }
    assert_eq!(taken_at, Some(p.siege_capital_ticks), "a capital holds siege.capital_ticks");
    // The march goes on: the next province has no defender and falls within the tick.
    let r = w.tick(&p).unwrap();
    assert_eq!(w.units[&u0].position, at(1, 1));
    assert!(r.events.iter().any(|e| matches!(e.event, Event::ProvinceChangedHands { province, from, to, how: Transfer::Conquest } if province == at(1, 1) && from == s1 && to == s0)));
    assert!(w.actors[&s0].provinces.contains(&at(1, 1)) && !w.actors[&s1].provinces.contains(&at(1, 1)));
    assert!(w.units[&u0].destination.is_none());
    // A defended province is not besieged.
    assert!(w.provinces[&at(1, 4)].siege.is_none());
}

#[test]
fn a_polity_without_land_falls_its_levies_join_the_people_and_its_war_closes() {
    let mut s = testworld::five_states_scenario();
    s.verbs.insert("disband".to_string(), "Disband".to_string());
    let mut g = Game::new(&s, params(), 2).unwrap();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let (u0, u1) = (g.world().units_of(s0)[0], g.world().units_of(s1)[0]);
    g.world_mut().declare_war(s0, s1).unwrap();
    set_soldiers(g.world_mut(), u0, 6000);
    // s1's army stands abroad, on s2's land, and is still there when the last province falls.
    let abroad = at(2, 4);
    g.world_mut().units.get_mut(&u1).unwrap().position = abroad;
    let people_abroad = g.world().population(abroad);
    let mut fell_in = None;
    'sweep: for target in [at(1, 0), at(1, 1), at(1, 2), at(1, 3), at(1, 4)] {
        g.world_mut().order_move(u0, target).unwrap();
        for _ in 0..12 {
            let r = g.tick(&[]).unwrap();
            // The levy and the fall move men between people and soldiers, never make or
            // lose them: those sources sum to zero over all accounts.
            let moved: i64 = r.postings.iter().filter(|q| matches!(q.source, Source::Recruit | Source::Disband)).map(|q| q.milli).sum();
            assert_eq!(moved, 0);
            assert!(r.postings.iter().all(|q| q.source != Source::Hire));
            if r.events.iter().any(|e| matches!(e.event, Event::ActorDied { actor } if actor == s1)) {
                let soldiers_out: i64 = r.postings.iter().filter(|q| q.source == Source::Disband && matches!(q.account, Account::Soldiers(_))).map(|q| q.milli).sum();
                let people_in: i64 = r.postings.iter().filter(|q| q.source == Source::Disband && matches!(q.account, Account::Population(_))).map(|q| q.milli).sum();
                assert!(soldiers_out < 0 && people_in == -soldiers_out, "the fallen polity's levies joined the people where they stood: {soldiers_out} {people_in}");
                assert!(g.world().population(abroad) >= people_abroad + 500, "into the province they stood in");
                fell_in = Some(r.tick);
                break 'sweep;
            }
            if g.world().provinces[&target].owner == s0 {
                break;
            }
        }
    }
    assert!(fell_in.is_some(), "s1 lost its last province");
    let w = g.world();
    assert!(!w.alive(s1));
    assert!(w.actors[&s1].provinces.is_empty());
    assert!(w.units_of(s1).is_empty());
    assert!(!w.units.contains_key(&u1));
    assert!(w.wars.is_empty(), "the war closed with the fall");
    assert!(w.actors.contains_key(&s1), "the record stays: no dangling id");
    assert!(g.verbs(ActorRef::Ruler(s1)).iter().all(|o| !o.available && o.why.as_deref() == Some("the polity has fallen")));
    let err = g.tick(&[VerbCall { actor: ActorRef::Ruler(s1), verb: VerbId::Recruit, target: Target::Province(at(1, 0)) }]).unwrap_err();
    assert!(matches!(err, SimError::Verb(engine2::economy::VerbError::Fallen(a)) if a == s1), "{err}");
}

#[test]
fn foraging_on_foreign_land_feeds_up_to_what_the_province_can_and_ruins_it_in_proportion_to_the_load() {
    let p = params();
    let field = at(1, 1);
    let run = |soldiers: i64| {
        let mut w = testworld::world(1);
        let u0 = w.units_of(ActorId(0))[0];
        w.units.get_mut(&u0).unwrap().position = field;
        set_soldiers(&mut w, u0, soldiers);
        let people = w.population(field);
        let r = w.tick(&p).unwrap();
        let ruin = r.postings.iter().filter(|q| q.source == Source::Ruin && q.account == Account::Population(field)).map(|q| -q.milli).sum::<i64>();
        let attrition = r.events.iter().any(|e| matches!(e.event, Event::Attrition { unit, .. } if unit == u0));
        let fed = r.events.iter().find_map(|e| match e.event {
            Event::Ruin { province, foragers } if province == field => Some(foragers as i64),
            _ => None,
        });
        (people, ruin, attrition, fed, w.soldiers(u0))
    };
    let (people, ruin_small, attrition_small, fed_small, left_small) = run(10);
    let capacity = people * p.forage_soldiers_per_1000 / 1000;
    let (_, ruin_big, attrition_big, fed_big, left_big) = run(capacity + 1000);
    assert_eq!((fed_small, fed_big), (Some(10), Some(capacity)));
    assert!(!attrition_small && left_small == 10, "a fed unit does not melt");
    assert!(attrition_big, "beyond the capacity the rest melts");
    let melted = capacity + 1000 - left_big;
    assert_eq!(melted, (1000 * p.army_attrition_unsupplied_ppm_per_tick + 999_999) / 1_000_000, "only the unfed thousand melts, by А7");
    // Full load costs the full rate; ten men cost ten men's share of it.
    assert_eq!(ruin_big, people * p.ruin_loss_ppm_at_full_load / 1000);
    assert_eq!(ruin_small, people * p.ruin_loss_ppm_at_full_load / 1000 * 10 / capacity);
    assert!(ruin_small * 10 < ruin_big, "ten men do not ruin like an army");
}

#[test]
fn the_levy_moves_people_into_soldiers_within_a1_and_a2_and_back_when_disbanded() {
    let p = params();
    let mut s = testworld::five_states_scenario();
    s.verbs.insert("disband".to_string(), "Disband".to_string());
    let mut g = Game::new(&s, params(), 1).unwrap();
    let s0 = ActorId(0);
    let who = ActorRef::Ruler(s0);
    let field = at(0, 1);
    let ceiling = g.world().recruit_ceiling(s0, &p);
    let per_tick = g.world().recruit_per_tick(s0, &p);
    assert_eq!(ceiling, (60_000 + 600) * p.recruit_ceiling_ppm / 1_000_000);
    assert_eq!(per_tick, ceiling * p.recruit_rate_ppm_per_tick / 1_000_000);
    let before = people_and_soldiers(g.world());
    let people_before = g.world().population(field);
    let r = g.tick(&[VerbCall { actor: who, verb: VerbId::Recruit, target: Target::Province(field) }]).unwrap();
    let new_unit = r.events.iter().find_map(|e| match e.event {
        Event::Recruited { unit, soldiers } => Some((unit, soldiers)),
        _ => None,
    });
    let (unit, raised) = new_unit.expect("a levy was raised");
    assert_eq!(raised as i64, per_tick);
    assert_eq!(g.world().units[&unit].position, field);
    assert!(!g.world().units[&unit].mercenary);
    assert_eq!(r.postings.iter().filter(|q| q.source == Source::Recruit).map(|q| q.milli).sum::<i64>(), 0, "a transfer: the sum of people and soldiers is unchanged by the levy");
    let growth: i64 = r.postings.iter().filter(|q| q.source == Source::Growth).map(|q| q.milli).sum::<i64>() / 1000;
    assert!((people_and_soldiers(g.world()) - before - growth).abs() <= 25, "a carried thousandth per province at most");
    assert!(g.world().population(field) < people_before);
    // Raise until the ceiling: the offer then closes.
    let mut raised_total = raised as i64;
    for _ in 0..8 {
        if !g.verbs(who).iter().any(|o| o.verb == VerbId::Recruit && o.available) {
            break;
        }
        let r = g.tick(&[VerbCall { actor: who, verb: VerbId::Recruit, target: Target::Province(field) }]).unwrap();
        raised_total += r.events.iter().filter_map(|e| if let Event::Recruited { soldiers, .. } = e.event { Some(soldiers as i64) } else { None }).sum::<i64>();
    }
    // The levy never passes the ceiling. (It does not sit at it either: the ceiling of А1 is
    // above the budget of А11, so the unpaid share melts at home — the budget is the limit.)
    let (army, ceiling) = (g.world().army_size(s0), g.world().recruit_ceiling(s0, &p));
    assert!(army <= ceiling, "army {army}, ceiling {ceiling}");
    assert!(raised_total + 600 >= ceiling * 95 / 100, "the levy reached the ceiling over the ticks: raised {raised_total}");
    // At the ceiling the offer closes.
    g.world_mut().post(Account::Soldiers(unit), Source::Test, ceiling * 1000).unwrap();
    let offer = g.verbs(who).into_iter().find(|o| o.verb == VerbId::Recruit).unwrap();
    assert!(!offer.available && offer.why.as_deref() == Some("at the levy ceiling"));
    g.world_mut().post(Account::Soldiers(unit), Source::Test, -ceiling * 1000).unwrap();
    // The second levy joined the unit standing there rather than founding another.
    assert_eq!(g.world().units_of(s0).len(), 2);
    // Disband: the levy goes back to the people of the province it stands in.
    let soldiers = g.world().soldiers(unit);
    let people = g.world().population(field);
    let r = g.tick(&[VerbCall { actor: who, verb: VerbId::Disband, target: Target::Unit(unit) }]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Disbanded { unit: u, soldiers: n } if u == unit && n as i64 == soldiers)));
    assert_eq!(r.postings.iter().filter(|q| q.source == Source::Disband).map(|q| q.milli).sum::<i64>(), 0);
    assert!(!g.world().units.contains_key(&unit));
    assert!(g.world().population(field) >= people + soldiers);
}

#[test]
fn mercenaries_come_from_outside_against_the_advance_leave_when_disbanded_and_a_loan_pays_the_advance_in_the_same_tick() {
    let p = params();
    let mut s = testworld::five_states_scenario();
    for (k, v) in [("hire_mercenaries", "Hire mercenaries"), ("disband", "Disband"), ("take_loan", "Take a loan")] {
        s.verbs.insert(k.to_string(), v.to_string());
    }
    let mut g = Game::new(&s, params(), 1).unwrap();
    let s0 = ActorId(0);
    let who = ActorRef::Ruler(s0);
    // No bank, no treasury: nobody hires.
    let t = g.world().treasury(s0);
    g.world_mut().post(Account::Treasury(s0), Source::Test, -t * 1000).unwrap();
    let hire = g.verbs(who).into_iter().find(|o| o.verb == VerbId::HireMercenaries).unwrap();
    assert!(!hire.available && hire.why.as_deref() == Some("cannot pay an advance"));
    // With a bank: borrow and hire in the same tick — the advance leaves before the sweep.
    g.world_mut().actors.get_mut(&s0).unwrap().institutions.insert(Institution::Bank);
    let people_before = 0;
    let r = g
        .tick(&[VerbCall { actor: who, verb: VerbId::TakeLoan, target: Target::None }, VerbCall { actor: who, verb: VerbId::HireMercenaries, target: Target::None }])
        .unwrap();
    let hired = r.events.iter().find_map(|e| match e.event {
        Event::Hired { unit, soldiers, advance_kg } => Some((unit, soldiers as i64, advance_kg as i64)),
        _ => None,
    });
    let (unit, n, advance) = hired.expect("hired on the loan");
    assert!(n > 0);
    assert_eq!(advance, n * p.mercenary_advance_kg);
    assert!(g.world().units[&unit].mercenary);
    assert_eq!(g.world().units[&unit].position, g.world().actors[&s0].capital);
    let on = |account: Account, src: Source| r.postings.iter().filter(|q| q.account == account && q.source == src).map(|q| q.milli).sum::<i64>();
    assert_eq!(on(Account::Treasury(s0), Source::Hire), -advance * 1000);
    assert_eq!(on(Account::Soldiers(unit), Source::Hire), n * 1000);
    assert!(r.postings.iter().all(|q| !(q.source == Source::Hire && matches!(q.account, Account::Population(_)))), "mercenaries come from outside the world");
    assert!(g.world().debt(s0) >= advance, "the advance was borrowed, not conjured");
    // Disbanded, they leave the world: no people appear anywhere.
    let r = g.tick(&[VerbCall { actor: who, verb: VerbId::Disband, target: Target::Unit(unit) }]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Disbanded { unit: u, .. } if u == unit)));
    assert_eq!(on_report(&r, Source::Disband, true), 0, "no leg on the people");
    assert_eq!(on_report(&r, Source::Disband, false), -n * 1000);
    assert!(!g.world().units.contains_key(&unit));
    let _ = people_before;
}

fn on_report(r: &engine2::TickReport, src: Source, people: bool) -> i64 {
    r.postings.iter().filter(|q| q.source == src && matches!(q.account, Account::Population(_)) == people).map(|q| q.milli).sum()
}

#[test]
fn scripted_wars_on_generated_worlds_keep_the_books_change_owners_and_the_stronger_side_wins_inside_a13() {
    let p = params();
    let band = p.entries["battle.observed_win_share_of_the_stronger"].band.unwrap();
    let (mut battles, mut stronger_won, mut transfers, mut deaths, mut destroyed) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let c = s.compile(&p).unwrap();
        let mut w = World::from_facts(&c.facts, c.names, &p, seed).unwrap();
        for t in 0..300 {
            if t % 5 == 0 {
                script_wars(&mut w);
            }
            script_levy(&mut w, &p);
            let r = w.tick(&p).unwrap();
            for e in &r.events {
                match e.event {
                    Event::Battle { outcome, soldiers, .. } => {
                        if soldiers.0 != soldiers.1 {
                            battles += 1;
                            let attacker_stronger = soldiers.0 > soldiers.1;
                            if attacker_stronger == (outcome == Outcome::AttackerWon) {
                                stronger_won += 1;
                            }
                        }
                    }
                    Event::ProvinceChangedHands { .. } => transfers += 1,
                    Event::ActorDied { .. } => deaths += 1,
                    Event::UnitDestroyed { .. } => destroyed += 1,
                    _ => {}
                }
            }
        }
        for (a, actor) in &w.actors {
            // Alive: land and a living ruler (a polity whose houses died out fell with its
            // land, plan §18).
            assert_eq!(w.alive(*a), !actor.provinces.is_empty() && w.person_alive(actor.ruler));
            for u in w.units_of(*a) {
                assert!(w.alive(*a), "seed {seed}: a fallen polity keeps unit {u:?}");
            }
            for (x, y) in &w.wars {
                assert!(w.alive(*x) && w.alive(*y), "seed {seed}: a war with a fallen polity");
            }
        }
        for (pid, pr) in &w.provinces {
            assert!(w.actors[&pr.owner].provinces.contains(pid), "seed {seed}: {pid:?} not in its owner's list");
        }
    }
    let share = stronger_won as f64 / battles as f64;
    eprintln!("scripted wars: {battles} unequal battles, the stronger side won {share:.3}; {transfers} transfers, {deaths} polities fell, {destroyed} units destroyed");
    assert!(battles >= 200, "too few battles to say anything: {battles}");
    assert!(transfers > 0, "no province ever changed hands");
    assert!((band[0]..=band[1]).contains(&share), "the stronger side won {share:.3} of battles, outside А13 {band:?}: the exponent k wants another value");
}

/// Every living polity at war with nobody declares war on its lowest-id living neighbour;
/// every army marches at the capital of its lowest-id enemy. A script for the mechanics, not
/// an agent: the sentinel measures nothing here (plan §17).
fn script_wars(w: &mut World) {
    let ids: Vec<ActorId> = w.actors.keys().copied().filter(|a| w.alive(*a)).collect();
    for a in &ids {
        let mut enemy = ids.iter().copied().find(|b| w.at_war(*a, *b));
        if enemy.is_none() {
            let mine = w.actors[a].provinces.clone();
            enemy = mine
                .iter()
                .flat_map(|p| w.provinces[p].neighbours.iter().chain(w.provinces[p].sea_neighbours.iter()))
                .map(|n| w.provinces[n].owner)
                .filter(|o| o != a && w.alive(*o))
                .min();
            if let Some(b) = enemy {
                w.declare_war(*a, b).unwrap();
            }
        }
        let Some(b) = enemy else { continue };
        let capital = w.actors[&b].capital;
        for u in w.units_of(*a) {
            if w.units[&u].destination.is_none() {
                w.order_move(u, capital).unwrap();
            }
        }
    }
}

/// Every living polity levies at its capital whenever it can.
fn script_levy(w: &mut World, p: &Params) {
    let ids: Vec<ActorId> = w.actors.keys().copied().filter(|a| w.alive(*a)).collect();
    for a in ids {
        let capital = w.actors[&a].capital;
        let _ = w.apply_verb(&VerbCall { actor: ActorRef::Ruler(a), verb: VerbId::Recruit, target: Target::Province(capital) }, p);
    }
}

#[test]
fn a_war_is_deterministic_through_save_and_load() {
    let p = params();
    let s = worldgen::generate(3, &p);
    let mut g = Game::new(&s, params(), 3).unwrap();
    let mut straight = Vec::new();
    for _ in 0..12 {
        script_wars(g.world_mut());
        script_levy(g.world_mut(), &p);
        straight.push(g.tick(&[]).unwrap());
    }
    let mut g2 = Game::new(&s, params(), 3).unwrap();
    let mut spliced = Vec::new();
    for _ in 0..6 {
        script_wars(g2.world_mut());
        script_levy(g2.world_mut(), &p);
        spliced.push(g2.tick(&[]).unwrap());
    }
    let save = g2.to_save().unwrap();
    let mut g3 = Game::from_save(&s, params(), &save).unwrap();
    for _ in 0..6 {
        script_wars(g3.world_mut());
        script_levy(g3.world_mut(), &p);
        spliced.push(g3.tick(&[]).unwrap());
    }
    assert_eq!(straight, spliced);
    assert!(straight.iter().any(|r| r.events.iter().any(|e| matches!(e.event, Event::Battle { .. }))), "the run had a battle to save");
}
