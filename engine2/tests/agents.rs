//! E3 step (а) acceptance (plan §23, §23a): goals and utility.
//!
//! - aspirations and regions are facts of the scenario, resolved by kind; the generator
//!   cuts every landmass into regions and draws two aspirations per polity by a weighted
//!   lot over reachable targets — the peaceful kinds heavier than the greedy, a vassal
//!   wanting to be free; the extreme worlds start with none;
//! - a goal's progress is read off the state alone: held, taken, subdued, free, united,
//!   allied against, of the faith; a goal reached goes on as «hold»; a goal without
//!   progress for the horizon is put aside and the next one taken by the rules;
//! - a bot calls only what is on offer (zero rejections), decides by softmax on one keyed
//!   draw per tick, «do nothing» always in the set, the player's actor left alone; the
//!   same seed gives the same game; a cold temperature is the best option;
//! - on 20 generated worlds × 300 ticks with every polity a bot, the measures of E2 are
//!   read again (owner's rule on #291): В5, В6, В7, В8, А11, А12, Д3, Э8 — and no polity
//!   falls but by conquest;
//! - the outcome depends on the strategy (a player who does nothing against the bot in its
//!   seat) and on the seed; a run with bots saves and loads bit for bit.

use std::collections::BTreeMap;

use engine2::sim::{Game, Simulation};
use engine2::{testworld, worldgen, ActorId, ActorRef, Event, GoalKind, GoalTarget, Params, ProvinceId, RegionId, ScenarioData, Source, Target, VerbCall, VerbId, World};
use sentinel::Observed;

fn params() -> Params {
    Params::load().unwrap()
}

/// Province `(x, y)` of the 5×5 test world; state `x` owns column `x`, capital at `y = 0`.
fn at(x: u32, y: u32) -> ProvinceId {
    ProvinceId(y * 5 + x)
}

fn all_verbs(mut s: ScenarioData) -> ScenarioData {
    for v in VerbId::ALL {
        s.verbs.insert(v.key().to_string(), v.key().replace('_', " "));
    }
    s
}

/// The refusals a call may meet at application: an earlier call of the same tick closed it
/// — the bride two houses asked for, a war closed by a peace of its principal pair (plan
/// §23b: first all decide, then all is applied in one order). A peace or a war both sides
/// made in one tick is not among them: the second is nothing (owner's rule on #291). A peace
/// refused because the other side stated its claim earlier in the tick (A′, plan §28.12)
/// is one: the claim gave it a cause after the peace was decided.
///
/// **«cannot pay»** is one since (ц1) (owner's decision 2026-11-12, plan §31.10; measured
/// on seed 16, tick 79): the price of an embassy is a share of the polity's revenue, and an
/// earlier call of the same tick — a neighbour's disbanding, whose men went home to land the
/// polity collects from — raised the revenue, and the price with it, above a treasury that
/// covered it when the bot decided; nothing touched the treasury before the call. **The
/// reason's line does not tell that race from a bot calling what it cannot pay**, so the
/// measures print the count of such refusals beside the assertion (`CANNOT_PAY`).
const CONFLICTS: [&str; 11] = ["at war", "its army paid", "no unmarried adult to marry", "the other house has no unmarried adult", "the relation is below the threshold", "a vassal fights only its lord", "a vassal makes no alliance", "at the levy ceiling", "not the polity's own province", "at the debt ceiling", CANNOT_PAY];

/// The refusal of a call the treasury cannot pay at application (`CONFLICTS`).
const CANNOT_PAY: &str = "cannot pay";

/// A conflict of the tick: a call closed by an earlier call of the same tick — or, since
/// E3 (в), by an opportunity taken at the turn's start: everyone decides on the state as
/// it stands, the answers to the last tick's opportunities are applied first, and a
/// secession or a usurpation they bring moves units and ceilings under the orders and
/// calls decided before it (a house's men leave with it, a usurper's house takes the
/// retinue of the throne, an extinct house's men go home).
fn is_conflict(k: &str) -> bool {
    CONFLICTS.iter().any(|c| k.ends_with(c)) || (k.starts_with("Order:") && (k.contains("does not command unit") || k.contains("unknown unit")))
}

fn assert_conflicts_only(g: &Game) {
    let r = g.preempted_reasons();
    assert!(r.keys().all(|k| is_conflict(k)), "a call preempted for another reason than a conflict of the tick: {r:?}");
}

fn world_of(s: &ScenarioData, seed: u64) -> World {
    let c = s.compile(&params()).unwrap();
    World::from_facts(&c.facts, c.names, &params(), seed).unwrap()
}

/// Leave polity `a` no free money (plan §31.3): a debt past everything it would have left
/// over the horizon, and a hundred years of its income beyond — every spending of its
/// decisions is then priced at the face value, as before the price of money.
fn leave_no_free_money(w: &mut World, a: ActorId, p: &Params) {
    let debt = w.free_money(a, p).max(0.0) as i64 + 100 * w.yearly_income(a, p);
    w.post(engine2::Account::Debt(a), Source::Test, debt * 1000).unwrap();
    assert!(w.free_money(a, p) < 0.0, "no free money left");
}

/// Leave polity `a` (at peace, no bank) `kg` of money at hand for a spending made once
/// (`money_over` over a tick, plan §31.11) and more over the horizon: the treasury emptied, men
/// added to its first unit until a tick leaves no more than `kg`, and the treasury topped up to
/// `kg`. A spending dearer than `kg` is then priced in part though the free money covers it.
fn leave_money_at_hand(w: &mut World, a: ActorId, kg: i64, p: &Params) {
    let all = w.treasury(a);
    w.post(engine2::Account::Treasury(a), Source::Test, -all * 1000).unwrap();
    let left = w.money_over(a, 1.0, p) as i64;
    if left > kg {
        let per_man = (engine2::economy::pay_per_tick(1000, p) / 1000).max(1);
        let unit = w.units_of(a)[0];
        w.post(engine2::Account::Soldiers(unit), Source::Test, ((left - kg) / per_man + 1) * 1000).unwrap();
    }
    let left = w.money_over(a, 1.0, p) as i64;
    assert!(left > 0 && left <= kg, "a tick leaves {left}, no more than {kg}");
    w.post(engine2::Account::Treasury(a), Source::Test, (kg - left) * 1000).unwrap();
    assert!((w.money_over(a, 1.0, p) - kg as f64).abs() < 1.0 && w.free_money(a, p) > kg as f64, "{kg} at hand, more over the horizon");
}

#[test]
fn aspirations_and_regions_are_facts_and_the_generator_draws_them_by_a_weighted_lot_over_reachable_targets() {
    let p = params();
    let mut by_kind: BTreeMap<GoalKind, usize> = BTreeMap::new();
    let (mut polities, mut vassals_free) = (0usize, 0usize);
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let g = Game::new(&s, params(), seed).unwrap();
        let w = g.world();
        // Regions: every province in exactly one, none larger than a kingdom's worth.
        let mut seen: BTreeMap<ProvinceId, usize> = BTreeMap::new();
        for (i, r) in w.regions.iter().enumerate() {
            assert!(!r.is_empty() && r.len() <= 8, "seed {seed}: region {i} of {}", r.len());
            for q in r {
                assert!(seen.insert(*q, i).is_none(), "seed {seed}: {q:?} in two regions");
            }
        }
        assert_eq!(seen.len(), w.provinces.len(), "seed {seed}: a province in no region");
        assert_eq!(w.names.regions.len(), w.regions.len());
        for (a, actor) in &w.actors {
            polities += 1;
            assert!((1..=3).contains(&actor.goals.len()), "seed {seed} {a:?}: {} goals", actor.goals.len());
            if actor.vassal_of.is_some() {
                assert!(actor.goals.iter().any(|g| g.kind == GoalKind::Free), "seed {seed} {a:?}: a vassal without the wish to be free");
                vassals_free += 1;
            }
            for g in &actor.goals {
                *by_kind.entry(g.kind).or_insert(0) += 1;
                match (g.kind, g.target) {
                    (GoalKind::Hold, GoalTarget::Province(q)) => assert_eq!(w.provinces[&q].owner, *a, "seed {seed}: holds another's"),
                    (GoalKind::Take, GoalTarget::Province(q)) => {
                        assert_ne!(w.provinces[&q].owner, *a);
                        assert!(w.borders(*a, w.provinces[&q].owner), "seed {seed}: a province to take beyond reach");
                    }
                    (GoalKind::Subdue, GoalTarget::Polity(b)) | (GoalKind::AllianceAgainst, GoalTarget::Polity(b)) => assert!(w.borders(*a, b), "seed {seed}: {b:?} is no neighbour of {a:?}"),
                    (GoalKind::Unite, GoalTarget::Region(r)) => assert!(w.regions[r.index()].contains(&actor.capital), "seed {seed}: the region to unite is not the capital's"),
                    (GoalKind::Free, GoalTarget::None) => {}
                    (GoalKind::Union, GoalTarget::Religion(f)) => {
                        assert_ne!(f, w.polity_religion(*a), "seed {seed}: a union with its own faith");
                        assert!(w.actors.keys().any(|b| w.borders(*a, *b) && w.polity_religion(*b) == f), "seed {seed}: a union with a faith no neighbouring court prays");
                    }
                    other => panic!("seed {seed}: the generator drew {other:?}"),
                }
                assert_eq!(g.since, engine2::Tick(0));
            }
        }
    }
    eprintln!("aspirations over {polities} polities: {by_kind:?}; {vassals_free} vassals wanting to be free");
    assert!(by_kind.get(&GoalKind::Union).copied().unwrap_or(0) > 0, "the generator draws a union where neighbouring courts pray otherwise (owner's rule on #293)");
    let n = |k: GoalKind| by_kind.get(&k).copied().unwrap_or(0);
    assert!(n(GoalKind::Hold) + n(GoalKind::AllianceAgainst) > n(GoalKind::Take) + n(GoalKind::Subdue), "the peaceful kinds outweigh the greedy (owner's answer 3)");
    assert!(n(GoalKind::Take) > 0 && n(GoalKind::Subdue) > 0 && n(GoalKind::Unite) > 0, "every drawn kind exists");
    for (name, s) in worldgen::extreme_worlds(&p) {
        let g = Game::new(&s, params(), 1).unwrap();
        assert!(g.world().actors.values().all(|a| a.goals.is_empty()), "{name}: a sketch of geography starts with no aspiration");
    }
}

#[test]
fn a_goals_progress_is_read_off_the_state_a_reached_goal_goes_on_as_hold_and_one_without_progress_is_put_aside() {
    let p = params();
    let mut s = testworld::five_states_scenario();
    s.regions = Some(BTreeMap::from([("left".to_string(), engine2::scenario::RegionData { name: "Left bank".into(), provinces: vec!["p00".into(), "p01".into(), "p05".into(), "p06".into()] })]));
    let mut w = world_of(&s, 41);
    let (s0, s1) = (ActorId(0), ActorId(1));
    let progress = |w: &World, a: ActorId, i: usize| w.goal_progress(a, &w.actors[&a].goals[i]);
    // Hold an own province: held while owned, not in revolt, not besieged.
    w.set_goal(s0, GoalKind::Hold, GoalTarget::Province(at(0, 1))).unwrap();
    assert_eq!(progress(&w, s0, 0), 1.0);
    w.rise_now(at(0, 1), &p).unwrap();
    assert_eq!(progress(&w, s0, 0), 0.0, "in revolt: not held");
    // Take a neighbour's province: nothing, a third at war, all when owned.
    w.set_goal(s0, GoalKind::Take, GoalTarget::Province(at(1, 1))).unwrap();
    assert_eq!(progress(&w, s0, 1), 0.0);
    w.declare_war(s0, s1).unwrap();
    assert!((progress(&w, s0, 1) - 0.3).abs() < 1e-9);
    // Subdue, free, unite, alliance against, union.
    w.set_goal(s0, GoalKind::Subdue, GoalTarget::Polity(s1)).unwrap();
    assert!((progress(&w, s0, 2) - 0.3).abs() < 1e-9, "at war with the polity to subdue");
    w.set_goal(s0, GoalKind::Unite, GoalTarget::Region(RegionId(0))).unwrap();
    assert_eq!(progress(&w, s0, 3), 0.5, "two of the four provinces of the region are s0's");
    w.set_goal(s0, GoalKind::AllianceAgainst, GoalTarget::Polity(s1)).unwrap();
    assert_eq!(progress(&w, s0, 4), 0.0);
    w.ally_now(s0, ActorId(2), &p).unwrap();
    assert_eq!(progress(&w, s0, 4), 1.0, "an ally that borders the target");
    w.set_goal(s0, GoalKind::Union, GoalTarget::Religion(engine2::ReligionId(0))).unwrap();
    assert_eq!(progress(&w, s0, 5), 1.0);
    w.set_goal(s1, GoalKind::Free, GoalTarget::None).unwrap();
    assert_eq!(progress(&w, s1, 0), 1.0, "nobody's vassal is free already");
    w.make_vassal(s1, ActorId(3)).unwrap();
    assert_eq!(progress(&w, s1, 0), 0.0);
    w.declare_war(s1, ActorId(3)).unwrap();
    assert_eq!(progress(&w, s1, 0), 0.5, "at war with the lord: halfway");
    // Take a polity: the share of its provinces at the goal's setting that are gone.
    w.set_goal(s0, GoalKind::Take, GoalTarget::Polity(s1)).unwrap();
    assert_eq!(w.actors[&s0].goals[6].baseline, 5);
    assert_eq!(progress(&w, s0, 6), 0.0);
    // The war takes the province: the goal «take» is reached and goes on as «hold».
    let u0 = w.units_of(s0)[0];
    w.post(engine2::Account::Soldiers(u0), Source::Test, (6000 - w.soldiers(u0)) * 1000).unwrap();
    w.units.get_mut(&w.units_of(s1)[0]).unwrap().position = at(1, 4);
    let mut calm = params();
    calm.revolt_per_tick = 0.0;
    let mut reached = None;
    for _ in 0..10 {
        if w.provinces[&at(1, 1)].owner != s0 && w.units[&u0].destination.is_none() {
            w.order_move(u0, at(1, 1)).unwrap();
        }
        let r = w.tick(&calm).unwrap();
        if let Some(g) = r.events.iter().find_map(|e| match e.event {
            Event::GoalReached { actor, goal } if actor == s0 && goal.kind == GoalKind::Take && goal.target == GoalTarget::Province(at(1, 1)) => Some(goal),
            _ => None,
        }) {
            reached = Some(g);
            break;
        }
    }
    assert!(reached.is_some(), "the province taken reaches the goal");
    assert!(w.actors[&s0].goals.iter().any(|g| g.kind == GoalKind::Hold && g.target == GoalTarget::Province(at(1, 1))), "and it goes on as «hold»");
    assert!(!w.actors[&s0].goals.iter().any(|g| g.kind == GoalKind::Take && g.target == GoalTarget::Province(at(1, 1))));
    let take_s1 = *w.actors[&s0].goals.iter().find(|g| g.kind == GoalKind::Take && g.target == GoalTarget::Polity(s1)).expect("the goal to take s1 stands");
    assert!(w.goal_progress(s0, &take_s1) > 0.0, "one province of five gone: the polity is a fifth taken");
    // A goal without progress for the horizon is put aside and the next one taken by the
    // rules: nobody's vassal holds its capital.
    let mut w = world_of(&testworld::five_states_scenario(), 42);
    w.set_goal(s0, GoalKind::Subdue, GoalTarget::Polity(ActorId(4))).unwrap();
    assert_eq!(w.actors[&s0].goals.len(), 1);
    let mut dropped_at = None;
    for t in 0..(p.goal_horizon_ticks + 3) {
        let r = w.tick(&calm).unwrap();
        if r.events.iter().any(|e| matches!(e.event, Event::GoalDropped { actor, goal } if actor == s0 && goal.kind == GoalKind::Subdue)) {
            dropped_at = Some(t);
            assert!(r.events.iter().any(|e| matches!(e.event, Event::GoalSet { actor, .. } if actor == s0)), "the freed slot is filled the same tick");
            break;
        }
    }
    assert_eq!(dropped_at, Some(p.goal_horizon_ticks), "put aside exactly at the horizon");
    // The freed slots are filled by the weighted lot of reachable kinds (E3 (б)), one draw a
    // tick, up to `goal.open_per_polity`.
    assert!(w.actors[&s0].goals.len() <= p.goal_open_per_polity);
    assert!(!w.actors[&s0].goals.iter().any(|g| g.kind == GoalKind::Subdue && g.target == GoalTarget::Polity(ActorId(4))), "the dropped goal is gone");
    // A polity with no goal draws them at the ends of its first ticks, one a tick.
    assert!(!w.actors[&s1].goals.is_empty() && w.actors[&s1].goals.len() <= p.goal_open_per_polity);
    for g in &w.actors[&s1].goals {
        match (g.kind, g.target) {
            (GoalKind::Hold, GoalTarget::Province(q)) => assert_eq!(w.provinces[&q].owner, s1),
            (GoalKind::Take, GoalTarget::Province(q)) => assert!(w.borders(s1, w.provinces[&q].owner)),
            (GoalKind::Subdue | GoalKind::AllianceAgainst, GoalTarget::Polity(b)) => assert!(w.borders(s1, b)),
            (GoalKind::Unite, GoalTarget::Region(_)) => {}
            other => panic!("a drawn goal out of the lot: {other:?}"),
        }
    }
    // Revanche: a province conquered from a polity is its goal at once, by the rule.
    let mut w = world_of(&testworld::five_states_scenario(), 46);
    w.declare_war(s0, s1).unwrap();
    let u0 = w.units_of(s0)[0];
    w.post(engine2::Account::Soldiers(u0), Source::Test, (6000 - w.soldiers(u0)) * 1000).unwrap();
    w.units.get_mut(&w.units_of(s1)[0]).unwrap().position = at(1, 4);
    // From its own province next to the target: no capital on the way to besiege.
    w.units.get_mut(&u0).unwrap().position = at(0, 1);
    w.order_move(u0, at(1, 1)).unwrap();
    let mut revanche = false;
    for _ in 0..3 {
        let r = w.tick(&calm).unwrap();
        if r.events.iter().any(|e| matches!(e.event, Event::ProvinceChangedHands { province, to, .. } if province == at(1, 1) && to == s0)) {
            revanche = r.events.iter().any(|e| matches!(e.event, Event::GoalSet { actor, goal } if actor == s1 && goal.kind == GoalKind::Take && goal.target == GoalTarget::Province(at(1, 1))));
            break;
        }
    }
    assert!(revanche, "the polity that lost a province wants it back the same tick");
}

#[test]
fn a_bot_calls_only_what_is_on_offer_decides_by_softmax_on_one_keyed_draw_and_leaves_the_players_actor_alone() {
    let p = params();
    let mut s = all_verbs(testworld::five_states_scenario());
    // Aspirations: without them a bot holds its capital and nothing is better than doing
    // nothing — the right answer, but nothing to test. State 1 wants a province of state 2,
    // state 3 wants state 4 as a vassal and an alliance against it.
    use engine2::scenario::AspirationData;
    let asp = |kind: GoalKind, target: &str| AspirationData { kind, target: Some(target.to_string()), route: None };
    s.actors.get_mut("s1").unwrap().aspirations = vec![asp(GoalKind::Take, "p07")];
    s.actors.get_mut("s3").unwrap().aspirations = vec![asp(GoalKind::Subdue, "s4"), asp(GoalKind::AllianceAgainst, "s4")];
    let mut g = Game::new(&s, params(), 43).unwrap();
    let s0 = ActorId(0);
    assert_eq!(g.player(), None);
    // Without goals nobody acts but on its tax: on the first tick, before any goal is drawn,
    // nothing is better than doing nothing — except the step of the tax toward the world's
    // equilibrium, a decision of the take, not of a goal (E3 (е1)). The army stands: the
    // chronicle is empty, so no threat is read and no levy pays, and the starting army is not
    // disbanded while the record is shorter than the horizon (E3 (г), owner's decision 3 on
    // §29.8 — an empty chronicle says nothing, not «no wars»).
    let quiet = all_verbs(testworld::five_states_scenario());
    let mut q = Game::new(&quiet, params(), 43).unwrap();
    let r = q.tick(&[]).unwrap();
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::Verb { verb, .. } if verb != VerbId::RaiseTax)), "a bot that wants nothing does nothing but its tax");
    g.set_player(Some(ActorRef::Ruler(s0)));
    // Nothing is always an option, at nothing.
    let options = g.world().options(ActorId(1), &VerbId::ALL, &p);
    assert!(options[0].call.is_none() && options[0].utility == 0.0);
    assert!(options.len() > 1, "a ruler at the start has something on offer");
    // The choice: nothing, then each verb once with its best target, only above nothing.
    let choices = g.world().choices(ActorId(1), &VerbId::ALL, &p);
    assert!(choices[0].call.is_none());
    let verbs: Vec<VerbId> = choices.iter().skip(1).map(|o| o.call.unwrap().verb).collect();
    let mut dedup = verbs.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(verbs.len(), dedup.len(), "a verb enters once, whatever its number of targets");
    assert!(choices.iter().skip(1).all(|o| o.utility > 0.0), "nothing no better than doing nothing");
    for c in choices.iter().skip(1) {
        let best = options.iter().filter(|o| o.call.map(|x| x.verb) == c.call.map(|x| x.verb)).map(|o| o.utility).fold(f64::MIN, f64::max);
        assert!((c.utility - best).abs() < 1e-12, "the verb's best target");
    }
    for o in &options {
        if let Some(c) = o.call {
            let mut w = g.world().clone();
            assert!(w.apply_verb(&c, &p).is_ok(), "an option the world refuses: {c:?}");
        }
    }
    let mut verbs_by_bots = 0;
    for _ in 0..50 {
        let living = g.world().actors.keys().filter(|a| g.world().alive(**a) && **a != s0).count();
        let r = g.tick(&[]).unwrap();
        let decisions: Vec<&engine2::RngDraw> = r.draws.iter().filter(|d| d.draw.point == engine2::DrawPoint::Decision).collect();
        assert_eq!(decisions.len(), living, "one keyed draw per deciding actor");
        assert!(decisions.iter().all(|d| d.draw.n == 0 && d.draw.id != s0.0), "once, never for the player's actor");
        for e in &r.events {
            if let Event::Verb { actor, .. } = e.event {
                assert_ne!(actor, ActorRef::Ruler(s0), "the bots left the player's actor alone");
                verbs_by_bots += 1;
            }
        }
    }
    // Every option was on offer at the decision (checked above on a clone); at application
    // an earlier call of the same tick may close a later one — only those conflicts.
    assert_conflicts_only(&g);
    assert!(verbs_by_bots > 0, "the bots did something in fifty ticks");
    // The same seed gives the same game; another seed another one.
    let play = |seed: u64, ticks: u32| {
        let mut g = Game::new(&s, params(), seed).unwrap();
        let mut reports = Vec::new();
        for _ in 0..ticks {
            reports.push(g.tick(&[]).unwrap());
        }
        reports
    };
    assert_eq!(play(44, 30), play(44, 30));
    assert_ne!(play(44, 30), play(45, 30));
    // A cold temperature is the best option; a hot one wanders, nothing included.
    let mut cold = params();
    cold.agent_temperature = 1e-6;
    let mut w = g.world().clone();
    let best = w.choices(ActorId(1), &VerbId::ALL, &cold).into_iter().max_by(|x, y| x.utility.partial_cmp(&y.utility).unwrap()).unwrap();
    let chosen = w.decide(ActorId(1), &VerbId::ALL, &cold);
    let chosen_u = w.choices(ActorId(1), &VerbId::ALL, &cold).into_iter().find(|o| o.call == chosen).unwrap().utility;
    assert!((chosen_u - best.utility).abs() < 1e-9, "cold: the best option, {chosen:?}");
    let mut hot = params();
    hot.agent_temperature = 100.0;
    let mut kinds: std::collections::BTreeSet<Option<VerbId>> = std::collections::BTreeSet::new();
    for _ in 0..40 {
        kinds.insert(w.decide(ActorId(1), &VerbId::ALL, &hot).map(|c| c.verb));
    }
    let n_choices = w.choices(ActorId(1), &VerbId::ALL, &hot).len();
    assert!(kinds.contains(&None) && kinds.len() >= n_choices.min(3), "hot: the choices spread, doing nothing among them: {kinds:?} of {n_choices}");
}

/// A side of a war as it stood when the war began (the base of (д1), plan §30.10): its yearly
/// income (`yearly_income`) and its army by the battle's measure (`army_strength` — with
/// every unit at quality 50, as in main, the number of its men). Read at the start of the
/// tick the war opened in; for a polity born in that tick (a secession), at the tick's end —
/// its first state.
#[derive(Clone, Copy)]
struct Side {
    income: i64,
    army: f64,
}

/// A war of principals under way: not a war of obligation (`War::of` none).
struct OpenWar {
    since: u32,
    battles: u32,
    /// Provinces taken in it by conquest, by the lower id and by the higher — the test's own
    /// count off the events, checked against the peace's record (`Event::Defeat`).
    taken: (u32, u32),
    start: (Side, Side),
}

/// How a decided war of principals ended.
struct Won {
    winner: Side,
    loser: Side,
    /// The provinces ceded on balance, by the peace's record (`Event::Defeat`); none when
    /// the war ended by the loser's fall — no peace was made.
    ceded: Option<u32>,
    /// Ended by the loser's fall with nothing taken from it by the survivor in this war: it
    /// fell to another.
    fell_to_another: bool,
}

/// A war of principals that ended in the run (plan §30.10). **Decided** — the peace found a
/// loser (the sides took unequal numbers of provinces, `War::taken`; the world's
/// `Event::Defeat`), or a side fell; the winner took more, or survived. Otherwise a status quo.
struct EndedWar {
    /// Ticks from the tick it opened in to the tick it ended in.
    length: u32,
    /// Battles of the two principals, and of those in the war by its obligations.
    battles: u32,
    won: Option<Won>,
}

/// The `q`-quantile of a sorted slice: the element at `⌊q · n⌋`, the last at most — the
/// median as this file already reads it (`len / 2`).
fn quantile<T: Copy + Default>(sorted: &[T], q: f64) -> T {
    sorted.get(((sorted.len() as f64 * q) as usize).min(sorted.len().saturating_sub(1))).copied().unwrap_or_default()
}

/// What one generated world adds to the measures of E2 with the bots (one world per thread,
/// `engine2::par`; folded in the order of the seeds — the run of the worlds one after
/// another, byte for byte).
#[derive(Default)]
struct Measures {
    reigns: Vec<f64>,
    usurpations: u64,
    polity_centuries: f64,
    risings: u64,
    large_ticks: u64,
    polities_start: usize,
    polities_century: usize,
    war_ticks: u64,
    polity_ticks: u64,
    at_peace_ticks: u64,
    army_at_war: i64,
    people_at_war: i64,
    army_at_peace: i64,
    people_at_peace: i64,
    /// А11 at peace by windows of fifty ticks (army, people): the swing «more armies → fewer
    /// wars → fewer armies» would show here, not in the mean (owner's answer 1 on §29.6).
    a11_peace_windows: Vec<(i64, i64)>,
    bound_century: usize,
    alive_century: usize,
    tax: i64,
    production: i64,
    falls: u64,
    falls_by_conquest: u64,
    reached: u64,
    dropped: u64,
    verbs: BTreeMap<VerbId, u64>,
    rejections: BTreeMap<String, u64>,
    per_polity_war_share: Vec<f64>,
    rates: Vec<f64>,
    revolt_ticks: u64,
    province_ticks: u64,
    revolt_lengths: Vec<u32>,
    revolts_started: u64,
    revolts_ended: u64,
    revolts_per_province: BTreeMap<(u64, ProvinceId), u32>,
    tax_changes: u64,
    tax_undone: u64,
    declarations: u64,
    /// (A′): claims stated by the attacked in a war declared on it — no new war.
    claims: u64,
    declared_after_peace: u64,
    declared_unpaid: u64,
    war_lengths: Vec<u32>,
    declared_within_five_years: u64,
    opportunities: BTreeMap<String, u64>,
    elections: u64,
    elections_turned: u64,
    people_start: i64,
    people_100: i64,
    people_end: i64,
    migrants: i64,
    migrants_across: i64,
    deficit_share: f64,
    deficit_samples: u64,
    deficit_heavy: u64,
    pay: i64,
    court: i64,
    conquests: u64,
    conquests_unfought: u64,
    /// The base of (д1) (plan §30.9 p. 4, §30.10) — printed, no bands: the wars of principals
    /// that ended, those still open at the run's end, and the wars that opened and were gone
    /// within one tick (whether of principals or of obligation cannot be read: the war is not
    /// in the world after the tick).
    principal_wars: Vec<EndedWar>,
    principal_wars_open: u64,
    wars_within_a_tick: u64,
    /// А13 on the bots' battles: (the men of the side with more, of the side with fewer, the
    /// side with more won); battles of equal numbers apart, and the drawn among all.
    battles: Vec<(u64, u64, bool)>,
    battles_even: u64,
    battles_drawn: u64,
}

/// The measures of E2/E3 with the bots, read off the worlds' sums.
struct Rates {
    b5: f64,
    b6: f64,
    b7: f64,
    b8: f64,
    a11_war: f64,
    a11_peace: f64,
    a12: f64,
    d3: f64,
    e8: f64,
    b9: f64,
    p7: f64,
}

impl Rates {
    /// Each measure with its band in the file: `(name, value, band, inside)`.
    fn against_bands(&self, p: &Params) -> Vec<(&'static str, f64, [f64; 2], bool)> {
        let band = |k: &str| p.entries[k].band.unwrap();
        [
            ("В5", self.b5, band("reign.observed_years")),
            ("В6", self.b6, band("usurpation.observed_per_century")),
            ("В7", self.b7, band("revolt.observed_per_century")),
            ("В8", self.b8, band("collapse.observed_polities_after_a_century")),
            ("А11", self.a11_peace, band("army.observed_share_of_population")),
            ("А12", self.a12, band("war.observed_share_of_years")),
            ("Д3", self.d3, band("diplomacy.observed_bound_share")),
            ("Э8", self.e8, band("tax.observed_share_of_production")),
            ("В9", self.b9, band("revolt.observed_share_of_provinces")),
            ("П7", self.p7, band("population.observed_growth_per_year")),
        ]
        .into_iter()
        .map(|(n, v, b)| (n, v, b, (b[0]..=b[1]).contains(&v)))
        .collect()
    }
}

impl Measures {
    fn rates(&self) -> Rates {
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        Rates {
            b5: mean(&self.reigns),
            b6: self.usurpations as f64 / self.polity_centuries,
            b7: self.risings as f64 / (self.large_ticks as f64 / 200.0),
            b8: self.polities_century as f64 / self.polities_start as f64,
            a11_war: self.army_at_war as f64 / self.people_at_war.max(1) as f64,
            a11_peace: self.army_at_peace as f64 / self.people_at_peace.max(1) as f64,
            a12: self.war_ticks as f64 / self.polity_ticks as f64,
            d3: self.bound_century as f64 / self.alive_century as f64,
            e8: self.tax as f64 / self.production as f64,
            b9: self.revolt_ticks as f64 / self.province_ticks as f64,
            // The growth of all the worlds' people over the first 100 ticks, a year (П7 is
            // read over 100 ticks, `tests/population.rs`).
            p7: (self.people_100 as f64 / self.people_start.max(1) as f64).powf(1.0 / 50.0) - 1.0,
        }
    }

    /// The measures over `seeds`, one world per thread, folded in the order of the seeds.
    fn over(seeds: &[u64], p: &Params) -> Measures {
        let mut m = Measures::default();
        for w in engine2::par::map(seeds, |seed| measure_world(*seed, p)) {
            m.add(w);
        }
        m
    }

    fn add(&mut self, o: Measures) {
        self.reigns.extend(o.reigns);
        self.usurpations += o.usurpations;
        self.polity_centuries += o.polity_centuries;
        self.risings += o.risings;
        self.large_ticks += o.large_ticks;
        self.polities_start += o.polities_start;
        self.polities_century += o.polities_century;
        self.war_ticks += o.war_ticks;
        self.polity_ticks += o.polity_ticks;
        self.at_peace_ticks += o.at_peace_ticks;
        self.army_at_war += o.army_at_war;
        self.people_at_war += o.people_at_war;
        self.army_at_peace += o.army_at_peace;
        for (i, (army, people)) in o.a11_peace_windows.into_iter().enumerate() {
            if self.a11_peace_windows.len() <= i {
                self.a11_peace_windows.resize(i + 1, (0, 0));
            }
            self.a11_peace_windows[i].0 += army;
            self.a11_peace_windows[i].1 += people;
        }
        self.people_at_peace += o.people_at_peace;
        self.bound_century += o.bound_century;
        self.alive_century += o.alive_century;
        self.tax += o.tax;
        self.production += o.production;
        self.falls += o.falls;
        self.falls_by_conquest += o.falls_by_conquest;
        self.reached += o.reached;
        self.dropped += o.dropped;
        for (key, v) in o.verbs {
            *self.verbs.entry(key).or_insert(0) += v;
        }
        for (key, v) in o.rejections {
            *self.rejections.entry(key).or_insert(0) += v;
        }
        self.per_polity_war_share.extend(o.per_polity_war_share);
        self.rates.extend(o.rates);
        self.revolt_ticks += o.revolt_ticks;
        self.province_ticks += o.province_ticks;
        self.revolt_lengths.extend(o.revolt_lengths);
        self.revolts_started += o.revolts_started;
        self.revolts_ended += o.revolts_ended;
        for (key, v) in o.revolts_per_province {
            *self.revolts_per_province.entry(key).or_insert(0) += v;
        }
        self.tax_changes += o.tax_changes;
        self.tax_undone += o.tax_undone;
        self.declarations += o.declarations;
        self.claims += o.claims;
        self.declared_after_peace += o.declared_after_peace;
        self.declared_unpaid += o.declared_unpaid;
        self.war_lengths.extend(o.war_lengths);
        self.declared_within_five_years += o.declared_within_five_years;
        for (key, v) in o.opportunities {
            *self.opportunities.entry(key).or_insert(0) += v;
        }
        self.elections += o.elections;
        self.elections_turned += o.elections_turned;
        self.people_start += o.people_start;
        self.people_100 += o.people_100;
        self.people_end += o.people_end;
        self.migrants += o.migrants;
        self.migrants_across += o.migrants_across;
        self.deficit_share += o.deficit_share;
        self.deficit_samples += o.deficit_samples;
        self.deficit_heavy += o.deficit_heavy;
        self.pay += o.pay;
        self.court += o.court;
        self.conquests += o.conquests;
        self.conquests_unfought += o.conquests_unfought;
        self.principal_wars.extend(o.principal_wars);
        self.principal_wars_open += o.principal_wars_open;
        self.wars_within_a_tick += o.wars_within_a_tick;
        self.battles.extend(o.battles);
        self.battles_even += o.battles_even;
        self.battles_drawn += o.battles_drawn;
    }

    /// The base of (д1) on main (plan §30.9 p. 4, §30.10), one line a group — **printed, never
    /// asserted**: no source gives a band (rule 1 of §24.1). The wars of principals (no war of
    /// obligation), how they ended, their length, battles and provinces ceded; of the decided,
    /// the share the richer and the share the larger army at the start won (equal incomes or
    /// armies out of the denominator, counted beside it); and А13 on the bots' battles.
    fn war_lines(&self) -> Vec<String> {
        let wars = &self.principal_wars;
        let decided: Vec<&Won> = wars.iter().filter_map(|x| x.won.as_ref()).collect();
        let fallen = decided.iter().filter(|x| x.ceded.is_none()).count();
        let fell_to_another = decided.iter().filter(|x| x.fell_to_another).count();
        let sorted_u32 = |mut v: Vec<u32>| {
            v.sort();
            v
        };
        let sorted_f64 = |mut v: Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v
        };
        let lengths = sorted_u32(wars.iter().map(|x| x.length).collect());
        let fought = sorted_u32(wars.iter().map(|x| x.battles).collect());
        let ceded = sorted_u32(decided.iter().filter_map(|x| x.ceded).collect());
        // Who won of the decided, by a measure of the sides at the start: (the greater won,
        // the wars with a greater side, the wars with equal sides).
        let by = |greater: &dyn Fn(&Won) -> Option<bool>| {
            let known: Vec<bool> = decided.iter().filter_map(|x| greater(x)).collect();
            (known.iter().filter(|x| **x).count(), known.len(), decided.len() - known.len())
        };
        let (rich_won, rich_of, rich_even) = by(&|x| (x.winner.income != x.loser.income).then_some(x.winner.income > x.loser.income));
        let (army_won, army_of, army_even) = by(&|x| (x.winner.army != x.loser.army).then_some(x.winner.army > x.loser.army));
        let share = |n: usize, of: usize| n as f64 / of.max(1) as f64;
        let incomes = sorted_f64(decided.iter().map(|x| x.winner.income.max(x.loser.income) as f64 / x.winner.income.min(x.loser.income).max(1) as f64).collect());
        let stronger_won = self.battles.iter().filter(|(_, _, won)| *won).count();
        let odds = sorted_f64(self.battles.iter().map(|(more, fewer, _)| *more as f64 / (*fewer).max(1) as f64).collect());
        vec![
            format!(
                "wars of principals (no war of obligation): ended {} — decided {} (of them by a fall {fallen}, of those the fallen lost nothing to the survivor in the war {fell_to_another}), status quo {}, without a battle {}; open at the run's end {}; wars opened and gone within one tick, kind unreadable {}",
                wars.len(),
                decided.len(),
                wars.len() - decided.len(),
                wars.iter().filter(|x| x.battles == 0).count(),
                self.principal_wars_open,
                self.wars_within_a_tick
            ),
            format!(
                "wars of principals: length q50 {} / q90 {} / max {} ticks; battles a war q50 {} / q90 {}; provinces ceded at a defeat (the peace's record) q50 {} / q90 {} / max {}",
                quantile(&lengths, 0.5),
                quantile(&lengths, 0.9),
                lengths.last().copied().unwrap_or(0),
                quantile(&fought, 0.5),
                quantile(&fought, 0.9),
                quantile(&ceded, 0.5),
                quantile(&ceded, 0.9),
                ceded.last().copied().unwrap_or(0)
            ),
            format!(
                "decided wars of principals: the richer at the start won {rich_won} of {rich_of} = {:.3} (equal incomes {rich_even}); the larger army at the start won {army_won} of {army_of} = {:.3} (equal armies {army_even}); income of the richer over the poorer q25 {:.2} / q50 {:.2} / q75 {:.2}",
                share(rich_won, rich_of),
                share(army_won, army_of),
                quantile(&incomes, 0.25),
                quantile(&incomes, 0.5),
                quantile(&incomes, 0.75)
            ),
            format!(
                "А13 on the bots' battles: the side with more men won {stronger_won} of {} = {:.3} (battles {}, of equal numbers {}, drawn {}); men of the stronger over the weaker q10 {:.2} / q25 {:.2} / q50 {:.2} / q75 {:.1} / q90 {:.0} — printed, the assertion comes with (д3)",
                self.battles.len(),
                share(stronger_won, self.battles.len()),
                self.battles.len() as u64 + self.battles_even,
                self.battles_even,
                self.battles_drawn,
                quantile(&odds, 0.1),
                quantile(&odds, 0.25),
                quantile(&odds, 0.5),
                quantile(&odds, 0.75),
                quantile(&odds, 0.9)
            ),
        ]
    }
}

/// One generated world with the bots, 300 ticks: what it adds to the measures.
fn measure_world(seed: u64, p: &Params) -> Measures {
    let (mut reigns, mut usurpations, mut polity_centuries) = (Vec::<f64>::new(), 0u64, 0.0f64);
    let (mut risings, mut large_ticks) = (0u64, 0u64);
    let (mut polities_start, mut polities_century) = (0usize, 0usize);
    let (mut war_ticks, mut polity_ticks, mut at_peace_ticks) = (0u64, 0u64, 0u64);
    let (mut army_at_war, mut people_at_war, mut army_at_peace, mut people_at_peace) = (0i64, 0i64, 0i64, 0i64);
    let mut a11_peace_windows: Vec<(i64, i64)> = vec![(0, 0); 6];
    let (mut bound_century, mut alive_century) = (0usize, 0usize);
    let (mut tax, mut production) = (0i64, 0i64);
    let (mut falls, mut falls_by_conquest, mut reached, mut dropped, mut verbs) = (0u64, 0u64, 0u64, 0u64, BTreeMap::<VerbId, u64>::new());
    let mut rejections: BTreeMap<String, u64> = BTreeMap::new();
    let mut per_polity_war_share: Vec<f64> = Vec::new();
    let mut rates: Vec<f64> = Vec::new();
    let (mut revolt_ticks, mut province_ticks, mut revolt_lengths, mut revolts_started, mut revolts_ended) = (0u64, 0u64, Vec::<u32>::new(), 0u64, 0u64);
    let mut revolts_per_province: BTreeMap<(u64, ProvinceId), u32> = BTreeMap::new();
    let (mut tax_changes, mut tax_undone) = (0u64, 0u64);
    let (mut declarations, mut declared_after_peace, mut declared_unpaid, mut war_lengths) = (0u64, 0u64, 0u64, Vec::<u32>::new());
    let mut claims = 0u64;
    let mut declared_within_five_years = 0u64;
    let mut opportunities: BTreeMap<String, u64> = BTreeMap::new();
    let (mut elections, mut elections_turned) = (0u64, 0u64);
    let mut kind_of: BTreeMap<u32, &'static str> = BTreeMap::new();
    let s = worldgen::generate(seed, p);
    let mut g = Game::new(&s, p.clone(), seed).unwrap();
    let people_of = |g: &Game| g.world().provinces.keys().map(|q| g.world().population(*q)).sum::<i64>();
    let (people_start, mut people_100, mut migrants, mut migrants_across) = (people_of(&g), 0i64, 0i64, 0i64);
    let (mut deficit_share, mut deficit_samples, mut deficit_heavy) = (0.0f64, 0u64, 0u64);
    let (mut pay, mut court) = (0i64, 0i64);
    // Conquests and those with no battle in the province since its siege could have begun
    // (owner's rule after #297: are the armies meeting?).
    let (mut conquests, mut conquests_unfought) = (0u64, 0u64);
    let mut last_battle: BTreeMap<ProvinceId, u32> = BTreeMap::new();
    let mut last_tax: BTreeMap<ActorId, (u32, bool)> = BTreeMap::new();
    let mut last_peace: BTreeMap<(ActorId, ActorId), u32> = BTreeMap::new();
    let mut war_since: BTreeMap<(ActorId, ActorId), u32> = BTreeMap::new();
    let n0 = g.world().actors.len();
    polities_start += n0;
    let mut since: BTreeMap<ActorId, u32> = g.world().actors.keys().map(|a| (*a, 0)).collect();
    let mut war_by_actor: BTreeMap<ActorId, (u64, u64)> = BTreeMap::new();
    // The base of (д1) (plan §30.10): the wars of principals by the events of the tick, in
    // their order; the wars of the starting facts are open from the first tick.
    let side_of = |w: &World, a: ActorId| Side { income: w.yearly_income(a, p), army: w.army_strength(a) };
    let mut open_wars: BTreeMap<(ActorId, ActorId), OpenWar> =
        g.world().wars.iter().filter(|(_, x)| x.of.is_none()).map(|(k, _)| (*k, OpenWar { since: 0, battles: 0, taken: (0, 0), start: (side_of(g.world(), k.0), side_of(g.world(), k.1)) })).collect();
    let (mut principal_wars, mut wars_within_a_tick) = (Vec::<EndedWar>::new(), 0u64);
    let (mut battles, mut battles_even, mut battles_drawn) = (Vec::<(u64, u64, bool)>::new(), 0u64, 0u64);
    for t in 0..300u32 {
        let w = g.world();
        let living: Vec<ActorId> = w.actors.keys().copied().filter(|a| w.alive(*a)).collect();
        let sides_before: BTreeMap<ActorId, Side> = living.iter().map(|a| (*a, side_of(w, *a))).collect();
        let obligations_before: BTreeMap<(ActorId, ActorId), (ActorId, ActorId)> = w.wars.iter().filter_map(|(k, x)| x.of.map(|principal| (*k, principal))).collect();
        let unpaid_seen: BTreeMap<ActorId, bool> = living.iter().map(|a| (*a, w.paid_share_seen_ppm(*a) < 1_000_000)).collect();
        for a in &living {
            let at_war = w.wars.keys().any(|(x, y)| x == a || y == a);
            let people: i64 = w.actors[a].provinces.iter().map(|q| w.population(*q)).sum();
            let army = w.army_size(*a);
            polity_ticks += 1;
            let e = war_by_actor.entry(*a).or_insert((0, 0));
            e.1 += 1;
            if at_war {
                war_ticks += 1;
                e.0 += 1;
                army_at_war += army;
                people_at_war += people;
            } else {
                at_peace_ticks += 1;
                army_at_peace += army;
                people_at_peace += people;
                a11_peace_windows[(t / 50) as usize].0 += army;
                a11_peace_windows[(t / 50) as usize].1 += people;
            }
            if w.actors[a].provinces.len() >= 5 {
                large_ticks += 1;
            }
            production += w.production_per_tick(*a, p);
        }
        let sizes: BTreeMap<ActorId, usize> = living.iter().map(|a| (*a, w.actors[a].provinces.len())).collect();
        province_ticks += w.provinces.len() as u64;
        revolt_ticks += w.provinces.values().filter(|pr| pr.revolt.is_some()).count() as u64;
        let started: BTreeMap<ProvinceId, u32> = w.provinces.iter().filter_map(|(q, pr)| pr.revolt.map(|t| (*q, t.0))).collect();
        let r = g.tick(&[]).unwrap();
        if t == 99 {
            people_100 = people_of(&g);
        }
        // Exhaustion of men (plan §27.1): the share of the ceiling of А1 the dead of the levies
        // hold, over the living polities.
        for a in g.world().actors.keys().filter(|a| g.world().alive(**a)) {
            let ceiling = g.world().recruit_ceiling(*a, p).max(1) as f64;
            let share = (g.world().actors[a].manpower_deficit / ceiling).min(1.0);
            deficit_share += share;
            deficit_samples += 1;
            deficit_heavy += u64::from(share > 0.5);
        }
        for e in &r.events {
            if let Event::Battle { province, .. } = e.event {
                last_battle.insert(province, t);
            }
        }
        for e in &r.events {
            if let Event::ProvinceChangedHands { province, how: engine2::event::Transfer::Conquest, .. } = e.event {
                conquests += 1;
                if !last_battle.get(&province).is_some_and(|b| t <= b + p.siege_capital_ticks) {
                    conquests_unfought += 1;
                }
            }
        }
        for e in &r.events {
            if let Event::Migration { from, to, people } = e.event {
                migrants += people as i64;
                if g.world().provinces[&from].owner != g.world().provinces[&to].owner {
                    migrants_across += people as i64;
                }
            }
        }
        if t == 199 {
            let w = g.world();
            // The polities living a century on: since the orders of step (б) a polity
            // can fall, and its record stays.
            polities_century += w.actors.keys().filter(|a| w.alive(**a)).count();
            alive_century += w.actors.keys().filter(|a| w.alive(**a)).count();
            bound_century += w.actors.keys().filter(|a| w.alive(**a) && (!w.allies_of(**a).is_empty() || w.lord_of(**a).is_some() || !w.vassals_of(**a).is_empty())).count();
        }
        let conquered_this_tick: Vec<ActorId> = r.events.iter().filter_map(|e| match e.event {
            Event::ProvinceChangedHands { from, how: engine2::event::Transfer::Conquest, .. } => Some(from),
            _ => None,
        }).collect();
        for e in &r.events {
            match &e.event {
                Event::RulerDied { actor, .. } => reigns.push(f64::from(t + 1 - since.get(actor).copied().unwrap_or(t + 1)) / 2.0),
                Event::Succession { actor, how, .. } => {
                    since.insert(*actor, t + 1);
                    if *how == engine2::Standing::Usurper && actor.index() < n0 {
                        usurpations += 1;
                    }
                }
                Event::ActorBorn { actor, from, .. } => {
                    since.insert(*actor, t + 1);
                    if sizes.get(from).copied().unwrap_or(0) >= 5 {
                        risings += 1;
                    }
                }
                Event::Revolt { from, province } => {
                    revolts_started += 1;
                    *revolts_per_province.entry((seed, *province)).or_insert(0) += 1;
                    if sizes.get(from).copied().unwrap_or(0) >= 5 {
                        risings += 1;
                    }
                }
                Event::RevoltEnded { province, .. } => {
                    revolts_ended += 1;
                    if let Some(s) = started.get(province) {
                        revolt_lengths.push(t + 1 - s);
                    }
                }
                Event::ActorDied { actor } => {
                    falls += 1;
                    if conquered_this_tick.contains(actor) {
                        falls_by_conquest += 1;
                    }
                }
                // A declaration claims a polity or one of its provinces (plan §28.9): the war
                // is on the province's owner — read before a conquest of the same tick moves it.
                Event::Verb { actor: engine2::ActorRef::Ruler(a), verb: VerbId::DeclareWar, target } => {
                    *verbs.entry(VerbId::DeclareWar).or_insert(0) += 1;
                    let b = match target {
                        engine2::Target::Actor(b) => Some(*b),
                        engine2::Target::Province(q) => Some(g.world().provinces[q].owner).filter(|b| b != a).or_else(|| r.events.iter().find_map(|e| match e.event { Event::ProvinceChangedHands { province, from, .. } if province == *q => Some(from), _ => None })),
                        _ => None,
                    };
                    let Some(b) = b else { continue };
                    // A claim in a war already fought (A′) opens no war: no `War` of the pair this tick.
                    if !r.events.iter().any(|e| matches!(e.event, Event::War { a: x, b: y } if (x, y) == ((*a).min(b), (*a).max(b)))) {
                        claims += 1;
                        continue;
                    }
                    declarations += 1;
                    if unpaid_seen.get(a).copied().unwrap_or(false) {
                        declared_unpaid += 1;
                    }
                    if last_peace.get(&((*a).min(b), (*a).max(b))).is_some_and(|t0| t0 + 1 == t) {
                        declared_after_peace += 1;
                    }
                    if last_peace.get(&((*a).min(b), (*a).max(b))).is_some_and(|t0| t <= t0 + 10) {
                        declared_within_five_years += 1;
                    }
                }
                Event::War { a, b } => {
                    war_since.insert(((*a).min(*b), (*a).max(*b)), t);
                }
                Event::Peace { a, b } => {
                    let k = ((*a).min(*b), (*a).max(*b));
                    last_peace.insert(k, t);
                    if let Some(t0) = war_since.remove(&k) {
                        war_lengths.push(t - t0);
                    }
                }
                Event::TaxRateChanged { actor, from_ppm, to_ppm } => {
                    tax_changes += 1;
                    let up = to_ppm > from_ppm;
                    if last_tax.get(actor).is_some_and(|(last, was_up)| last + 1 == t && *was_up != up) {
                        tax_undone += 1;
                    }
                    last_tax.insert(*actor, (t, up));
                }
                Event::OpportunityOpened { opportunity } => {
                    kind_of.insert(opportunity.id, opportunity.what.kind());
                    *opportunities.entry(format!("{} opened", opportunity.what.kind())).or_insert(0) += 1;
                }
                Event::Elected { interim, winner, .. } => {
                    elections += 1;
                    elections_turned += u64::from(interim != winner);
                }
                Event::OpportunityTaken { id, .. } => *opportunities.entry(format!("{} taken", kind_of.get(id).unwrap_or(&"?"))).or_insert(0) += 1,
                Event::OpportunityDeclined { id, .. } => *opportunities.entry(format!("{} declined", kind_of.get(id).unwrap_or(&"?"))).or_insert(0) += 1,
                Event::OpportunityVoid { id, .. } => *opportunities.entry(format!("{} void", kind_of.get(id).unwrap_or(&"?"))).or_insert(0) += 1,
                Event::OpportunityLapsed { id, .. } => *opportunities.entry(format!("{} lapsed", kind_of.get(id).unwrap_or(&"?"))).or_insert(0) += 1,
                Event::GoalReached { .. } => reached += 1,
                Event::GoalDropped { .. } => dropped += 1,
                Event::Verb { verb, .. } => *verbs.entry(*verb).or_insert(0) += 1,
                _ => {}
            }
        }
        // The wars of principals and the battles (plan §30.10), in the order of the events.
        {
            let after = g.world();
            let pair = |a: ActorId, b: ActorId| (a.min(b), a.max(b));
            let side = |a: ActorId| sides_before.get(&a).copied().unwrap_or_else(|| side_of(after, a));
            for e in &r.events {
                match e.event {
                    Event::War { a, b } => match after.wars.get(&pair(a, b)) {
                        Some(x) if x.of.is_none() => {
                            let k = pair(a, b);
                            open_wars.insert(k, OpenWar { since: t, battles: 0, taken: (0, 0), start: (side(k.0), side(k.1)) });
                        }
                        Some(_) => {}
                        None => wars_within_a_tick += 1,
                    },
                    Event::Battle { attacker, defender, outcome, soldiers, .. } => {
                        // A battle of a war of obligation is its principals' war's.
                        let k = pair(attacker, defender);
                        let principal = after.wars.get(&k).and_then(|x| x.of).or_else(|| obligations_before.get(&k).copied()).unwrap_or(k);
                        if let Some(x) = open_wars.get_mut(&principal) {
                            x.battles += 1;
                        }
                        battles_drawn += u64::from(outcome == engine2::event::Outcome::Drawn);
                        match soldiers.0.cmp(&soldiers.1) {
                            std::cmp::Ordering::Greater => battles.push((soldiers.0, soldiers.1, outcome == engine2::event::Outcome::AttackerWon)),
                            std::cmp::Ordering::Less => battles.push((soldiers.1, soldiers.0, outcome == engine2::event::Outcome::DefenderWon)),
                            std::cmp::Ordering::Equal => battles_even += 1,
                        }
                    }
                    Event::ProvinceChangedHands { from, to, how: engine2::event::Transfer::Conquest, .. } => {
                        if let Some(x) = open_wars.get_mut(&pair(from, to)) {
                            if to < from {
                                x.taken.0 += 1;
                            } else {
                                x.taken.1 += 1;
                            }
                        }
                    }
                    Event::Peace { a, b } => {
                        let k = pair(a, b);
                        if let Some(x) = open_wars.remove(&k) {
                            // The peace's own record of the loser, if it found one.
                            let defeat = r.events.iter().find_map(|e| match e.event {
                                Event::Defeat { actor, by, provinces } if pair(actor, by) == k => Some((by, provinces)),
                                _ => None,
                            });
                            assert_eq!(x.taken.0.abs_diff(x.taken.1), defeat.map_or(0, |(_, n)| n), "the test's count of the war's conquests parts with the peace's record");
                            let won = defeat.map(|(winner, ceded)| {
                                let (winner, loser) = if winner == k.0 { x.start } else { (x.start.1, x.start.0) };
                                Won { winner, loser, ceded: Some(ceded), fell_to_another: false }
                            });
                            principal_wars.push(EndedWar { length: t - x.since, battles: x.battles, won });
                        }
                    }
                    // A fall closes every war of the fallen (`army::fall`): the survivor won.
                    Event::ActorDied { actor } => {
                        let closed: Vec<(ActorId, ActorId)> = open_wars.keys().copied().filter(|k| k.0 == actor || k.1 == actor).collect();
                        for k in closed {
                            let x = open_wars.remove(&k).unwrap();
                            let (winner, loser, taken_by_survivor) = if actor == k.1 { (x.start.0, x.start.1, x.taken.0) } else { (x.start.1, x.start.0, x.taken.1) };
                            principal_wars.push(EndedWar { length: t - x.since, battles: x.battles, won: Some(Won { winner, loser, ceded: None, fell_to_another: taken_by_survivor == 0 }) });
                        }
                    }
                    _ => {}
                }
            }
        }
        tax += r.postings.iter().filter(|q| matches!(q.account, engine2::Account::Treasury(_)) && q.source == Source::Tax).map(|q| q.milli / 1000).sum::<i64>();
        pay -= r.postings.iter().filter(|q| matches!(q.account, engine2::Account::Treasury(_)) && q.source == Source::Upkeep).map(|q| q.milli / 1000).sum::<i64>();
        court -= r.postings.iter().filter(|q| matches!(q.account, engine2::Account::Treasury(_)) && q.source == Source::Court).map(|q| q.milli / 1000).sum::<i64>();
    }
    // The instrument checks itself against the world: a war of principals the test lost the
    // end of — or never saw open — would part the count «open at the run's end» from the
    // world's in silence ((д1) and (д2) change how wars open and end).
    assert_eq!(open_wars.keys().copied().collect::<Vec<_>>(), g.world().wars.iter().filter(|(_, x)| x.of.is_none()).map(|(k, _)| *k).collect::<Vec<_>>(), "seed {seed}: the open wars of the test part with the world's");
    polity_centuries += n0 as f64 * 1.5;
    let people_end = people_of(&g);
    let w = g.world();
    rates.extend(w.actors.iter().filter(|(a, _)| w.alive(**a)).map(|(_, x)| x.tax_rate_ppm as f64 / 1e6));
    for (k, v) in g.preempted_reasons() {
        *rejections.entry(k.clone()).or_insert(0) += v;
    }
    for (_, (w, n)) in war_by_actor {
        if n >= 100 {
            per_polity_war_share.push(w as f64 / n as f64);
        }
    }
    Measures { reigns, usurpations, polity_centuries, risings, large_ticks, polities_start, polities_century, war_ticks, polity_ticks, at_peace_ticks, army_at_war, people_at_war, army_at_peace, people_at_peace, a11_peace_windows, bound_century, alive_century, tax, production, falls, falls_by_conquest, reached, dropped, verbs, rejections, per_polity_war_share, rates, revolt_ticks, province_ticks, revolt_lengths, revolts_started, revolts_ended, revolts_per_province, tax_changes, tax_undone, declarations, claims, declared_after_peace, declared_unpaid, war_lengths, declared_within_five_years, opportunities, elections, elections_turned, people_start, people_100, people_end, migrants, migrants_across, deficit_share, deficit_samples, deficit_heavy, pay, court, conquests, conquests_unfought, principal_wars, principal_wars_open: open_wars.len() as u64, wars_within_a_tick, battles, battles_even, battles_drawn }
}

/// The measures of E2 read again with every polity a bot (owner's rule on #291): В5, В6,
/// В7, В8, А11, А12, Д3, Э8; no polity falls but by conquest; goals are reached and put aside.
#[test]
fn on_generated_worlds_with_bots_the_measures_of_e2_hold_and_nothing_falls_but_by_conquest() {
    let p = params();
    let band = |k: &str| p.entries[k].band.unwrap();
    let seeds: Vec<u64> = (1..=20).collect();
    let m = Measures::over(&seeds, &p);
    let Rates { b5, b6, b7, b8, a11_war, a11_peace, a12, d3, e8, b9, p7 } = m.rates();
    let war_lines = m.war_lines();
    let Measures { reigns, usurpations: _, polity_centuries: _, risings: _, large_ticks: _, polities_start: _, polities_century: _, war_ticks: _, polity_ticks: _, at_peace_ticks, army_at_war: _, people_at_war: _, army_at_peace: _, people_at_peace: _, a11_peace_windows, bound_century: _, alive_century: _, tax: _, production: _, falls, falls_by_conquest, reached, dropped, verbs, rejections, per_polity_war_share, rates, revolt_ticks: _, province_ticks: _, mut revolt_lengths, revolts_started, revolts_ended, revolts_per_province, tax_changes, tax_undone, declarations, claims, declared_after_peace, declared_unpaid, mut war_lengths, declared_within_five_years, opportunities, elections, elections_turned, people_start, people_100: _, people_end, migrants, migrants_across, deficit_share, deficit_samples, deficit_heavy, pay, court, conquests, conquests_unfought, principal_wars: _, principal_wars_open: _, wars_within_a_tick: _, battles: _, battles_even: _, battles_drawn: _ } = m;
    let never = per_polity_war_share.iter().filter(|s| **s < 0.1).count();
    let always = per_polity_war_share.iter().filter(|s| **s > 0.9).count();
    eprintln!(
        "with bots, 20 worlds × 300 ticks: В5 {b5:.1} years ({} reigns); В6 {b6:.1} per century per polity; В7 {b7:.2}; В8 {b8:.2}; А11 at war {a11_war:.4}, at peace {a11_peace:.4} ({at_peace_ticks} peace polity-ticks); А12 share of polity-ticks at war {a12:.3} — polities under a tenth {never}, over nine tenths {always} of {}; Д3 at a century {d3:.2}; Э8 {e8:.3}; falls {falls} of which by conquest {falls_by_conquest}; goals reached {reached}, put aside {dropped}; verbs {verbs:?}; calls preempted by the tick's conflicts {rejections:?}; tax rates at the end — mean {:.3}, below the tithe {}, at it {}, above {}",
        reigns.len(),
        per_polity_war_share.len(),
        rates.iter().sum::<f64>() / rates.len().max(1) as f64,
        rates.iter().filter(|r| **r < 0.0999).count(),
        rates.iter().filter(|r| (0.0999..=0.1001).contains(*r)).count(),
        rates.iter().filter(|r| **r > 0.1001).count()
    );
    // А11 at peace over time (E3 (г), owner's answer 1 on §29.6): the swing «more armies →
    // fewer wars → a lower rate of war read → fewer armies» would show as a wave here.
    eprintln!("А11 at peace by windows of 50 ticks: {:?}", a11_peace_windows.iter().map(|(a, n)| format!("{:.4}", *a as f64 / (*n).max(1) as f64)).collect::<Vec<_>>());
    // The tax does not swing (owner's rule on #292): a step undone the next tick costs the
    // ruler half a step's price in legitimacy for nothing — the bots that read the pay of
    // one tick raised and lowered by turns, 28 % of their changes, and usurped themselves.
    let undone = tax_undone as f64 / tax_changes.max(1) as f64;
    eprintln!("tax changes {tax_changes}, undone the next tick {tax_undone} ({undone:.3})");
    // No war with an unpaid army (E3 (в), the class of the tax's swing): by the rule of the
    // peace an unpaid declarer is exhausted and the war ends at the first peace offered —
    // the bots declared four fifths of their wars the tick after a peace of the same pair,
    // most of them already short of their pay. And no revanche the tick after the peace
    // (owner's rule on #293: a fresh treaty costs more to break, a lost war costs
    // legitimacy): under a tenth of the declarations (plan §23l).
    let renewed = declared_after_peace as f64 / declarations.max(1) as f64;
    war_lengths.sort();
    eprintln!(
        "war declarations {declarations}, claims in a war declared on the claimant {claims}, by an unpaid army {declared_unpaid}, the tick after a peace of the same pair {declared_after_peace} ({renewed:.3}), within five years of it {declared_within_five_years} ({:.3}); wars ended {}, length median {} ticks, of one tick {}",
        declared_within_five_years as f64 / declarations.max(1) as f64,
        war_lengths.len(),
        war_lengths.get(war_lengths.len() / 2).copied().unwrap_or(0),
        war_lengths.iter().filter(|l| **l <= 1).count()
    );
    eprintln!(
        "exhaustion of men: the dead of the levies hold {:.3} of the ceiling on average, more than half of it in {:.3} of polity-ticks; provinces conquered {conquests}, without a battle {conquests_unfought} ({:.3})",
        deficit_share / deficit_samples.max(1) as f64,
        deficit_heavy as f64 / deficit_samples.max(1) as f64,
        conquests_unfought as f64 / conquests.max(1) as f64
    );
    // Э9 with the bots — asserted since (ц1) (owner's decision 2026-11-12, plan §31.10): with
    // the price of money the bots spend what they take, and the army's share of spending
    // stands inside the source's band. Before it the measure was printed, a warning only
    // (owner's answer 3 on §27.5); `army.campaign_pay_factor` is never fitted to it.
    let band_e9 = band("budget.observed_military_share");
    let e9 = pay as f64 / (pay + court).max(1) as f64;
    eprintln!("Э9 with bots: the army's share of spending {e9:.3} (band {band_e9:?})");
    assert!(e9 >= band_e9[0] && e9 <= band_e9[1], "Э9 with bots {e9:.3} outside {band_e9:?}");
    assert_eq!(declared_unpaid, 0, "a bot declared war with an unpaid army");
    assert!(renewed < 0.10, "the loser's revanche: {declared_after_peace} of {declarations} declarations the tick after a peace of the same pair");
    eprintln!("opportunities {opportunities:?}; elections {elections}, the throne to another than the interim {elections_turned}");
    // Every chance of the channel happens on the worlds, not only in the tests (owner's
    // rule on #293: an unreachable condition is a defect).
    for kind in ["Secession", "Usurpation", "Vote", "Union", "Policy", "Marriage"] {
        assert!(opportunities.get(&format!("{kind} taken")).copied().unwrap_or(0) > 0, "no {kind} taken on 20 worlds: {opportunities:?}");
    }
    assert!(undone < 0.05, "the bots' tax swings: {tax_undone} of {tax_changes} changes undone the next tick");
    // The tax as a flow (owner's rule on #291): no anchor. Under the weights of #291 the
    // bots kept near the tithe; in the world's wheat (E3 (е1)) they keep where its prices put
    // them — two to three steps above, collecting a share inside Э8 (asserted below) — and
    // never at the top of Э4, where the bottom of legitimacy once drove the usurpers (#292).
    let mean_rate = rates.iter().sum::<f64>() / rates.len().max(1) as f64;
    assert!(mean_rate * 1e6 + p.tax_step_ppm as f64 <= p.tax_band_ppm.1 as f64, "the bots' mean tax {mean_rate:.3} at the top of Э4");
    assert!(rejections.keys().all(|k| is_conflict(k)), "a call preempted for another reason than a conflict of the tick: {rejections:?}");
    // The refusals for a treasury that cannot pay: a race of the tick by the one case read
    // (`CONFLICTS`), not told by its line from a bot calling what it cannot pay — printed.
    eprintln!("calls refused at application for a treasury that cannot pay: {} of {} preempted", rejections.iter().filter(|(k, _)| k.ends_with(CANNOT_PAY)).map(|(_, n)| n).sum::<u64>(), rejections.values().sum::<u64>());
    assert_eq!(falls, falls_by_conquest, "a polity falls only by conquest: every fall in the tick its last province was taken");
    assert!(reached > 0 && dropped > 0, "goals are reached and put aside");
    assert!(verbs.len() >= 6, "the bots use the verbs: {verbs:?}");
    // With the orders of step (б) a war has its means: А12 is a check, not print (owner's
    // rule on #291) — but **its bottom is a warning, not an assertion** (owner's decision
    // 2026-11-13, plan §31.14, §31.17.5): the edge is written in E0 by memory, and a band by
    // memory is print, not a stop (rule 1 of §24.1). The top and «always at war» below stay
    // assertions.
    let a12_band = band("war.observed_share_of_years");
    if a12 < a12_band[0] {
        eprintln!("А12 {a12:.3} below the bottom of {a12_band:?} — printed: the bottom is by memory (plan §31.14)");
    }
    assert!(a12 <= a12_band[1], "А12 {a12:.3} above the top of {a12_band:?}");
    assert!(always * 10 < per_polity_war_share.len(), "«always at war»: {always} of {} polities over nine tenths of their ticks", per_polity_war_share.len());
    assert!(never * 2 < per_polity_war_share.len(), "«never at war»: {never} of {} polities under a tenth of their ticks", per_polity_war_share.len());
    // Д3's band is by memory (rule 1 of §24.1: a band without a source is a warning, not a
    // check — owner's decisions 2026-10-20 on §28.12 and 2 on §29.10). Printed: a known
    // deviation until (г2), the worth of an alliance read by the table of §28.10 — it follows
    // А12 (more wars, more bonds; §28.12, §29.9), 0,87 with the army at peace of (г1).
    let d3_band = band("diplomacy.observed_bound_share");
    eprintln!("Д3 at a century {d3:.2} (band by memory {d3_band:?}) — printed: a known deviation until (г2), plan §29.11");
    revolt_lengths.sort();
    let repeated = revolts_per_province.values().filter(|n| **n > 1).count();
    eprintln!(
        "В9 share of province-ticks in revolt {b9:.4}; revolts started {revolts_started}, put down {revolts_ended}, length median {} ticks, mean {:.1}, longest {}; provinces that rose again {repeated} of {}",
        revolt_lengths.get(revolt_lengths.len() / 2).copied().unwrap_or(0),
        revolt_lengths.iter().sum::<u32>() as f64 / revolt_lengths.len().max(1) as f64,
        revolt_lengths.last().copied().unwrap_or(0),
        revolts_per_province.len()
    );
    // В9's band is by memory too (rule 1; owner's decision 3 on §29.10): printed until a
    // source is found. With the army at peace of (г1) a rising is put down in two ticks (the
    // median; up to 192 before), and the share fell under the remembered band.
    let b9_band = band("revolt.observed_share_of_provinces");
    eprintln!("В9 share of province-ticks in revolt {b9:.4} (band by memory {b9_band:?}) — printed: a source to be found, plan §29.11");
    assert!(revolts_ended * 2 > revolts_started, "the bots put down most revolts: {revolts_ended} of {revolts_started}");
    let b5_band = band("reign.observed_years");
    assert!((b5_band[0]..=b5_band[1]).contains(&b5), "В5 {b5:.1} outside {b5_band:?}");
    let b6_band = band("usurpation.observed_per_century");
    assert!((b6_band[0]..=b6_band[1]).contains(&b6), "В6 {b6:.1} outside {b6_band:?}");
    let b8_band = band("collapse.observed_polities_after_a_century");
    assert!((b8_band[0]..=b8_band[1]).contains(&b8), "В8 {b8:.2} outside {b8_band:?}");
    let e8_band = band("tax.observed_share_of_production");
    assert!((e8_band[0]..=e8_band[1]).contains(&e8), "Э8 {e8:.3} outside {e8_band:?}");
    // П7 with the bots (owner's rule after #295: the flight from the tax is checked through
    // П7 and Э8): the growth of the worlds' people over the first 100 ticks, and the people
    // who moved — across a border, the flight from the heavier tax.
    eprintln!(
        "П7 {p7:.4} a year over 100 ticks; people {people_start} → {people_end} over 300 ticks; migrants {migrants}, of them across a border {migrants_across} ({:.3})",
        migrants_across as f64 / migrants.max(1) as f64
    );
    let p7_band = band("population.observed_growth_per_year");
    assert!((p7_band[0]..=p7_band[1]).contains(&p7), "П7 {p7:.4} outside {p7_band:?}");
    // А11 at peace has a source (Venice about 1 %, Milan about 1,7 % — Mallett–Hale; the band
    // from 0,5 %), so under it is a deviation, not a warning (owner's decision 4 on §29.10).
    // It stood under the band until the price of money ((ц1), plan §31): the bots hoarded what
    // they collected and priced every kilogram of pay at its face value. Not to be tuned
    // (rule 1 of §24.1).
    let a11_band = band("army.observed_share_of_population");
    let settled = a11_peace_windows.iter().skip(1).fold((0i64, 0i64), |acc, (army, people)| (acc.0 + army, acc.1 + people));
    let a11_settled = settled.0 as f64 / settled.1.max(1) as f64;
    eprintln!("А11 at peace {a11_peace:.4}, after the first 50 ticks {a11_settled:.4} (the file's band {a11_band:?})");
    // With the price of money (E3 (ц1), plan §31.3, §31.10) the army at peace is inside the
    // band of the source: the band is the assertion again, and the floor against the
    // regression of the known deviation is gone (plan §29.7 p. 4; owner's decisions 5 and 6 on
    // §31.8 — the file's band judges). The level after the first fifty ticks — the plateau,
    // which the mean hid behind the starting armies — is held to the same band.
    assert!((a11_band[0]..=a11_band[1]).contains(&a11_peace), "А11 at peace {a11_peace:.4} outside {a11_band:?}");
    assert!((a11_band[0]..=a11_band[1]).contains(&a11_settled), "А11 at peace after the first 50 ticks {a11_settled:.4} outside {a11_band:?}");
    let b7_band = band("revolt.observed_per_century");
    assert!((b7_band[0]..=b7_band[1]).contains(&b7), "В7 {b7:.2} outside {b7_band:?}");
    // In war 2–3 % of the people under arms (E0 А11, Jones, Mallett–Hale — the source's band;
    // owner's rule 2026-10-17, plan §28.9). Printed, not asserted (owner's decision 2026-10-20,
    // plan §28.12): a known deviation until (д) — the world's war is decided in three ticks
    // (the median) and costs one province, so a second levy has nothing to pay for; the army at
    // war is one tick's levy, А2 × А1 ≈ 0,75 % of the people. The limit of the model of war,
    // not an approximation of the bot — not to be tuned.
    let a11_war_band = band("army.observed_share_at_war");
    eprintln!("А11 at war {a11_war:.4} (E0: {a11_war_band:?} in war) — printed: a known deviation until (д), plan §28.12");
    // A floor against a regression of the known deviation, not a band (owner's rule
    // 2026-10-20): a bot that stops levying at war would fall below it. It stood at 0,007 —
    // the measure was 0,0080 on #305 (0,0078–0,0095 over the stops of (3), plan
    // §28.9–28.13) — and is raised to the level after (ц1б) (owner's decision 4 on §31.11,
    // 2026-11-12): the measure on these seeds in the state merged, 0,0178, rounded down to
    // the thousandth. It guards what was reached; the band of the source [0,02; 0,03] stays a
    // known deviation until the war chest (after (ц3)).
    assert!(a11_war >= 0.017, "А11 at war {a11_war:.4} below the regression floor 0,017 (plan §31.11)");
    // The base of (д1) on main (plan §30.9 p. 4, §30.10): the wars of principals and А13 on
    // the bots' battles — printed, no bands (rule 1 of §24.1: no source gives one).
    for line in war_lines {
        eprintln!("{line}");
    }
}

#[test]
fn the_outcome_depends_on_the_strategy_and_on_the_seed_and_a_run_with_bots_saves_bit_for_bit() {
    let p = params();
    let (mut world_differs, mut differs_by_strategy, mut differs_by_seed) = (0usize, 0usize, 0usize);
    // Every world on its own thread (`engine2::par`), counted in the order of the seeds.
    let seeds: Vec<u64> = (1..=10).collect();
    for (world, strategy, by_seed) in engine2::par::map(&seeds, |seed| {
        let seed = *seed;
        let s = worldgen::generate(seed, &p);
        let run = |player: Option<ActorRef>, game_seed: u64| {
            let mut g = Game::new(&s, params(), game_seed).unwrap();
            g.set_player(player);
            for _ in 0..200 {
                g.tick(&[]).unwrap();
            }
            (g.observe(), g.world().relations.clone(), g.world().treaties.clone())
        };
        // The bot in the seat against a player who sits on its hands.
        let bots = run(None, seed);
        let idle = run(Some(ActorRef::Ruler(ActorId(0))), seed);
        (bots.0 != idle.0 || bots.1 != idle.1 || bots.2 != idle.2, bots.0.outcome() != idle.0.outcome(), bots.0.outcome() != run(None, seed + 100).0.outcome())
    }) {
        world_differs += usize::from(world);
        differs_by_strategy += usize::from(strategy);
        differs_by_seed += usize::from(by_seed);
    }
    eprintln!("the bot in the seat against an idle player: the world differs in {world_differs} of 10, the outcome (who lives, who owns what) in {differs_by_strategy} of 10; the outcome differs by seed in {differs_by_seed} of 10");
    // The bot's choices move the world everywhere; the outcome of brief §1 — who lives, who
    // owns what — moves with them only once a war can take land, with the orders of step
    // (б): there the brief's acceptance «the outcome depends on the strategy» is asserted.
    assert_eq!(world_differs, 10, "a player who does nothing leaves the world as the bot would: the bot's choices do not matter");
    assert!(differs_by_seed >= 5, "the outcome barely depends on the seed");
    // Through a save, bit for bit, bots and goals included.
    let s = worldgen::generate(5, &p);
    let mut straight = Game::new(&s, params(), 5).unwrap();
    let mut a = Vec::new();
    for _ in 0..60 {
        a.push(straight.tick(&[]).unwrap());
    }
    let mut g = Game::new(&s, params(), 5).unwrap();
    let mut b = Vec::new();
    for _ in 0..30 {
        b.push(g.tick(&[]).unwrap());
    }
    let save = g.to_save().unwrap();
    assert!(save.contains("\"goals\":[") && save.contains("\"regions\":"));
    let mut g2 = Game::from_save(&s, params(), &save).unwrap();
    for _ in 0..30 {
        b.push(g2.tick(&[]).unwrap());
    }
    assert_eq!(a, b);
    assert_eq!(straight.to_save().unwrap(), g2.to_save().unwrap());
    assert!(a.iter().any(|r| r.events.iter().any(|e| matches!(e.event, Event::Verb { .. }))), "the run had bots' calls to carry");
    let _ = (Target::None, VerbCall { actor: ActorRef::Ruler(ActorId(0)), verb: VerbId::Recruit, target: Target::None });
}


#[test]
fn every_call_of_a_tick_is_applied_in_one_order_for_all_the_player_at_the_place_of_its_actor() {
    let p = params();
    let mut s = all_verbs(testworld::five_states_scenario());
    // One unmarried adult in house h4 of polity 4 — the bride both polity 1 and polity 3
    // ask for: c4 is married into the other house of polity 0, r4 is left.
    s.persons.get_mut("c4").unwrap().spouse = Some("o0a".to_string());
    s.persons.get_mut("o0a").unwrap().spouse = Some("c4".to_string());
    let house = |sid: &str| engine2::HouseId(s.houses.keys().position(|k| k == sid).unwrap() as u32);
    let (h1, h3, h4) = (house("h1"), house("h3"), house("h4"));
    // Across polities a marriage needs the relation at the threshold.
    let friendly = |g: &mut Game| {
        for a in [1u32, 3] {
            g.world_mut().relations.insert((ActorId(a), ActorId(4)), 80.0);
        }
    };
    let mut g = Game::new(&s, params(), 51).unwrap();
    g.set_bots(false);
    friendly(&mut g);
    let marry = |a: u32, g: engine2::HouseId| VerbCall { actor: ActorRef::Ruler(ActorId(a)), verb: VerbId::ProposeMarriage, target: Target::House(g) };
    // The order: by the caller's polity, whatever order the calls came in.
    let order = engine2::sim::application_order(g.world(), &[marry(3, h4), marry(1, h4), VerbCall { actor: ActorRef::House(h3), verb: VerbId::ProposeMarriage, target: Target::House(h4) }]);
    assert_eq!(order.iter().map(|c| c.actor).collect::<Vec<_>>(), vec![ActorRef::Ruler(ActorId(1)), ActorRef::Ruler(ActorId(3)), ActorRef::House(h3)]);
    // Played: both offers go to house 4 in that order (E3 (в): a marriage is the other
    // house's answer); at the next turn polity 1's is taken first and marries, polity 3's is
    // void — the bride is married — not an error.
    let opened = g.tick(&[marry(3, h4), marry(1, h4)]).unwrap();
    let offers: Vec<(u32, engine2::HouseId)> = opened.events.iter().filter_map(|e| match &e.event {
        Event::OpportunityOpened { opportunity } => match opportunity.what {
            engine2::Chance::Marriage { from, .. } => Some((opportunity.id, from)),
            _ => None,
        },
        _ => None,
    }).collect();
    assert_eq!(offers.iter().map(|(_, h)| *h).collect::<Vec<_>>(), vec![h1, h3], "offered in the one order");
    let r = g.tick(&[]).unwrap();
    let married = r.events.iter().find_map(|e| match e.event {
        Event::Marriage { a, b, .. } => Some((a, b)),
        _ => None,
    });
    let (x, y) = married.expect("one marriage");
    let houses_married = [g.world().persons[&x].house, g.world().persons[&y].house];
    assert!(houses_married.contains(&h1) && houses_married.contains(&h4), "the lower polity married: {houses_married:?}");
    assert!(r.events.iter().any(|e| matches!(e.event, Event::OpportunityVoid { id, .. } if id == offers[1].0)), "polity 3's offer is void");
    assert_eq!(g.preempted(), 0);
    // The player at the place of its actor: a player playing polity 3 against a bot of
    // polity 1 loses the bride the same way; playing polity 0 it would win — the queue is
    // the actors', not the player's.
    let mut g = Game::new(&s, params(), 51).unwrap();
    g.set_bots(false);
    friendly(&mut g);
    g.set_player(Some(ActorRef::Ruler(ActorId(3))));
    g.tick(&[marry(3, h4), marry(1, h4)]).unwrap();
    let r = g.tick(&[]).unwrap();
    let (x, y) = r.events.iter().find_map(|e| match e.event {
        Event::Marriage { a, b, .. } => Some((a, b)),
        _ => None,
    }).expect("one marriage");
    assert!([g.world().persons[&x].house, g.world().persons[&y].house].contains(&h1), "the player's call came first in the list and still goes after polity 1");
    let _ = p;
}

#[test]
fn a_peace_or_a_war_both_sides_made_in_one_tick_is_made_once_and_a_players_call_not_on_offer_touches_nothing() {
    let p = params();
    let s = all_verbs(testworld::five_states_scenario());
    let mut g = Game::new(&s, params(), 52).unwrap();
    g.set_bots(false);
    let call = |a: u32, verb: VerbId, b: u32| VerbCall { actor: ActorRef::Ruler(ActorId(a)), verb, target: Target::Actor(ActorId(b)) };
    let r = g.tick(&[call(0, VerbId::DeclareWar, 1), call(1, VerbId::DeclareWar, 0)]).unwrap();
    assert_eq!(r.events.iter().filter(|e| matches!(e.event, Event::War { .. })).count(), 1, "one war");
    assert_eq!(r.events.iter().filter(|e| matches!(e.event, Event::Verb { verb: VerbId::DeclareWar, .. })).count(), 1, "declared once: the second is nothing");
    let l1 = g.world().actors[&ActorId(1)].legitimacy;
    assert!(l1 > p.legitimacy_norm_hereditary - 1.0 || g.world().actors[&ActorId(1)].standing != engine2::Standing::Hereditary, "the second side paid no price for a war already made");
    let r = g.tick(&[call(1, VerbId::MakePeace, 0), call(0, VerbId::MakePeace, 1)]).unwrap();
    assert_eq!(r.events.iter().filter(|e| matches!(e.event, Event::Peace { .. })).count(), 1, "one peace");
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::Preempted { .. })), "the second proposal is nothing, not a refusal");
    assert_eq!(g.preempted(), 0, "the parity is not cluttered");
    // A player's call not on offer: an error, and nothing touched — no decision drawn, no
    // call applied, the tick not played.
    let mut g = Game::new(&s, params(), 53).unwrap();
    g.set_player(Some(ActorRef::Ruler(ActorId(0))));
    let before = g.to_save().unwrap();
    let bad = [call(0, VerbId::MakePeace, 1)];
    assert!(g.tick(&bad).is_err(), "peace with a polity not at war is not on offer");
    assert_eq!(g.to_save().unwrap(), before, "nothing touched");
}

/// E3 (е1): with no weight on legitimacy the equilibrium is where the world's prices put it,
/// not the tithe — the take a step adds against the collection it loses and the usurpations,
/// murders and risings the shifted norm invites. Under the weights of #291 it was the tithe;
/// the world's own prices hold the bot a few steps above it, below the top of Э4 (the
/// collected share, Э8, stays in its band: `on_generated_worlds_…`). The one-off prices of a
/// step (a raise costs, a lowering gives half back) leave a narrow band where neither step
/// pays: the bot climbs to its lower edge from below and comes down to its upper edge from
/// above.
#[test]
fn the_tax_is_a_lasting_decision_valued_as_a_flow_and_the_bot_comes_back_to_its_equilibrium_by_itself() {
    let p = params();
    let mut w = testworld::world(54);
    let a = ActorId(0);
    let u = |w: &World, v: VerbId| w.tax_utility(a, v, &p);
    let set = |w: &mut World, rate: i64| {
        w.actors.get_mut(&a).unwrap().tax_rate_ppm = rate;
        let norm = w.legitimacy_norm(a, &p);
        w.actors.get_mut(&a).unwrap().legitimacy = norm;
    };
    // The band: from the bottom of Э4 up while a step up pays, from the top down while a
    // step down pays — the rating at its norm.
    // On the grid of the steps from the tithe, where every polity starts.
    let mut low = p.tax_default_ppm - (p.tax_default_ppm - p.tax_band_ppm.0) / p.tax_step_ppm * p.tax_step_ppm;
    set(&mut w, low);
    while low + p.tax_step_ppm <= p.tax_band_ppm.1 && u(&w, VerbId::RaiseTax) > 0.0 {
        low += p.tax_step_ppm;
        set(&mut w, low);
    }
    let mut high = p.tax_default_ppm + (p.tax_band_ppm.1 - p.tax_default_ppm) / p.tax_step_ppm * p.tax_step_ppm;
    set(&mut w, high);
    while high - p.tax_step_ppm >= p.tax_band_ppm.0 && u(&w, VerbId::LowerTax) > 0.0 {
        high -= p.tax_step_ppm;
        set(&mut w, high);
    }
    eprintln!("the band where neither step pays on the test world: {:.2}–{:.2} (the tithe {:.2})", low as f64 / 1e6, high as f64 / 1e6, p.tax_default_ppm as f64 / 1e6);
    assert!(low <= high && high - low <= 2 * p.tax_step_ppm, "one band, narrow");
    assert!(high + p.tax_step_ppm <= p.tax_band_ppm.1, "the world's prices hold the bot below the top of Э4");
    // Inside it both steps cost; a step below it raising pays, a step above it lowering pays.
    for rate in (low..=high).step_by(p.tax_step_ppm as usize) {
        set(&mut w, rate);
        assert!(u(&w, VerbId::RaiseTax) < 0.0 && u(&w, VerbId::LowerTax) < 0.0, "inside the band: raise {:.2}, lower {:.2}", u(&w, VerbId::RaiseTax), u(&w, VerbId::LowerTax));
    }
    set(&mut w, low - p.tax_step_ppm);
    assert!(u(&w, VerbId::RaiseTax) > 0.0, "below it the step up pays: {:.2}", u(&w, VerbId::RaiseTax));
    set(&mut w, high + p.tax_step_ppm);
    assert!(u(&w, VerbId::LowerTax) > 0.0, "above it the step down pays: {:.2}", u(&w, VerbId::LowerTax));
    let rate = low;
    // A step down and back up cancel in the take: the log.
    let years = f64::from(p.goal_horizon_ticks) / 2.0;
    assert!(years > 1.0);
    // At war with the pay short the take weighs more: at the equilibrium a third of the pay
    // met makes the step up worth it.
    set(&mut w, rate);
    w.actors.get_mut(&a).unwrap().paid_share_ppm = 330_000;
    assert!(u(&w, VerbId::RaiseTax) > 0.0, "a third of the pay met: {:.2}", u(&w, VerbId::RaiseTax));
    // The option set of a bot reads the same function.
    w.actors.get_mut(&a).unwrap().paid_share_ppm = 1_000_000;
    let call = VerbCall { actor: ActorRef::Ruler(a), verb: VerbId::RaiseTax, target: Target::None };
    assert!((w.utility(a, &call, &p) - u(&w, VerbId::RaiseTax)).abs() < 1e-12);
}

// ---------- step (б): orders, suppression, the peace by its declarer ----------

use engine2::Order;

/// The test world with a second adult in the other house of polity 0, the two of them
/// governing p05 and p10: a house to carry a secession.
fn carrier_scenario() -> ScenarioData {
    use engine2::scenario::PersonData;
    let mut s = all_verbs(testworld::five_states_scenario());
    s.persons.insert("o0b".to_string(), PersonData { name: "Other 0 second".into(), house: "o0".into(), born: 1445, parent: None, spouse: None });
    s.provinces.get_mut("p05").unwrap().governor = Some("o0a".to_string());
    s.provinces.get_mut("p10").unwrap().governor = Some("o0b".to_string());
    s
}

#[test]
fn an_order_is_a_channel_of_the_contract_a_ruler_commands_its_units_and_the_ruling_houses_retinue_a_house_its_own() {
    let p = params();
    let mut s = all_verbs(testworld::five_states_scenario());
    s.houses.get_mut("h0").unwrap().soldiers = 30;
    s.houses.get_mut("o0").unwrap().soldiers = 20;
    let mut g = Game::new(&s, params(), 61).unwrap();
    g.set_bots(false);
    let house = |sid: &str| engine2::HouseId(s.houses.keys().position(|k| k == sid).unwrap() as u32);
    let (h0, o0) = (house("h0"), house("o0"));
    let w = g.world();
    let s0 = ActorId(0);
    let own = w.units.iter().find(|(_, u)| u.owner == s0 && u.house.is_none()).map(|(id, _)| *id).unwrap();
    let ruling_retinue = w.units_of_house(h0)[0];
    let other_retinue = w.units_of_house(o0)[0];
    let ruler = ActorRef::Ruler(s0);
    assert!(w.commands(ruler, own) && w.commands(ruler, ruling_retinue), "the ruler commands its units and the ruling house's men");
    assert!(!w.commands(ruler, other_retinue), "not the men of a house that does not rule");
    assert!(w.commands(ActorRef::House(o0), other_retinue) && !w.commands(ActorRef::House(o0), own));
    // Played through the contract: the units march.
    let r = g.tick_turn(&[], &[Order { actor: ruler, unit: own, to: at(0, 4) }, Order { actor: ActorRef::House(o0), unit: other_retinue, to: at(0, 2) }]).unwrap();
    assert_eq!(g.world().units[&own].position, at(0, 4));
    assert_eq!(g.world().units[&other_retinue].position, at(0, 2));
    assert!(r.events.iter().any(|e| matches!(e.event, Event::UnitMoved { unit, .. } if unit == own)));
    // An order to another's unit is an error, and nothing is touched.
    let before = g.to_save().unwrap();
    let foreign = g.world().units_of(ActorId(1))[0];
    let err = g.tick_turn(&[], &[Order { actor: ruler, unit: foreign, to: at(0, 0) }]).unwrap_err();
    assert!(err.to_string().contains("does not command"), "{err}");
    assert_eq!(g.to_save().unwrap(), before, "nothing touched");
    assert!(g.tick_turn(&[], &[Order { actor: ruler, unit: engine2::UnitId(999), to: at(0, 0) }]).is_err());
    let _ = p;
}

/// At peace the bot puts a revolt down with the nearest unit and keeps the rest at home. At
/// war the three duties are weighed in wheat (E3 (е2), plan §26, §28.2, §28.9): with a goal
/// against the enemy and the stronger army the war is worth going on with — the unit marches
/// on the wanted province and the revolt waits; at equal strength a war for one province
/// risks as much as it wins and is not (plan §28.9: the land at risk, one battle at a time);
/// outnumbered five to one it is worth less than nothing and the unit holds the capital;
/// without a goal there is nothing to win and it stays home; attacked, with an enemy unit a
/// tick's march from its land, it goes to the threatened province whose defence is worth
/// most; and with a goal worth more than that defence it campaigns on the enemy's capital.
#[test]
fn a_bot_puts_a_revolt_down_at_peace_and_at_war_weighs_the_campaign_the_defence_and_the_suppression_in_wheat() {
    let p = params();
    let mut w = world_of(&testworld::five_states_scenario(), 62);
    let (s0, s1) = (ActorId(0), ActorId(1));
    let u0 = w.units_of(s0)[0];
    // At peace, everything at home: no order.
    assert!(w.bot_orders(s0, &p).is_empty(), "at home and at peace: nothing to order");
    // A revolt at peace: the nearest unit goes there and the revolt ends.
    w.rise_now(at(0, 2), &p).unwrap();
    assert!(w.in_revolt(at(0, 2)));
    let orders = w.bot_orders(s0, &p);
    assert_eq!(orders, vec![Order { actor: ActorRef::Ruler(s0), unit: u0, to: at(0, 2) }]);
    let mut calm = params();
    calm.revolt_per_tick = 0.0;
    for o in &orders {
        w.apply_order(o).unwrap();
    }
    let r = w.tick(&calm).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::RevoltEnded { province, .. } if province == at(0, 2))), "put down");
    // Back home at peace.
    assert_eq!(w.bot_orders(s0, &p), vec![Order { actor: ActorRef::Ruler(s0), unit: u0, to: at(0, 0) }]);
    // At war with a goal and equal armies: the war is worth going on with, the unit marches
    // on the wanted province, the revolt waits (its take for the war's length is worth less
    // than what the front loses without the only unit).
    // (The prices here are at the face value: polity 0 has no free money — plan §31.3. With
    // the money at hand over the war's length the pay of the campaign costs nothing, and the
    // same war is worth going on with at equal strength: read below.)
    let mut w = world_of(&testworld::five_states_scenario(), 63);
    leave_no_free_money(&mut w, s0, &p);
    w.set_goal(s0, GoalKind::Take, GoalTarget::Province(at(1, 2))).unwrap();
    // The bot's declaration claims the province its goal wants (plan §28.9); on a polity its
    // goals want nothing of, the polity's submission.
    let claims: Vec<Target> = w.options(s0, &[VerbId::DeclareWar], &p).into_iter().filter_map(|o| o.call.map(|c| c.target)).collect();
    assert!(claims.contains(&Target::Province(at(1, 2))) && claims.contains(&Target::Actor(ActorId(2))), "{claims:?}");
    w.declare_war(s0, s1).unwrap();
    w.rise_now(at(0, 2), &p).unwrap();
    assert!(w.war_worth(s0, s1, false, 0.0, &p) <= 0.0, "at equal strength a war for one province risks as much as it wins");
    // The same war with money (plan §31.10, owner's decisions 2026-11-12): the pay of the
    // campaign is a spending over the war's length and is priced against the money of that
    // length (`campaign_money`) — nothing within it, the face value past it.
    let mut rich = world_of(&testworld::five_states_scenario(), 63);
    rich.set_goal(s0, GoalKind::Take, GoalTarget::Province(at(1, 2))).unwrap();
    rich.declare_war(s0, s1).unwrap();
    rich.rise_now(at(0, 2), &p).unwrap();
    let ticks = rich.war_ticks(s0, s1, &p);
    let income = rich.yearly_income(s0, &p) as f64;
    let campaign = rich.campaign_extra_per_strength(s0, ticks, &p) * rich.army_strength(s0);
    // As the world stands the polity can afford the campaign over the horizon and has not
    // the money for it at hand over the war's length: the war is worth what it has more.
    let at_hand = rich.campaign_money(s0, ticks, &p);
    assert!(campaign > 0.0 && campaign * income <= rich.free_money(s0, &p), "the campaign's pay is within the free money of the horizon");
    assert!(at_hand > 0.0 && at_hand < campaign * income, "and past the money over the war's length: {at_hand} against {}", campaign * income);
    assert!((rich.war_worth(s0, s1, false, 0.0, &p) - w.war_worth(s0, s1, false, 0.0, &p) - at_hand / income).abs() < 1e-9, "the war is worth the money over its length more: {} against {}", rich.war_worth(s0, s1, false, 0.0, &p), w.war_worth(s0, s1, false, 0.0, &p));
    // With the campaign's pay in the treasury it costs nothing: the war is worth exactly
    // that pay more than at the face value.
    rich.post(engine2::Account::Treasury(s0), Source::Test, (campaign * income).ceil() as i64 * 1000).unwrap();
    assert!(campaign * income <= rich.campaign_money(s0, ticks, &p), "the campaign's pay is within the money over the war's length");
    assert!((rich.war_worth(s0, s1, false, 0.0, &p) - w.war_worth(s0, s1, false, 0.0, &p) - campaign).abs() < 1e-9, "with the money at hand the war is worth the campaign's pay more: {} against {} and {campaign}", rich.war_worth(s0, s1, false, 0.0, &p), w.war_worth(s0, s1, false, 0.0, &p));
    assert!(rich.war_worth(s0, s1, false, 0.0, &p) > 0.0, "and at equal strength it is worth going on with");
    w.post(engine2::Account::Soldiers(u0), Source::Test, 1200 * 1000).unwrap();
    assert!(w.war_worth(s0, s1, false, 0.0, &p) > 0.0, "three to one, the war is worth going on with");
    assert_eq!(w.bot_orders(s0, &p), vec![Order { actor: ActorRef::Ruler(s0), unit: u0, to: at(1, 2) }], "the stronger army with a goal: the campaign; the revolt waits");
    // Outnumbered five to one the war is worth less than nothing: hold the capital.
    let u1 = w.units_of(s1)[0];
    w.post(engine2::Account::Soldiers(u1), Source::Test, (9000 - w.soldiers(u1)) * 1000).unwrap();
    assert!(w.war_worth(s0, s1, false, 0.0, &p) < 0.0);
    assert!(w.bot_orders(s0, &p).is_empty(), "outnumbered: hold the capital, where the unit stands");
    // Without a goal there is nothing to win: home.
    let mut w = world_of(&testworld::five_states_scenario(), 64);
    w.declare_war(s0, s1).unwrap();
    assert!(w.war_worth(s0, s1, false, 0.0, &p) < 0.0, "nothing to win, something to lose");
    assert!(w.bot_orders(s0, &p).is_empty(), "no goal: the unit stays home");
    // Attacked, with the enemy's unit a tick's march from its land: the defence of the
    // threatened province worth most (the enemy's position is read, not its order).
    let mut w = world_of(&testworld::five_states_scenario(), 65);
    w.declare_war(s1, s0).unwrap();
    let u1 = w.units_of(s1)[0];
    w.units.get_mut(&u1).unwrap().position = at(1, 2);
    w.post(engine2::Account::Soldiers(u0), Source::Test, 1200 * 1000).unwrap();
    let (mine, theirs) = (w.unit_strength(u0), w.unit_strength(u1));
    let threatened: Vec<ProvinceId> = w.actors[&s0].provinces.iter().copied().filter(|q| w.reaches_in_a_tick(s1, at(1, 2), *q, &p)).collect();
    assert!(threatened.contains(&at(0, 2)) && threatened.len() > 1, "{threatened:?}");
    let best = threatened.iter().copied().max_by(|x, y| w.defence_worth(s0, *x, s1, mine, theirs, &p).total_cmp(&w.defence_worth(s0, *y, s1, mine, theirs, &p))).unwrap();
    assert!(w.defence_worth(s0, best, s1, mine, theirs, &p) > 0.0 && best != at(0, 0), "a province other than the capital is worth defending most: {best:?}");
    assert_eq!(w.bot_orders(s0, &p), vec![Order { actor: ActorRef::Ruler(s0), unit: u0, to: best }], "attacked: the unit defends the threatened province worth most");
    // With a goal worth more than the defence it campaigns on the enemy's capital instead.
    w.set_goal(s0, GoalKind::Take, GoalTarget::Polity(s1)).unwrap();
    assert!(w.war_worth(s0, s1, false, 0.0, &p) > 0.0);
    assert_eq!(w.bot_orders(s0, &p), vec![Order { actor: ActorRef::Ruler(s0), unit: u0, to: at(1, 0) }], "the campaign worth more than the defence: on the enemy's capital");
}

/// The rule of the peace (plan §28.1, §28.8): the forced side accepts any peace; a side with
/// a cause — a live war goal, or provinces lost with the better odds — refuses below the
/// threshold of the relation unless exhausted; a side with nothing to fight for accepts. Who
/// declared the war does not enter.
#[test]
fn a_side_with_nothing_to_fight_for_accepts_a_peace_one_with_a_cause_at_the_threshold_or_exhausted_the_forced_always() {
    let p = params();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let fresh = |seed: u64| {
        let mut w = world_of(&testworld::five_states_scenario(), seed);
        w.set_goal(s0, GoalKind::Take, GoalTarget::Polity(s1)).unwrap();
        w.declare_war(s0, s1).unwrap();
        w
    };
    let w = fresh(71);
    assert_eq!(w.wars[&(s0, s1)].by, Some(s0), "the declarer of record");
    assert!(w.relation(s0, s1) < p.treaty_accept_at_relation);
    // The declarer proposes a status quo: the attacked, with nothing to fight for, accepts.
    assert!(w.peace_room(s0, s1, &p).is_ok());
    // The attacked proposes: the declarer refuses — its goal alive, its army paid.
    assert_eq!(w.peace_room(s1, s0, &p), Err("the other side wants its war: the relation below the threshold, its cause alive (its claim unmet, or provinces lost with the better odds), its army paid"));
    // … accepts at the threshold,
    let mut x = fresh(72);
    x.relations.insert((s0, s1), p.treaty_accept_at_relation);
    assert!(x.peace_room(s1, s0, &p).is_ok());
    // … when its claim is met (plan §28.9) — the test's declaration claims the polity's
    // submission, met once s1 is down to half the land it held when the war began — and a
    // goal put aside changes nothing, the claim being the public act,
    let mut x = fresh(73);
    x.actors.get_mut(&s0).unwrap().goals.clear();
    assert!(x.peace_room(s1, s0, &p).is_err(), "the claim stands without the goal");
    x.wars.get_mut(&(s0, s1)).unwrap().held = (5, 10);
    assert!(x.claim_met(s0, s1, engine2::WarAim::Polity(s1)) && x.peace_room(s1, s0, &p).is_ok(), "the claim met: no cause left");
    // … when exhausted: its army's pay short,
    let mut x = fresh(74);
    x.actors.get_mut(&s0).unwrap().paid_share_ppm = 800_000;
    assert!(x.peace_room(s1, s0, &p).is_ok(), "the pay short");
    // … and always when it is forced to peace (plan §28.1): lost more provinces than it took
    // and its side the weaker by the battle formula (at even strength it is not forced).
    let mut x = fresh(75);
    x.wars.get_mut(&(s0, s1)).unwrap().taken = (0, 1);
    assert_eq!(x.losing_side(s0, s1), Some(s0));
    assert!(x.peace_room(s1, s0, &p).is_err(), "a loser of provinces at even strength is not forced");
    let theirs = x.units.iter().find(|(_, u)| u.owner == s1).map(|(u, _)| *u).unwrap();
    x.post(engine2::Account::Soldiers(theirs), Source::Test, 1200 * 1000).unwrap();
    assert_eq!(x.forced_to_peace(s0, s1, &p), Some(s0));
    assert!(x.peace_room(s1, s0, &p).is_ok(), "the forced declarer accepts always");
    // A war of the starting facts states no claim (plan §28.9): a goal is not a cause, and a
    // status quo is accepted both ways.
    let mut x = world_of(&testworld::five_states_scenario(), 76);
    x.declare_war(s0, s1).unwrap();
    x.wars.get_mut(&(s0, s1)).unwrap().by = None;
    x.wars.get_mut(&(s0, s1)).unwrap().aim = None;
    x.set_goal(s0, GoalKind::Take, GoalTarget::Polity(s1)).unwrap();
    assert!(x.peace_room(s0, s1, &p).is_ok() && x.peace_room(s1, s0, &p).is_ok());
    // The mirror of the forced case (plan §28.8, the stop of (е2)): the attacked lost a
    // province but has the stronger army — a cause to go on — and refuses the declarer's
    // peace below the threshold; at the threshold it accepts.
    let mut x = fresh(78);
    x.wars.get_mut(&(s0, s1)).unwrap().taken = (1, 0);
    let theirs = x.units.iter().find(|(_, u)| u.owner == s1).map(|(u, _)| *u).unwrap();
    x.post(engine2::Account::Soldiers(theirs), Source::Test, 1200 * 1000).unwrap();
    assert_eq!((x.losing_side(s0, s1), x.forced_to_peace(s0, s1, &p)), (Some(s1), None));
    assert!(x.war_cause(s1, s0, &p) && !x.war_goal_against(s1, s0), "a cause without a goal: the land lost with the better odds");
    assert!(x.peace_room(s0, s1, &p).is_err(), "the stronger loser of a province refuses the declarer's peace");
    x.relations.insert((s0, s1), p.treaty_accept_at_relation);
    assert!(x.peace_room(s0, s1, &p).is_ok(), "at the threshold it accepts");
    // The war of secession is the parent's: the parent is its declarer and wants the new
    // polity's land; the new polity accepts a status quo always.
    let s = carrier_scenario();
    let mut x = world_of(&s, 77);
    let new = x.rise_now(at(0, 1), &p).unwrap().unwrap();
    let key = (ActorId(0), new);
    assert_eq!(x.wars[&key].by, Some(ActorId(0)), "the parent declares the war of secession");
    assert!(x.actors[&ActorId(0)].goals.iter().any(|g| g.kind == GoalKind::Take && g.target == GoalTarget::Polity(new)), "and wants its provinces back");
    assert!(x.peace_room(ActorId(0), new, &p).is_ok(), "the new polity accepts a status quo");
    assert!(x.peace_room(new, ActorId(0), &p).is_err(), "the parent refuses one while its goal stands and its army is paid");
}

/// The player's strategies of the paired run: the bot in the player's seat, a player who
/// does nothing, a conqueror who goes to war on its weakest neighbour and marches on its
/// capital — through the contract, with only what is on offer, one call a tick as a bot
/// (B42, parity (а), plan §31.18): the declaration first, the levy in the ticks with none.
/// The conqueror levies within its budget: while the pay of its whole army on campaign fits
/// the tick's income (the world's pay functions). A probe that levied every tick could not
/// pay the double pay of the campaign, was exhausted by the rule of the peace, accepted the
/// enemy's peace and declared again — 121 wars of a tick on the test world's seed 3, its
/// outcome the idle player's (plan §28.12): a foolish probe, not a degeneration of the
/// engine. The change is to the probe player, not to the measures.
fn play_strategy(s: &ScenarioData, seed: u64, strategy: &str, ticks: u32) -> sentinel::RunSummary {
    let p = params();
    let me = ActorId(0);
    let mut g = Game::new(s, params(), seed).unwrap();
    if strategy != "none" {
        g.set_player(Some(ActorRef::Ruler(me)));
    }
    let (_, summary) = sentinel::run(&mut g, ticks, seed, strategy, sentinel::Thresholds::default(), |g| {
        let (mut calls, mut orders) = (Vec::new(), Vec::new());
        if strategy == "conqueror" && g.world().alive(me) {
            let w = g.world();
            let enemy = w.actors.keys().copied().find(|b| w.at_war(me, *b) && w.alive(*b)).or_else(|| {
                let offered = g.verbs(ActorRef::Ruler(me)).iter().any(|o| o.verb == VerbId::DeclareWar && o.available);
                let weakest = w.actors.keys().copied().filter(|b| *b != me && w.alive(*b) && w.borders(me, *b) && w.war_room(me, *b).is_ok()).min_by_key(|b| (w.army_size(*b), *b));
                if offered {
                    if let Some(b) = weakest {
                        calls.push(VerbCall { actor: ActorRef::Ruler(me), verb: VerbId::DeclareWar, target: Target::Actor(b) });
                    }
                }
                weakest
            });
            // One call a tick, as a bot (B42, parity (а), plan §31.18): the levy only in a tick
            // with no declaration.
            if calls.is_empty() && g.verbs(ActorRef::Ruler(me)).iter().any(|o| o.verb == VerbId::Recruit && o.available) {
                let capital = w.actors[&me].capital;
                let within_budget = engine2::economy::campaign_pay_per_tick(w.army_size(me) + w.recruit_per_tick(me, &p), &p) <= w.yearly_income(me, &p) / 2;
                if within_budget && !w.in_revolt(capital) && w.population(capital) > 0 {
                    calls.push(VerbCall { actor: ActorRef::Ruler(me), verb: VerbId::Recruit, target: Target::Province(capital) });
                }
            }
            if let Some(b) = enemy {
                let target = w.actors[&b].capital;
                for u in w.units.keys().copied().filter(|u| w.commands(ActorRef::Ruler(me), *u)) {
                    if w.units[&u].destination != Some(target) && w.path(me, w.units[&u].position, target).is_some() {
                        orders.push(Order { actor: ActorRef::Ruler(me), unit: u, to: target });
                    }
                }
            }
        }
        // A call the probe refuses (a war the bots' calls made redundant) is skipped, not
        // fatal: the player plays on.
        if g.tick_turn(&calls, &orders).is_err() {
            g.tick_turn(&[], &orders).or_else(|_| g.tick(&[])).unwrap();
        }
    });
    summary
}

/// The brief's acceptance of E3 (§8): the outcome depends on the strategy and on the seed —
/// the sentinel's `StrategyIndependent` and `SeedIndependent` over a matrix of seeds ×
/// strategies on every world.
#[test]
fn the_outcome_depends_on_the_players_strategy_and_on_the_seed_the_sentinel_matrix_on_every_world() {
    let p = params();
    const WORLDS: u64 = 10;
    const TICKS: u32 = 150;
    let (mut strategy_flags, mut seed_flags, mut matrices) = (0usize, 0usize, 0usize);
    let mut flagged: Vec<String> = Vec::new();
    // Every run on its own thread (`engine2::par`), gathered in the order of the runs.
    let worlds: Vec<ScenarioData> = (1..=WORLDS).map(|w| worldgen::generate(w, &p)).collect();
    let runs: Vec<(usize, u64, &str)> = (0..worlds.len()).flat_map(|w| [1u64, 2, 3].into_iter().flat_map(move |seed| ["none", "idle", "conqueror"].into_iter().map(move |st| (w, seed, st)))).collect();
    let mut summaries = engine2::par::map(&runs, |(w, seed, st)| play_strategy(&worlds[*w], *seed, st, TICKS)).into_iter();
    for world in 1..=WORLDS {
        let mut m = sentinel::Matrix::new();
        for _ in 0..9 {
            m.add(summaries.next().expect("nine runs a world"));
        }
        matrices += 1;
        for f in m.flags(&sentinel::Thresholds::default(), &[], None) {
            match f {
                sentinel::Flag::StrategyIndependent { seed, .. } => {
                    strategy_flags += 1;
                    flagged.push(format!("world {world} seed {seed}: strategy independent"));
                }
                sentinel::Flag::SeedIndependent { strategy, .. } => {
                    seed_flags += 1;
                    flagged.push(format!("world {world} {strategy}: seed independent"));
                }
                _ => {}
            }
        }
    }
    eprintln!("the sentinel's matrix, {matrices} worlds × 3 seeds × 3 strategies × {TICKS} ticks: StrategyIndependent {strategy_flags}, SeedIndependent {seed_flags}; {flagged:?}");
    // The test world of the brief (§8): five states, the player at state 0.
    let mut s = all_verbs(testworld::five_states_scenario());
    use engine2::scenario::AspirationData;
    for k in ["s1", "s2", "s3", "s4"] {
        s.actors.get_mut(k).unwrap().aspirations = vec![AspirationData { kind: GoalKind::Take, target: Some(format!("p{:02}", 5 + k[1..].parse::<usize>().unwrap() - 1)), route: None }];
    }
    let mut m = sentinel::Matrix::new();
    let runs: Vec<(u64, &str)> = [1u64, 2, 3].into_iter().flat_map(|seed| ["none", "idle", "conqueror"].into_iter().map(move |st| (seed, st))).collect();
    for summary in engine2::par::map(&runs, |(seed, st)| play_strategy(&s, *seed, st, TICKS)) {
        m.add(summary);
    }
    let test_world: Vec<sentinel::Flag> = m.flags(&sentinel::Thresholds::default(), &[], None).into_iter().filter(|f| matches!(f, sentinel::Flag::StrategyIndependent { .. } | sentinel::Flag::SeedIndependent { .. })).collect();
    eprintln!("the test world: {test_world:?}");
    assert!(test_world.is_empty(), "the brief's acceptance on the test world: {test_world:?}");
    assert_eq!(strategy_flags, 0, "the outcome does not depend on the strategy: {flagged:?}");
    assert_eq!(seed_flags, 0, "the outcome does not depend on the seed: {flagged:?}");
}

/// The held-out seeds (owner's rules after #293): the parameters are calibrated on the seeds
/// 1–20; the acceptance is read also on 100–129, which no calibration has seen. Printed in
/// this step; checked from step (е) on.
#[test]
fn on_held_out_seeds_the_measures_are_read_beside_the_calibration_seeds() {
    let p = params();
    for (label, seeds) in [("calibration 1–20", (1..=20).collect::<Vec<u64>>()), ("held out 100–129", (100..=129).collect())] {
        let m = Measures::over(&seeds, &p);
        let r = m.rates();
        let row: Vec<String> = r.against_bands(&p).into_iter().map(|(n, v, b, ok)| format!("{n} {v:.3}{}", if ok { String::new() } else { format!(" OUTSIDE {b:?}") })).collect();
        eprintln!("{label}: {}", row.join("; "));
        // The base of (д1) on main (plan §30.10) on both sets of seeds: the measures with no
        // band in the file the row above does not carry, and the wars of principals — printed.
        eprintln!(
            "{label}: А11 at war {:.4}; А11 at peace {:.4}, by windows of 50 ticks {:?}; Э9 the army's share of spending {:.3}; war declarations {}, the tick after a peace of the same pair {} ({:.3}), within five years of it {} ({:.3})",
            r.a11_war,
            r.a11_peace,
            m.a11_peace_windows.iter().map(|(a, n)| format!("{:.4}", *a as f64 / (*n).max(1) as f64)).collect::<Vec<_>>(),
            m.pay as f64 / (m.pay + m.court).max(1) as f64,
            m.declarations,
            m.declared_after_peace,
            m.declared_after_peace as f64 / m.declarations.max(1) as f64,
            m.declared_within_five_years,
            m.declared_within_five_years as f64 / m.declarations.max(1) as f64
        );
        for line in m.war_lines() {
            eprintln!("{label}: {line}");
        }
    }
}

/// The narrow-window rule (owner's rules after #293): a measure inside its band only in a
/// narrow window of a parameter is a sign about the model, not a point to pick. A sweep of
/// the (в) values fitted to bands, each over a range around its value, on the calibration
/// seeds: the measure it is checked through at every point, and where all the bands hold.
/// Run on demand: `cargo test --release -p engine2 --all-features --test agents windows -- --ignored --nocapture`.
#[test]
#[ignore]
fn windows_of_the_fitted_values() {
    let base = params();
    let seeds: Vec<u64> = (1..=20).collect();
    type Set = fn(&mut Params, f64);
    let sweeps: [(&str, &[f64], Set); 4] = [
        ("usurpation.per_year → В6", &[0.02, 0.03, 0.04, 0.05, 0.06, 0.08], |p, v| p.usurpation_per_tick = v / 2.0),
        ("revolt.per_year → В7, В9", &[0.0015, 0.002, 0.0025, 0.003, 0.004, 0.005], |p, v| p.revolt_per_tick = v / 2.0),
        // The horizon of every worth (E3 (е1)): chosen, not fitted, and since the weights of
        // the throne and of independence went into the model, the bots' one lever on В6 and В8.
        ("goal.horizon_years → all", &[10.0, 15.0, 20.0, 25.0, 30.0, 40.0], |p, v| p.goal_horizon_ticks = (v * 2.0) as u32),
        // By memory, not fitted (owner's answer 3 on §27.5): its window printed beside it.
        ("army.campaign_pay_factor → all", &[1.0, 1.5, 2.0, 3.0, 4.0], |p, v| p.campaign_pay_ppm = (v * 1e6) as i64),
    ];
    for (name, values, set) in sweeps {
        let mut inside = Vec::new();
        for v in values {
            let mut p = base.clone();
            set(&mut p, *v);
            let rates = Measures::over(&seeds, &p).rates().against_bands(&p);
            let all = rates.iter().all(|(_, _, _, ok)| *ok);
            let row: Vec<String> = rates.iter().map(|(n, x, _, ok)| format!("{n} {x:.3}{}", if *ok { "" } else { "!" })).collect();
            eprintln!("{name} = {v}: {}{}", row.join(" "), if all { "  — all bands" } else { "" });
            if all {
                inside.push(*v);
            }
        }
        eprintln!("{name}: all bands at {inside:?} of {values:?}");
    }
}

// ---------- step (е1): the bot calls the world's formulas ----------

/// E3 (е1) acceptance (owner's rules after #293, plan §24.6): the bot **calls** the world's
/// formulas, so a change of the world's numbers — and nothing in the bot — changes its
/// choices: the collection and the norm's shift move the tax it settles at, the hazard of
/// usurpation moves what the throne is worth to a house (and, through the house's answer,
/// what the ruler fears), the battle's exponent moves what a polity of its own and an ally
/// are worth.
#[test]
fn a_change_of_the_worlds_numbers_alone_changes_the_bots_choices() {
    let p = params();
    let a = ActorId(0);
    // The tax: where a step up stops paying, from the bottom of the tithe's grid, the rating
    // at its norm.
    let settles = |p: &Params| -> i64 {
        let mut w = testworld::world(54);
        let mut rate = p.tax_default_ppm - (p.tax_default_ppm - p.tax_band_ppm.0) / p.tax_step_ppm * p.tax_step_ppm;
        loop {
            w.actors.get_mut(&a).unwrap().tax_rate_ppm = rate;
            let norm = w.legitimacy_norm(a, p);
            w.actors.get_mut(&a).unwrap().legitimacy = norm;
            if rate + p.tax_step_ppm > p.tax_band_ppm.1 || w.tax_utility(a, VerbId::RaiseTax, p) <= 0.0 {
                return rate;
            }
            rate += p.tax_step_ppm;
        }
    };
    let shipped = settles(&p);
    let mut evasive = p.clone();
    evasive.tax_evasion_ppm_per_legitimacy_point *= 2;
    let mut shifting = p.clone();
    shifting.legitimacy_norm_per_tax_step *= 2.0;
    let (e, s) = (settles(&evasive), settles(&shifting));
    // Usurpers four times as frequent and ruling at 10: the throne is worth little to the
    // candidate's house, which would decline it — and the ruler, reading the house's answer,
    // fears the usurpation no more (printed: a property of the model, plan §24b).
    let mut harsh = p.clone();
    harsh.legitimacy_norm_usurper = 10.0;
    harsh.usurpation_per_tick *= 4.0;
    let tw = testworld::world(54);
    let c = tw.usurper_candidate(a, &p).expect("a candidate");
    eprintln!(
        "the tax settles at {:.2}; with the evasion doubled {:.2}; with the norm's shift per step doubled {:.2}; with usurpers four times as frequent and ruling at 10 {:.2} (the candidate's house: the throne {:.2} shipped, {:.2} there)",
        shipped as f64 / 1e6,
        e as f64 / 1e6,
        s as f64 / 1e6,
        settles(&harsh) as f64 / 1e6,
        tw.throne_worth(a, c, &p),
        tw.throne_worth(a, c, &harsh)
    );
    assert!(e < shipped, "the collection lost per point of legitimacy moves the tax down");
    assert!(s < shipped, "a costlier shift of the norm per step moves the tax down");
    // The flight (owner's rule after #295): what a step up drives across the border is read
    // off the world's migration — four times its rate, four times the people a step drives
    // away, by the bot's own count, with nothing changed in the bot.
    let mut w = testworld::world(54);
    let rate = p.tax_band_ppm.1 - p.tax_step_ppm;
    w.actors.get_mut(&a).unwrap().tax_rate_ppm = rate;
    let mut mobile = p.clone();
    mobile.migration_rate_ppm_per_tick *= 4;
    let mut lenient = p.clone();
    lenient.migration_min_gap_ppm /= 4;
    let (f0, f1, f2) = (w.flight_worth(a, rate + p.tax_step_ppm, &p), w.flight_worth(a, rate + p.tax_step_ppm, &mobile), w.flight_worth(a, rate + p.tax_step_ppm, &lenient));
    eprintln!("the flight of a step to the top of Э4: {f0:+.4} years of income; migration four times as fast {f1:+.4}; the threshold of a move a quarter {f2:+.4}");
    assert!(f1 != f0 || f2 != f0, "the world's migration moves the worth of the flight");
    assert!(f2 <= f0, "a lower threshold lets more people go");
    // The choice itself, at the tithe: the step up the shipped world makes worth it, the
    // evasive world does not.
    let w = testworld::world(54);
    assert!(w.tax_utility(a, VerbId::RaiseTax, &p) > 0.0 && w.tax_utility(a, VerbId::RaiseTax, &evasive) < 0.0, "the bot's choice at the tithe flips with the world's collection");
    // The throne: its seat is held until the next chance of usurpation — oftener chances,
    // a shorter hold, a throne worth less to the candidate's house.
    let w = world_of(&carrier_scenario(), 7);
    let candidate = w.usurper_candidate(a, &p).expect("a candidate");
    let mut frequent = p.clone();
    frequent.usurpation_per_tick *= 4.0;
    let (t0, t1) = (w.throne_worth(a, candidate, &p), w.throne_worth(a, candidate, &frequent));
    eprintln!("the throne to the candidate's house: {t0:.2}, with usurpations four times as frequent {t1:.2}");
    assert!(t1 < t0, "the hazard of usurpation moves the throne's worth");
    // A polity of its own: the chance to hold out is the battle's — a militia smaller than
    // the parent's army holds out the less the more the battle favours the stronger.
    let h = engine2::HouseId(carrier_scenario().houses.keys().position(|k| k == "o0").unwrap() as u32);
    let province = at(0, 1);
    let mut steep = p.clone();
    steep.battle_strength_exponent *= 2.0;
    let (i0, i1) = (w.independence_worth(h, a, province, &p), w.independence_worth(h, a, province, &steep));
    eprintln!("a polity of its own to the carrier house: {i0:.2}, with the battle's exponent doubled {i1:.2}");
    assert!((i1 - i0).abs() > 1e-9, "the battle's exponent moves the worth of independence");
    // An ally against a goal's enemy: the battle formula with the ally's army and without it.
    let mut w = world_of(&carrier_scenario(), 7);
    w.set_goal(a, GoalKind::AllianceAgainst, GoalTarget::Polity(ActorId(1))).unwrap();
    let (al0, al1) = (w.alliance_worth(a, ActorId(2), &p), w.alliance_worth(a, ActorId(2), &steep));
    eprintln!("an alliance with polity 2 against polity 1: {al0:.3}, with the battle's exponent doubled {al1:.3}");
    assert!((al1 - al0).abs() > 1e-9, "the battle's exponent moves the worth of an ally");
}

/// The bot calls the world's formulas, never copies them (E3 (е1)): the parameters of the
/// world's hazards, drift, mortality, battle, levy, gift and collection appear in the bot's
/// code only behind the world's functions. A copy — `params.usurpation_per_tick * (100.0 −
/// l) / 50.0` written into `worth.rs` — fails here (checked by injection when the guard was
/// written).
#[test]
fn the_bots_code_reads_no_parameter_of_the_worlds_formulas() {
    const SOURCES: [(&str, &str); 2] = [("agents.rs", include_str!("../src/agents.rs")), ("worth.rs", include_str!("../src/worth.rs"))];
    let formulas = [
        "usurpation_per_tick",
        "violent_death_per_tick",
        "revolt_per_tick",
        "legitimacy_rate_per_tick",
        "mortality_per_year",
        "mortality_elite_factor",
        "battle_strength_exponent",
        "recruit_ceiling_ppm",
        "bribe_gain_per_share",
        "tax_evasion_ppm_per_legitimacy_point",
        "output_per_head_kg",
    ];
    for (file, src) in SOURCES {
        let code: Vec<&str> = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
        for f in formulas {
            let copied: Vec<&&str> = code.iter().filter(|l| l.contains(&format!("params.{f}"))).collect();
            assert!(copied.is_empty(), "{file} copies a formula of the world through `params.{f}`: {copied:?}");
        }
    }
}

/// The bot sees what a player would see (owner's rule 2026-10-17, plan §28.4, §28.7):
/// positions, owners, wars, treaties, the view and the events — not another's goals, its
/// house's treasury, its count of the dead or its orders. Checked by injection: the hidden
/// state of another changes, and the worth and the choices of polity 0 and of the house
/// offered a match do not. On the code before the fix the goal of polity 2 moved what an
/// alliance with it was worth to polity 0, and the proposer's treasury moved the house's
/// worth of the match (checked when the guard was written). The registry of the hidden:
/// `Actor.goals`, `Actor.manpower_deficit`, `Actor.paid_share_history_ppm` (the bot's own
/// memory), `House.treasury` (not one's own), `Unit.destination` (not one's own), and the
/// treasury of another polity, `Account::Treasury` (owner's decision 2026-11-11, plan §30.9
/// p. 1: the view shows a polity its own only; the debt stays open). On main the bots read
/// no treasury but their own, so that injection holds before the fix too: it guards the
/// rule of the peace by what a side can field ((д1), step Б).
#[test]
fn the_bot_reads_nothing_of_another_that_a_player_would_not_see() {
    let p = params();
    let (a, b, c) = (ActorId(0), ActorId(1), ActorId(2));
    // With the price of money (plan §31.3): the money polity 0 prices its spendings against
    // — over the horizon (`free_money`) and over a span (`money_over`: a tick, a war's
    // length) — and the army its peace keeps (`peace_army`, plan §31.15) are its own state
    // alone; they are in what is seen, and so is every price read off them (`options` carries
    // the levy, the hire and the disbanding; `war_worth` the campaign).
    let seen = |w: &World| (w.alliance_worth(a, b, &p), w.alliance_worth(a, c, &p), w.war_worth(a, b, false, 0.0, &p), w.war_worth(a, c, false, 0.0, &p), w.bot_orders(a, &p), w.options(a, &VerbId::ALL, &p), w.free_money(a, &p), w.money_over(a, 1.0, &p), w.money_over(a, 8.0, &p), w.campaign_money(a, 8.0, &p), w.peace_army(a, &p));
    let mut w = world_of(&carrier_scenario(), 7);
    let before = seen(&w);
    // Another's goals: polity 2 wants an ally against polity 1, polity 1 wants polity 0's land.
    w.set_goal(c, GoalKind::AllianceAgainst, GoalTarget::Polity(b)).unwrap();
    w.set_goal(b, GoalKind::Take, GoalTarget::Province(w.actors[&a].capital)).unwrap();
    assert_eq!(seen(&w), before, "another's goals are its own");
    // Another's count of the dead.
    w.actors.get_mut(&b).unwrap().manpower_deficit = 1.0e6;
    w.actors.get_mut(&c).unwrap().manpower_deficit = 1.0e6;
    assert_eq!(seen(&w), before, "another's count of the dead is its own");
    // Another's orders: every unit of polity 1 ordered onto polity 0's capital.
    let capital = w.actors[&a].capital;
    let theirs: Vec<engine2::UnitId> = w.units.iter().filter(|(_, u)| u.owner == b).map(|(u, _)| *u).collect();
    assert!(!theirs.is_empty(), "polity 1 has units to order");
    for u in theirs {
        w.units.get_mut(&u).unwrap().destination = Some(capital);
    }
    assert_eq!(seen(&w), before, "another's orders are its own");
    // Another's treasury: the neighbours' grow a thousandfold, and one is emptied.
    for x in [b, c] {
        let more = 1000 * w.treasury(x).max(1000) * 1000;
        w.post(engine2::Account::Treasury(x), Source::Test, more).unwrap();
    }
    assert_eq!(seen(&w), before, "another's treasury is its own");
    let all = w.treasury(b) * 1000;
    w.post(engine2::Account::Treasury(b), Source::Test, -all).unwrap();
    assert_eq!(w.treasury(b), 0, "polity 1's treasury emptied");
    assert_eq!(seen(&w), before, "another's empty treasury is its own");
    // Another's debt: polity 2 owes a hundred years of its income.
    let owed = 100 * w.yearly_income(c, &p);
    w.post(engine2::Account::Debt(c), Source::Test, owed * 1000).unwrap();
    assert_eq!(seen(&w), before, "another's debt moves nothing polity 0 prices");
    // The injections have teeth: the same things moved the free money of the others.
    let fresh = world_of(&carrier_scenario(), 7);
    assert!(w.free_money(b, &p) != fresh.free_money(b, &p) && w.free_money(c, &p) != fresh.free_money(c, &p), "the neighbours' own free money moved with their treasuries and debts");
    assert!(w.money_over(b, 8.0, &p) != fresh.money_over(b, 8.0, &p) && w.money_over(c, 8.0, &p) != fresh.money_over(c, 8.0, &p), "and their money over a span");
    // With part of an embassy's price at hand (plan §31.13), so that the money at hand moves
    // the decision: the neighbours' treasuries and debts still move nothing polity 0 prices.
    let mut short = world_of(&carrier_scenario(), 7);
    let at_hand = short.embassy_cost(a, &p) / 2;
    leave_money_at_hand(&mut short, a, at_hand, &p);
    // Every other polity starts with nothing, then holds a thousandfold its former treasury.
    let others: Vec<ActorId> = short.actors.keys().copied().filter(|x| *x != a).collect();
    let theirs: Vec<i64> = others.iter().map(|x| short.treasury(*x)).collect();
    for x in others.iter().copied() {
        let all = short.treasury(x);
        short.post(engine2::Account::Treasury(x), Source::Test, -all * 1000).unwrap();
    }
    // (The embassy is past the treasury and off the menu: its worth is read as the price itself.)
    let embassy = |w: &World| w.utility(a, &VerbCall { actor: ActorRef::Ruler(a), verb: VerbId::SendEmbassy, target: Target::Actor(b) }, &p);
    let before = (seen(&short), embassy(&short));
    for (x, had) in others.into_iter().zip(theirs) {
        let more = 1000 * had.max(1000) * 1000;
        short.post(engine2::Account::Treasury(x), Source::Test, more).unwrap();
        short.post(engine2::Account::Debt(x), Source::Test, 100 * short.yearly_income(x, &p) * 1000).unwrap();
    }
    assert_eq!((seen(&short), embassy(&short)), before, "with money at hand for part of an embassy, another's treasury and debt are its own");
    // Another house's treasury: the match offered, the proposer's treasury then grows a
    // thousandfold — the house answers the dowry the offer named.
    let s = all_verbs(testworld::five_states_scenario());
    let o0 = engine2::HouseId(s.houses.keys().position(|k| k == "o0").unwrap() as u32);
    let mut g = Game::new(&s, params(), 12).unwrap();
    g.set_bots(false);
    let h0 = g.world().ruling_house(a);
    g.tick(&[VerbCall { actor: ActorRef::Ruler(a), verb: VerbId::ProposeMarriage, target: Target::House(o0) }]).unwrap();
    let offer = g.world().opportunities.iter().find(|o| matches!(o.what, engine2::Chance::Marriage { .. })).cloned().expect("the match offered");
    let engine2::Chance::Marriage { dowry_kg, .. } = offer.what else { unreachable!() };
    let answered = |w: &World| (w.bot_reply(&offer, &p), w.marriage_worth(o0, h0, dowry_kg, &p));
    let before = answered(g.world());
    let more = 1000 * g.world().house_treasury(h0).max(1000) * 1000;
    g.world_mut().post(engine2::Account::HouseTreasury(h0), Source::Test, more).unwrap();
    assert_eq!(answered(g.world()), before, "another house's treasury is its own: the offer named the dowry");
}

/// The bot's sources name no hidden state of another that has no use of one's own in them
/// (plan §28.7): the count of the dead and a house's treasury are reached through the
/// world's functions on the deciding actor only, never by these names. The difference between
/// one's own and another's (`goals`, `dowry`, `recruit_room`, `destination`) is the
/// injection's to hold (`the_bot_reads_nothing_of_another_that_a_player_would_not_see`).
#[test]
fn the_bots_code_names_no_hidden_state_of_another() {
    const SOURCES: [(&str, &str); 2] = [("agents.rs", include_str!("../src/agents.rs")), ("worth.rs", include_str!("../src/worth.rs"))];
    for (file, src) in SOURCES {
        let code: Vec<&str> = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
        for hidden in ["manpower_deficit", "house_treasury("] {
            let read: Vec<&&str> = code.iter().filter(|l| l.contains(hidden)).collect();
            assert!(read.is_empty(), "{file} reads hidden state by name `{hidden}`: {read:?}");
        }
    }
}

/// The rule of the peace by the battle formula (plan §28.1, owner's rule after the stop of
/// (е2) on #298): the side forced to accept any peace is the one that lost more provinces in
/// the war **and** whose side is the weaker by the battle formula, counted with those in the
/// war by its obligations (`war_sides`). Polity 9's case: the loser of undefended provinces
/// with the stronger army is not forced — it refuses the weaker side's peace and would take
/// back what it lost; the same loser with the smaller army is forced; a loser stronger than
/// its enemy alone but weaker than the enemy with its ally in the war is forced; at even
/// strength nobody is.
#[test]
fn a_loser_of_provinces_is_forced_to_peace_only_when_its_side_is_the_weaker_by_the_battle_formula() {
    let p = params();
    let (s0, s1, s2) = (ActorId(0), ActorId(1), ActorId(2));
    let unit_of = |w: &World, a: ActorId| w.units.iter().find(|(_, u)| u.owner == a).map(|(u, _)| *u).expect("a unit");
    // s0 declares on s1 and loses a province in the war.
    let fresh = |seed: u64, allied: bool| {
        let mut w = world_of(&testworld::five_states_scenario(), seed);
        if allied {
            w.ally_now(s1, s2, &p).unwrap();
        }
        w.set_goal(s0, GoalKind::Take, GoalTarget::Polity(s1)).unwrap();
        w.declare_war(s0, s1).unwrap();
        w.wars.get_mut(&(s0, s1)).unwrap().taken = (0, 1);
        assert_eq!(w.losing_side(s0, s1), Some(s0), "s0 lost more provinces");
        w
    };
    // Even armies (the test world's 600 each): nobody is forced; the declarer refuses.
    let w = fresh(81, false);
    assert_eq!(w.forced_to_peace(s0, s1, &p), None, "even strength: nobody is forced");
    assert!(w.peace_room(s1, s0, &p).is_err(), "the declarer, not forced, wants its war");
    // The loser of provinces with the stronger army (polity 9's case): not forced.
    let mut w = fresh(82, false);
    w.post(engine2::Account::Soldiers(unit_of(&w, s0)), Source::Test, 1200 * 1000).unwrap();
    assert_eq!(w.forced_to_peace(s0, s1, &p), None, "the stronger loser of provinces would take them back");
    assert!(w.peace_room(s1, s0, &p).is_err(), "it refuses the weaker side's peace");
    assert!(w.peace_room(s0, s1, &p).is_ok(), "the attacked accepts a status quo from it");
    // The same loser with the smaller army: forced, accepts any peace. Not stricter than
    // that (plan §28.12, owner's decision 2026-10-20): «beaten» read as the capital taken or
    // besieged gave wars without end — 17 polities of 149 always at war.
    let mut w = fresh(83, false);
    w.post(engine2::Account::Soldiers(unit_of(&w, s1)), Source::Test, 1200 * 1000).unwrap();
    assert_eq!(w.forced_to_peace(s0, s1, &p), Some(s0));
    assert_eq!(w.forced_to_peace(s1, s0, &p), Some(s0), "the same either way round");
    assert!(w.peace_room(s1, s0, &p).is_ok(), "the forced declarer accepts");
    // Stronger than the enemy alone, weaker than the enemy with its ally in the war: forced.
    let mut w = fresh(84, true);
    assert!(w.at_war(s0, s2) && w.wars[&(s0, s2)].of == Some((s0, s1)), "the ally entered by the obligation");
    assert_eq!(w.war_sides(s0, s1), (vec![s0], vec![s1, s2]));
    w.post(engine2::Account::Soldiers(unit_of(&w, s0)), Source::Test, 300 * 1000).unwrap();
    assert!(w.army_strength(s0) > w.army_strength(s1) && w.army_strength(s0) < w.army_strength(s1) + w.army_strength(s2));
    assert_eq!(w.forced_to_peace(s0, s1, &p), Some(s0), "the side is weighed, not the polity alone");
}

/// The chronicle of war (E3 (г), plan §29.2; owner's decision 2 on §29.9): a war goes into the
/// public record of everyone in it — the declarer, the attacked, the entrant by its bonds —
/// and the wars of the starting facts are nobody's. The rate a bot reads off it (`war_rate`,
/// owner's answer 1 on §29.6) mixes the polity's own record with the world's over the horizon:
/// with no record yet — the own count over the horizon alone; with a record — `(own + world ×
/// H) / (seen + H)`, so a polity that was never at war is read by the world's rate. The wars it
/// brings a tick on a neighbour (`wars_a_tick_from`) are nothing where the rate is, or where
/// their land does not touch.
#[test]
fn a_war_goes_into_the_public_record_of_everyone_in_it_and_the_rate_of_war_mixes_it_with_the_worlds() {
    let p = params();
    let (s0, s1, s2) = (ActorId(0), ActorId(1), ActorId(2));
    let mut w = world_of(&testworld::five_states_scenario(), 31);
    let h = f64::from(p.goal_horizon_ticks);
    assert!(w.actors.values().all(|a| a.wars_entered == 0), "the starting facts are nobody's war");
    assert_eq!(w.war_rate(s0, &p), 0.0, "no record and no wars: nothing");
    assert!(w.actors.keys().all(|t| w.wars_a_tick_from(*t, s0, 0.0, &p) == 0.0));
    w.ally_now(s1, s2, &p).unwrap();
    w.declare_war(s0, s1).unwrap();
    assert!(w.at_war(s0, s2), "the ally enters");
    let declared = |w: &World| (w.actors[&s0].wars_entered, w.actors[&s1].wars_entered, w.actors[&s2].wars_entered);
    assert_eq!(declared(&w), (1, 1, 1), "one war each: the declarer's, the attacked's, the entrant's");
    assert_eq!(w.actors[&ActorId(3)].wars_entered, 0, "a polity outside the war has none");
    assert!((w.war_rate(s0, &p) - 1.0 / h).abs() < 1e-12, "no ticks seen yet: the own count over the horizon, {}", w.war_rate(s0, &p));
    let n = 10u32;
    for _ in 0..n {
        w.tick(&p).unwrap();
    }
    let living = w.actors.keys().filter(|x| w.alive(**x)).count() as f64;
    let world = w.actors.values().filter(|x| x.wars_entered > 0).map(|x| f64::from(x.wars_entered)).sum::<f64>() / (living * f64::from(n));
    let expect = |own: u32| (f64::from(own) + world * h) / (f64::from(n) + h);
    assert!((w.war_rate(s0, &p) - expect(declared(&w).0)).abs() < 1e-12, "{} vs {}", w.war_rate(s0, &p), expect(declared(&w).0));
    let s3 = ActorId(3);
    assert!((w.war_rate(s3, &p) - expect(0)).abs() < 1e-12 && w.war_rate(s3, &p) > 0.0, "a polity that was never at war is read by the world's rate");
    assert!(w.war_rate(s0, &p) > w.war_rate(s3, &p), "its own record counts");
    for t in w.actors.keys() {
        let c = w.wars_a_tick_from(*t, s0, 0.0, &p);
        assert!(c >= 0.0 && c <= w.war_rate(*t, &p) + 1e-12, "a share of the rate: {c}");
        assert!(c == 0.0 || (w.war_rate(*t, &p) > 0.0 && w.borders(*t, s0)), "wars only where there is a rate and a border");
        assert!(w.borders(*t, s0) || c == 0.0, "nothing from a polity that does not touch");
    }
}

/// A standing army at peace is worth the threat it answers (E3 (г), plan §29.3): with no
/// record of war anywhere the levy is worth nothing but its pay for the horizon; with a
/// neighbour's record it is worth what it takes off the loss that neighbour's war would bring,
/// at the chance it declares — the more the record, the more the levy and the less the
/// disbanding. The numbers 0.5 and −1 of (б) are gone from the code.
#[test]
fn at_peace_a_levy_is_worth_the_threat_it_answers_and_nothing_but_its_pay_without_one() {
    let p = params();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let mut w = world_of(&testworld::five_states_scenario(), 33);
    assert!(w.borders(s0, s1) && !w.at_war(s0, s1));
    let levy = VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Recruit, target: Target::Province(w.actors[&s0].capital) };
    let unit = w.units.iter().find(|(_, u)| u.owner == s0).map(|(u, _)| *u).expect("a unit of polity 0");
    let disband = VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Disband, target: Target::Unit(unit) };
    let men = w.recruit_per_tick(s0, &p);
    let (pay, heads) = (w.soldiers_cost(s0, men, 1.0, &p), w.heads_cost(s0, men, 1.0, &p));
    assert!(w.army_size(s0) > 0, "with units in being the levy adds nothing to the suppression");
    // The price of money (plan §31.3): polity 0 has free money, and the levy's pay costs it
    // nothing — the heads off the land are all a levy that answers nothing costs.
    assert!(w.free_money(s0, &p) > 0.0 && heads > 0.0 && pay > heads);
    assert!((w.utility(s0, &levy, &p) + heads).abs() < 1e-9, "no threat, free money: the levy costs the heads off the land for a tick and answers nothing");
    // With no free money: a tick of its pay at the face value (the decision is retaken every tick).
    let mut poor = world_of(&testworld::five_states_scenario(), 33);
    leave_no_free_money(&mut poor, s0, &p);
    assert!((poor.utility(s0, &levy, &p) + pay).abs() < 1e-9, "no threat, no free money: the levy costs a tick of its pay and answers nothing");
    // **The same threat, and the polity with free money takes the levy the other does not.**
    poor.actors.get_mut(&s1).unwrap().wars_entered = 3;
    let mut rich = world_of(&testworld::five_states_scenario(), 33);
    rich.actors.get_mut(&s1).unwrap().wars_entered = 3;
    let (taken, left) = (rich.utility(s0, &levy, &p), poor.utility(s0, &levy, &p));
    assert!(taken > 0.0 && left < 0.0, "at the same threat the polity with free money takes the levy ({taken}), the one with none does not ({left})");
    assert!((taken - left - (pay - heads)).abs() < 1e-9, "and the difference is the pay at its face value");
    let (levy_without, disband_without) = (w.utility(s0, &levy, &p), w.utility(s0, &disband, &p));
    w.actors.get_mut(&s1).unwrap().wars_entered = 3;
    let (levy_some, disband_some) = (w.utility(s0, &levy, &p), w.utility(s0, &disband, &p));
    assert!(levy_some > levy_without && disband_some < disband_without, "a neighbour with a record of war: the levy worth more, the disbanding less ({levy_without} → {levy_some}; {disband_without} → {disband_some})");
    w.actors.get_mut(&s1).unwrap().wars_entered = 9;
    assert!(w.utility(s0, &levy, &p) > levy_some && w.utility(s0, &disband, &p) < disband_some, "the more the record, the more the chance read");
}

/// The world lets any number of men be levied up to the tick's levy (`Target::Levy`; E3 (г),
/// owner's decision 4 on §29.8): so many men from a province, capped by the tick's levy and the
/// province's people, none is no call; `Target::Province` alone stays the whole tick's levy. The
/// bot's options are the tick's levy and its halves down to a thirty-second, from its most
/// peopled province — it picks the size whose last man still pays.
#[test]
fn a_levy_of_any_size_up_to_the_ticks_levy_and_the_bots_sizes_are_the_halves_of_it() {
    let p = params();
    let s0 = ActorId(0);
    let mut w = world_of(&testworld::five_states_scenario(), 35);
    let levy = |q: ProvinceId, men: u32| VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Recruit, target: Target::Levy(q, men) };
    let tick = w.recruit_per_tick(s0, &p);
    assert!(tick > 100);
    let before = w.army_size(s0);
    w.apply_verb(&levy(at(0, 1), 100), &p).unwrap();
    assert_eq!(w.army_size(s0), before + 100, "a hundred men levied");
    w.apply_verb(&levy(at(0, 2), u32::MAX), &p).unwrap();
    assert_eq!(w.army_size(s0), before + 100 + w.recruit_per_tick(s0, &p).max(tick).min(w.population(at(0, 2)) + tick), "capped by the tick's levy");
    assert!(w.apply_verb(&levy(at(0, 3), 0), &p).is_err(), "no men is no call");
    // The bot's sizes: the tick's levy halved down to a thirty-second, one province.
    let w = world_of(&testworld::five_states_scenario(), 35);
    let tick = w.recruit_per_tick(s0, &p);
    let mut sizes: Vec<(ProvinceId, u32)> = w.options(s0, &[VerbId::Recruit], &p).iter().filter_map(|o| o.call).filter_map(|c| match c.target { Target::Levy(q, men) => Some((q, men)), _ => None }).collect();
    sizes.sort();
    let q = sizes[0].0;
    assert!(sizes.iter().all(|(x, _)| *x == q), "one province, the most peopled: {sizes:?}");
    let mut expect: Vec<u32> = (0..6).map(|k| (tick >> k) as u32).filter(|m| *m > 0).collect();
    expect.sort();
    let mut got: Vec<u32> = sizes.iter().map(|(_, m)| *m).collect();
    got.sort();
    assert_eq!(got, expect, "{sizes:?}");
}

/// The deterrence (E3 (г), owner's decision 1 on §29.8): the share of a neighbour's wars that
/// falls on a polity follows the battle formula — the stronger the polity's army, the fewer of
/// the neighbour's wars are its; the wars a tick over the neighbour's borders sum to its rate
/// (owner's decision 3 on §29.9: the expected number, not the chance of one).
#[test]
fn an_army_at_peace_lowers_the_neighbours_wars_on_it_by_the_battle_formula() {
    let p = params();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let mut w = world_of(&testworld::five_states_scenario(), 37);
    w.actors.get_mut(&s1).unwrap().wars_entered = 5;
    let rate = w.war_rate(s1, &p);
    assert!(rate > 0.0);
    let wars = |w: &World, extra: f64| w.wars_a_tick_from(s1, s0, extra, &p);
    assert!(wars(&w, 0.0) > 0.0);
    let more = engine2::World::new_soldiers_strength(w.recruit_per_tick(s0, &p));
    assert!(wars(&w, more) < wars(&w, 0.0), "a levy lowers the wars on it: {} -> {}", wars(&w, 0.0), wars(&w, more));
    assert!(wars(&w, -w.army_strength(s0)) > wars(&w, 0.0), "an army sent home raises them");
    // The shares over the neighbour's borders sum to one: the wars a tick add up to the rate.
    let neighbours: Vec<ActorId> = w.actors.keys().copied().filter(|x| w.borders(s1, *x)).collect();
    let total: f64 = neighbours.iter().map(|x| w.wars_a_tick_from(s1, *x, 0.0, &p)).sum();
    assert!((total - rate).abs() < 1e-12, "{total} vs {rate}");
}

/// **A neighbour's threat stands on the sides of the neighbour's declaration** (plan §31.20,
/// owner's decision 2026-11-13): the war a threat is, is the war `t` would declare on `a` —
/// with `a` its allies, lord and vassals, with `t` its vassals alone (§19, `war_sides(t, a)`).
/// Read on the sides of the war `a` would declare, the threat left `a`'s allies and lord out
/// of its defence and put the neighbour's into the attack. Both readers follow: the share of
/// the neighbour's wars that falls on the polity (`wars_a_tick_from`) and the odds of the
/// threat's terms (`threats` → `WarTerms::odds`, the price of that war). A polity with no
/// ally and no lord, against a neighbour with none, reads the threat as before, bit for bit.
#[test]
fn a_neighbours_threat_stands_on_the_sides_of_the_neighbours_declaration() {
    let p = params();
    let (s0, s1, s2, s3, s4) = (ActorId(0), ActorId(1), ActorId(2), ActorId(3), ActorId(4));
    let mut w = world_of(&testworld::five_states_scenario(), 41);
    w.actors.get_mut(&s1).unwrap().wars_entered = 5;
    assert!(w.borders(s0, s1) && !w.at_war(s0, s1) && w.war_rate(s1, &p) > 0.0);
    let threat = |w: &World| {
        let blow = w.ceded_province_blow(s0, &w.risk_prices(s0, &p), &p);
        w.threats(s0, blow, &p).into_iter().find(|(t, _)| *t == s1).map(|(_, terms)| terms).expect("polity 1 threatens polity 0")
    };
    // The share by the sides of the polity's own declaration: the reading before the rule.
    let by_its_own_declaration = |w: &World| World::wars_a_tick_in(&w.threat_rate(s1, s0, &p), s0, w.war_odds(s0, s1, 0.0, &p));
    // No bonds on either side: the two wars have the same sides, and nothing moves.
    let sides = w.threat_sides_strength(s0, s1);
    assert_eq!(sides, w.war_sides_strength(s0, s1), "no bonds: one war either way");
    assert_eq!(sides, (w.army_strength(s0), w.army_strength(s1)));
    let wars = w.wars_a_tick_from(s1, s0, 0.0, &p);
    assert_eq!(wars, by_its_own_declaration(&w), "without an ally or a lord the threat is what it was, bit for bit");
    let odds = threat(&w).odds(0.0, &p);
    assert_eq!(odds, w.war_odds(s0, s1, 0.0, &p), "and so are the odds of its terms");
    assert!(wars > 0.0 && odds > 0.0 && odds < 1.0);
    // An ally of the polity stands in its defence: the threat is lower, the odds of its terms
    // higher — while the war it would declare itself has no ally in it.
    for (bond, friend) in [("an ally", s3), ("a lord", s4)] {
        let mut bound = w.clone();
        if friend == s3 {
            bound.ally_now(s0, s3, &p).unwrap();
        } else {
            bound.make_vassal(s0, s4).unwrap();
        }
        let strength = bound.army_strength(friend);
        assert!(strength > 0.0);
        let (mine, theirs) = bound.threat_sides_strength(s0, s1);
        assert!((mine - (sides.0 + strength)).abs() < 1e-9 && theirs == sides.1, "{bond} of the polity is in its defence: {mine} against {theirs}");
        assert_eq!(bound.war_sides_strength(s0, s1), sides, "{bond} stays out of the war the polity declares");
        assert!(threat(&bound).odds(0.0, &p) > odds, "{bond}: the odds of the threat's terms are better");
        assert_eq!(threat(&bound).odds(0.0, &p), World::war_odds_of((mine, theirs), 0.0, &p));
        assert!(bound.wars_a_tick_from(s1, s0, 0.0, &p) < by_its_own_declaration(&bound), "{bond}: fewer of the neighbour's wars fall on the polity than by the sides of its own declaration");
        assert!(bound.wars_a_tick_from(s1, s0, 0.0, &p) < wars, "{bond}: and fewer than without it ({} against {wars})", bound.wars_a_tick_from(s1, s0, 0.0, &p));
        // The shares over the neighbour's borders still sum to its rate: each neighbour's
        // slot is read on the sides of the same declaration as the others' (`war_odds(t, x)`).
        let total: f64 = bound.actors.keys().copied().filter(|x| bound.borders(s1, *x)).map(|x| bound.wars_a_tick_from(s1, x, 0.0, &p)).sum();
        assert!((total - bound.war_rate(s1, &p)).abs() < 1e-12, "{bond}: the wars a tick add up to the rate: {total} vs {}", bound.war_rate(s1, &p));
    }
    // The neighbour's ally stays out of the war the neighbour declares — and enters the one
    // declared on it, the polity's own declaration.
    let mut their_ally = w.clone();
    their_ally.ally_now(s1, s2, &p).unwrap();
    assert!(their_ally.army_strength(s2) > 0.0);
    assert_eq!(their_ally.threat_sides_strength(s0, s1), sides, "the neighbour's ally adds nothing to the neighbour in the threat");
    assert_eq!(threat(&their_ally).odds(0.0, &p), odds);
    assert!(their_ally.war_sides_strength(s0, s1).1 > sides.1, "— it stands against the polity's own declaration");
    // The neighbour's vassal marches with it.
    let mut their_vassal = w.clone();
    their_vassal.make_vassal(s2, s1).unwrap();
    let (mine, theirs) = their_vassal.threat_sides_strength(s0, s1);
    assert!(mine == sides.0 && (theirs - (sides.1 + their_vassal.army_strength(s2))).abs() < 1e-9, "the neighbour's vassal adds its strength: {theirs}");
    assert!(threat(&their_vassal).odds(0.0, &p) < odds, "the odds of the threat's terms are worse");
    // The strength a decision adds goes to the polity's side.
    let more = engine2::World::new_soldiers_strength(w.recruit_per_tick(s0, &p));
    assert!(threat(&w).odds(more, &p) > odds && w.wars_a_tick_from(s1, s0, more, &p) < wars);
}

/// **A polity's own declaration is weighed on the sides of its own declaration** (plan
/// §31.20, the requirement of §31.20.2): the bot reuses the terms it read for the neighbour's
/// threat, and those stand on the sides of the neighbour's declaration — the polity's ally in,
/// the neighbour's out. A declaration weighed on them would count on an ally its own war does
/// not bring. The worth of the declaration is what it was before the rule: the war on the
/// terms of `war_terms` (`war_worth`) and the price of the declaration.
#[test]
fn a_declaration_is_weighed_on_the_sides_of_ones_own_declaration_not_of_the_threat() {
    let p = params();
    let (s0, s1, s2, s3) = (ActorId(0), ActorId(1), ActorId(2), ActorId(3));
    let mut w = world_of(&testworld::five_states_scenario(), 43);
    // Polity 0 has an ally, its neighbour has one too: the two wars differ on both sides.
    w.ally_now(s0, s3, &p).unwrap();
    w.ally_now(s1, s2, &p).unwrap();
    let (threat, own) = (w.threat_sides_strength(s0, s1), w.war_sides_strength(s0, s1));
    assert!(threat.0 > own.0 && threat.1 < own.1, "the threat's sides {threat:?}, the declaration's {own:?}");
    let risk = w.risk_prices(s0, &p);
    let blow = w.ceded_province_blow(s0, &risk, &p);
    let read = w.threats(s0, blow, &p).into_iter().find(|(t, _)| *t == s1).map(|(_, terms)| terms).expect("the neighbour is a threat the context reads");
    let unpaid = w.paid_share_seen_ppm(s0) < 1_000_000;
    let declare = VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::DeclareWar, target: Target::Actor(s1) };
    let expect = w.war_worth(s0, s1, unpaid, 0.0, &p) + w.blow_worth(s0, w.war_price(s0, s1, &p), &risk, &p);
    assert_eq!(w.utility(s0, &declare, &p), expect, "the declaration on its own sides, as before the rule");
    // The same through the bot's menu.
    let offered = w.options(s0, &[VerbId::DeclareWar], &p).into_iter().find(|o| o.call.is_some_and(|c| c.target == Target::Actor(s1) || matches!(c.target, Target::Province(q) if w.provinces[&q].owner == s1))).expect("the declaration is on the menu");
    assert_eq!(offered.utility, expect);
    // On the threat's sides it would be another number: the guard has teeth.
    let on_the_threats_sides = w.war_worth_of(s0, &read, unpaid, 0.0, &p) + w.blow_worth(s0, w.war_price(s0, s1, &p), &risk, &p);
    assert!((on_the_threats_sides - expect).abs() > 1e-9, "the threat's terms would weigh the declaration otherwise: {on_the_threats_sides} vs {expect}");
    assert_eq!(read.on_sides(own), w.war_terms(s0, s1, blow, &p), "the threat's terms on the declaration's sides are the declaration's terms");
}

/// **The sides of a threat read only what is seen** (plan §31.20, §28.7; the guard by
/// injection): alliances, lordship and the armies are public; the hidden state of the polity's
/// ally, of the neighbour and of the neighbour's vassal — goals, the count of the dead, the
/// orders, the treasuries, the debts, the memory of the pay — moves neither the sides nor the
/// threat nor what the polity chooses from.
#[test]
fn the_sides_of_a_threat_read_only_what_is_seen() {
    let p = params();
    let (s0, s1, s2, s3) = (ActorId(0), ActorId(1), ActorId(2), ActorId(3));
    let mut w = world_of(&testworld::five_states_scenario(), 45);
    w.actors.get_mut(&s1).unwrap().wars_entered = 5;
    w.ally_now(s0, s3, &p).unwrap();
    w.make_vassal(s2, s1).unwrap();
    let more = engine2::World::new_soldiers_strength(w.recruit_per_tick(s0, &p));
    let seen = |w: &World| {
        let blow = w.ceded_province_blow(s0, &w.risk_prices(s0, &p), &p);
        (w.threat_sides_strength(s0, s1), w.wars_a_tick_from(s1, s0, 0.0, &p), w.wars_a_tick_from(s1, s0, more, &p), w.threats(s0, blow, &p), w.options(s0, &VerbId::ALL, &p))
    };
    let before = seen(&w);
    assert!(before.0 .0 > w.army_strength(s0) && before.0 .1 > w.army_strength(s1), "the ally and the vassal are in the sides: {:?}", before.0);
    let capital = w.actors[&s0].capital;
    for x in [s1, s2, s3] {
        w.set_goal(x, GoalKind::Take, GoalTarget::Province(capital)).unwrap();
        let actor = w.actors.get_mut(&x).unwrap();
        actor.manpower_deficit = 1.0e6;
        actor.paid_share_history_ppm = vec![1; 4];
        let units: Vec<engine2::UnitId> = w.units.iter().filter(|(_, u)| u.owner == x).map(|(u, _)| *u).collect();
        for u in units {
            w.units.get_mut(&u).unwrap().destination = Some(capital);
        }
        let more = 1000 * w.treasury(x).max(1000) * 1000;
        w.post(engine2::Account::Treasury(x), Source::Test, more).unwrap();
        w.post(engine2::Account::Debt(x), Source::Test, 100 * w.yearly_income(x, &p) * 1000).unwrap();
    }
    assert_eq!(seen(&w), before, "the hidden state of the ally, the neighbour and its vassal moves nothing");
    // What is seen does move them: the ally's army sent home.
    let theirs: Vec<engine2::UnitId> = w.units.iter().filter(|(_, u)| u.owner == s3).map(|(u, _)| *u).collect();
    for u in theirs {
        w.apply_verb(&VerbCall { actor: ActorRef::Ruler(s3), verb: VerbId::Disband, target: Target::Unit(u) }, &p).unwrap();
    }
    assert!(seen(&w).0 .0 < before.0 .0 && seen(&w).1 > before.1, "the ally's army is seen: without it the threat is greater");
}

/// The fall (E3 (г), owner's decision 2 on §29.8, in the polity's currency): when the enemy's
/// chain of conquests covers all the polity's land, the loss is its whole income to the
/// horizon, not the provinces' takes with their discounts; a polity with land beyond the chain
/// loses provinces.
#[test]
fn a_chain_of_conquests_over_all_the_land_is_the_fall_worth_the_whole_income_to_the_horizon() {
    let p = params();
    let (s0, s1, s2) = (ActorId(0), ActorId(1), ActorId(2));
    let mut w = world_of(&testworld::five_states_scenario(), 39);
    let blow = w.ceded_province_blow(s0, &w.risk_prices(s0, &p), &p);
    assert!(!w.war_terms(s0, s1, blow, &p).fall(), "five provinces: more than a war's chain");
    // Polity 0 down to its capital: everything the enemy reaches is all of it.
    for y in 1..5 {
        let q = at(0, y);
        w.provinces.get_mut(&q).unwrap().owner = s2;
        w.actors.get_mut(&s0).unwrap().provinces.retain(|x| *x != q);
        w.actors.get_mut(&s2).unwrap().provinces.push(q);
    }
    let t = w.war_terms(s0, s1, blow, &p);
    assert!(t.fall(), "one province within the enemy's reach: the fall");
    let q = 1.0 - w.war_odds(s0, s1, 0.0, &p);
    let years = f64::from(p.goal_horizon_ticks) / 2.0;
    let worth = w.war_worth_of(s0, &t, false, 0.0, &p);
    assert!(worth <= -q * years * 0.999, "the whole income to the horizon at the odds of the fall: {worth} vs {}", -q * years);
}

/// The rule of the peace read for a war not yet open (E3 (г), owner's decision 1 on §29.10 —
/// derived from `forced_to_peace`, not a case of its own): the side the weaker by the battle
/// formula is forced the moment it has lost a province on balance, so when the enemy's first
/// conquest in its land (`WarTerms::first_blow`) comes no later than its own march and siege
/// to the stake, the rule closes the war before the stake is reached and the war wins nothing
/// — its worth does not move with the stake, as at the threshold of the relation. The same
/// polity with the stronger army counts its stake, and so does the weaker one whose enemy's
/// units reach none of its land first. The loser of a forced peace that declared again the tick
/// after it, with the same odds and the enemy's units a step from its land (59 of 496 on the
/// third stop of (г1)), is read as forced again: its revanche is worth its loss alone.
#[test]
fn a_war_the_rule_of_the_peace_would_close_before_the_stake_is_reached_wins_nothing() {
    let p = params();
    let (s0, s1, s3) = (ActorId(0), ActorId(1), ActorId(3));
    let unit_of = |w: &World, a: ActorId| w.units.iter().find(|(_, u)| u.owner == a).map(|(u, _)| *u).expect("a unit");
    // The worth of the war with the stake and with the stake struck out (`unpaid`: an
    // exhausted army wins nothing, every other term the same).
    let worth = |w: &World, b: ActorId| {
        let blow = w.ceded_province_blow(s0, &w.risk_prices(s0, &p), &p);
        let t = w.war_terms(s0, b, blow, &p);
        let (with, without) = (w.war_worth_of(s0, &t, false, 0.0, &p), w.war_worth_of(s0, &t, true, 0.0, &p));
        (t, with, without)
    };
    // The weaker side against the neighbour whose units stand a step from its land: the
    // stake counts for nothing.
    let mut w = world_of(&testworld::five_states_scenario(), 91);
    w.set_goal(s0, GoalKind::Take, GoalTarget::Province(at(1, 1))).unwrap();
    w.post(engine2::Account::Soldiers(unit_of(&w, s1)), Source::Test, 1200 * 1000).unwrap();
    assert!(w.war_odds(s0, s1, 0.0, &p) < 0.5, "the weaker side");
    let (t, with, without) = worth(&w, s1);
    assert!(t.first_blow() <= t.ticks, "the enemy's first conquest {} comes no later than the march and siege to the stake {}", t.first_blow(), t.ticks);
    assert!((with - without).abs() < 1e-12, "the rule closes the war first: the stake counts for nothing, {with} against {without} with none");
    assert!(with < 0.0, "a war that wins nothing is worth its loss alone: {with}");
    // The same polity the stronger: the other side is the one the rule forces, the stake counts.
    w.post(engine2::Account::Soldiers(unit_of(&w, s0)), Source::Test, 2400 * 1000).unwrap();
    assert!(w.war_odds(s0, s1, 0.0, &p) > 0.5, "the stronger side");
    let (_, with, without) = worth(&w, s1);
    assert!(with > without, "the stronger side counts its stake: {with} against {without} with none");
    // The weaker side whose enemy's units reach none of its land before it reaches the stake
    // (two polities between them and no bay across): the rule cannot close the war against
    // it first, the stake counts.
    let mut w = world_of(&testworld::five_states_scenario(), 92);
    w.set_goal(s0, GoalKind::Take, GoalTarget::Province(at(3, 1))).unwrap();
    w.post(engine2::Account::Soldiers(unit_of(&w, s3)), Source::Test, 1200 * 1000).unwrap();
    assert!(w.war_odds(s0, s3, 0.0, &p) < 0.5);
    let (t, with, without) = worth(&w, s3);
    assert!(t.first_blow() > t.ticks, "the enemy's first conquest {} comes after the march and siege to the stake {}", t.first_blow(), t.ticks);
    assert!(with > without, "the weaker side the rule cannot close first counts its stake: {with} against {without} with none");
}

/// The cold start (E3 (г), owner's decision 3 on §29.8): while a polity's record is shorter than
/// the horizon a bot does not disband below the army it started with — an empty chronicle says
/// nothing, not «no wars»; a levy above it may go, and once the record is as long as the
/// horizon the starting army is a decision like any other.
#[test]
fn while_the_record_is_shorter_than_the_horizon_a_bot_keeps_the_army_it_started_with() {
    let p = params();
    let s0 = ActorId(0);
    let mut w = world_of(&testworld::five_states_scenario(), 41);
    assert!(w.actors[&s0].army_at_start > 0 && w.actors[&s0].army_at_start == w.army_size(s0));
    let disbands = |w: &World| w.options(s0, &[VerbId::Disband], &p).iter().filter(|o| o.call.is_some()).count();
    assert_eq!(disbands(&w), 0, "the starting army stands");
    w.apply_verb(&VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Recruit, target: Target::Levy(at(0, 1), 50) }, &p).unwrap();
    assert_eq!(disbands(&w), 1, "the levy above it may go");
    w.tick.0 = w.actors[&s0].since.0 + p.goal_horizon_ticks;
    assert_eq!(disbands(&w), 2, "the record as long as the horizon: every unit a decision");
}

/// The second duty of an army at peace (E3 (г), owner's decision 1 on §29.9): a revolt ends
/// when any unit of the owner has stood in the province a tick, and a levy takes a tick — so a
/// polity with no unit left values its first levy at the take the revolts withhold, a tick of
/// it for every province in revolt and the hazard's share for every province that can rise; a
/// second unit adds nothing to that; the levy that puts a standing revolt down pays.
#[test]
fn a_unit_in_being_is_worth_the_take_the_risings_withhold_and_the_first_levy_puts_a_revolt_down() {
    let p = params();
    let s0 = ActorId(0);
    let mut w = world_of(&testworld::five_states_scenario(), 43);
    w.tick.0 = w.actors[&s0].since.0 + p.goal_horizon_ticks;
    for u in w.units_of(s0) {
        w.apply_verb(&VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Disband, target: Target::Unit(u) }, &p).unwrap();
    }
    assert_eq!(w.army_size(s0), 0);
    let expected = w.suppression_a_tick(s0, &p);
    assert!(expected > 0.0, "provinces that can rise withhold their take at the hazard");
    let income = w.yearly_income(s0, &p) as f64;
    let a_tick = |w: &World, q: ProvinceId| w.take_of(s0, q, &p) / 2.0 / income;
    let by_hand: f64 = w.actors[&s0].provinces.iter().map(|q| w.revolt_chance_per_tick(*q, &p) * a_tick(&w, *q)).sum();
    assert!((expected - by_hand).abs() < 1e-12, "{expected} vs {by_hand}");
    let levy = |men: u32| VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Recruit, target: Target::Levy(at(0, 1), men) };
    let first = w.utility(s0, &levy(50), &p);
    // Polity 0 has free money: the levy's pay costs it nothing (plan §31.3), the heads do.
    assert!(w.free_money(s0, &p) > 0.0);
    assert!((first - (expected - w.heads_cost(s0, 50, 1.0, &p))).abs() < 1e-9, "the first unit: the take brought back less the heads off the land for a tick, no threat read: {first}");
    // A province in revolt: its whole take a tick is the first levy's; the levy pays.
    w.provinces.get_mut(&at(0, 3)).unwrap().revolt = Some(w.tick);
    assert!(w.in_revolt(at(0, 3)));
    assert!(w.suppression_a_tick(s0, &p) > expected, "a standing revolt withholds its whole take");
    assert!(w.utility(s0, &levy(50), &p) > 0.0, "the levy that puts the revolt down pays");
    // With a unit in being a second adds nothing to the suppression.
    w.apply_verb(&levy(50), &p).unwrap();
    assert!((w.utility(s0, &levy(50), &p) + w.heads_cost(s0, 50, 1.0, &p)).abs() < 1e-9, "a second unit: the heads off the land for nothing more");
    // Without free money the second unit is a tick of pay at the face value for nothing more.
    leave_no_free_money(&mut w, s0, &p);
    assert!((w.utility(s0, &levy(50), &p) + w.soldiers_cost(s0, 50, 1.0, &p)).abs() < 1e-9, "a second unit, no free money: a tick of pay for nothing more");
}

/// **The price of money** (plan §31.3, §31.10; owner's decisions 2026-11-12): a spending
/// within the polity's free money over the horizon costs it nothing, past it the face value
/// (`spending_price`); a spending not made is worth the same price read backwards
/// (`saving_worth`). The free money is the polity's own: its treasury less its debt and what
/// a tick leaves once everything **due** is counted, over the horizon (`free_money`). A
/// standing army is a commitment over the horizon and is weighed against the money of the
/// horizon — a tick of pay against a tick's share of it; a war's pay is finite and stands
/// against the free money whole; a hire's advance is taken out of it first.
#[test]
fn a_spending_within_the_free_money_costs_nothing_and_past_it_the_face_value() {
    let p = params();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let horizon = f64::from(p.goal_horizon_ticks);
    // The price itself.
    assert_eq!(World::spending_price(10.0, 100.0, 2.0), 0.0, "within the free money: nothing");
    assert_eq!(World::spending_price(10.0, 4.0, 2.0), 3.0, "past it: the part it does not cover, in years of income");
    assert_eq!(World::spending_price(10.0, -5.0, 2.0), 5.0, "with none: the face value");
    assert_eq!(World::saving_worth(10.0, 100.0, 2.0), 0.0, "money free anyway: a saving is worth nothing");
    assert_eq!(World::saving_worth(10.0, -3.0, 2.0), 1.5, "short by less than the saving: the shortage it closes");
    assert_eq!(World::saving_worth(10.0, -50.0, 2.0), 5.0, "short by more: the face value");
    // The free money by its parts — what is due, not what was paid.
    let free_by_hand = |w: &World| {
        let b = w.tick_budget(s0, &p);
        let left = b.income - b.tribute_due - b.seats.iter().map(|(_, kg)| kg).sum::<i64>() - b.court - b.interest - b.pay_due;
        (w.treasury(s0) - w.debt(s0)) as f64 + left as f64 * horizon
    };
    let mut w = world_of(&testworld::five_states_scenario(), 51);
    assert!((w.free_money(s0, &p) - free_by_hand(&w)).abs() < 1e-6 && w.free_money(s0, &p) > 0.0);
    // A polity short of its pay: the pay due counts whole, though the treasury cannot meet it.
    let unit = w.units_of(s0)[0];
    w.post(engine2::Account::Soldiers(unit), Source::Test, 20_000 * 1000).unwrap();
    let all = w.treasury(s0);
    w.post(engine2::Account::Treasury(s0), Source::Test, -all * 1000).unwrap();
    let b = w.tick_budget(s0, &p);
    assert!(b.paid < b.pay_due, "the tick cannot pay its army");
    assert!((w.free_money(s0, &p) - free_by_hand(&w)).abs() < 1e-6 && w.free_money(s0, &p) < 0.0, "a polity short of its pay has no free money: what is due is counted, not what was paid");
    // A standing army: a tick of pay against a tick's share of the free money.
    let levy = |w: &World| VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Recruit, target: Target::Province(w.actors[&s0].capital) };
    let mut w = world_of(&testworld::five_states_scenario(), 52);
    let men = w.recruit_per_tick(s0, &p);
    let income = w.yearly_income(s0, &p) as f64;
    let (pay, heads) = (World::soldiers_pay(men, 1.0, &p), w.heads_cost(s0, men, 1.0, &p));
    // A debt that leaves about `left` of the free money: the debt itself and its interest
    // over the horizon come out of it.
    let owe_down_to = |w: &mut World, left: f64| {
        let debt = (w.free_money(s0, &p) - left) / (1.0 + p.loan_interest_ppm_per_tick as f64 / 1e6 * horizon);
        w.post(engine2::Account::Debt(s0), Source::Test, debt as i64 * 1000).unwrap();
    };
    // Free money for about half the levy's pay over the horizon.
    owe_down_to(&mut w, pay * horizon / 2.0);
    let free = w.free_money(s0, &p);
    assert!(free > pay && free < pay * horizon, "the free money covers many ticks of the levy's pay and not the horizon of it: {free} against {pay} a tick");
    let price = World::spending_price(pay, free / horizon, income);
    assert!(price > 0.0 && price < pay / income, "so the tick of pay is priced in part: {price}");
    assert!((w.utility(s0, &levy(&w), &p) + price + heads).abs() < 1e-9, "no threat: the levy costs the part of its pay the free money does not cover over the horizon, and the heads");
    // A hire at peace: the advance out of the free money first, the pay against what it leaves.
    let hire = VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::HireMercenaries, target: Target::None };
    let hired = |w: &World| (w.hire_count(s0, &p), w.hire_advance(s0, &p) as f64);
    let hire_cost = |w: &World| {
        let (n, advance) = hired(w);
        let free = w.free_money(s0, &p);
        World::spending_price(advance, free, income) + World::spending_price(World::soldiers_pay(n, 1.0, &p), (free - advance) / horizon, income) + w.heads_cost(s0, n, 1.0, &p)
    };
    let mut w = world_of(&testworld::five_states_scenario(), 53);
    let (n, advance) = hired(&w);
    assert!(n > 0 && w.free_money(s0, &p) > advance + World::soldiers_pay(n, 1.0, &p) * horizon);
    assert!((w.utility(s0, &hire, &p) + w.heads_cost(s0, n, 1.0, &p)).abs() < 1e-9, "a hire within the free money: the heads only");
    owe_down_to(&mut w, advance / 2.0);
    let free = w.free_money(s0, &p);
    assert!(free > 0.0 && free < advance, "free money for a part of the advance: {free} of {advance}");
    let expected = (advance - free) / income + World::soldiers_pay(n, 1.0, &p) / income + w.heads_cost(s0, n, 1.0, &p);
    assert!((hire_cost(&w) - expected).abs() < 1e-9 && (w.utility(s0, &hire, &p) + expected).abs() < 1e-9, "the advance takes the free money first; nothing is left for the pay, at its face value");
    // At war the pay is finite — the war's length — against the free money whole: the polity
    // with free money levies for nothing but the heads, the one with none at the face value.
    let at_war = |poor: bool| {
        let mut w = world_of(&testworld::five_states_scenario(), 54);
        if poor {
            leave_no_free_money(&mut w, s0, &p);
        }
        w.declare_war(s1, s0).unwrap();
        w
    };
    let (rich, poor) = (at_war(false), at_war(true));
    let ticks = rich.war_ticks(s0, s1, &p);
    let war_pay = World::soldiers_pay(men, ticks, &p);
    assert!(ticks > 1.0 && war_pay <= rich.free_money(s0, &p), "the pay of the war's length is within the free money");
    assert!((rich.utility(s0, &levy(&rich), &p) - poor.utility(s0, &levy(&poor), &p) - war_pay / income).abs() < 1e-9, "at war the levy of the polity with free money is worth the war's pay more");
}

/// **A spending over a span is weighed against the money of the same span** (E3 (ц1), step
/// III; plan §31.10, owner's decisions 2026-11-12). The money over a span (`money_over`) is
/// the lesser of what the polity can afford over the horizon (`free_money`) and what it will
/// have at hand within the span — its treasury, what the span's ticks leave by what is due,
/// and its room to borrow. Over the horizon that is the free money itself. Over a war's
/// length a polity with an empty treasury and no credit has only what the war's own ticks
/// leave, though it could afford more over the horizon; a treasury, or a bank's room, is
/// money at hand. An advance at peace stands against the money of a tick — and since a hire
/// is bounded by the treasury (`hire_count`), that prices every hire there is as the free
/// money does.
///
/// Since (ц1б) (plan §31.11) the money over a span prices the spendings that end — the pay
/// of a campaign (`a_bot_puts_a_revolt_down_…`), a hire's advance. A soldier's pay does not
/// end with the war and is priced against the free money of the horizon
/// (`a_soldiers_pay_stands_for_the_horizon_at_war_as_at_peace`): the levy of this test,
/// priced against the money of the war's length under step III, is read here by the money
/// alone.
#[test]
fn a_spending_over_a_span_is_weighed_against_the_money_of_the_same_span() {
    let p = params();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let horizon = f64::from(p.goal_horizon_ticks);
    let left = |w: &World| {
        let b = w.tick_budget(s0, &p);
        (b.income - b.tribute_due - b.seats.iter().map(|(_, kg)| kg).sum::<i64>() - b.court - b.interest - b.pay_due) as f64
    };
    let by_hand = |w: &World, span: f64| {
        let afford = (w.treasury(s0) - w.debt(s0)) as f64 + left(w) * horizon;
        let at_hand = w.treasury(s0) as f64 + left(w) * span + w.loan_room(s0, &p) as f64;
        afford.min(at_hand)
    };
    // The money by its parts — with no debt and no bank, then with both.
    let mut w = world_of(&testworld::five_states_scenario(), 55);
    assert!(w.debt(s0) == 0 && w.loan_room(s0, &p) == 0 && left(&w) > 0.0);
    for span in [1.0, 6.0, horizon] {
        assert!((w.money_over(s0, span, &p) - by_hand(&w, span)).abs() < 1e-6, "over {span} ticks");
    }
    assert!(w.money_over(s0, 1.0, &p) < w.money_over(s0, 6.0, &p) && w.money_over(s0, 6.0, &p) < w.free_money(s0, &p), "a longer span has more of the ticks' remainder at hand, up to what the horizon affords");
    assert_eq!(w.money_over(s0, horizon, &p), w.free_money(s0, &p), "over the horizon: the free money itself");
    w.actors.get_mut(&s0).unwrap().institutions.insert(engine2::Institution::Bank);
    w.post(engine2::Account::Debt(s0), Source::Test, w.yearly_income(s0, &p) / 2 * 1000).unwrap();
    assert!(w.debt(s0) > 0 && w.loan_room(s0, &p) > 0);
    for span in [1.0, 6.0, horizon] {
        assert!((w.money_over(s0, span, &p) - by_hand(&w, span)).abs() < 1e-6, "with a debt and a bank, over {span} ticks");
    }
    assert_eq!(w.money_over(s0, horizon, &p), w.free_money(s0, &p), "with a debt and the room to borrow the horizon's money is still the free money: the second lacks the debt taken off and has the room added");
    // Over a war's length: an empty treasury and no credit, the treasury, a bank's room. The
    // army here is large enough that a tick leaves less than a levy's tick of pay.
    let at_war = |treasury: bool, bank: bool| {
        let mut w = world_of(&testworld::five_states_scenario(), 54);
        w.declare_war(s1, s0).unwrap();
        let unit = w.units_of(s0)[0];
        w.post(engine2::Account::Soldiers(unit), Source::Test, 400 * 1000).unwrap();
        let ticks = w.war_ticks(s0, s1, &p);
        let pay = World::soldiers_pay(w.recruit_per_tick(s0, &p), ticks, &p);
        let all = w.treasury(s0);
        w.post(engine2::Account::Treasury(s0), Source::Test, -all * 1000).unwrap();
        if treasury {
            w.post(engine2::Account::Treasury(s0), Source::Test, pay.ceil() as i64 * 1000).unwrap();
        }
        if bank {
            w.actors.get_mut(&s0).unwrap().institutions.insert(engine2::Institution::Bank);
        }
        w
    };
    let (rich, empty, credit) = (at_war(true, false), at_war(false, false), at_war(false, true));
    let ticks = empty.war_ticks(s0, s1, &p);
    let pay = World::soldiers_pay(empty.recruit_per_tick(s0, &p), ticks, &p);
    assert!(ticks > 1.0 && pay > 0.0 && empty.treasury(s0) == 0 && empty.loan_room(s0, &p) == 0);
    let at_hand = left(&empty) * ticks;
    assert!((empty.money_over(s0, ticks, &p) - at_hand).abs() < 1e-6 && at_hand > 0.0 && at_hand < pay, "an empty treasury and no credit: over the war's length only what its ticks leave, less than a levy's pay for it: {at_hand} against {pay}");
    assert!(empty.free_money(s0, &p) >= pay, "though over the horizon the polity affords that much whole");
    assert!((rich.money_over(s0, ticks, &p) - at_hand - pay.ceil()).abs() < 1e-6, "the treasury is money at hand");
    // The room to borrow is money at hand too: the world lends by itself for the pay a
    // treasury is short of.
    assert!(credit.treasury(s0) == 0 && credit.loan_room(s0, &p) > 0);
    assert!((credit.money_over(s0, ticks, &p) - (at_hand + credit.loan_room(s0, &p) as f64).min(credit.free_money(s0, &p))).abs() < 1e-6 && credit.money_over(s0, ticks, &p) > at_hand, "with a bank's room there is more at hand, up to what the horizon affords");
    // An advance at peace stands against the money of a tick. A hire is bounded by the
    // treasury, so the advance is within the treasury whenever there is a hire: the money of
    // a tick prices it as the free money does — in plenty, in debt and with a tick in deficit.
    let advance_price = |w: &World, money: f64| World::spending_price(w.hire_advance(s0, &p) as f64, money, w.yearly_income(s0, &p) as f64);
    let plenty = world_of(&testworld::five_states_scenario(), 53);
    let mut indebted = world_of(&testworld::five_states_scenario(), 53);
    let owed = (indebted.free_money(s0, &p) - indebted.hire_advance(s0, &p) as f64 / 2.0) / (1.0 + p.loan_interest_ppm_per_tick as f64 / 1e6 * horizon);
    indebted.post(engine2::Account::Debt(s0), Source::Test, owed as i64 * 1000).unwrap();
    let mut deficit = world_of(&testworld::five_states_scenario(), 53);
    let unit = deficit.units_of(s0)[0];
    deficit.post(engine2::Account::Soldiers(unit), Source::Test, 1000 * 1000).unwrap();
    assert!(left(&deficit) < 0.0, "a tick in deficit");
    let mut priced = 0;
    for (name, w) in [("plenty", &plenty), ("indebted", &indebted), ("deficit", &deficit)] {
        assert!(w.hire_count(s0, &p) > 0 && w.hire_advance(s0, &p) <= w.treasury(s0), "{name}: a hire, its advance within the treasury");
        assert!((advance_price(w, w.money_over(s0, 1.0, &p)) - advance_price(w, w.free_money(s0, &p))).abs() < 1e-12, "{name}: the advance against the money of a tick and against the free money");
        priced += u8::from(advance_price(w, w.money_over(s0, 1.0, &p)) > 0.0);
    }
    assert!(advance_price(&plenty, plenty.money_over(s0, 1.0, &p)) == 0.0 && priced >= 1, "free in plenty, priced where the horizon does not afford it");
}

/// **A soldier's pay is a standing spending, at war as at peace** (E3 (ц1б); plan §31.11,
/// owner's decision 2026-11-12: the span of a spending is by its nature, not by whether a war
/// is on). A soldier is paid as long as he is under arms, so a tick of his pay stands against
/// a tick's share of the free money of the horizon, `M / H`, in war too: a hire the polity
/// cannot keep over the horizon costs the part of the pay its free money does not cover, though
/// the money over the war's length covers advance and pay whole; a hire within its means costs
/// nothing but the heads — and, made, leaves the free money above nothing, so the pay saved
/// by sending the hired men home the next tick is worth nothing. The pay of a campaign ends
/// with the march and is no part of the free money of the horizon: a polity with its unit on
/// campaign has the free money it has with the unit at home, and less at hand over the war.
/// And it is priced once: against the money with the pay at the rate of home, so that a war
/// is worth the same to a polity whose unit already marches and to one whose unit is at home.
///
/// Under step III of (ц1) the hire stood against the money of the war's length, was within
/// it, and left the free money of the horizon below nothing — the hired men stand in every
/// tick's budget for the horizon; they were sent home the tick after (plan §31.10, (б)).
#[test]
fn a_soldiers_pay_stands_for_the_horizon_at_war_as_at_peace() {
    let p = params();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let horizon = f64::from(p.goal_horizon_ticks);
    let hire = VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::HireMercenaries, target: Target::None };
    // Polity 0 attacked, its treasury the advance of all the men it may hire and `beyond` more.
    let at_war = |beyond: i64| {
        let mut w = world_of(&testworld::five_states_scenario(), 54);
        w.declare_war(s1, s0).unwrap();
        let room = w.recruit_ceiling(s0, &p) - w.army_size(s0);
        let treasury = room * p.mercenary_advance_kg + beyond;
        w.post(engine2::Account::Treasury(s0), Source::Test, (treasury - w.treasury(s0)) * 1000).unwrap();
        w
    };
    // The same world with no free money: every spending at its face value.
    let at_face = |w: &World, call: &VerbCall| {
        let mut w = w.clone();
        leave_no_free_money(&mut w, s0, &p);
        w.utility(s0, call, &p)
    };
    let bare = at_war(0);
    let (n, advance) = (bare.hire_count(s0, &p), bare.hire_advance(s0, &p) as f64);
    let ticks = bare.war_ticks(s0, s1, &p);
    let income = bare.yearly_income(s0, &p) as f64;
    let pay = World::soldiers_pay(n, 1.0, &p);
    // With the advance paid the treasury is empty: the free money is what the ticks leave.
    let left = (bare.free_money(s0, &p) - advance) / horizon;
    assert!(n > 0 && ticks > 1.0 && 2.0 * ticks < horizon && left > 0.0 && pay > left, "the hired men's tick of pay is more than a tick leaves: {pay} against {left}");
    // A war chest — the treasury covers what the ticks are short of for twice the war's
    // length, not for the horizon — and a treasury that keeps the hired men for the horizon.
    let short = pay - left;
    let (chest, kept) = (at_war((short * ticks * 2.0) as i64), at_war((short * horizon).ceil() as i64));
    for w in [&chest, &kept] {
        assert!(w.hire_count(s0, &p) == n && w.hire_advance(s0, &p) as f64 == advance, "the same hire");
        assert!(advance + pay * ticks <= w.money_over(s0, ticks, &p), "the advance and the pay of the war's length are within the money over the war's length");
    }
    assert!(chest.free_money(s0, &p) - advance < pay * horizon, "the war chest does not keep the hired men over the horizon");
    assert!(kept.free_money(s0, &p) - advance >= pay * horizon, "the other treasury does");
    // Within its means the hire costs nothing but the heads: its advance and its pay for the
    // war's length less than at the face value.
    assert!((kept.utility(s0, &hire, &p) - at_face(&kept, &hire) - (advance + pay * ticks) / income).abs() < 1e-9, "a hire within the polity's means is free: {} against {} at the face value", kept.utility(s0, &hire, &p), at_face(&kept, &hire));
    // With the war chest (plan §31.15) the hire is free as well: its men within the army the
    // peace keeps stand within their share of the free money for the war's length, and its
    // soldiers of war beyond it within the money at hand over the war's length. Under (ц1б),
    // all of them read as standing for the horizon, it cost the part the free money did not
    // cover.
    let army = chest.paid_soldiers(s0, &p);
    let split = |w: &World| {
        let room = (w.peace_army(s0, &p) - w.paid_soldiers(s0, &p)).max(0);
        (n.min(room), n - n.min(room))
    };
    let (peace, war) = split(&chest);
    assert!(peace > 0 && war > 0 && army < chest.peace_army(s0, &p), "the hire crosses the army the peace keeps: {peace} of the peace, {war} of the war");
    assert!((kept.utility(s0, &hire, &p) - chest.utility(s0, &hire, &p)).abs() < 1e-9, "with the war chest the hire is free: {} against {}", chest.utility(s0, &hire, &p), kept.utility(s0, &hire, &p));
    // Without it — the bare treasury of the advance — each part is priced by its own money:
    // the men of the peace by the part of their pay a tick's share of the free money does not
    // cover, the soldiers of war by the part the money at hand over the war's length, less the
    // advance and what the men of the peace commit over the horizon, does not cover.
    let (peace, war) = split(&bare);
    let peace_price = (World::soldiers_pay(peace, ticks, &p) - (bare.free_money(s0, &p) - advance) * ticks / horizon).max(0.0) / income;
    let war_money = bare.money_over(s0, ticks, &p).min(bare.free_money(s0, &p) - World::soldiers_pay(peace, horizon, &p)) - advance;
    let war_price = (World::soldiers_pay(war, ticks, &p) - war_money.max(0.0)).max(0.0) / income;
    assert!(war_price > 0.0, "the bare treasury does not cover the soldiers of war: {war_price}");
    assert!((kept.utility(s0, &hire, &p) - bare.utility(s0, &hire, &p) - peace_price - war_price).abs() < 1e-9, "the bare treasury pays {peace_price} for the men of the peace and {war_price} for the soldiers of war: {} against {}", bare.utility(s0, &hire, &p), kept.utility(s0, &hire, &p));
    // The hire made. Within the polity's means it leaves the free money above nothing, and
    // the pay saved by sending the hired men home is worth nothing: the disbanding is worth
    // that pay less than at the face value, and is not worth making.
    let made = |w: &World| {
        let mut after = w.clone();
        after.apply_verb(&hire, &p).unwrap();
        let unit = *after.units_of(s0).iter().find(|u| !w.units.contains_key(u)).unwrap();
        (after, VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Disband, target: Target::Unit(unit) })
    };
    let (after, send_home) = made(&kept);
    assert!(after.free_money(s0, &p) >= 0.0, "the hire within its means leaves the free money above nothing: {}", after.free_money(s0, &p));
    assert!((at_face(&after, &send_home) - after.utility(s0, &send_home, &p) - pay * ticks / income).abs() < 1e-9, "the pay saved by sending the hired men home is worth nothing: {} against {} at the face value", after.utility(s0, &send_home, &p), at_face(&after, &send_home));
    assert!(after.utility(s0, &send_home, &p) < 0.0, "and the disbanding is not worth making the tick after: {}", after.utility(s0, &send_home, &p));
    // With the chest the hire leaves the free money above nothing too: its soldiers of war
    // are counted for the war's length (plan §31.15). Beyond its means — the bare treasury —
    // it leaves none: the price it paid.
    let (after, _) = made(&chest);
    assert!(after.free_money(s0, &p) >= 0.0, "the hire within the war chest leaves the free money above nothing: {}", after.free_money(s0, &p));
    let (after, _) = made(&bare);
    assert!(after.free_money(s0, &p) < 0.0, "the hire beyond its means leaves no free money");
    // The pay of the campaign is no part of the free money of the horizon: with the unit
    // ordered onto the enemy's land the polity has the free money it had, and at hand over
    // the war's length less by what the campaign adds to the pay.
    let mut marching = kept.clone();
    let unit = marching.units_of(s0)[0];
    marching.units.get_mut(&unit).unwrap().destination = Some(marching.actors[&s1].capital);
    let budget = marching.tick_budget(s0, &p);
    assert!(marching.on_campaign(unit) && !kept.on_campaign(unit) && budget.pay_due > budget.pay_home && budget.pay_home == kept.tick_budget(s0, &p).pay_due, "the unit on campaign is charged the campaign's pay");
    assert_eq!(marching.free_money(s0, &p), kept.free_money(s0, &p), "the free money of the horizon counts the pay at the rate of home");
    assert!((kept.money_over(s0, ticks, &p) - marching.money_over(s0, ticks, &p) - (budget.pay_due - budget.pay_home) as f64 * ticks).abs() < 1e-6, "the money at hand over the war's length is less by the campaign's pay for it");
    // The pay of a campaign is priced once (owner's rule: no spending is counted twice; the
    // correction of (ц1б), plan §31.12). A war polity 0 wants, its campaign's pay past the
    // money of the war's length: with the unit already marching the campaign's pay is in what
    // the ticks are charged, and were the money it is priced against read off those ticks,
    // the same pay would be taken off twice. It is priced against the money with the pay at
    // the rate of home (`campaign_money`): marching or at home, the same war is worth the same.
    let mut home = world_of(&testworld::five_states_scenario(), 63);
    home.set_goal(s0, GoalKind::Take, GoalTarget::Province(at(1, 2))).unwrap();
    home.declare_war(s0, s1).unwrap();
    home.rise_now(at(0, 2), &p).unwrap();
    let mut marching = home.clone();
    let unit = marching.units_of(s0)[0];
    marching.units.get_mut(&unit).unwrap().destination = Some(at(1, 2));
    let ticks = home.war_ticks(s0, s1, &p);
    let campaign = home.campaign_extra_per_strength(s0, ticks, &p) * home.army_strength(s0) * home.yearly_income(s0, &p) as f64;
    assert!(marching.on_campaign(unit) && !home.on_campaign(unit) && marching.war_ticks(s0, s1, &p) == ticks);
    assert!(home.campaign_money(s0, ticks, &p) > 0.0 && home.campaign_money(s0, ticks, &p) < campaign, "the campaign's pay is priced in part: past the money of the war's length");
    assert!(marching.money_over(s0, ticks, &p) < home.money_over(s0, ticks, &p), "marching, the polity has less at hand: the world charges the campaign's pay");
    assert_eq!(marching.campaign_money(s0, ticks, &p), home.campaign_money(s0, ticks, &p), "the money the campaign is priced against is the same");
    assert_eq!(home.campaign_money(s0, ticks, &p), home.money_over(s0, ticks, &p), "and with every unit at home it is the money over the span itself");
    assert_eq!(marching.war_worth(s0, s1, false, 0.0, &p), home.war_worth(s0, s1, false, 0.0, &p), "the same war is worth the same with the unit marching and at home");
}

/// **An embassy, an alliance's gifts and a bribe are spendings made once; the order bought
/// back is read at its face value** (E3 (ц2); plan §31.13, owner's decisions 2026-11-12 and
/// 2026-11-13). The three diplomatic spendings stand, as a hire's advance, against the money at
/// hand for a spending made once (`money_over` over a tick at peace): within it they cost
/// nothing, past it the face value. The price of the order is no spending from a source but the
/// world's rule against turning money into legitimacy (`order.cost_share_of_revenue`, derived
/// by the guard `prices_leave_no_arbitrage`): the bot reads it as it is, whatever its money —
/// at the first measurement of (ц2), with the order free within the money, the bots bought it
/// every twenty ticks, at two thirds of the treasury a time, and the armies fell.
#[test]
fn an_embassy_a_gift_and_an_alliance_cost_what_the_money_at_hand_does_not_cover_and_the_order_its_face_value() {
    let p = params();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let call = |verb: VerbId, target: Target| VerbCall { actor: ActorRef::Ruler(s0), verb, target };
    // A treasury that covers the dearest of the four, the order, twice over.
    let mut rich = world_of(&testworld::five_states_scenario(), 71);
    let more = 2 * rich.order_cost(s0, &p);
    rich.post(engine2::Account::Treasury(s0), Source::Test, more * 1000).unwrap();
    let mut poor = rich.clone();
    leave_no_free_money(&mut poor, s0, &p);
    let income = rich.yearly_income(s0, &p) as f64;
    assert_eq!(poor.yearly_income(s0, &p) as f64, income, "the same take");
    let (embassy, gift, order) = (rich.embassy_cost(s0, &p) as f64, rich.gift_size(s0, &p) as f64, rich.order_cost(s0, &p) as f64);
    assert!(embassy > 0.0 && gift > 0.0 && order > 0.0 && rich.at_war(s0, s1) == false);
    assert!(embassy.max(gift).max(order) <= rich.money_over(s0, 1.0, &p), "with money at hand for each: {} of the money at hand of a tick", rich.money_over(s0, 1.0, &p));
    assert!(poor.money_over(s0, 1.0, &p) < 0.0, "and none");
    // What each call is worth before its price, read off the same state.
    let push = |w: &World, kg: f64| w.relation_push_worth(s0, s1, kg, &p);
    let embassy_push = |w: &World| push(w, p.embassy_gain);
    let gift_push = |w: &World| push(w, w.gift_push(s0, s1, &p));
    let order_gain = |w: &World| w.blow_worth(s0, -p.order_gain, &w.risk_prices(s0, &p), &p);
    for (name, w, money) in [("with money", &rich, true), ("without", &poor, false)] {
        let price = |kg: f64| if money { 0.0 } else { kg / income };
        let send = w.utility(s0, &call(VerbId::SendEmbassy, Target::Actor(s1)), &p);
        assert!((send - (embassy_push(w) - price(embassy))).abs() < 1e-9, "{name}: the embassy costs {} — {send}", price(embassy));
        let bribe = w.utility(s0, &call(VerbId::Bribe, Target::Actor(s1)), &p);
        assert!((bribe - (gift_push(w) - price(gift))).abs() < 1e-9, "{name}: the gift costs {} — {bribe}", price(gift));
        let ally = w.utility(s0, &call(VerbId::ProposeAlliance, Target::Actor(s1)), &p);
        assert!((ally - (w.alliance_worth(s0, s1, &p) - price(embassy))).abs() < 1e-9, "{name}: the alliance's gifts cost {} — {ally}", price(embassy));
        // The order: its face value either way.
        let enforce = w.utility(s0, &call(VerbId::EnforceOrder, Target::None), &p);
        assert!((enforce - (order_gain(w) - order / income)).abs() < 1e-9, "{name}: the order costs its face value {} — {enforce}", order / income);
    }
    // Past the money at hand — though within the free money of the horizon — each costs the
    // part the money at hand does not cover: a spending made once stands against the money of
    // a tick, not of the horizon.
    let mut short = world_of(&testworld::five_states_scenario(), 71);
    let at_hand = short.embassy_cost(s0, &p).min(short.gift_size(s0, &p)) / 2;
    leave_money_at_hand(&mut short, s0, at_hand, &p);
    let (embassy, gift) = (short.embassy_cost(s0, &p) as f64, short.gift_size(s0, &p) as f64);
    let income = short.yearly_income(s0, &p) as f64;
    let part = |kg: f64| (kg - at_hand as f64) / income;
    assert!(short.free_money(s0, &p) > embassy.max(gift), "the free money of the horizon covers both");
    let send = short.utility(s0, &call(VerbId::SendEmbassy, Target::Actor(s1)), &p);
    assert!((send - (embassy_push(&short) - part(embassy))).abs() < 1e-9, "past the money at hand the embassy costs the part it does not cover — {send}");
    let bribe = short.utility(s0, &call(VerbId::Bribe, Target::Actor(s1)), &p);
    assert!((bribe - (gift_push(&short) - part(gift))).abs() < 1e-9, "and so the gift — {bribe}");
    // The order's price is read whole even with the treasury many times over it.
    let mut flush = rich.clone();
    flush.post(engine2::Account::Treasury(s0), Source::Test, (order * 100.0) as i64 * 1000).unwrap();
    let enforce = flush.utility(s0, &call(VerbId::EnforceOrder, Target::None), &p);
    assert!((enforce - (order_gain(&flush) - order / income)).abs() < 1e-9 && order / income >= 1.0, "a hundred orders in the treasury: the order still costs its face value");
}

/// **The war chest: a soldier beyond the army the peace keeps is a soldier of the war**
/// (plan §31.15, owner's decisions 2026-11-13). The army the peace keeps (`peace_army`) is the
/// soldiers the tick's remainder pays without the treasury — the take less the tribute due,
/// the seats, the court and the interest, over a soldier's pay at the rate of home. At war the
/// men beyond it are a spending of the war: their pay for the war's length stands against the
/// money at hand over that length, and the free money of the horizon counts them for the
/// war's length too — one reading. So a polity with a chest for the war levies them for
/// nothing but the heads, one without at the face value; the levy made leaves the free money
/// above nothing and sending the men home the tick after saves nothing. At peace there are no
/// soldiers of war: the same men stand for the horizon (`M / H`) and the bot sends them home
/// when the peace does not keep them.
#[test]
fn a_soldier_beyond_the_army_the_peace_keeps_is_priced_against_the_money_of_the_war() {
    let p = params();
    let (s0, s1) = (ActorId(0), ActorId(1));
    let horizon = f64::from(p.goal_horizon_ticks);
    // The levy in a province with no unit of polity 0: it raises a unit of its own.
    let levy = |w: &World| {
        let held: Vec<ProvinceId> = w.units_of(s0).iter().map(|u| w.units[u].position).collect();
        let q = *w.actors[&s0].provinces.iter().find(|q| !held.contains(q) && !w.in_revolt(**q) && w.population(**q) > 0).unwrap();
        VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Recruit, target: Target::Province(q) }
    };
    // The army the peace keeps, by the parts of the budget.
    let base = world_of(&testworld::five_states_scenario(), 54);
    let b = base.tick_budget(s0, &p);
    let before_pay = b.income - b.tribute_due - b.seats.iter().map(|(_, kg)| kg).sum::<i64>() - b.court - b.interest;
    assert_eq!(base.peace_army(s0, &p), before_pay.max(0) / engine2::economy::pay_per_tick(1, &p), "the army the peace keeps: the tick's remainder before the pay over a soldier's pay");
    assert!(base.paid_soldiers(s0, &p) < base.peace_army(s0, &p));
    // Polity 0 with its army at the army the peace keeps; at peace and attacked; a treasury of
    // `treasury` and no bank.
    let world = |war: bool, treasury: i64| {
        let mut w = world_of(&testworld::five_states_scenario(), 54);
        let unit = w.units_of(s0)[0];
        let more = w.peace_army(s0, &p) - w.paid_soldiers(s0, &p);
        w.post(engine2::Account::Soldiers(unit), Source::Test, more * 1000).unwrap();
        if war {
            w.declare_war(s1, s0).unwrap();
        }
        let all = w.treasury(s0);
        w.post(engine2::Account::Treasury(s0), Source::Test, (treasury - all) * 1000).unwrap();
        w
    };
    let at_face = |w: &World, call: &VerbCall| {
        let mut w = w.clone();
        leave_no_free_money(&mut w, s0, &p);
        w.utility(s0, call, &p)
    };
    let probe = world(true, 0);
    assert_eq!(probe.paid_soldiers(s0, &p), probe.peace_army(s0, &p), "the army at the army the peace keeps");
    let (n, ticks) = (probe.recruit_per_tick(s0, &p), probe.war_ticks(s0, s1, &p));
    let income = probe.yearly_income(s0, &p) as f64;
    let war_pay = World::soldiers_pay(n, ticks, &p);
    assert!(n > 0 && ticks > 1.0);
    // A chest for the war — the pay of the levy for twice the war's length — and none.
    let (chest, empty) = (world(true, (2.0 * war_pay) as i64), world(true, 0));
    assert!(chest.money_over(s0, ticks, &p) >= war_pay && empty.money_over(s0, ticks, &p) <= 0.0, "money at hand over the war's length: {} and {}", chest.money_over(s0, ticks, &p), empty.money_over(s0, ticks, &p));
    assert!((chest.utility(s0, &levy(&chest), &p) - at_face(&chest, &levy(&chest)) - war_pay / income).abs() < 1e-9, "with a chest for the war the levy beyond the army the peace keeps costs nothing but the heads");
    assert!((chest.utility(s0, &levy(&chest), &p) - empty.utility(s0, &levy(&empty), &p) - war_pay / income).abs() < 1e-9, "without one it costs its pay for the war's length at the face value: {} against {}", empty.utility(s0, &levy(&empty), &p), chest.utility(s0, &levy(&chest), &p));
    // The levy made: the free money stays above nothing, and the men sent home the tick after
    // save nothing — the disbanding is worth their pay less than at the face value, below nothing.
    let mut after = chest.clone();
    after.apply_verb(&levy(&chest), &p).unwrap();
    let unit = *after.units_of(s0).iter().find(|u| !chest.units.contains_key(u)).unwrap();
    let send_home = VerbCall { actor: ActorRef::Ruler(s0), verb: VerbId::Disband, target: Target::Unit(unit) };
    assert!(after.paid_soldiers(s0, &p) > after.peace_army(s0, &p), "the levy made soldiers of war");
    assert!(after.free_money(s0, &p) >= 0.0, "the levy for the war leaves the free money above nothing: {}", after.free_money(s0, &p));
    // Worth nothing: more money changes nothing of what sending them home is worth.
    let mut richer = after.clone();
    richer.post(engine2::Account::Treasury(s0), Source::Test, (10.0 * war_pay) as i64 * 1000).unwrap();
    assert!(after.paid_soldiers(s0, &p) - after.peace_army(s0, &p) >= after.soldiers(unit), "the men sent home are all soldiers of war");
    assert!((after.utility(s0, &send_home, &p) - richer.utility(s0, &send_home, &p)).abs() < 1e-9, "the pay the soldiers of war would save is worth nothing: {} against {} with ten times the chest", after.utility(s0, &send_home, &p), richer.utility(s0, &send_home, &p));
    assert!(after.utility(s0, &send_home, &p) < 0.0, "and sending them home the tick after is not worth it: {}", after.utility(s0, &send_home, &p));
    // One reading: at war the free money counts the soldiers of war for the war's length; at
    // peace the same men stand for the horizon.
    let parts = |w: &World| {
        let b = w.tick_budget(s0, &p);
        let before_pay = b.income - b.tribute_due - b.seats.iter().map(|(_, kg)| kg).sum::<i64>() - b.court - b.interest;
        ((w.treasury(s0) - w.debt(s0)) as f64, (before_pay - b.pay_home) as f64)
    };
    let war_men = after.paid_soldiers(s0, &p) - after.peace_army(s0, &p);
    let (own, left) = parts(&after);
    let war_tick = engine2::economy::pay_per_tick(war_men, &p) as f64;
    assert!((after.free_money(s0, &p) - (own + (left + war_tick) * horizon - war_tick * ticks)).abs() < 1e-6, "at war: the soldiers of war for the war's length");
    let mut peace = world(false, chest.treasury(s0));
    peace.apply_verb(&levy(&peace), &p).unwrap();
    let (own, left) = parts(&peace);
    assert!(peace.paid_soldiers(s0, &p) > peace.peace_army(s0, &p) && (peace.free_money(s0, &p) - (own + left * horizon)).abs() < 1e-6, "at peace: every man for the horizon");
}

/// **A player makes one call a tick, as a bot** (B42 — the player and the bot by one rule;
/// parity (а), owner's decision 2026-11-13 on the stop of plan §31.18). Two calls of one
/// caller in a tick are an error before the world is touched — the save byte for byte the
/// same; one call goes through.
#[test]
fn a_player_makes_one_call_a_tick_as_a_bot_does() {
    let s = all_verbs(testworld::five_states_scenario());
    let mut g = Game::new(&s, params(), 5).unwrap();
    let me = ActorRef::Ruler(ActorId(0));
    g.set_player(Some(me));
    let embassy = VerbCall { actor: me, verb: VerbId::SendEmbassy, target: Target::Actor(ActorId(1)) };
    let levy = VerbCall { actor: me, verb: VerbId::Recruit, target: Target::Province(g.world().actors[&ActorId(0)].capital) };
    let before = g.to_save().unwrap();
    let err = g.tick(&[embassy, levy]).unwrap_err();
    assert!(matches!(err, engine2::sim::SimError::OneCallATick(a) if a == me), "{err}");
    assert_eq!(g.to_save().unwrap(), before, "refused before the world is touched");
    let err = g.tick(&[embassy, embassy]).unwrap_err();
    assert!(matches!(err, engine2::sim::SimError::OneCallATick(_)), "the same call twice too: {err}");
    let r = g.tick(&[levy]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Verb { actor, verb: VerbId::Recruit, .. } if actor == me)), "one call goes through");
}

// ---- Diagnostics: why the armies are small (plan §30.13) ----
//
// Printed, never asserted, and run on demand (`#[ignore]`): the world is not changed, no
// parameter is fitted (rules 1 and 2 of §24.1). In the commit so that the numbers of §30.13
// can be read again:
// `cargo test --release -p engine2 --all-features --test agents diagnosis -- --ignored --nocapture --test-threads=1`.

/// What the diagnosis counts, sums and samples, by name; folded in the order of the seeds.
#[derive(Default)]
struct Diagnosis {
    counts: BTreeMap<String, u64>,
    sums: BTreeMap<String, f64>,
    samples: BTreeMap<String, Vec<f64>>,
}

impl Diagnosis {
    fn count(&mut self, key: &str, n: u64) {
        *self.counts.entry(key.to_string()).or_insert(0) += n;
    }

    fn sum(&mut self, key: &str, x: f64) {
        *self.sums.entry(key.to_string()).or_insert(0.0) += x;
    }

    fn sample(&mut self, key: &str, x: f64) {
        self.samples.entry(key.to_string()).or_default().push(x);
    }

    fn add(&mut self, o: Diagnosis) {
        for (k, n) in o.counts {
            *self.counts.entry(k).or_insert(0) += n;
        }
        for (k, x) in o.sums {
            *self.sums.entry(k).or_insert(0.0) += x;
        }
        for (k, v) in o.samples {
            self.samples.entry(k).or_default().extend(v);
        }
    }

    fn n(&self, key: &str) -> u64 {
        self.counts.get(key).copied().unwrap_or(0)
    }

    fn total(&self, key: &str) -> f64 {
        self.sums.get(key).copied().unwrap_or(0.0)
    }

    /// `n(key)` as a share of `n(of)`.
    fn share(&self, key: &str, of: &str) -> String {
        format!("{:.3}", self.n(key) as f64 / self.n(of).max(1) as f64)
    }

    /// The sample's size and its quantiles q10 / q25 / q50 / q75 / q90.
    fn q(&self, key: &str) -> String {
        let mut v = self.samples.get(key).cloned().unwrap_or_default();
        v.sort_by(f64::total_cmp);
        if v.is_empty() {
            return "no sample".into();
        }
        format!("{:.3} / {:.3} / {:.3} / {:.3} / {:.3} (n {})", quantile(&v, 0.1), quantile(&v, 0.25), quantile(&v, 0.5), quantile(&v, 0.75), quantile(&v, 0.9), v.len())
    }

    fn mean(&self, key: &str) -> String {
        let v = self.samples.get(key).cloned().unwrap_or_default();
        format!("{:.4}", v.iter().sum::<f64>() / v.len().max(1) as f64)
    }

    /// The share of the sample above nothing.
    fn positive(&self, key: &str) -> String {
        let v = self.samples.get(key).cloned().unwrap_or_default();
        format!("{:.3}", v.iter().filter(|x| **x > 0.0).count() as f64 / v.len().max(1) as f64)
    }
}

/// One generated world with the bots, 300 ticks, read at the start of every tick as the bots
/// read it (the valuations every fifth tick: they cost a decision each).
fn diagnose_world(seed: u64, p: &Params) -> Diagnosis {
    let mut d = Diagnosis::default();
    let s = worldgen::generate(seed, p);
    let mut g = Game::new(&s, p.clone(), seed).unwrap();
    let years = f64::from(p.goal_horizon_ticks) / 2.0;
    let mut army_before: BTreeMap<ActorId, (i64, bool)> = BTreeMap::new();
    for t in 0..300u32 {
        let w = g.world();
        let living: Vec<ActorId> = w.actors.keys().copied().filter(|a| w.alive(*a)).collect();
        let mut at_war_now: BTreeMap<ActorId, bool> = BTreeMap::new();
        for a in living.iter().copied() {
            let enemies: Vec<ActorId> = w.actors.keys().copied().filter(|b| w.at_war(a, *b) && w.alive(*b)).collect();
            let at_war = w.actors.keys().any(|b| w.at_war(a, *b));
            at_war_now.insert(a, at_war);
            let side = if at_war { "war" } else { "peace" };
            let key = |name: &str| format!("{side}: {name}");
            let ceiling = w.recruit_ceiling(a, p).max(1);
            let (army, treasury) = (w.army_size(a), w.treasury(a));
            let income = w.yearly_income(a, p).max(1) as f64;
            let people: i64 = w.actors[&a].provinces.iter().map(|q| w.population(*q)).sum();
            d.count(&key("polity-ticks"), 1);
            d.sum(&key("people"), people as f64);
            d.sum(&key("army"), army as f64);
            // The army against the ceiling of А1; at war, by who declared.
            d.sample(&key("army over the ceiling"), army as f64 / ceiling as f64);
            d.count(&key("no army"), u64::from(army == 0));
            if at_war {
                let principal: Vec<&engine2::War> = w.wars.iter().filter(|(k, x)| (k.0 == a || k.1 == a) && x.of.is_none()).map(|(_, x)| x).collect();
                let role = if principal.iter().any(|x| x.by == Some(a)) {
                    "declared"
                } else if principal.iter().any(|x| x.by.is_some()) {
                    "attacked"
                } else {
                    "by its bonds or the starting facts only"
                };
                d.sample(&format!("war: army over the ceiling — {role}"), army as f64 / ceiling as f64);
                d.count(&format!("war: polity-ticks — {role}"), 1);
                d.count(&format!("war: no army — {role}"), u64::from(army == 0));
            }
            // The money.
            d.sample(&key("treasury, years of income"), treasury as f64 / income);
            d.sample(&key("ticks of the whole ceiling's pay the treasury covers"), treasury as f64 / engine2::economy::pay_per_tick(ceiling, p).max(1) as f64);
            d.sum(&format!("window {}: treasury", t / 50), treasury as f64);
            d.sum(&format!("window {}: income", t / 50), income);
            d.sample(&format!("window {}: treasury, years of income", t / 50), treasury as f64 / income);
            // The money the bot prices its soldiers' pay against (plan §31.3, §31.11): the
            // free money over the horizon, at peace and at war.
            let free = w.free_money(a, p);
            d.sample(&key("free money, years of income"), free / income);
            d.count(&key("free money above nothing"), u64::from(free > 0.0));
            // The army's turnover: men gained or lost since the tick before, by the same state.
            if let Some((before, was_at_war)) = army_before.get(&a) {
                if *was_at_war == at_war {
                    d.sum(&key("men moved since the tick before"), (army - before).abs() as f64);
                    d.sum(&key("men under arms, tick after tick"), army as f64);
                }
            }
            army_before.insert(a, (army, at_war));
            if t % 5 != 0 {
                continue;
            }
            // The valuations, as the bot's decision reads them (`World::options`).
            d.count(&key("sampled"), 1);
            let unpaid = w.paid_share_seen_ppm(a) < 1_000_000;
            d.count(&key("unpaid as the bot reads it"), u64::from(unpaid));
            let options = w.options(a, &VerbId::ALL, p);
            let levies: Vec<(i64, f64)> = options
                .iter()
                .filter_map(|o| match o.call {
                    Some(VerbCall { verb: VerbId::Recruit, target: Target::Levy(_, men), .. }) => Some((i64::from(men), o.utility)),
                    _ => None,
                })
                .collect();
            let blow = w.ceded_province_blow(a, &w.risk_prices(a, p), p);
            let wars: Vec<engine2::worth::WarTerms> = enemies.iter().map(|b| w.war_terms(a, *b, blow, p)).collect();
            // What the bot charges for soldiers kept: at war for the longest of its fronts, at peace a tick.
            let kept_for = if at_war { wars.iter().map(|x| x.ticks).fold(0.0, f64::max) } else { 1.0 };
            // … and what its pay is priced against (`pay_money`): a tick's share of the free
            // money over the horizon for every tick charged.
            let free = free * kept_for / f64::from(p.goal_horizon_ticks);
            if let Some(hire) = options.iter().find(|o| o.call.is_some_and(|c| c.verb == VerbId::HireMercenaries)) {
                d.count(&key("a hire on offer"), 1);
                d.count(&key("a hire above nothing"), u64::from(hire.utility > 0.0));
            }
            for o in options.iter().filter(|o| o.call.is_some_and(|c| c.verb == VerbId::Disband)) {
                let Some(VerbCall { target: Target::Unit(u), .. }) = o.call else { continue };
                // Disbanding is worth what it saves (the pay at what a spending not made is worth,
                // `saving_worth`, and the heads) less what the strength was worth: so the worth of
                // keeping — read against the unit's cost at the face value of its pay.
                let saved = World::saving_worth(World::soldiers_pay(w.soldiers(u), kept_for, p), free, income) + w.heads_cost(a, w.soldiers(u), kept_for, p);
                let cost = w.soldiers_cost(a, w.soldiers(u), kept_for, p);
                d.count(&key("units it could disband"), 1);
                d.count(&key("units whose disbanding is above nothing"), u64::from(o.utility > 0.0));
                d.count(&key("units whose pay saved is worth nothing (the money is free)"), u64::from(World::saving_worth(World::soldiers_pay(w.soldiers(u), kept_for, p), free, income) <= 0.0));
                d.sample(&key("a unit kept: its worth over its cost"), (saved - o.utility) / cost.max(1e-12));
            }
            if levies.is_empty() {
                let reason = if w.recruit_room(a, p) == 0 {
                    if w.actors[&a].manpower_deficit > 0.0 && w.recruit_ceiling(a, p) > army {
                        "the dead of the cohort hold the room"
                    } else {
                        "the army is at the ceiling"
                    }
                } else if w.recruit_per_tick(a, p) == 0 {
                    "the rate of a tick rounds to nothing"
                } else if !w.actors[&a].provinces.iter().any(|q| !w.in_revolt(*q) && w.population(*q) > 0) {
                    "no province to levy from"
                } else {
                    "another reason"
                };
                d.count(&key(&format!("no levy on offer — {reason}")), 1);
                continue;
            }
            d.count(&key("a levy on offer"), 1);
            // The tick's levy (the first, the largest) and the best of the sizes the bot weighs.
            let (men, net) = levies[0];
            // The cost as the bot prices it (`kept_soldiers_cost`): the pay by the free money
            // (`spending_price`, plan §31.3) and the heads; beside it, the cost at the face value.
            let cost = World::spending_price(World::soldiers_pay(men, kept_for, p), free, income) + w.heads_cost(a, men, kept_for, p);
            let gain = net + cost;
            d.sample(&key("the tick's levy: its cost at the face value of the pay, years of income"), w.soldiers_cost(a, men, kept_for, p));
            d.count(&key("the tick's levy: its pay within the free money"), u64::from(World::soldiers_pay(men, kept_for, p) <= free.max(0.0)));
            let best = levies.iter().map(|(_, u)| *u).fold(f64::MIN, f64::max);
            d.sample(&key("the tick's levy: net, years of income"), net);
            d.sample(&key("the tick's levy: its gain, years of income"), gain);
            d.sample(&key("the tick's levy: its cost, years of income"), cost);
            d.sample(&key("the tick's levy: gain over cost"), gain / cost.max(1e-12));
            d.sample(&key("the best levy: net, years of income"), best);
            d.sample(&key("the tick's levy: men over the ceiling"), men as f64 / ceiling as f64);
            let extra = World::new_soldiers_strength(men);
            if at_war {
                // The gain by its parts, war by war (`war_worth_of`): the odds, the stake, the flags.
                for x in &wars {
                    let (p0, p1) = (w.war_odds(a, x.b, 0.0, p), w.war_odds(a, x.b, extra, p));
                    let closed = |odds: f64| odds < 0.5 && x.first_blow() <= x.ticks;
                    let at_threshold = w.relation(a, x.b) >= p.treaty_accept_at_relation;
                    let stake = w.war_stake(a, x.b, p) * years / income;
                    let won = |odds: f64| if unpaid || at_threshold || closed(odds) { 0.0 } else { odds * stake };
                    let moved = w.war_worth_of(a, x, unpaid, extra, p) - w.war_worth_of(a, x, unpaid, 0.0, p);
                    d.count("war: wars weighed", 1);
                    d.count("war: wars closed by the rule of the peace as foreseen", u64::from(closed(p0)));
                    d.count("war: wars the tick's levy opens (closed without it)", u64::from(closed(p0) && !closed(p1)));
                    d.count("war: wars at the threshold of the relation", u64::from(at_threshold));
                    d.count("war: wars of an unpaid army", u64::from(unpaid));
                    d.count("war: wars with no stake", u64::from(stake <= 0.0));
                    d.count("war: wars that win nothing (closed, at the threshold, unpaid or no stake)", u64::from(unpaid || at_threshold || closed(p0) || stake <= 0.0));
                    d.sample("war: odds without the levy", p0);
                    d.sample("war: odds the levy adds", p1 - p0);
                    d.sample("war: the stake for the horizon, years of income", stake);
                    d.sample("war: the levy's gain in the war — the stake won", won(p1) - won(p0));
                    d.sample("war: the levy's gain in the war — the losses spared and the rest", moved - (won(p1) - won(p0)));
                    d.sample("war: the levy's gain in the war — all", moved);
                    d.sample("war: the war's length the bot reads, ticks", x.ticks);
                }
            } else {
                // At peace: the threat and the suppression (`strength_gain`).
                let suppression = if w.army_strength(a) == 0.0 { w.suppression_a_tick(a, p) } else { 0.0 };
                d.sample("peace: the tick's levy: its gain — the suppression", suppression);
                d.sample("peace: the tick's levy: its gain — the threat", gain - suppression);
                let threats = w.threats(a, blow, p);
                d.sample("peace: neighbours it reads as threats", threats.len() as f64);
                d.sample("peace: wars a tick its neighbours bring, as it reads them", threats.iter().map(|(x, _)| w.wars_a_tick_from(*x, a, 0.0, p)).sum());
            }
        }
        let r = g.tick(&[]).unwrap();
        // What each chose (one call a tick, `World::decide`).
        let chosen: BTreeMap<ActorId, VerbId> = r
            .events
            .iter()
            .filter_map(|e| match e.event {
                Event::Verb { actor: ActorRef::Ruler(a), verb, .. } => Some((a, verb)),
                _ => None,
            })
            .collect();
        for (a, at_war) in &at_war_now {
            let side = if *at_war { "war" } else { "peace" };
            d.count(&format!("{side}: chose {}", chosen.get(a).map_or("nothing".to_string(), |v| format!("{v:?}"))), 1);
            d.count(&format!("{side}: short of its pay this tick"), u64::from(r.events.iter().any(|e| matches!(e.event, Event::PayShortfall { actor, .. } if actor == *a))));
        }
        // The budget: every posting on a polity's treasury, by its source, and what the tick left.
        for q in &r.postings {
            if let engine2::Account::Treasury(a) = q.account {
                if let Some(at_war) = at_war_now.get(&a) {
                    let side = if *at_war { "war" } else { "peace" };
                    let way = if q.milli >= 0 { "in" } else { "out" };
                    d.sum(&format!("{side}: flow {:?} {way}", q.source), q.milli as f64 / 1000.0);
                }
            }
        }
    }
    d
}

/// The diagnosis of §30.13, parts А–В: at war and at peace, what army the bots keep against
/// the ceiling of А1, whether a levy is on offer and what it is worth to them against its
/// price, what moves that worth, how much money they hold, and where the take goes.
#[test]
#[ignore]
fn diagnosis_why_the_armies_are_small() {
    let p = params();
    for (label, seeds) in [("calibration 1–20", (1..=20).collect::<Vec<u64>>()), ("held out 100–129", (100..=129).collect())] {
        let mut d = Diagnosis::default();
        for w in engine2::par::map(&seeds, |seed| diagnose_world(*seed, &p)) {
            d.add(w);
        }
        let line = |text: String| eprintln!("{label}: {text}");
        for side in ["war", "peace"] {
            let key = |name: &str| format!("{side}: {name}");
            line(format!("[{side}] polity-ticks {}; army over people {:.4}; army over the ceiling of А1 q10/q25/q50/q75/q90 {}; with no army {}", d.n(&key("polity-ticks")), d.total(&key("army")) / d.total(&key("people")).max(1.0), d.q(&key("army over the ceiling")), d.share(&key("no army"), &key("polity-ticks"))));
            if side == "war" {
                for role in ["declared", "attacked", "by its bonds or the starting facts only"] {
                    line(format!("[war] {role}: polity-ticks {}; army over the ceiling {}; with no army {}", d.n(&format!("war: polity-ticks — {role}")), d.q(&format!("war: army over the ceiling — {role}")), d.share(&format!("war: no army — {role}"), &format!("war: polity-ticks — {role}"))));
                }
            }
            let shut: Vec<String> = d.counts.iter().filter(|(k, _)| k.starts_with(&key("no levy on offer"))).map(|(k, n)| format!("{} {:.3}", k.trim_start_matches(&key("no levy on offer — ")), *n as f64 / d.n(&key("sampled")).max(1) as f64)).collect();
            line(format!("[{side}] decisions read {} (every fifth tick); a levy on offer {}; none: {shut:?}; unpaid as the bot reads it {}", d.n(&key("sampled")), d.share(&key("a levy on offer"), &key("sampled")), d.share(&key("unpaid as the bot reads it"), &key("sampled"))));
            line(format!("[{side}] the tick's levy, years of income: men over the ceiling {}; gain {}; cost {}; gain over cost {}; net {} — above nothing in {}", d.q(&key("the tick's levy: men over the ceiling")), d.q(&key("the tick's levy: its gain, years of income")), d.q(&key("the tick's levy: its cost, years of income")), d.q(&key("the tick's levy: gain over cost")), d.q(&key("the tick's levy: net, years of income")), d.positive(&key("the tick's levy: net, years of income"))));
            line(format!("[{side}] the best of the levy's sizes: net {} — above nothing in {}", d.q(&key("the best levy: net, years of income")), d.positive(&key("the best levy: net, years of income"))));
            line(format!(
                "[{side}] the money (plan §31.3): free money in years of income {}; above nothing in {} of polity-ticks; the tick's levy's pay within it in {} of the levies on offer; its cost at the face value {}; the pay saved by a disbanding worth nothing in {} of the units; short of its pay in {} of polity-ticks; the army's turnover a tick {:.4}",
                d.q(&key("free money, years of income")),
                d.share(&key("free money above nothing"), &key("polity-ticks")),
                d.share(&key("the tick's levy: its pay within the free money"), &key("a levy on offer")),
                d.q(&key("the tick's levy: its cost at the face value of the pay, years of income")),
                d.share(&key("units whose pay saved is worth nothing (the money is free)"), &key("units it could disband")),
                d.share(&key("short of its pay this tick"), &key("polity-ticks")),
                d.total(&key("men moved since the tick before")) / d.total(&key("men under arms, tick after tick")).max(1.0)
            ));
            line(format!("[{side}] a hire on offer {} of decisions, above nothing in {} of those; units it could disband {}, disbanding above nothing in {}; a unit kept — its worth over its cost {}", d.share(&key("a hire on offer"), &key("sampled")), d.share(&key("a hire above nothing"), &key("a hire on offer")), d.n(&key("units it could disband")), d.share(&key("units whose disbanding is above nothing"), &key("units it could disband")), d.q(&key("a unit kept: its worth over its cost"))));
            let chose: Vec<String> = d.counts.iter().filter(|(k, _)| k.starts_with(&key("chose "))).map(|(k, n)| format!("{} {:.3}", k.trim_start_matches(&key("chose ")), *n as f64 / d.n(&key("polity-ticks")).max(1) as f64)).collect();
            line(format!("[{side}] what it chose, of polity-ticks: {chose:?}"));
            line(format!("[{side}] the treasury in years of income {}; ticks of the whole ceiling's pay it covers {}", d.q(&key("treasury, years of income")), d.q(&key("ticks of the whole ceiling's pay the treasury covers"))));
        }
        line(format!(
            "[war] wars weighed {}: closed by the rule as foreseen {}, opened by the tick's levy {}, at the threshold {}, of an unpaid army {}, with no stake {}; winning nothing for any of these {}",
            d.n("war: wars weighed"),
            d.share("war: wars closed by the rule of the peace as foreseen", "war: wars weighed"),
            d.share("war: wars the tick's levy opens (closed without it)", "war: wars weighed"),
            d.share("war: wars at the threshold of the relation", "war: wars weighed"),
            d.share("war: wars of an unpaid army", "war: wars weighed"),
            d.share("war: wars with no stake", "war: wars weighed"),
            d.share("war: wars that win nothing (closed, at the threshold, unpaid or no stake)", "war: wars weighed")
        ));
        line(format!("[war] the odds without the levy {}; the odds the tick's levy adds {}; the stake for the horizon, years of income {}; the war's length as the bot reads it {}", d.q("war: odds without the levy"), d.q("war: odds the levy adds"), d.q("war: the stake for the horizon, years of income"), d.q("war: the war's length the bot reads, ticks")));
        line(format!(
            "[war] what the tick's levy adds to a war, years of income: all {} (mean {}); the stake won {} (mean {}, above nothing in {}); the losses spared and the rest {} (mean {})",
            d.q("war: the levy's gain in the war — all"),
            d.mean("war: the levy's gain in the war — all"),
            d.q("war: the levy's gain in the war — the stake won"),
            d.mean("war: the levy's gain in the war — the stake won"),
            d.positive("war: the levy's gain in the war — the stake won"),
            d.q("war: the levy's gain in the war — the losses spared and the rest"),
            d.mean("war: the levy's gain in the war — the losses spared and the rest")
        ));
        line(format!(
            "[peace] the tick's levy's gain by its parts, years of income: the threat {} (mean {}), the suppression {} (mean {}); neighbours read as threats {}; wars a tick they bring {}",
            d.q("peace: the tick's levy: its gain — the threat"),
            d.mean("peace: the tick's levy: its gain — the threat"),
            d.q("peace: the tick's levy: its gain — the suppression"),
            d.mean("peace: the tick's levy: its gain — the suppression"),
            d.q("peace: neighbours it reads as threats"),
            d.q("peace: wars a tick its neighbours bring, as it reads them")
        ));
        let windows: Vec<String> = (0..6).map(|i| format!("{:.2} (median {})", d.total(&format!("window {i}: treasury")) / d.total(&format!("window {i}: income")).max(1.0), {
            let mut v = d.samples.get(&format!("window {i}: treasury, years of income")).cloned().unwrap_or_default();
            v.sort_by(f64::total_cmp);
            format!("{:.2}", quantile(&v, 0.5))
        })).collect();
        line(format!("the treasuries in years of income by windows of 50 ticks, all polities — the sum over the sum (and the median polity-tick): {windows:?}"));
        // В: the budget in shares of the take, and the arithmetic of the bands.
        let rate = engine2::economy::pay_per_tick(1000, &p) as f64 / 1000.0;
        for side in ["war", "peace"] {
            let flow = |name: &str| d.total(&format!("{side}: flow {name}"));
            let take = flow("Tax in").max(1.0);
            let flows: Vec<String> = d.sums.iter().filter(|(k, _)| k.starts_with(&format!("{side}: flow "))).map(|(k, x)| format!("{} {:+.3}", k.trim_start_matches(&format!("{side}: flow ")), x / take)).collect();
            let left: f64 = d.sums.iter().filter(|(k, _)| k.starts_with(&format!("{side}: flow "))).map(|(_, x)| x).sum();
            line(format!("[{side}] the budget in shares of the take, by the postings on the treasuries: {flows:?}; left in the treasury {:+.3}", left / take));
            let (court, pay, people, army) = (-flow("Court out"), -flow("Upkeep out"), d.total(&format!("{side}: people")), d.total(&format!("{side}: army")));
            let seats = -flow("Seat out");
            let at_share = |e: f64, rate: f64| court * e / (1.0 - e) / rate / people.max(1.0);
            line(format!(
                "[{side}] Э9 as it is {:.3}; the take a head a tick {:.1} kg, a soldier's pay a tick {:.0} kg (band {:.0}–{:.0}); А11 as it is {:.4}; the army the pay would keep at Э9 = 0,50: {:.4} of the people (pay band {:.4}–{:.4}), taking {:.3} of the take; at Э9 = 0,75: {:.4} (pay band {:.4}–{:.4}), taking {:.3} of the take; the army all the take past the court and the seats would keep {:.4}",
                pay / (pay + court).max(1.0),
                take / people.max(1.0),
                rate,
                rate * 2.0 / 3.0,
                rate * 4.0 / 3.0,
                army / people.max(1.0),
                at_share(0.5, rate),
                at_share(0.5, rate * 4.0 / 3.0),
                at_share(0.5, rate * 2.0 / 3.0),
                court / take,
                at_share(0.75, rate),
                at_share(0.75, rate * 4.0 / 3.0),
                at_share(0.75, rate * 2.0 / 3.0),
                3.0 * court / take,
                (take - court - seats) / rate / people.max(1.0)
            ));
        }
    }
}

/// The diagnosis of §30.13, part Г: what the army answers to at all — А11 at peace (the mean,
/// and the level it settles at after the first fifty ticks) and at war on the calibration
/// seeds with one parameter replaced **in a copy of the parameters**, two points each, А12 and
/// the revanche beside them for the price. No calibration: nothing is changed by it (rules 1
/// and 2 of §24.1).
#[test]
#[ignore]
fn diagnosis_what_the_army_answers_to() {
    let base = params();
    let seeds: Vec<u64> = (1..=20).collect();
    type Set = fn(&mut Params);
    let band = base.entries["army.pay_per_soldier_outputs"].band.unwrap();
    assert_eq!(band, [2.0, 4.0], "the band of E0 А3 the two points of the pay are read from");
    let points: [(&str, Set); 7] = [
        ("as shipped", |_| {}),
        ("recruit.rate_per_tick (А2) 0,125", |p| p.recruit_rate_ppm_per_tick = 125_000),
        ("recruit.rate_per_tick (А2) 0,5", |p| p.recruit_rate_ppm_per_tick = 500_000),
        ("battle.strength_exponent (k) 1,2", |p| p.battle_strength_exponent = 1.2),
        ("battle.strength_exponent (k) 2,0", |p| p.battle_strength_exponent = 2.0),
        ("army.pay_per_soldier_outputs (А3) 2,0 — the band's bottom", |p| p.pay_per_soldier_kg_per_year = (2.0 * p.output_per_head_kg as f64).round() as i64),
        ("army.pay_per_soldier_outputs (А3) 4,0 — the band's top", |p| p.pay_per_soldier_kg_per_year = (4.0 * p.output_per_head_kg as f64).round() as i64),
    ];
    for (name, set) in points {
        let mut p = base.clone();
        set(&mut p);
        let m = Measures::over(&seeds, &p);
        let r = m.rates();
        let settled = m.a11_peace_windows.iter().skip(1).fold((0i64, 0i64), |acc, (a, n)| (acc.0 + a, acc.1 + n));
        eprintln!(
            "{name}: А11 at peace {:.4}, settled after the first 50 ticks {:.4}; А11 at war {:.4}; А12 {:.3}; war declarations {}, the tick after a peace of the same pair {:.3}; Э9 {:.3}; a soldier's pay {} kg a year",
            r.a11_peace,
            settled.0 as f64 / settled.1.max(1) as f64,
            r.a11_war,
            r.a12,
            m.declarations,
            m.declared_after_peace as f64 / m.declarations.max(1) as f64,
            m.pay as f64 / (m.pay + m.court).max(1) as f64,
            p.pay_per_soldier_kg_per_year
        );
    }
}

/// What a polity at war reads of its money and its wars when it hires or disbands — as the
/// bot reads it (`diagnosis_why_the_hired_men_are_sent_home`).
struct WarRead {
    unpaid: bool,
    /// No war of its wins anything: closed by the rule of the peace, at the threshold, or
    /// with no stake — the gain of every war is nothing whether its army is paid or not.
    nothing_to_win: bool,
    /// The length of its longest war, ticks (what soldiers kept are charged for).
    kept: f64,
    /// Its free money over the horizon (`World::free_money`: the pay at the rate of home, the
    /// soldiers of war for the war's length), and its money at hand within the war's length
    /// (the pay as the world charges it), kg — the two bounds of `World::money_over`.
    afford: f64,
    at_hand: f64,
    /// The soldiers it pays and the army the peace keeps (plan §31.15).
    army: i64,
    peace_army: i64,
    /// The ticks of the horizon.
    horizon: f64,
    income: f64,
    terms: Vec<engine2::worth::WarTerms>,
    enemies: Vec<ActorId>,
}

impl WarRead {
    fn of(w: &World, a: ActorId, p: &Params) -> WarRead {
        let enemies: Vec<ActorId> = w.actors.keys().copied().filter(|b| w.at_war(a, *b) && w.alive(*b)).collect();
        let blow = w.ceded_province_blow(a, &w.risk_prices(a, p), p);
        let terms: Vec<_> = enemies.iter().map(|b| w.war_terms(a, *b, blow, p)).collect();
        let kept = terms.iter().map(|t| t.ticks).fold(0.0, f64::max);
        let b = w.tick_budget(a, p);
        let before_pay = b.income - b.tribute_due - b.seats.iter().map(|(_, kg)| kg).sum::<i64>() - b.court - b.interest;
        let horizon = f64::from(p.goal_horizon_ticks);
        WarRead {
            unpaid: w.paid_share_seen_ppm(a) < 1_000_000,
            nothing_to_win: terms.iter().all(|t| (w.war_worth_of(a, t, false, 0.0, p) - w.war_worth_of(a, t, true, 0.0, p)).abs() < 1e-12),
            kept,
            afford: w.free_money(a, p),
            army: w.paid_soldiers(a, p),
            peace_army: w.peace_army(a, p),
            at_hand: w.treasury(a) as f64 + (before_pay - b.pay_due) as f64 * kept + w.loan_room(a, p) as f64,
            horizon,
            income: w.yearly_income(a, p).max(1) as f64,
            terms,
            enemies,
        }
    }

    /// The money for a spending made once over the war's length (`World::money_over`).
    fn money(&self) -> f64 {
        self.afford.min(self.at_hand)
    }

    /// The share of the free money that stands against the pay of the war's length, with an
    /// advance taken out first (`agents::pay_money`).
    fn pay_money(&self, advance: f64) -> f64 {
        (self.afford - advance) * self.kept / self.horizon
    }

    /// `n` men raised (or sent home) split at the army the peace keeps: `(of the peace, of the
    /// war)` (`agents::split_at_peace_army`).
    fn split(&self, n: i64, raised: bool) -> (i64, i64) {
        let war = if raised { n - n.min((self.peace_army - self.army).max(0)) } else { n.min((self.army - self.peace_army).max(0)) };
        (n - war, war)
    }

    /// The money the soldiers of war of a call stand against (`agents::kept_soldiers_cost`).
    fn war_money(&self, peace: i64, advance: f64, p: &Params) -> f64 {
        self.money().min(self.afford - World::soldiers_pay(peace, self.horizon, p)) - advance
    }
}

/// One generated world with the bots, 300 ticks: every hire and every disbanding a polity at
/// war chose, with the state it chose it in read as its bot read it (the world before the tick).
fn diagnose_war_turnover(seed: u64, p: &Params) -> Diagnosis {
    /// The ticks after a hire within which a disbanding counts as following it.
    const WITHIN: u32 = 4;
    let mut d = Diagnosis::default();
    let mut g = Game::new(&worldgen::generate(seed, p), p.clone(), seed).unwrap();
    let mut raised: BTreeMap<engine2::UnitId, u32> = BTreeMap::new();
    // A polity's last hire at war: the tick, what it read, and what the hire was to leave of
    // its money over the horizon (the advance paid, the hired men's pay in every tick of it).
    let mut hired: BTreeMap<ActorId, (u32, WarRead, bool, f64)> = BTreeMap::new();
    // The hires at war still followed: polity, tick, a shortfall since, a disbanding since,
    // whether the hire was within the polity's money, the hired unit, its disbanding since.
    let mut followed: Vec<(ActorId, u32, bool, bool, bool, Option<engine2::UnitId>, bool)> = Vec::new();
    for t in 0..300u32 {
        let before = g.world().clone();
        let r = g.tick(&[]).unwrap();
        for e in &r.events {
            if let Event::Recruited { unit, .. } | Event::Hired { unit, .. } = e.event {
                raised.insert(unit, t);
            }
        }
        for h in followed.iter_mut().filter(|h| t > h.1) {
            h.2 |= r.events.iter().any(|e| matches!(e.event, Event::PayShortfall { actor, .. } if actor == h.0));
            h.3 |= r.events.iter().any(|e| matches!(e.event, Event::Verb { actor: ActorRef::Ruler(x), verb: VerbId::Disband, .. } if x == h.0));
            h.6 |= r.events.iter().any(|e| matches!(e.event, Event::Verb { verb: VerbId::Disband, target: Target::Unit(u), .. } if Some(u) == h.5));
        }
        followed.retain(|h| {
            if t < h.1 + WITHIN {
                return true;
            }
            d.count("hires followed", 1);
            d.count("hire: a shortfall within", u64::from(h.2));
            d.count("hire: a disbanding within", u64::from(h.3));
            // The same by whether the hire was within the polity's money (cost it nothing).
            let means = if h.4 { "within its means" } else { "past its means" };
            d.count(&format!("hires {means}"), 1);
            d.count(&format!("hires {means}: a disbanding within"), u64::from(h.3));
            d.count(&format!("hires {means}: the hired men disbanded within"), u64::from(h.6));
            false
        });
        // The unit of a hire: its `Hired` comes right before the `Verb` of the call that made it.
        let mut last_hired = None;
        for e in &r.events {
            if let Event::Hired { unit, .. } = e.event {
                last_hired = Some(unit);
            }
            let Event::Verb { actor: ActorRef::Ruler(a), verb, target } = e.event else { continue };
            if !before.actors.keys().any(|b| before.at_war(a, *b)) {
                continue;
            }
            match (verb, target) {
                (VerbId::HireMercenaries, _) => {
                    let x = WarRead::of(&before, a, p);
                    let (n, advance) = (before.hire_count(a, p), before.hire_advance(a, p) as f64);
                    let (peace, war) = x.split(n, true);
                    let within = advance <= x.money().max(0.0) && World::soldiers_pay(peace, x.kept, p) <= x.pay_money(advance).max(0.0) && World::soldiers_pay(war, x.kept, p) <= x.war_money(peace, advance, p).max(0.0);
                    d.count("hires", 1);
                    d.count("hire: unpaid", u64::from(x.unpaid));
                    d.count("hire: nothing to win", u64::from(x.nothing_to_win));
                    d.count("hire: within the money", u64::from(within));
                    let (peace, war) = x.split(n, true);
                    let left_of_the_horizon = x.afford - advance - World::soldiers_pay(peace, f64::from(p.goal_horizon_ticks), p) - World::soldiers_pay(war, x.kept, p);
                    let left_at_hand = x.at_hand - advance - World::soldiers_pay(n, x.kept, p);
                    d.sample("hire: left over the horizon, years of income", left_of_the_horizon / x.income);
                    d.sample("hire: left at hand over the war, years of income", left_at_hand / x.income);
                    hired.insert(a, (t, x, within, left_of_the_horizon));
                    followed.push((a, t, false, false, within, last_hired, false));
                }
                (VerbId::Disband, Target::Unit(u)) => {
                    let x = WarRead::of(&before, a, p);
                    let (n, strength) = (before.soldiers(u), before.unit_strength(u));
                    // What keeping the unit is worth to its wars, and what its pay saved is worth.
                    let keep: f64 = x.terms.iter().map(|t| before.war_worth_of(a, t, x.unpaid, 0.0, p) - before.war_worth_of(a, t, x.unpaid, -strength, p)).sum();
                    let (peace, war) = x.split(n, false);
                    let pay = World::saving_worth(World::soldiers_pay(war, x.kept, p), x.money(), x.income) + World::saving_worth(World::soldiers_pay(peace, x.kept, p), x.pay_money(0.0), x.income);
                    let cause = if x.unpaid {
                        "1 unpaid as the bot reads it"
                    } else if x.nothing_to_win {
                        "2 paid, nothing to win in any war"
                    } else if keep <= 0.0 {
                        "3 a stake alive, the unit's strength worth nothing or less"
                    } else if pay > 0.0 {
                        "4 a stake alive, the strength worth less than its pay past the money"
                    } else {
                        "5 a stake alive, the pay within the money, the strength worth less than the heads"
                    };
                    d.count("disbandings", 1);
                    d.count(&format!("cause {cause}"), 1);
                    d.count("disbanding: the money below nothing", u64::from(x.afford < 0.0));
                    d.sum("men disbanded", n as f64);
                    if before.units[&u].mercenary && raised.get(&u).is_some_and(|at| t - at <= WITHIN) {
                        d.sum("men disbanded: hired within the ticks", n as f64);
                    }
                    let Some((at, then, within, left_of_the_horizon)) = hired.get(&a).filter(|h| t - h.0 <= WITHIN) else { continue };
                    d.count("disbandings after a hire", 1);
                    d.sample("after a hire: ticks since", f64::from(t - at));
                    d.count("after a hire: the hire was within the money", u64::from(*within));
                    d.count("after a hire: the hire left the money over the horizon below nothing", u64::from(*left_of_the_horizon < 0.0));
                    d.count("after a hire: the horizon is the lesser bound now", u64::from(x.afford < x.at_hand));
                    d.count("after a hire: the money below nothing now", u64::from(x.afford < 0.0));
                    d.count("after a hire: unpaid now, paid then", u64::from(x.unpaid && !then.unpaid));
                    d.count("after a hire: nothing to win now, something then", u64::from(!x.unpaid && x.nothing_to_win && !then.nothing_to_win));
                    d.count("after a hire: other wars now", u64::from(x.enemies != then.enemies));
                    d.sample("after a hire: the war's length now over then", x.kept / then.kept.max(1.0));
                }
                _ => {}
            }
        }
    }
    d
}

/// The diagnosis of §31.10: **why the hired men are sent home** — the army's turnover at war
/// (known deviation (б) of (ц1)). On both sets of seeds: what follows a hire at war, the state
/// a polity at war disbands in by its cause as the bot reads it, and what moved between a hire
/// and the disbanding after it. Printed, never asserted, run on demand (`#[ignore]`): the
/// world is not changed, nothing is fitted (rules 1 and 2 of §24.1).
/// `cargo test --release -p engine2 --all-features --test agents diagnosis_why_the_hired -- --ignored --nocapture`.
#[test]
#[ignore]
fn diagnosis_why_the_hired_men_are_sent_home() {
    let p = params();
    for (label, seeds) in [("calibration 1–20", (1..=20).collect::<Vec<u64>>()), ("held out 100–129", (100..=129).collect())] {
        let mut d = Diagnosis::default();
        for part in engine2::par::map(&seeds, |seed| diagnose_war_turnover(*seed, &p)) {
            d.add(part);
        }
        eprintln!(
            "{label}: hires at war {}: a disbanding by the same polity within 4 ticks {}, a shortfall of pay within 4 ticks {}; made unpaid as the bot reads it {}, with nothing to win in any war {}, within its means — the advance within the money at hand, the pay within its share of the free money — {}; what the hire leaves, years of income q10/q25/q50/q75/q90 — over the horizon {}, at hand over the war {}",
            d.n("hires"),
            d.share("hire: a disbanding within", "hires followed"),
            d.share("hire: a shortfall within", "hires followed"),
            d.share("hire: unpaid", "hires"),
            d.share("hire: nothing to win", "hires"),
            d.share("hire: within the money", "hires"),
            d.q("hire: left over the horizon, years of income"),
            d.q("hire: left at hand over the war, years of income")
        );
        eprintln!(
            "{label}: of the hires at war within the polity's means (free) {}: a disbanding by the same polity within 4 ticks {}, of the hired men themselves {}; of the hires past its means (priced) {}: {} and {}",
            d.n("hires within its means"),
            d.share("hires within its means: a disbanding within", "hires within its means"),
            d.share("hires within its means: the hired men disbanded within", "hires within its means"),
            d.n("hires past its means"),
            d.share("hires past its means: a disbanding within", "hires past its means"),
            d.share("hires past its means: the hired men disbanded within", "hires past its means")
        );
        let causes: Vec<String> = d.counts.keys().filter(|k| k.starts_with("cause ")).map(|k| format!("{} {}", &k[6..], d.share(k, "disbandings"))).collect();
        eprintln!(
            "{label}: disbandings at war {}, by cause {causes:?}; the money below nothing (the pay saved is worth something) {}; of the men disbanded, hired within 4 ticks {:.3}",
            d.n("disbandings"),
            d.share("disbanding: the money below nothing", "disbandings"),
            d.total("men disbanded: hired within the ticks") / d.total("men disbanded").max(1.0)
        );
        let after = "disbandings after a hire";
        eprintln!(
            "{label}: disbandings within 4 ticks of the polity's hire {} ({} of all): ticks since the hire {}; the hire was within the money {}; it left the money over the horizon below nothing {}; now the horizon is the lesser bound {}, the money below nothing {}; unpaid now and paid then {}; nothing to win now and something then {}; other wars now {}; the war's length now over then {}",
            d.n(after),
            d.share(after, "disbandings"),
            d.q("after a hire: ticks since"),
            d.share("after a hire: the hire was within the money", after),
            d.share("after a hire: the hire left the money over the horizon below nothing", after),
            d.share("after a hire: the horizon is the lesser bound now", after),
            d.share("after a hire: the money below nothing now", after),
            d.share("after a hire: unpaid now, paid then", after),
            d.share("after a hire: nothing to win now, something then", after),
            d.share("after a hire: other wars now", after),
            d.q("after a hire: the war's length now over then")
        );
    }
}

// ---- Diagnostics: the threat by the sides of the neighbour's declaration (plan §31.20) ----

/// One seed of `diagnosis_the_threat_by_the_sides_of_the_neighbours_declaration`: every tick,
/// every living polity at peace — the wars a tick its neighbours bring on it as it reads them
/// (`wars_a_tick_from`), apart for the polities with an ally or a lord and for those with none;
/// and the threats whose sides differ between the neighbour's declaration (`war_sides(t, a)`)
/// and the polity's own (`war_sides(a, t)`) — by the public sides alone, so that the same
/// reading runs on the code before the rule.
fn diagnose_threat_sides(seed: u64, p: &Params) -> Diagnosis {
    let mut d = Diagnosis::default();
    let s = worldgen::generate(seed, p);
    let mut g = Game::new(&s, p.clone(), seed).unwrap();
    let sorted = |mut v: Vec<ActorId>| {
        v.sort();
        v
    };
    for _ in 0..300 {
        let w = g.world();
        for a in w.actors.keys().copied().filter(|a| w.alive(*a)) {
            if w.actors.keys().any(|b| w.at_war(a, *b)) {
                continue;
            }
            let threats: Vec<ActorId> = w.actors.keys().copied().filter(|t| *t != a && w.alive(*t) && w.borders(a, *t)).collect();
            let bound = w.allies_of(a).into_iter().chain(w.lord_of(a)).any(|x| w.alive(x));
            let key = if bound { "with an ally or a lord" } else { "with neither" };
            let moved = threats
                .iter()
                .filter(|t| {
                    let ((own_a, own_t), (their_t, their_a)) = (w.war_sides(a, **t), w.war_sides(**t, a));
                    sorted(own_a) != sorted(their_a) || sorted(own_t) != sorted(their_t)
                })
                .count();
            d.count("polity-ticks at peace", 1);
            d.count(&format!("polity-ticks at peace {key}"), 1);
            d.count("polity-ticks at peace with a threat", u64::from(!threats.is_empty()));
            d.count("polity-ticks at peace where a threat's sides differ", u64::from(moved > 0));
            d.count("threats", threats.len() as u64);
            d.count("threats whose sides differ", moved as u64);
            d.sample(&format!("wars a tick {key}"), threats.iter().map(|t| w.wars_a_tick_from(*t, a, 0.0, p)).sum());
        }
        g.tick(&[]).unwrap();
    }
    d
}

/// The measures of plan §31.20.4 the measures' test does not print: the expected wars a tick
/// on a polity at peace, with an ally or a lord and with neither, and how many threats the
/// rule moves the sides of. Printed, never asserted, run on demand.
#[test]
#[ignore]
fn diagnosis_the_threat_by_the_sides_of_the_neighbours_declaration() {
    let p = params();
    for (label, seeds) in [("calibration 1–20", (1..=20).collect::<Vec<u64>>()), ("held out 100–129", (100..=129).collect())] {
        let mut d = Diagnosis::default();
        // The worlds the rule moves no threat's sides in, over the whole run: there the bots
        // read the threat as before, and the run is the run before the rule, byte for byte.
        let mut unmoved: Vec<u64> = Vec::new();
        for (seed, part) in seeds.iter().zip(engine2::par::map(&seeds, |seed| diagnose_threat_sides(*seed, &p))) {
            if part.n("threats whose sides differ") == 0 {
                unmoved.push(*seed);
            }
            d.add(part);
        }
        eprintln!("{label}: worlds where no threat's sides differ in any tick: {unmoved:?}");
        eprintln!(
            "{label}: polity-ticks at peace {} — with an ally or a lord {}, with neither {}; the wars a tick the neighbours bring, q10/q25/q50/q75/q90 — with an ally or a lord {}, mean {}; with neither {}, mean {}",
            d.n("polity-ticks at peace"),
            d.share("polity-ticks at peace with an ally or a lord", "polity-ticks at peace"),
            d.share("polity-ticks at peace with neither", "polity-ticks at peace"),
            d.q("wars a tick with an ally or a lord"),
            d.mean("wars a tick with an ally or a lord"),
            d.q("wars a tick with neither"),
            d.mean("wars a tick with neither")
        );
        eprintln!(
            "{label}: threats (a neighbour not at war, a polity-tick each) {}: the sides of the neighbour's declaration differ from those of the polity's own in {} ({}); polity-ticks at peace with such a threat {} of {} with a threat ({})",
            d.n("threats"),
            d.n("threats whose sides differ"),
            d.share("threats whose sides differ", "threats"),
            d.n("polity-ticks at peace where a threat's sides differ"),
            d.n("polity-ticks at peace with a threat"),
            d.share("polity-ticks at peace where a threat's sides differ", "polity-ticks at peace with a threat")
        );
    }
}
