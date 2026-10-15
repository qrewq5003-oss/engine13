//! E3 (в) acceptance: the channel of opportunities (owner's correction on §23, plan §23a).
//!
//! - **parity**: a player answering its house's opportunities as the bot would leaves the
//!   world bit for bit as the bots leave it — one rule for both;
//! - a player's silence lets its chances lapse: the lot never drags the player's house into
//!   a secession or onto a throne (owner's rule on §21);
//! - a chance taken is made at the next turn's start, a declined one is nothing;
//! - an answer to what is not open, not the player's, or twice is an error, and nothing is
//!   played;
//! - open opportunities go through a save;
//! - a ruler answers for its ruling house, never for a usurpation of its own throne;
//! - the bots' answers: a usurpation of a weak ruler, not of a legitimate one; a secession
//!   only with a chance against the parent's army.

use engine2::opportunity::AnswerError;
use engine2::scenario::PersonData;
use engine2::sim::{Game, SimError, Simulation, Turn};
use engine2::{testworld, Account, ActorId, ActorRef, Answer, Chance, Event, HouseId, Opportunity, Params, PersonId, ProvinceId, ReligionId, ScenarioData, Source, Standing, World};

fn params() -> Params {
    Params::load().unwrap()
}

/// Province `(x, y)` of the 5×5 test world; state `x` owns column `x`, capital at `y = 0`.
fn at(x: u32, y: u32) -> ProvinceId {
    ProvinceId(y * 5 + x)
}

fn house(s: &ScenarioData, sid: &str) -> HouseId {
    HouseId(s.houses.keys().position(|k| k == sid).unwrap() as u32)
}

fn person(s: &ScenarioData, sid: &str) -> PersonId {
    PersonId(s.persons.keys().position(|k| k == sid).unwrap() as u32)
}

/// The test world with the second house `o0` of state 0 governing p05 and p10: the carrier
/// of a secession and the candidate of a usurpation (a governor first).
fn scenario() -> ScenarioData {
    let mut s = testworld::five_states_scenario();
    s.persons.insert("o0b".to_string(), PersonData { name: "Other 0 second".into(), house: "o0".into(), born: 1445, parent: None, spouse: None });
    s.provinces.get_mut("p05").unwrap().governor = Some("o0a".to_string());
    s.provinces.get_mut("p10").unwrap().governor = Some("o0b".to_string());
    s
}

/// Chances certain for state 0 while its legitimacy is low: a usurpation draw and the
/// rising of p05, alienated by faith.
fn eager() -> Params {
    let mut p = params();
    p.usurpation_per_tick = 1.0;
    p.revolt_per_tick = 0.5;
    p
}

fn game(seed: u64) -> Game {
    let mut g = Game::new(&scenario(), eager(), seed).unwrap();
    let w = g.world_mut();
    w.actors.get_mut(&ActorId(0)).unwrap().legitimacy = 20.0;
    w.provinces.get_mut(&at(0, 1)).unwrap().religion = ReligionId(1);
    g
}

fn o0() -> ActorRef {
    ActorRef::House(house(&scenario(), "o0"))
}

fn as_the_bot(g: &Game, who: ActorRef) -> Vec<Answer> {
    let p = g.params().clone();
    g.opportunities(who).iter().map(|o| Answer { who, opportunity: o.id, reply: g.world().bot_reply(o, &p) }).collect()
}

#[test]
fn a_player_answering_as_the_bot_would_leaves_the_world_bit_for_bit_as_the_bots_do() {
    let mut bots = game(1);
    let mut player = game(1);
    player.set_player(Some(o0()));
    let mut answered = 0;
    for _ in 0..8 {
        let answers = as_the_bot(&player, o0());
        answered += answers.len();
        let a = bots.tick(&[]).unwrap();
        let b = player.play(&Turn { answers, ..Turn::default() }).unwrap();
        assert_eq!(a, b, "the same tick, whoever answers");
    }
    assert!(answered >= 2, "the player's house was offered chances: {answered}");
    assert_eq!(bots.to_save().unwrap(), player.to_save().unwrap());
}

#[test]
fn a_players_silence_lets_its_chances_lapse_and_the_lot_never_drags_its_house() {
    let mut g = game(2);
    g.set_player(Some(o0()));
    let ActorRef::House(h) = o0() else { unreachable!() };
    let mut lapsed = 0;
    for _ in 0..8 {
        let offered: Vec<u32> = g.opportunities(o0()).iter().map(|o| o.id).collect();
        let r = g.tick(&[]).unwrap();
        for id in offered {
            assert!(r.events.iter().any(|e| matches!(e.event, Event::OpportunityLapsed { id: x, .. } if x == id)), "an unanswered chance lapses");
            lapsed += 1;
        }
        assert!(!r.events.iter().any(|e| matches!(e.event, Event::Secession { house, .. } if house == h)), "no secession of the player's house without its answer");
        assert!(!r.events.iter().any(|e| matches!(&e.event, Event::OpportunityTaken { by, .. } if *by == o0())));
    }
    assert!(lapsed >= 2, "the player's house was offered chances: {lapsed}");
}

#[test]
fn a_chance_taken_is_made_at_the_next_turns_start_and_a_declined_one_is_nothing() {
    let s = scenario();
    let ActorRef::House(h) = o0() else { unreachable!() };
    // Opened in the first tick: the usurpation (phase of the people) and the secession of
    // p05 (phase of collapse), both the house's.
    let first = |g: &mut Game| {
        g.set_player(Some(o0()));
        g.tick(&[]).unwrap();
        let open = g.opportunities(o0());
        let usurp = open.iter().find(|o| matches!(o.what, Chance::Usurpation { .. })).expect("a usurpation offered").clone();
        let secede = open.iter().find(|o| matches!(o.what, Chance::Secession { .. })).expect("a secession offered").clone();
        (usurp, secede)
    };
    // The usurpation taken: the governor on the throne as a usurper, at the next turn.
    let mut g = game(3);
    let (usurp, _) = first(&mut g);
    let Chance::Usurpation { candidate, .. } = usurp.what else { unreachable!() };
    assert_eq!(candidate, person(&s, "o0a"), "the eldest governor");
    let r = g.play(&Turn { answers: vec![Answer { who: o0(), opportunity: usurp.id, reply: engine2::Reply::Take }], ..Turn::default() }).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Succession { actor, to, how: Standing::Usurper, .. } if actor == ActorId(0) && to == candidate)));
    assert!(r.events.iter().any(|e| matches!(&e.event, Event::OpportunityTaken { id, by } if *id == usurp.id && *by == o0())));
    // The secession taken: the house's polity born at the next turn.
    let mut g = game(3);
    let (_, secede) = first(&mut g);
    let r = g.play(&Turn { answers: vec![Answer { who: o0(), opportunity: secede.id, reply: engine2::Reply::Take }], ..Turn::default() }).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::ActorBorn { from, house, .. } if from == ActorId(0) && house == h)));
    // Both declined: nothing.
    let mut g = game(3);
    let (usurp, secede) = first(&mut g);
    let decline = |o: &Opportunity| Answer { who: o0(), opportunity: o.id, reply: engine2::Reply::Decline };
    let r = g.play(&Turn { answers: vec![decline(&usurp), decline(&secede)], ..Turn::default() }).unwrap();
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::ActorBorn { .. } | Event::Succession { how: Standing::Usurper, .. })));
    assert_eq!(r.events.iter().filter(|e| matches!(&e.event, Event::OpportunityDeclined { by, .. } if *by == o0())).count(), 2);
}

#[test]
fn an_answer_to_what_is_not_open_not_the_players_or_twice_is_an_error_and_nothing_is_played() {
    let mut g = game(4);
    g.set_player(Some(o0()));
    g.tick(&[]).unwrap();
    let save = g.to_save().unwrap();
    let mine = g.opportunities(o0())[0].id;
    let theirs = g.world().opportunities.iter().find(|o| !g.world().answers_for(o0(), o)).expect("another decider's chance").id;
    let try_answers = |g: &mut Game, answers: Vec<Answer>| g.play(&Turn { answers, ..Turn::default() });
    let take = |id: u32| Answer { who: o0(), opportunity: id, reply: engine2::Reply::Take };
    assert!(matches!(try_answers(&mut g, vec![take(9_999)]), Err(SimError::Answer(AnswerError::Unknown(9_999)))));
    assert!(matches!(try_answers(&mut g, vec![take(theirs)]), Err(SimError::Answer(AnswerError::NotYours(..)))));
    assert!(matches!(try_answers(&mut g, vec![take(mine), take(mine)]), Err(SimError::Answer(AnswerError::Twice(..)))));
    let other = ActorRef::House(house(&scenario(), "h1"));
    assert!(matches!(try_answers(&mut g, vec![Answer { who: other, opportunity: mine, reply: engine2::Reply::Take }]), Err(SimError::Answer(AnswerError::NotYours(..)))), "answering in another's name");
    g.set_player(None);
    assert!(matches!(try_answers(&mut g, vec![take(mine)]), Err(SimError::Answer(AnswerError::NotYours(..)))), "no player, no answers");
    assert_eq!(g.to_save().unwrap(), save, "nothing was played");
}

#[test]
fn open_opportunities_go_through_a_save() {
    let mut a = game(5);
    a.tick(&[]).unwrap();
    assert!(!a.world().opportunities.is_empty());
    let save = a.to_save().unwrap();
    let mut b = Game::from_save(&scenario(), eager(), &save).unwrap();
    assert_eq!(a.world().opportunities, b.world().opportunities);
    for _ in 0..4 {
        assert_eq!(a.tick(&[]).unwrap(), b.tick(&[]).unwrap());
    }
}

#[test]
fn a_ruler_answers_for_its_ruling_house_but_never_for_a_usurpation_of_its_own_throne() {
    let s = scenario();
    let g = game(6);
    let w = g.world();
    let (s0, h0) = (ActorId(0), house(&s, "h0"));
    let to_h0 = |what: Chance| Opportunity { id: 100, to: ActorRef::House(h0), what, opened: w.tick };
    let secession = to_h0(Chance::Secession { polity: s0, province: at(0, 1) });
    let usurpation = to_h0(Chance::Usurpation { polity: s0, candidate: person(&s, "b0"), ruler: person(&s, "r0") });
    assert!(w.answers_for(ActorRef::Ruler(s0), &secession), "the ruler speaks for its house");
    assert!(!w.answers_for(ActorRef::Ruler(s0), &usurpation), "a brother's plot is not the head's to answer");
    assert!(w.answers_for(ActorRef::House(h0), &usurpation));
    assert!(!w.answers_for(ActorRef::Ruler(ActorId(1)), &secession), "another polity's ruler");
}

#[test]
fn the_bots_usurp_a_weak_ruler_not_a_legitimate_one_and_secede_only_with_a_chance_against_the_parent() {
    let p = params();
    let s = scenario();
    let c = s.compile(&p).unwrap();
    let mut w = World::from_facts(&c.facts, c.names, &p, 7).unwrap();
    let (s0, o0a, h) = (ActorId(0), person(&s, "o0a"), house(&s, "o0"));
    w.actors.get_mut(&s0).unwrap().legitimacy = 10.0;
    assert!(w.usurpation_utility(s0, o0a, &p) > 0.0, "a weak ruler is worth deposing: {:.2}", w.usurpation_utility(s0, o0a, &p));
    w.actors.get_mut(&s0).unwrap().legitimacy = 70.0;
    assert!(w.usurpation_utility(s0, o0a, &p) < 0.0, "a legitimate one is not: {:.2}", w.usurpation_utility(s0, o0a, &p));
    // The secession: the ruler's weakness against the odds of being crushed.
    w.actors.get_mut(&s0).unwrap().legitimacy = 50.0;
    let u = w.secession_utility(h, s0, at(0, 1), &p);
    assert!(u > 0.0, "a militia of two provinces against the parent's 600: {u:.2}");
    let unit = w.units_of(s0)[0];
    w.post(Account::Soldiers(unit), Source::Recruit, 100_000 * 1000).unwrap();
    let u = w.secession_utility(h, s0, at(0, 1), &p);
    assert!(u < 0.0, "against an army a hundred times its own: {u:.2}");
    w.actors.get_mut(&s0).unwrap().legitimacy = 0.0;
    assert!(w.secession_utility(h, s0, at(0, 1), &p) > 0.0, "a ruler with no legitimacy at all invites even a hopeless rising");
}

// ---------- elections (owner's answer 5 on §23) ----------

/// State 2 (election) with an adult of the ruling house (`b2`) and a second house `e2` of
/// two adults governing p07 and p12 — or, `one_seat`, of one adult governing p07.
fn elective(one_seat: bool) -> ScenarioData {
    let mut s = testworld::five_states_scenario();
    s.persons.insert("b2".to_string(), PersonData { name: "Brother 2".into(), house: "h2".into(), born: 1432, parent: None, spouse: None });
    s.houses.insert("e2".to_string(), engine2::scenario::HouseData { name: "Electors 2".into(), polity: "s2".into(), treasury_kg: 0, culture: "c0".into(), religion: "f0".into(), soldiers: 0 });
    s.persons.insert("e2a".to_string(), PersonData { name: "Elector 2 eldest".into(), house: "e2".into(), born: 1427, parent: None, spouse: None });
    s.provinces.get_mut("p07").unwrap().governor = Some("e2a".to_string());
    if !one_seat {
        s.persons.insert("e2b".to_string(), PersonData { name: "Elector 2 second".into(), house: "e2".into(), born: 1437, parent: None, spouse: None });
        s.provinces.get_mut("p12").unwrap().governor = Some("e2b".to_string());
    }
    s
}

/// The elective world, its ruler killed before the first tick; the votes are open after it.
fn election(one_seat: bool, bots: bool, player: Option<ActorRef>) -> (Game, ScenarioData) {
    let s = elective(one_seat);
    let p = params();
    let mut g = Game::new(&s, p.clone(), 8).unwrap();
    g.set_bots(bots);
    g.set_player(player);
    let r2 = person(&s, "r2");
    g.world_mut().kill(r2, engine2::event::Cause::Natural, &p).unwrap();
    let r = g.tick(&[]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Succession { actor, from, to, .. } if actor == ActorId(2) && from == r2 && to == person(&s, "b2"))), "the interim: the eldest adult of the ruling house");
    (g, s)
}

#[test]
fn an_elective_throne_goes_at_the_next_turn_to_the_candidate_of_the_heaviest_house_by_seats_and_adults() {
    let (mut g, s) = election(false, true, None);
    let (b2, e2a) = (person(&s, "b2"), person(&s, "e2a"));
    let votes: Vec<Opportunity> = g.world().opportunities.iter().filter(|o| matches!(o.what, Chance::Vote { .. })).cloned().collect();
    assert_eq!(votes.len(), 2, "a vote for each house with an adult");
    let Chance::Vote { candidates, interim, .. } = &votes[0].what else { unreachable!() };
    assert_eq!((candidates.clone(), *interim), (vec![e2a, b2], b2), "the eldest adult of each house, by house");
    // The weights: the ruling house the throne and one adult — 2; the electors two seats and
    // two adults — 4.
    assert_eq!(g.world().house_weight(house(&s, "h2"), g.params()), 2);
    assert_eq!(g.world().house_weight(house(&s, "e2"), g.params()), 4);
    let r = g.tick(&[]).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Elected { actor, winner, weight: 4, total: 6, .. } if actor == ActorId(2) && winner == e2a)), "each house for its own: the heavier wins");
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Succession { from, to, how: Standing::Elected, .. } if from == b2 && to == e2a)));
    assert_eq!(g.world().actors[&ActorId(2)].ruler, e2a);
    assert!(g.world().person_alive(b2), "the interim steps down alive");
    assert!(!g.world().provinces[&at(2, 1)].governor.is_some_and(|x| x == e2a), "the winner leaves its governorship");
}

#[test]
fn with_nobody_voting_the_interim_stays_as_in_e2_and_a_tie_keeps_the_interim() {
    let (mut g, s) = election(false, false, None);
    let r = g.tick(&[]).unwrap();
    assert!(!r.events.iter().any(|e| matches!(e.event, Event::Elected { .. })), "a world without the contract does not vote");
    assert_eq!(g.world().actors[&ActorId(2)].ruler, person(&s, "b2"));
    // One seat and one adult against the throne and one adult: 2 against 2.
    let (mut g, s) = election(true, true, None);
    let r = g.tick(&[]).unwrap();
    let b2 = person(&s, "b2");
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Elected { winner, weight: 2, total: 4, .. } if winner == b2)), "a tie keeps the interim");
    assert_eq!(g.world().actors[&ActorId(2)].ruler, b2);
}

#[test]
fn a_player_house_votes_through_the_contract_and_a_vote_for_no_candidate_is_an_error() {
    let s = elective(false);
    let e2 = ActorRef::House(house(&s, "e2"));
    let (mut g, s) = election(false, true, Some(e2));
    let (b2, e2a) = (person(&s, "b2"), person(&s, "e2a"));
    let mine = g.opportunities(e2);
    assert_eq!(mine.len(), 1);
    let vote = |reply| Turn { answers: vec![Answer { who: e2, opportunity: mine[0].id, reply }], ..Turn::default() };
    let save = g.to_save().unwrap();
    assert!(matches!(g.play(&vote(engine2::Reply::Vote(person(&s, "r1")))), Err(SimError::Answer(AnswerError::BadReply(..)))), "not a candidate");
    assert!(matches!(g.play(&vote(engine2::Reply::Take)), Err(SimError::Answer(AnswerError::BadReply(..)))), "a vote names its candidate");
    assert_eq!(g.to_save().unwrap(), save, "nothing was played");
    // The electors give their weight to the interim: 6 of 6, the interim stays.
    let r = g.play(&vote(engine2::Reply::Vote(b2))).unwrap();
    assert!(r.events.iter().any(|e| matches!(e.event, Event::Elected { winner, weight: 6, total: 6, .. } if winner == b2)));
    assert_ne!(g.world().actors[&ActorId(2)].ruler, e2a);
}
