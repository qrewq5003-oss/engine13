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
/// made in one tick is not among them: the second is nothing (owner's rule on #291).
const CONFLICTS: [&str; 6] = ["at war", "no unmarried adult to marry", "the other house has no unmarried adult", "the relation is below the threshold", "a vassal fights only its lord", "a vassal makes no alliance"];

fn assert_conflicts_only(g: &Game) {
    let r = g.preempted_reasons();
    assert!(r.keys().all(|k| CONFLICTS.iter().any(|c| k.ends_with(c))), "a call preempted for another reason than a conflict of the tick: {r:?}");
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
                    other => panic!("seed {seed}: the generator drew {other:?}"),
                }
                assert_eq!(g.since, engine2::Tick(0));
            }
        }
    }
    eprintln!("aspirations over {polities} polities: {by_kind:?}; {vassals_free} vassals wanting to be free");
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
    // Without goals nobody acts: on the first tick, before any goal is drawn, nothing is
    // better than doing nothing.
    let quiet = all_verbs(testworld::five_states_scenario());
    let mut q = Game::new(&quiet, params(), 43).unwrap();
    let r = q.tick(&[]).unwrap();
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::Verb { .. })), "a bot that wants nothing does nothing");
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

/// The measures of E2 read again with every polity a bot (owner's rule on #291): В5, В6,
/// В7, В8, А11, А12, Д3, Э8; no polity falls but by conquest; goals are reached and put aside.
#[test]
fn on_generated_worlds_with_bots_the_measures_of_e2_hold_and_nothing_falls_but_by_conquest() {
    let p = params();
    let band = |k: &str| p.entries[k].band.unwrap();
    let (mut reigns, mut usurpations, mut polity_centuries) = (Vec::<f64>::new(), 0u64, 0.0f64);
    let (mut risings, mut large_ticks) = (0u64, 0u64);
    let (mut polities_start, mut polities_century) = (0usize, 0usize);
    let (mut war_ticks, mut polity_ticks, mut at_peace_ticks) = (0u64, 0u64, 0u64);
    let (mut army_at_war, mut people_at_war, mut army_at_peace, mut people_at_peace) = (0i64, 0i64, 0i64, 0i64);
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
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let mut g = Game::new(&s, params(), seed).unwrap();
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
                }
                if w.actors[a].provinces.len() >= 5 {
                    large_ticks += 1;
                }
                production += w.production_per_tick(*a, &p);
            }
            let sizes: BTreeMap<ActorId, usize> = living.iter().map(|a| (*a, w.actors[a].provinces.len())).collect();
            province_ticks += w.provinces.len() as u64;
            revolt_ticks += w.provinces.values().filter(|pr| pr.revolt.is_some()).count() as u64;
            let started: BTreeMap<ProvinceId, u32> = w.provinces.iter().filter_map(|(q, pr)| pr.revolt.map(|t| (*q, t.0))).collect();
            let r = g.tick(&[]).unwrap();
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
                    Event::Verb { actor: engine2::ActorRef::Ruler(a), verb: VerbId::DeclareWar, target: engine2::Target::Actor(b) } => {
                        *verbs.entry(VerbId::DeclareWar).or_insert(0) += 1;
                        declarations += 1;
                        if unpaid_seen.get(a).copied().unwrap_or(false) {
                            declared_unpaid += 1;
                        }
                        if last_peace.get(&((*a).min(*b), (*a).max(*b))).is_some_and(|t0| t0 + 1 == t) {
                            declared_after_peace += 1;
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
                    Event::GoalReached { .. } => reached += 1,
                    Event::GoalDropped { .. } => dropped += 1,
                    Event::Verb { verb, .. } => *verbs.entry(*verb).or_insert(0) += 1,
                    _ => {}
                }
            }
            tax += r.postings.iter().filter(|q| matches!(q.account, engine2::Account::Treasury(_)) && q.source == Source::Tax).map(|q| q.milli / 1000).sum::<i64>();
        }
        polity_centuries += n0 as f64 * 1.5;
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
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let b5 = mean(&reigns);
    let b6 = usurpations as f64 / polity_centuries;
    let b7 = risings as f64 / (large_ticks as f64 / 200.0);
    let b8 = polities_century as f64 / polities_start as f64;
    let a11_war = army_at_war as f64 / people_at_war.max(1) as f64;
    let a11_peace = army_at_peace as f64 / people_at_peace.max(1) as f64;
    let a12 = war_ticks as f64 / polity_ticks as f64;
    let never = per_polity_war_share.iter().filter(|s| **s < 0.1).count();
    let always = per_polity_war_share.iter().filter(|s| **s > 0.9).count();
    let d3 = bound_century as f64 / alive_century as f64;
    let e8 = tax as f64 / production as f64;
    eprintln!(
        "with bots, 20 worlds × 300 ticks: В5 {b5:.1} years ({} reigns); В6 {b6:.1} per century per polity; В7 {b7:.2}; В8 {b8:.2}; А11 at war {a11_war:.4}, at peace {a11_peace:.4} ({at_peace_ticks} peace polity-ticks); А12 share of polity-ticks at war {a12:.3} — polities under a tenth {never}, over nine tenths {always} of {}; Д3 at a century {d3:.2}; Э8 {e8:.3}; falls {falls} of which by conquest {falls_by_conquest}; goals reached {reached}, put aside {dropped}; verbs {verbs:?}; calls preempted by the tick's conflicts {rejections:?}; tax rates at the end — mean {:.3}, below the tithe {}, at it {}, above {}",
        reigns.len(),
        per_polity_war_share.len(),
        rates.iter().sum::<f64>() / rates.len().max(1) as f64,
        rates.iter().filter(|r| **r < 0.0999).count(),
        rates.iter().filter(|r| (0.0999..=0.1001).contains(*r)).count(),
        rates.iter().filter(|r| **r > 0.1001).count()
    );
    // The tax does not swing (owner's rule on #292): a step undone the next tick costs the
    // ruler half a step's price in legitimacy for nothing — the bots that read the pay of
    // one tick raised and lowered by turns, 28 % of their changes, and usurped themselves.
    let undone = tax_undone as f64 / tax_changes.max(1) as f64;
    eprintln!("tax changes {tax_changes}, undone the next tick {tax_undone} ({undone:.3})");
    // No war with an unpaid army (E3 (в), the class of the tax's swing): by the rule of the
    // peace an unpaid declarer is exhausted and the war ends at the first peace offered —
    // the bots declared four fifths of their wars the tick after a peace of the same pair,
    // most of them already short of their pay. What is left of the renewals is the loser's
    // revanche (a declarer that lost a province accepts the peace by the rule and declares
    // again) — printed, a question to the owner (plan §23g).
    let renewed = declared_after_peace as f64 / declarations.max(1) as f64;
    war_lengths.sort();
    eprintln!(
        "war declarations {declarations}, by an unpaid army {declared_unpaid}, the tick after a peace of the same pair {declared_after_peace} ({renewed:.3}); wars ended {}, length median {} ticks, of one tick {}",
        war_lengths.len(),
        war_lengths.get(war_lengths.len() / 2).copied().unwrap_or(0),
        war_lengths.iter().filter(|l| **l <= 1).count()
    );
    assert_eq!(declared_unpaid, 0, "a bot declared war with an unpaid army");
    assert!(undone < 0.05, "the bots' tax swings: {tax_undone} of {tax_changes} changes undone the next tick");
    // The tax as a flow (owner's rule on #291): no anchor, the bots keep near the tithe
    // by their utility — the mean rate inside a step of it.
    let mean_rate = rates.iter().sum::<f64>() / rates.len().max(1) as f64;
    assert!((mean_rate - 0.10).abs() <= 0.02, "the bots' mean tax {mean_rate:.3} drifted more than a step from the tithe");
    assert!(rejections.keys().all(|k| CONFLICTS.iter().any(|c| k.ends_with(c))), "a call preempted for another reason than a conflict of the tick: {rejections:?}");
    assert_eq!(falls, falls_by_conquest, "a polity falls only by conquest: every fall in the tick its last province was taken");
    assert!(reached > 0 && dropped > 0, "goals are reached and put aside");
    assert!(verbs.len() >= 6, "the bots use the verbs: {verbs:?}");
    // With the orders of step (б) a war has its means: А12, Д3 and В9 are checks, not
    // print (owner's rule on #291).
    let a12_band = band("war.observed_share_of_years");
    assert!((a12_band[0]..=a12_band[1]).contains(&a12), "А12 {a12:.3} outside {a12_band:?}");
    assert!(always * 10 < per_polity_war_share.len(), "«always at war»: {always} of {} polities over nine tenths of their ticks", per_polity_war_share.len());
    assert!(never * 2 < per_polity_war_share.len(), "«never at war»: {never} of {} polities under a tenth of their ticks", per_polity_war_share.len());
    let d3_band = band("diplomacy.observed_bound_share");
    assert!((d3_band[0]..=d3_band[1]).contains(&d3), "Д3 {d3:.2} outside {d3_band:?}");
    let b9 = revolt_ticks as f64 / province_ticks as f64;
    revolt_lengths.sort();
    let repeated = revolts_per_province.values().filter(|n| **n > 1).count();
    eprintln!(
        "В9 share of province-ticks in revolt {b9:.4}; revolts started {revolts_started}, put down {revolts_ended}, length median {} ticks, mean {:.1}, longest {}; provinces that rose again {repeated} of {}",
        revolt_lengths.get(revolt_lengths.len() / 2).copied().unwrap_or(0),
        revolt_lengths.iter().sum::<u32>() as f64 / revolt_lengths.len().max(1) as f64,
        revolt_lengths.last().copied().unwrap_or(0),
        revolts_per_province.len()
    );
    let b9_band = band("revolt.observed_share_of_provinces");
    assert!((b9_band[0]..=b9_band[1]).contains(&b9), "В9 {b9:.4} outside {b9_band:?}");
    assert!(revolts_ended * 2 > revolts_started, "the bots put down most revolts: {revolts_ended} of {revolts_started}");
    let b5_band = band("reign.observed_years");
    assert!((b5_band[0]..=b5_band[1]).contains(&b5), "В5 {b5:.1} outside {b5_band:?}");
    let b6_band = band("usurpation.observed_per_century");
    assert!((b6_band[0]..=b6_band[1]).contains(&b6), "В6 {b6:.1} outside {b6_band:?}");
    let b8_band = band("collapse.observed_polities_after_a_century");
    assert!((b8_band[0]..=b8_band[1]).contains(&b8), "В8 {b8:.2} outside {b8_band:?}");
    let e8_band = band("tax.observed_share_of_production");
    assert!((e8_band[0]..=e8_band[1]).contains(&e8), "Э8 {e8:.3} outside {e8_band:?}");
    let a11_band = band("army.observed_share_of_population");
    assert!((a11_band[0]..=a11_band[1]).contains(&a11_peace), "А11 at peace {a11_peace:.4} outside {a11_band:?}");
    let b7_band = band("revolt.observed_per_century");
    assert!((b7_band[0]..=b7_band[1]).contains(&b7), "В7 {b7:.2} outside {b7_band:?}");
    eprintln!("А11 at war {a11_war:.4} (E0: 2–3 % in war) — printed: the levy of agents at war is the bots' own choice");
}

#[test]
fn the_outcome_depends_on_the_strategy_and_on_the_seed_and_a_run_with_bots_saves_bit_for_bit() {
    let p = params();
    let (mut world_differs, mut differs_by_strategy, mut differs_by_seed) = (0usize, 0usize, 0usize);
    for seed in 1..=10u64 {
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
        if bots.0 != idle.0 || bots.1 != idle.1 || bots.2 != idle.2 {
            world_differs += 1;
        }
        if bots.0.outcome() != idle.0.outcome() {
            differs_by_strategy += 1;
        }
        if bots.0.outcome() != run(None, seed + 100).0.outcome() {
            differs_by_seed += 1;
        }
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
    // Played: polity 1 comes first and marries; polity 3's call is preempted, not an error.
    let r = g.tick(&[marry(3, h4), marry(1, h4)]).unwrap();
    let married = r.events.iter().find_map(|e| match e.event {
        Event::Marriage { a, b, .. } => Some((a, b)),
        _ => None,
    });
    let (x, y) = married.expect("one marriage");
    let houses_married = [g.world().persons[&x].house, g.world().persons[&y].house];
    assert!(houses_married.contains(&h1) && houses_married.contains(&h4), "the lower polity married: {houses_married:?}");
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Preempted { actor: ActorRef::Ruler(ActorId(3)), verb: VerbId::ProposeMarriage, .. })));
    assert_eq!(g.preempted(), 1);
    // The player at the place of its actor: a player playing polity 3 against a bot of
    // polity 1 loses the bride the same way; playing polity 0 it would win — the queue is
    // the actors', not the player's.
    let mut g = Game::new(&s, params(), 51).unwrap();
    g.set_bots(false);
    friendly(&mut g);
    g.set_player(Some(ActorRef::Ruler(ActorId(3))));
    let r = g.tick(&[marry(1, h4), marry(3, h4)]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Preempted { actor: ActorRef::Ruler(ActorId(3)), .. })), "the player's call came first in the list and still goes after polity 1");
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

#[test]
fn the_tax_is_a_lasting_decision_valued_as_a_flow_and_the_bot_comes_back_to_the_tithe_by_itself() {
    let p = params();
    let mut w = testworld::world(54);
    let a = ActorId(0);
    let u = |w: &World, v: VerbId| w.tax_utility(a, v, &p);
    // At the tithe both steps cost: the equilibrium of the shipped weights.
    assert_eq!(w.actors[&a].tax_rate_ppm, p.tax_default_ppm);
    assert!(u(&w, VerbId::RaiseTax) < 0.0 && u(&w, VerbId::LowerTax) < 0.0, "at the tithe: raise {:.2}, lower {:.2}", u(&w, VerbId::RaiseTax), u(&w, VerbId::LowerTax));
    // A step below: raising pays; a step above: lowering pays.
    w.actors.get_mut(&a).unwrap().tax_rate_ppm = p.tax_default_ppm - p.tax_step_ppm;
    assert!(u(&w, VerbId::RaiseTax) > 0.0, "below the tithe the step up pays: {:.2}", u(&w, VerbId::RaiseTax));
    w.actors.get_mut(&a).unwrap().tax_rate_ppm = p.tax_default_ppm + p.tax_step_ppm;
    assert!(u(&w, VerbId::LowerTax) > 0.0, "above the tithe the step down pays: {:.2}", u(&w, VerbId::LowerTax));
    // A step down and back up cancel in the take: the log.
    let years = f64::from(p.goal_horizon_ticks) / 2.0;
    assert!(years > 1.0);
    // At war with the pay short the take weighs more: at half the pay still no, at a third yes.
    w.actors.get_mut(&a).unwrap().tax_rate_ppm = p.tax_default_ppm;
    w.actors.get_mut(&a).unwrap().paid_share_ppm = 500_000;
    assert!(u(&w, VerbId::RaiseTax) < 0.0, "half the pay unmet: {:.2}", u(&w, VerbId::RaiseTax));
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

#[test]
fn a_bot_puts_a_revolt_down_at_peace_lets_it_wait_at_war_advances_when_strong_enough_and_holds_its_capital_when_not() {
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
    // At war a revolt waits; the units advance on the enemy when strong enough, else hold.
    let mut w = world_of(&testworld::five_states_scenario(), 63);
    w.set_goal(s0, GoalKind::Take, GoalTarget::Province(at(1, 2))).unwrap();
    w.declare_war(s0, s1).unwrap();
    w.rise_now(at(0, 2), &p).unwrap();
    let orders = w.bot_orders(s0, &p);
    assert_eq!(orders, vec![Order { actor: ActorRef::Ruler(s0), unit: u0, to: at(1, 2) }], "equal armies: advance on the wanted province, the revolt waits");
    let u1 = w.units_of(s1)[0];
    w.post(engine2::Account::Soldiers(u1), Source::Test, (3000 - w.soldiers(u1)) * 1000).unwrap();
    assert!(w.bot_orders(s0, &p).is_empty(), "outnumbered: hold the capital, where the unit stands");
    // Without a wanted province the target is the enemy's capital.
    let mut w = world_of(&testworld::five_states_scenario(), 64);
    w.declare_war(s0, s1).unwrap();
    assert_eq!(w.bot_orders(s0, &p), vec![Order { actor: ActorRef::Ruler(s0), unit: u0, to: at(1, 0) }]);
}

#[test]
fn the_attacked_accepts_a_peace_always_the_declarer_at_the_threshold_without_a_war_goal_or_exhausted() {
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
    // The declarer proposes a status quo: the attacked accepts always.
    assert!(w.peace_room(s0, s1, &p).is_ok());
    // The attacked proposes: the declarer refuses — its goal alive, its army paid.
    assert_eq!(w.peace_room(s1, s0, &p), Err("the declarer wants its war: the relation below the threshold, its war goal alive, its army paid"));
    // … accepts at the threshold,
    let mut x = fresh(72);
    x.relations.insert((s0, s1), p.treaty_accept_at_relation);
    assert!(x.peace_room(s1, s0, &p).is_ok());
    // … when its war goal is gone,
    let mut x = fresh(73);
    x.actors.get_mut(&s0).unwrap().goals.clear();
    assert!(x.peace_room(s1, s0, &p).is_ok(), "no war goal left");
    // … when exhausted: its army's pay short,
    let mut x = fresh(74);
    x.actors.get_mut(&s0).unwrap().paid_share_ppm = 800_000;
    assert!(x.peace_room(s1, s0, &p).is_ok(), "the pay short");
    // … and always when it is losing (lost more than it took).
    let mut x = fresh(75);
    x.wars.get_mut(&(s0, s1)).unwrap().taken = (0, 1);
    assert_eq!(x.losing_side(s0, s1), Some(s0));
    assert!(x.peace_room(s1, s0, &p).is_ok(), "the losing declarer accepts always");
    // A war of the starting facts has no declarer: a status quo is accepted both ways.
    let mut x = world_of(&testworld::five_states_scenario(), 76);
    x.declare_war(s0, s1).unwrap();
    x.wars.get_mut(&(s0, s1)).unwrap().by = None;
    x.set_goal(s0, GoalKind::Take, GoalTarget::Polity(s1)).unwrap();
    assert!(x.peace_room(s0, s1, &p).is_ok() && x.peace_room(s1, s0, &p).is_ok());
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
/// capital — through the contract, with only what is on offer.
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
                if !w.in_revolt(capital) && w.population(capital) > 0 {
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
    let _ = p;
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
    for world in 1..=WORLDS {
        let s = worldgen::generate(world, &p);
        let mut m = sentinel::Matrix::new();
        for seed in [1u64, 2, 3] {
            for strategy in ["none", "idle", "conqueror"] {
                m.add(play_strategy(&s, seed, strategy, TICKS));
            }
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
    for seed in [1u64, 2, 3] {
        for strategy in ["none", "idle", "conqueror"] {
            m.add(play_strategy(&s, seed, strategy, TICKS));
        }
    }
    let test_world: Vec<sentinel::Flag> = m.flags(&sentinel::Thresholds::default(), &[], None).into_iter().filter(|f| matches!(f, sentinel::Flag::StrategyIndependent { .. } | sentinel::Flag::SeedIndependent { .. })).collect();
    eprintln!("the test world: {test_world:?}");
    assert!(test_world.is_empty(), "the brief's acceptance on the test world: {test_world:?}");
    assert_eq!(strategy_flags, 0, "the outcome does not depend on the strategy: {flagged:?}");
    assert_eq!(seed_flags, 0, "the outcome does not depend on the seed: {flagged:?}");
}
