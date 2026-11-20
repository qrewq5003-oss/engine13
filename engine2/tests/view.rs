//! E5, part 1 acceptance (plan §31.77, executor's decision № 28): the playable view and the
//! offer of a verb by its target.
//!
//! - the view shows the provinces, the units, the wars, the treaties and the houses of the
//!   world, and of the viewer's own polity alone its treasury, its house's treasury, the
//!   orders of the units it commands, its goals and its budget;
//! - **an offer by target is open exactly where the application goes through, and at the
//!   price the application charges** (lesson B42, of the target and of the price), on the
//!   test world, a generated world and the scenario of Milan; every call a bot would choose
//!   among is an open offer;
//! - **a player is given nothing hidden of another by any output of the contract** — the
//!   view, the offers by target, the offers of the verbs, the opportunities — by injection,
//!   with the reverse half: the same of its own does show;
//! - the outputs change nothing in the world;
//! - an order is offered with the path the world finds, and refused where there is none;
//! - the provinces of the view of Milan are the provinces of its map, id for id.

use std::collections::BTreeSet;

use engine2::politics::saturate;
use engine2::scenario::ScenarioData;
use engine2::sim::{Game, SimError, Simulation};
use engine2::{testworld, worldgen, Account, ActorId, ActorRef, Chance, GoalKind, GoalTarget, Order, Params, Source, Target, VerbCall, VerbId, World};

const SCENARIO: &str = include_str!("../../public/geodata2/milan_1477/scenario.toml");
const PROVINCES: &str = include_str!("../../public/geodata2/milan_1477/provinces.toml");
const MAP: &str = include_str!("../../public/geodata2/milan_1477/provinces.geojson");

fn params() -> Params {
    Params::load().unwrap()
}

fn milan() -> ScenarioData {
    ScenarioData::parse_parts(SCENARIO, PROVINCES).expect("the two files of milan_1477 join into the scenario")
}

fn all_verbs(mut s: ScenarioData) -> ScenarioData {
    for v in VerbId::ALL {
        s.verbs.insert(v.key().to_string(), v.key().replace('_', " "));
    }
    s
}

fn actor(g: &Game, id: &str) -> ActorId {
    ActorId(g.world().names.actors.iter().position(|(sid, _)| sid == id).expect("a polity of that id") as u32)
}

#[test]
fn the_view_shows_the_world_that_is_seen_and_of_the_viewer_alone_its_own() {
    let p = params();
    let mut g = Game::new(&all_verbs(testworld::five_states_scenario()), params(), 5).unwrap();
    g.set_bots(false);
    let (s0, s1, s2) = (ActorId(0), ActorId(1), ActorId(2));
    let (u0, u1) = (g.world().units_of(s0)[0], g.world().units_of(s1)[0]);
    {
        let w = g.world_mut();
        w.declare_war(s0, s1).unwrap();
        w.ally_now(s0, s2, &p).unwrap();
        w.make_vassal(ActorId(3), s2).unwrap();
        w.set_goal(s0, GoalKind::Take, GoalTarget::Polity(s1)).unwrap();
        w.set_goal(s1, GoalKind::Take, GoalTarget::Polity(s0)).unwrap();
        let (to0, to1) = (w.actors[&s1].capital, w.actors[&s0].capital);
        w.order_move(u0, to0).unwrap();
        w.order_move(u1, to1).unwrap();
        // A unit of a quality that makes its strength other than its number.
        w.units.get_mut(&u0).unwrap().quality = 80.0;
        // One of the living who holds no seat has died: the dead are not among the members.
        let seated: BTreeSet<engine2::PersonId> = w.actors.values().flat_map(|x| [Some(x.ruler), x.heir, x.regent]).flatten().chain(w.provinces.values().filter_map(|pr| pr.governor)).collect();
        let gone = *w.persons.iter().find(|(id, x)| x.died.is_none() && !seated.contains(id)).expect("someone without a seat").0;
        let now = w.tick;
        w.persons.get_mut(&gone).unwrap().died = Some(now);
    }
    g.set_player(Some(ActorRef::Ruler(s0)));
    let v = g.view();
    let w = g.world();
    let id = |a: ActorId| w.names.actors[a.index()].0.clone();
    let pid = |q: engine2::ProvinceId| w.names.provinces[q.index()].0.clone();
    assert_eq!(v.viewer, Some(id(s0)));

    // Every province with its owner, its people and the capital it is.
    assert_eq!(v.provinces.len(), w.provinces.len());
    for (q, pr) in &w.provinces {
        let shown = v.provinces.iter().find(|x| x.id == pid(*q)).expect("every province is shown");
        assert_eq!((shown.owner.clone(), shown.population, shown.land, shown.coastal), (id(pr.owner), w.population(*q), pr.land, pr.coastal));
        assert_eq!(shown.capital_of.is_some(), w.actors.values().any(|x| x.capital == *q));
    }
    assert_eq!(v.provinces.iter().find(|x| x.id == pid(w.actors[&s1].capital)).unwrap().capital_of, Some(id(s1)));
    let first = &v.actors[0];
    assert!(first.alive && first.capital == pid(w.actors[&s0].capital) && first.lord.is_none());
    assert_eq!(v.actors[3].lord, Some(id(s2)), "a vassal shows its lord");

    // Every unit where it stands; the orders of the viewer's own only.
    assert_eq!(v.units.len(), w.units.len());
    let (mine, theirs) = (v.units.iter().find(|x| x.id == u0.0).unwrap(), v.units.iter().find(|x| x.id == u1.0).unwrap());
    assert!(mine.yours && mine.owner == id(s0) && mine.soldiers == w.soldiers(u0) && mine.strength == w.unit_strength(u0));
    assert!(mine.strength != mine.soldiers as f64, "the strength by the battle's measure, not the number");
    assert_eq!(mine.destination, Some(pid(w.actors[&s1].capital)), "the order of a unit the viewer commands");
    assert!(w.units[&u1].destination.is_some() && !theirs.yours && theirs.destination.is_none(), "another's order is hidden");
    assert_eq!((theirs.position.clone(), theirs.soldiers), (pid(w.units[&u1].position), w.soldiers(u1)), "its place and its number are seen");

    // The war with who declared it and what was claimed; the treaty with its term.
    assert_eq!(v.wars.len(), w.wars.len());
    let war = v.wars.iter().find(|x| (x.a.clone(), x.b.clone()) == (id(s0), id(s1))).expect("the war of the pair");
    assert_eq!(war.declared_by, Some(id(s0)));
    assert_eq!(war.aim, Some(engine2::view::AimView::Polity(id(s1))));
    assert_eq!(war.since.year, w.year());
    let treaty = v.treaties.iter().find(|x| (x.a.clone(), x.b.clone()) == (id(s0), id(s2))).expect("the alliance of the pair");
    assert_eq!(treaty.kind, engine2::TreatyKind::Alliance);
    assert!(treaty.until.year > w.year());

    // The houses with their living members and seats; the treasury of the viewer's house only.
    assert_eq!(v.houses.len(), w.houses.len());
    let ruling = w.ruling_house(s0);
    assert!(w.persons.values().any(|x| x.died.is_some()), "a world with its dead");
    for (h, x) in &w.houses {
        let shown = v.houses.iter().find(|y| y.id == w.names.houses[h.index()].0).expect("every house is shown");
        assert_eq!(shown.members.len(), w.members(*h).len(), "its living members");
        assert_eq!(shown.polity, id(x.polity));
        assert_eq!(shown.treasury_kg, (*h == ruling).then(|| w.house_treasury(*h)), "the treasury of the viewer's house and of no other");
    }
    let ruler_name = &w.names.persons[w.actors[&s0].ruler.index()].1;
    let house = v.houses.iter().find(|y| y.ruling && y.polity == id(s0)).unwrap();
    let ruler = house.members.iter().find(|m| &m.name == ruler_name).expect("the ruler among the members of the ruling house");
    assert!(ruler.seats.contains(&engine2::view::SeatView::Ruler(id(s0))));

    // The viewer's own: its goals with their progress, what it lives on, its army.
    let own = v.own.as_ref().expect("the viewer's own");
    assert_eq!(own.goals.len(), w.actors[&s0].goals.len());
    let goal = own.goals.iter().find(|x| x.kind == GoalKind::Take).unwrap();
    assert_eq!(goal.target, engine2::view::GoalTargetView::Polity(id(s1)));
    assert_eq!(goal.progress, w.goal_progress(s0, w.actors[&s0].goals.iter().find(|x| x.kind == GoalKind::Take).unwrap()));
    assert_eq!((own.income_per_year_kg, own.army, own.levy_this_tick), (w.yearly_income(s0, &p), w.army_size(s0), w.recruit_per_tick(s0, &p)));
    let b = w.tick_budget(s0, &p);
    assert_eq!((own.budget.income_kg, own.budget.court_kg, own.budget.pay_due_kg), (b.income, b.court, b.pay_due));
    let text = serde_json::to_string(&v).unwrap();
    assert!(!text.contains(&format!("\"Polity\":\"{}\"}},\"since\"", id(s0))), "the goal of another — to take the viewer's land — is nowhere in the view");

    // No viewer: nothing of anybody's own.
    g.set_player(None);
    let v = g.view();
    assert!(v.viewer.is_none() && v.own.is_none());
    assert!(v.units.iter().all(|x| !x.yours && x.destination.is_none()));
    assert!(v.houses.iter().all(|x| x.treasury_kg.is_none()) && v.actors.iter().all(|x| x.treasury_kg.is_none()));
}

/// What the test tries of an offer: the target itself, and for a levy the sizes inside it.
fn calls_of(who: ActorRef, verb: VerbId, o: &engine2::TargetOffer) -> Vec<VerbCall> {
    let call = |target| VerbCall { actor: who, verb, target };
    match (verb, o.target, o.up_to) {
        (VerbId::Recruit, Target::Province(q), Some(n)) if n > 1 => vec![call(Target::Province(q)), call(Target::Levy(q, n)), call(Target::Levy(q, 1))],
        (VerbId::Recruit, Target::Province(q), _) => vec![call(Target::Province(q)), call(Target::Levy(q, 1))],
        _ => vec![call(o.target)],
    }
}

/// Lesson B42 of the target and of the price: over every polity, every enabled verb and
/// every target the world could name, the offer is open exactly where the application of
/// that call on a copy of the world goes through, and what the application takes is what
/// the offer names. Returns how many offers were tried, open and closed.
fn offers_agree_with_the_application(w: &World, enabled: &[VerbId], p: &Params, label: &str) -> (usize, usize) {
    let (mut open, mut closed) = (0, 0);
    for a in w.actors.keys().copied().filter(|a| w.alive(*a)) {
        let who = ActorRef::Ruler(a);
        for verb in enabled.iter().copied() {
            let offers = w.target_offers(who, verb, p);
            for o in &offers {
                assert_eq!(o.available, o.why.is_none(), "{label}: {verb:?} {:?}: a closed offer says why", o.target);
                for call in calls_of(who, verb, o) {
                    let at = format!("{label}: polity {a:?}, {call:?}");
                    let mut probe = w.clone();
                    let applied = probe.apply_verb(&call, p);
                    // An alliance the rule of the state refuses is taken by the world all the
                    // same — the gifts spent, `Refused` — and offered as closed.
                    let offered = probe.opportunities.len() > w.opportunities.len();
                    let went_through = if verb == VerbId::ProposeAlliance { applied.is_ok() && offered } else { applied.is_ok() };
                    assert_eq!(o.available, went_through, "{at}: offered {:?}, applied {applied:?}", o.why);
                    if !o.available {
                        closed += 1;
                        continue;
                    }
                    open += 1;
                    let (paid, legitimacy) = (w.treasury(a) - probe.treasury(a), probe.actors[&a].legitimacy);
                    // The treasury pays what the offer names — of every verb, with no
                    // exception; a loan brings what the offer names besides.
                    assert_eq!(paid + o.receives_kg.unwrap_or(0), o.cost_kg, "{at}: the treasury pays what the offer names");
                    assert_eq!(o.receives_kg.is_some(), verb == VerbId::TakeLoan, "{at}: only a loan brings money");
                    // The dowry is no price of the proposal: nothing leaves the house's
                    // treasury now, and the offer opened to the other house names it.
                    assert_eq!(o.dowry_kg.is_some(), verb == VerbId::ProposeMarriage, "{at}: only a marriage names a dowry");
                    if verb == VerbId::ProposeMarriage {
                        let h = w.ruling_house(a);
                        assert_eq!(w.house_treasury(h), probe.house_treasury(h), "{at}: the house pays nothing at the proposal");
                        let named = probe.opportunities.iter().rev().find_map(|x| match x.what {
                            Chance::Marriage { from, dowry_kg, .. } if from == h => Some(dowry_kg),
                            _ => None,
                        });
                        assert_eq!(named, o.dowry_kg, "{at}: the dowry the proposal names");
                    }
                    if o.cost_legitimacy == 0.0 {
                        assert_eq!(legitimacy, w.actors[&a].legitimacy, "{at}: no price in legitimacy");
                    } else {
                        assert_eq!(legitimacy, saturate(w.actors[&a].legitimacy, -o.cost_legitimacy), "{at}: the price in legitimacy the offer names, saturated as the world does");
                    }
                    let men = probe.army_size(a) - w.army_size(a);
                    match (verb, call.target) {
                        (VerbId::HireMercenaries, _) => assert_eq!(Some(men as u32), o.up_to, "{at}: as many hired as offered"),
                        (VerbId::Recruit, Target::Levy(_, n)) => assert_eq!(men, i64::from(n), "{at}: as many levied as asked within the offer"),
                        (VerbId::Recruit, _) => assert_eq!(Some(men as u32), o.up_to, "{at}: the whole levy of the offer"),
                        _ => {}
                    }
                }
            }
            // Every call a bot would choose among is an open offer.
            for call in w.options(a, &[verb], p).into_iter().filter_map(|x| x.call) {
                let found = offers.iter().any(|o| {
                    o.available
                        && match (call.target, o.target) {
                            (Target::Levy(q, n), Target::Province(r)) => q == r && o.up_to.is_some_and(|m| n <= m),
                            (x, y) => x == y,
                        }
                });
                assert!(found, "{label}: polity {a:?}: the bot's {call:?} is no open offer");
            }
        }
    }
    (open, closed)
}

#[test]
fn an_offer_by_target_is_open_exactly_where_the_application_goes_through_and_at_the_price_it_charges() {
    let p = params();
    let worlds: [(&str, ScenarioData, u64, &[u32]); 3] =
        [("the test world", all_verbs(testworld::five_states_scenario()), 4, &[0, 30, 70]), ("a generated world", all_verbs(worldgen::generate(7, &p)), 7, &[0, 40]), ("milan", milan(), 1, &[0, 25, 60])];
    let (mut open, mut closed) = (0, 0);
    for (name, s, seed, at) in worlds {
        let mut g = Game::new(&s, params(), seed).unwrap();
        let enabled: Vec<VerbId> = g.display().verbs.iter().map(|(v, _)| *v).collect();
        assert_eq!(enabled.len(), VerbId::ALL.len(), "{name}: every verb enabled");
        for tick in 0..=*at.last().unwrap() {
            if at.contains(&tick) {
                let (o, c) = offers_agree_with_the_application(g.world(), &enabled, &p, &format!("{name}, tick {tick}"));
                open += o;
                closed += c;
                // The offer of the verb is the offer of its targets: open where one of them is.
                for a in g.world().actors.keys().copied().filter(|a| g.world().alive(*a)) {
                    let who = ActorRef::Ruler(a);
                    for offer in g.verbs(who) {
                        let any = g.targets(who, offer.verb).iter().any(|o| o.available);
                        assert_eq!(offer.available, any, "{name}, tick {tick}, polity {a:?}: {:?} is offered ({:?}) where a target of it is open", offer.verb, offer.why);
                    }
                }
            }
            g.tick(&[]).unwrap();
        }
    }
    eprintln!("offers tried against the application: {open} open, {closed} closed");
    assert!(open > 500 && closed > 500, "the worlds gave both to try: {open} open, {closed} closed");
}

/// What the shell is offered of a declaration and of the verbs beside it: the polities and
/// their provinces as targets, **each at its own price** — a war on an ally costs the breach,
/// not the base — a peace only with whom there is a war, a levy from each province of its
/// own up to what that province can give, and nothing of a verb the scenario does not enable.
#[test]
fn the_offers_name_every_target_of_a_verb_and_price_each_target() {
    let p = params();
    let mut g = Game::new(&all_verbs(testworld::five_states_scenario()), params(), 8).unwrap();
    g.set_bots(false);
    let (s0, s1, s2) = (ActorId(0), ActorId(1), ActorId(2));
    let who = ActorRef::Ruler(s0);
    g.world_mut().ally_now(s0, s2, &p).unwrap();
    let w = g.world();
    let wars = g.targets(who, VerbId::DeclareWar);
    let of = |t: Target| wars.iter().find(|o| o.target == t).unwrap_or_else(|| panic!("no offer at {t:?}"));
    assert_eq!(wars.iter().filter(|o| matches!(o.target, Target::Actor(_))).count(), 4, "the four other polities");
    assert_eq!(wars.iter().filter(|o| matches!(o.target, Target::Province(_))).count(), 20, "and every province that is not its own");
    let (plain, ally) = (of(Target::Actor(s1)), of(Target::Actor(s2)));
    assert!(plain.available && ally.available);
    assert_eq!(plain.cost_legitimacy, p.legitimacy_cost_declare_war, "a war on a polity it has no treaty with: the base price");
    assert_eq!(ally.cost_legitimacy, w.war_price(s0, s2, &p), "a war on its ally: the price of that target");
    assert!(ally.cost_legitimacy > plain.cost_legitimacy, "the breach costs more: {} against {}", ally.cost_legitimacy, plain.cost_legitimacy);
    let theirs = w.actors[&s2].capital;
    assert_eq!(of(Target::Province(theirs)).cost_legitimacy, ally.cost_legitimacy, "a claim on the ally's province is a war on the ally");
    assert_eq!(plain.relation, Some(w.relation(s0, s1)));
    assert!(g.targets(who, VerbId::MakePeace).is_empty(), "no war, nobody to make peace with");
    let alliances = g.targets(who, VerbId::ProposeAlliance);
    assert!(alliances.iter().all(|o| o.cost_kg == w.embassy_cost(s0, &p) && o.threshold == Some(p.treaty_accept_at_relation.round() as i64)), "the embassy's gifts and the threshold at every target");
    let levies = g.targets(who, VerbId::Recruit);
    assert_eq!(levies.len(), w.actors[&s0].provinces.len(), "a levy from each of its provinces");
    for o in &levies {
        let Target::Province(q) = o.target else { panic!("a province") };
        assert_eq!(o.up_to, Some(w.recruit_per_tick(s0, &p).min(w.population(q)) as u32), "up to what the province can give this tick");
    }
    let loan = &g.targets(who, VerbId::TakeLoan)[0];
    assert_eq!(loan.receives_kg, Some(w.yearly_income(s0, &p).min(w.loan_room(s0, &p)).max(0)), "the sum of the loan is the world's");
    let (up, down) = (&g.targets(who, VerbId::RaiseTax)[0], &g.targets(who, VerbId::LowerTax)[0]);
    assert_eq!((up.cost_legitimacy, down.cost_legitimacy), (w.tax_step_cost(s0, true, &p), w.tax_step_cost(s0, false, &p)), "a step of the tax at its price, up and down");
    let order = &g.targets(who, VerbId::EnforceOrder)[0];
    assert_eq!((order.cost_kg, order.cost_legitimacy), (w.order_cost(s0, &p), -p.order_gain), "order is bought with the treasury and gives legitimacy back");
    // The rule of a target is read at every target: another's unit is not its to disband, a
    // province in revolt gives no levy, a vassal offers no alliance and fights only its lord.
    let u1 = w.units_of(s1)[0];
    assert!(w.target_room(who, VerbId::Disband, Target::Unit(u1), &p).is_err() && g.targets(who, VerbId::Disband).iter().all(|o| o.available && o.target != Target::Unit(u1)));
    let risen = *w.actors[&s0].provinces.iter().find(|q| **q != w.actors[&s0].capital).unwrap();
    let mut rebel = Game::new(&all_verbs(testworld::five_states_scenario()), params(), 8).unwrap();
    let now = rebel.world().tick;
    rebel.world_mut().provinces.get_mut(&risen).unwrap().revolt = Some(now);
    let levies = rebel.targets(who, VerbId::Recruit);
    let closed = levies.iter().find(|o| o.target == Target::Province(risen)).unwrap();
    assert_eq!((closed.available, closed.why.as_deref(), closed.up_to), (false, Some("the province is in revolt"), Some(0)));
    assert!(levies.iter().filter(|o| o.target != Target::Province(risen)).all(|o| o.available), "the others give their levy");
    let mut vassal = Game::new(&all_verbs(testworld::five_states_scenario()), params(), 8).unwrap();
    // (With the gifts of an embassy in its treasury: the caller's side reads the money first.)
    vassal.world_mut().post(Account::Treasury(s0), Source::Test, 1_000_000_000_000).unwrap();
    assert!(vassal.targets(who, VerbId::ProposeAlliance).iter().all(|o| o.why.as_deref() != Some("cannot pay") && o.why.as_deref() != Some("a vassal makes no alliance")), "free and able to pay: only the target's side can close the offer");
    vassal.world_mut().make_vassal(s0, s1).unwrap();
    let whys: Vec<(bool, Option<String>)> = vassal.targets(who, VerbId::ProposeAlliance).into_iter().map(|o| (o.available, o.why)).collect();
    assert!(!whys.is_empty() && whys.iter().all(|(open, why)| !open && why.as_deref() == Some("a vassal makes no alliance")), "{whys:?}");
    let wars = vassal.targets(who, VerbId::DeclareWar);
    assert!(wars.iter().find(|o| o.target == Target::Actor(s1)).unwrap().available, "its lord it may fight");
    assert_eq!(wars.iter().find(|o| o.target == Target::Actor(s2)).unwrap().why.as_deref(), Some("a vassal fights only its lord"));
    // A claim met already is no claim: the submission of its own vassal cannot be claimed
    // again, a province of the vassal can.
    let mut lord = Game::new(&all_verbs(testworld::five_states_scenario()), params(), 8).unwrap();
    lord.world_mut().make_vassal(s1, s0).unwrap();
    let wars = lord.targets(who, VerbId::DeclareWar);
    assert_eq!(wars.iter().find(|o| o.target == Target::Actor(s1)).unwrap().why.as_deref(), Some("the claim is met already: claim a province"));
    assert!(wars.iter().find(|o| o.target == Target::Province(lord.world().actors[&s1].capital)).unwrap().available);
    // No bank, no loan: the test world's polities have none, and with one the loan is offered.
    let mut banked = Game::new(&all_verbs(testworld::five_states_scenario()), params(), 8).unwrap();
    assert_eq!(banked.targets(who, VerbId::TakeLoan)[0].why.as_deref(), Some("no bank"));
    banked.world_mut().actors.get_mut(&s0).unwrap().institutions.insert(engine2::Institution::Bank);
    let loan = &banked.targets(who, VerbId::TakeLoan)[0];
    assert!(loan.available && loan.receives_kg.is_some_and(|kg| kg > 0), "with a bank the loan is offered: {loan:?}");
    // No marriage into a house of the enemy, whatever the relation.
    let mut foes = Game::new(&all_verbs(testworld::five_states_scenario()), params(), 8).unwrap();
    foes.world_mut().declare_war(s0, s1).unwrap();
    foes.world_mut().push_relation(s0, s1, 1000.0);
    assert!(foes.world().relation(s0, s1) >= p.treaty_accept_at_relation, "the relation above the threshold");
    let theirs: Vec<engine2::HouseId> = foes.world().houses.iter().filter(|(_, x)| x.polity == s1).map(|(h, _)| *h).collect();
    let matches = foes.targets(who, VerbId::ProposeMarriage);
    let into_the_enemy: Vec<&engine2::TargetOffer> = matches.iter().filter(|o| matches!(o.target, Target::House(h) if theirs.contains(&h))).collect();
    assert!(!into_the_enemy.is_empty() && into_the_enemy.iter().all(|o| !o.available), "{into_the_enemy:?}");
    assert!(into_the_enemy.iter().any(|o| o.why.as_deref() == Some("at war")), "a house of the enemy with someone to marry is closed by the war: {into_the_enemy:?}");
    // A verb the scenario does not enable is not offered at any target.
    let narrow = Game::new(&testworld::five_states_scenario(), params(), 8).unwrap();
    assert!(narrow.verbs(who).iter().all(|o| o.verb != VerbId::DeclareWar) && narrow.targets(who, VerbId::DeclareWar).is_empty());
}

/// Everything the contract hands a player, as bytes.
fn outputs(g: &Game, who: ActorRef) -> Vec<String> {
    let mut out = vec![serde_json::to_string(&g.view()).unwrap(), serde_json::to_string(&g.verbs(who)).unwrap(), serde_json::to_string(&g.opportunities(who)).unwrap()];
    out.extend(VerbId::ALL.iter().map(|v| serde_json::to_string(&g.targets(who, *v)).unwrap()));
    out
}

/// The registry of the hidden (plan §28.7, §30.9 p. 1) against every output of the
/// contract: another polity's treasury and goals, its count of the dead and the memory of
/// its pay, another house's treasury, the orders of another's units. Changed for everyone
/// but the viewer, they move no byte of what the player is given; changed for the viewer,
/// its own shows — so the guard is not blind.
#[test]
fn a_player_is_given_nothing_hidden_of_another_by_any_output_of_the_contract() {
    let p = params();
    let mut g = Game::new(&milan(), params(), 3).unwrap();
    let me = actor(&g, "milan");
    let who = ActorRef::Ruler(me);
    g.set_player(Some(who));
    for _ in 0..40 {
        g.tick(&[]).unwrap();
    }
    assert!(g.world().alive(me) && !g.world().wars.is_empty(), "a world with wars to offer peace in");
    let before = outputs(&g, who);
    let my_house = g.world().ruling_house(me);
    let somewhere = g.world().actors[&me].capital;
    let inject = |g: &mut Game, of_mine: bool| {
        let w = g.world_mut();
        let polities: Vec<ActorId> = w.actors.keys().copied().filter(|a| w.alive(*a) && (*a == me) == of_mine).collect();
        for a in polities {
            w.post(Account::Treasury(a), Source::Test, 7_000_000_000).unwrap();
            w.set_goal(a, GoalKind::Take, GoalTarget::Province(somewhere)).unwrap();
            let x = w.actors.get_mut(&a).unwrap();
            x.manpower_deficit += 5_000.0;
            x.paid_share_history_ppm.push(1);
        }
        let houses: Vec<engine2::HouseId> = w.houses.keys().copied().filter(|h| (*h == my_house) == of_mine).collect();
        for h in houses {
            w.post(Account::HouseTreasury(h), Source::Test, 3_000_000_000).unwrap();
        }
        let units: Vec<engine2::UnitId> = w.units.keys().copied().filter(|u| w.commands(who, *u) == of_mine).collect();
        for u in units {
            let to = if w.units[&u].position == somewhere { w.provinces[&somewhere].neighbours[0] } else { somewhere };
            w.units.get_mut(&u).unwrap().destination = Some(to);
        }
    };
    inject(&mut g, false);
    let after = outputs(&g, who);
    for (i, (x, y)) in before.iter().zip(&after).enumerate() {
        assert_eq!(x, y, "output {i} moved with the hidden state of others (0 the view, 1 the verbs, 2 the opportunities, then the targets of each verb)");
    }
    // The reverse half: the same of the viewer's own is shown.
    let own = |g: &Game| {
        let v = g.view();
        let mine = v.actors.iter().find(|x| Some(&x.id) == v.viewer.as_ref()).unwrap().treasury_kg;
        let house = v.houses.iter().find_map(|x| x.treasury_kg);
        let goals = v.own.as_ref().map(|x| x.goals.len());
        let orders: Vec<Option<String>> = v.units.iter().filter(|x| x.yours).map(|x| x.destination.clone()).collect();
        (mine, house, goals, orders)
    };
    let mine_before = own(&g);
    inject(&mut g, true);
    let mine_after = own(&g);
    assert!(mine_before.0 != mine_after.0 && mine_before.1 != mine_after.1 && mine_before.2 != mine_after.2 && mine_before.3 != mine_after.3, "the viewer's own treasury, its house's, its goals and its units' orders are shown: {mine_before:?} → {mine_after:?}");
    let _ = p;
}

/// The view's source names nothing of the hidden that has no use of the viewer's own in it,
/// nor the worth a bot gives a call.
#[test]
fn the_views_code_names_no_hidden_state() {
    let src = include_str!("../src/view.rs");
    let code: Vec<&str> = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
    for hidden in ["manpower_deficit", "paid_share_history_ppm", "utility"] {
        let read: Vec<&&str> = code.iter().filter(|l| l.contains(hidden)).collect();
        assert!(read.is_empty(), "view.rs reads `{hidden}`: {read:?}");
    }
}

#[test]
fn the_outputs_of_the_contract_change_nothing_in_the_world() {
    let mut g = Game::new(&milan(), params(), 2).unwrap();
    let me = actor(&g, "milan");
    let who = ActorRef::Ruler(me);
    g.set_player(Some(who));
    for _ in 0..10 {
        g.tick(&[]).unwrap();
    }
    let save = g.to_save().unwrap();
    let _ = outputs(&g, who);
    for u in g.world().units.keys().copied().collect::<Vec<_>>() {
        let _ = g.order_offer(&Order { actor: who, unit: u, to: g.world().actors[&me].capital });
    }
    assert_eq!(g.to_save().unwrap(), save, "reading the view, the offers and an order's path left the world byte for byte");
}

#[test]
fn an_order_is_offered_with_the_path_the_world_finds_and_refused_where_there_is_none() {
    let mut g = Game::new(&all_verbs(testworld::five_states_scenario()), params(), 6).unwrap();
    g.set_bots(false);
    let (s0, s1) = (ActorId(0), ActorId(1));
    let who = ActorRef::Ruler(s0);
    let (u0, u1) = (g.world().units_of(s0)[0], g.world().units_of(s1)[0]);
    let here = g.world().units[&u0].position;
    let own = |q: &engine2::ProvinceId| g.world().provinces[q].owner == s0;
    let next = *g.world().provinces[&here].neighbours.iter().find(|q| own(q)).expect("a neighbour of its own");
    let far = g.world().actors[&s0].provinces.iter().copied().max_by_key(|q| g.world().path(s0, here, *q).map_or(0, |x| x.len())).unwrap();
    assert_eq!(g.order_offer(&Order { actor: who, unit: u0, to: here }).unwrap().steps, 0, "where it stands: no march");
    let near = g.order_offer(&Order { actor: who, unit: u0, to: next }).unwrap();
    assert_eq!((near.steps, near.ticks), (1, 1), "a neighbour: a step, a tick");
    let path = g.world().path(s0, here, far).expect("its own land is one");
    assert!(path.len() > 1, "a province of its own further than a step");
    let offered = g.order_offer(&Order { actor: who, unit: u0, to: far }).unwrap();
    assert_eq!(offered.steps, path.len(), "the steps of the world's own path");
    assert_eq!(offered.ticks as usize, path.len().div_ceil(g.world().march_steps_per_tick(g.params())), "the ticks of the march at the world's pace");
    // Another's unit: the rule of the order refuses, as the application does.
    let err = g.order_offer(&Order { actor: who, unit: u1, to: next }).unwrap_err();
    assert!(matches!(err, SimError::Order(engine2::movement::OrderError::NotYours(..))), "{err}");
    assert!(g.tick_turn(&[], &[Order { actor: who, unit: u1, to: next }]).is_err(), "and the world refuses the order itself");
    // No path: the land of a polity it is at peace with is closed to its march. The offer
    // refuses with the reason; the world takes the order and puts it aside when the tick is
    // played. At war the same province is on a path, and the order is offered.
    let abroad = g.world().actors[&s1].capital;
    let err = g.order_offer(&Order { actor: who, unit: u0, to: abroad }).unwrap_err();
    assert!(matches!(err, SimError::Order(engine2::movement::OrderError::NoPath(u, q)) if u == u0 && q == abroad), "{err}");
    let r = g.tick_turn(&[], &[Order { actor: who, unit: u0, to: abroad }]).expect("the world takes an order with no path");
    assert!(r.events.iter().any(|e| matches!(e.event, engine2::Event::NoPath { unit, to } if unit == u0 && to == abroad)), "and puts it aside with its event");
    g.world_mut().declare_war(s0, s1).unwrap();
    assert!(g.order_offer(&Order { actor: who, unit: u0, to: abroad }).unwrap().steps >= 1, "at war the enemy's land is on a path");
}

/// The provinces of the view are the provinces of the map (the reviewer's requirement and
/// the cartographer's list, `docs/engine2_map.md` §37.3): the set of ids the view of Milan
/// shows is the set of `properties.id` of the scenario's `provinces.geojson`, and every
/// owner the view names is a polity it shows.
#[test]
fn the_provinces_of_the_view_of_milan_are_the_provinces_of_its_map_id_for_id() {
    let g = Game::new(&milan(), params(), 1).unwrap();
    let v = g.view();
    let map: serde_json::Value = serde_json::from_str(MAP).expect("the map is GeoJSON");
    let on_the_map: BTreeSet<String> = map["features"].as_array().expect("features").iter().map(|f| f["properties"]["id"].as_str().expect("a feature's id").to_string()).collect();
    let in_the_view: BTreeSet<String> = v.provinces.iter().map(|x| x.id.clone()).collect();
    assert_eq!(in_the_view.len(), v.provinces.len(), "no id twice in the view");
    assert_eq!(on_the_map.len(), map["features"].as_array().unwrap().len(), "no id twice on the map");
    assert_eq!(in_the_view, on_the_map, "the view and the map name the same provinces");
    let polities: BTreeSet<&String> = v.actors.iter().map(|a| &a.id).collect();
    assert!(v.provinces.iter().all(|x| polities.contains(&x.owner)), "every owner is a polity of the view");
    assert!(v.units.iter().all(|u| in_the_view.contains(&u.position) && polities.contains(&u.owner)), "every unit stands in a province of the map");
}
