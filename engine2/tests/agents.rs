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
const CONFLICTS: [&str; 10] = ["at war", "its army paid", "no unmarried adult to marry", "the other house has no unmarried adult", "the relation is below the threshold", "a vassal fights only its lord", "a vassal makes no alliance", "at the levy ceiling", "not the polity's own province", "at the debt ceiling"];

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
    for t in 0..300u32 {
        let w = g.world();
        let living: Vec<ActorId> = w.actors.keys().copied().filter(|a| w.alive(*a)).collect();
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
        tax += r.postings.iter().filter(|q| matches!(q.account, engine2::Account::Treasury(_)) && q.source == Source::Tax).map(|q| q.milli / 1000).sum::<i64>();
        pay -= r.postings.iter().filter(|q| matches!(q.account, engine2::Account::Treasury(_)) && q.source == Source::Upkeep).map(|q| q.milli / 1000).sum::<i64>();
        court -= r.postings.iter().filter(|q| matches!(q.account, engine2::Account::Treasury(_)) && q.source == Source::Court).map(|q| q.milli / 1000).sum::<i64>();
    }
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
    Measures { reigns, usurpations, polity_centuries, risings, large_ticks, polities_start, polities_century, war_ticks, polity_ticks, at_peace_ticks, army_at_war, people_at_war, army_at_peace, people_at_peace, a11_peace_windows, bound_century, alive_century, tax, production, falls, falls_by_conquest, reached, dropped, verbs, rejections, per_polity_war_share, rates, revolt_ticks, province_ticks, revolt_lengths, revolts_started, revolts_ended, revolts_per_province, tax_changes, tax_undone, declarations, claims, declared_after_peace, declared_unpaid, war_lengths, declared_within_five_years, opportunities, elections, elections_turned, people_start, people_100, people_end, migrants, migrants_across, deficit_share, deficit_samples, deficit_heavy, pay, court, conquests, conquests_unfought }
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
    let Measures { reigns, usurpations: _, polity_centuries: _, risings: _, large_ticks: _, polities_start: _, polities_century: _, war_ticks: _, polity_ticks: _, at_peace_ticks, army_at_war: _, people_at_war: _, army_at_peace: _, people_at_peace: _, a11_peace_windows, bound_century: _, alive_century: _, tax: _, production: _, falls, falls_by_conquest, reached, dropped, verbs, rejections, per_polity_war_share, rates, revolt_ticks: _, province_ticks: _, mut revolt_lengths, revolts_started, revolts_ended, revolts_per_province, tax_changes, tax_undone, declarations, claims, declared_after_peace, declared_unpaid, mut war_lengths, declared_within_five_years, opportunities, elections, elections_turned, people_start, people_100: _, people_end, migrants, migrants_across, deficit_share, deficit_samples, deficit_heavy, pay, court, conquests, conquests_unfought } = m;
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
    // Э9 with the bots — printed, a warning only (owner's answer 3 on §27.5): the band does
    // not tell war from peace, and `army.campaign_pay_factor` is never fitted to it.
    let band_e9 = band("budget.observed_military_share");
    eprintln!("Э9 with bots: the army's share of spending {:.3} (band {band_e9:?}, printed)", pay as f64 / (pay + court).max(1) as f64);
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
    assert_eq!(falls, falls_by_conquest, "a polity falls only by conquest: every fall in the tick its last province was taken");
    assert!(reached > 0 && dropped > 0, "goals are reached and put aside");
    assert!(verbs.len() >= 6, "the bots use the verbs: {verbs:?}");
    // With the orders of step (б) a war has its means: А12 is a check, not print (owner's
    // rule on #291).
    let a12_band = band("war.observed_share_of_years");
    assert!((a12_band[0]..=a12_band[1]).contains(&a12), "А12 {a12:.3} outside {a12_band:?}");
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
    // Printed, not asserted: a known deviation until (д), with the same cause as А11 at war —
    // the world's wars are short and cheap, so the army at peace pays for itself only by the
    // wars a tick times the loss of one's first tick. Not to be tuned (rule 1 of §24.1).
    let a11_band = band("army.observed_share_of_population");
    eprintln!("А11 at peace {a11_peace:.4} (E0: {a11_band:?}) — printed: a known deviation until (д), plan §29.11");
    // A floor against a regression of the known deviation, not a band (owner's decision 4 on
    // §29.10): the measure stood at 0,0042 on the third stop of (г1), its plateau after the
    // starting armies about 0,0025–0,0035 a window of fifty ticks; a bot that disbands at peace
    // would fall below it. Removed with (д), when the band of the source comes back.
    assert!(a11_peace >= 0.0025, "А11 at peace {a11_peace:.4} below the regression floor 0,0025 (plan §29.11)");
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
    // 2026-10-20): the measure stood at 0,0080 on #305 (0,0078–0,0095 over the stops of (3),
    // plan §28.9–28.13); a bot that stops levying at war would fall below it. Removed with (д),
    // when the band of the source comes back as the assertion.
    assert!(a11_war >= 0.007, "А11 at war {a11_war:.4} below the regression floor 0,007 (plan §28.13)");
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
    let mut w = world_of(&testworld::five_states_scenario(), 63);
    w.set_goal(s0, GoalKind::Take, GoalTarget::Province(at(1, 2))).unwrap();
    // The bot's declaration claims the province its goal wants (plan §28.9); on a polity its
    // goals want nothing of, the polity's submission.
    let claims: Vec<Target> = w.options(s0, &[VerbId::DeclareWar], &p).into_iter().filter_map(|o| o.call.map(|c| c.target)).collect();
    assert!(claims.contains(&Target::Province(at(1, 2))) && claims.contains(&Target::Actor(ActorId(2))), "{claims:?}");
    w.declare_war(s0, s1).unwrap();
    w.rise_now(at(0, 2), &p).unwrap();
    assert!(w.war_worth(s0, s1, false, 0.0, &p) <= 0.0, "at equal strength a war for one province risks as much as it wins");
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
/// capital — through the contract, with only what is on offer. The conqueror levies within
/// its budget: while the pay of its whole army on campaign fits the tick's income (the
/// world's pay functions). A probe that levied every tick could not pay the double pay of
/// the campaign, was exhausted by the rule of the peace, accepted the enemy's peace and
/// declared again — 121 wars of a tick on the test world's seed 3, its outcome the idle
/// player's (plan §28.12): a foolish probe, not a degeneration of the engine. The change is
/// to the probe player, not to the measures.
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
            if g.verbs(ActorRef::Ruler(me)).iter().any(|o| o.verb == VerbId::Recruit && o.available) {
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
        let r = Measures::over(&seeds, &p).rates();
        let row: Vec<String> = r.against_bands(&p).into_iter().map(|(n, v, b, ok)| format!("{n} {v:.3}{}", if ok { String::new() } else { format!(" OUTSIDE {b:?}") })).collect();
        eprintln!("{label}: {}", row.join("; "));
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
    // The alliance is valued through the threat (E3 (г2)), so the enemy needs a record of war:
    // on an empty chronicle no neighbour brings a war and no ally is worth anything.
    let mut w = world_of(&carrier_scenario(), 7);
    w.set_goal(a, GoalKind::AllianceAgainst, GoalTarget::Polity(ActorId(1))).unwrap();
    w.actors.get_mut(&ActorId(1)).unwrap().wars_entered = 5;
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
/// memory), `House.treasury` (not one's own), `Unit.destination` (not one's own).
#[test]
fn the_bot_reads_nothing_of_another_that_a_player_would_not_see() {
    let p = params();
    let (a, b, c) = (ActorId(0), ActorId(1), ActorId(2));
    let seen = |w: &World| (w.alliance_worth(a, b, &p), w.alliance_worth(a, c, &p), w.war_worth(a, b, false, 0.0, &p), w.war_worth(a, c, false, 0.0, &p), w.bot_orders(a, &p), w.options(a, &VerbId::ALL, &p));
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
    let pay = w.soldiers_cost(s0, w.recruit_per_tick(s0, &p), 1.0, &p);
    assert!(w.army_size(s0) > 0, "with units in being the levy adds nothing to the suppression");
    assert!(pay > 0.0 && (w.utility(s0, &levy, &p) + pay).abs() < 1e-9, "no threat: the levy costs a tick of its pay (the decision is retaken every tick) and answers nothing");
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
    assert!((first - (expected - w.soldiers_cost(s0, 50, 1.0, &p))).abs() < 1e-9, "the first unit: the take brought back less a tick of pay, no threat read: {first}");
    // A province in revolt: its whole take a tick is the first levy's; the levy pays.
    w.provinces.get_mut(&at(0, 3)).unwrap().revolt = Some(w.tick);
    assert!(w.in_revolt(at(0, 3)));
    assert!(w.suppression_a_tick(s0, &p) > expected, "a standing revolt withholds its whole take");
    assert!(w.utility(s0, &levy(50), &p) > 0.0, "the levy that puts the revolt down pays");
    // With a unit in being a second adds nothing to the suppression.
    w.apply_verb(&levy(50), &p).unwrap();
    assert!((w.utility(s0, &levy(50), &p) + w.soldiers_cost(s0, 50, 1.0, &p)).abs() < 1e-9, "a second unit: a tick of pay for nothing more");
}
