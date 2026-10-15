//! The channel of opportunities (E3 (в), owner's correction on §23, plan §23a): everything a
//! house or a court decides — a secession, a usurpation, the vote of an election, the union
//! of the court's faith, the policy of conversion, a marriage offered to a house — the
//! engine opens as an **opportunity** with its price, addressed to the one who decides; the
//! player and the bots answer it in the **next** tick's turn through the same contract
//! (`Simulation::play`), and an opportunity nobody answers **lapses**. The hooks
//! `house_would_*` (`agents.rs`) are only the bots' answers. One rule for the player and the
//! bots: the same opportunities, the same check of an answer, the same effect.
//!
//! **Who answers.** The opportunity names its decider: a house (`ActorRef::House`) or a
//! court (`ActorRef::Ruler`). A ruler speaks for its ruling house (as in diplomacy), so the
//! player on a throne answers what is offered to its house too — except a usurpation: a
//! member of the ruling house who would take the throne from its head is not the head.
//!
//! **Elections** (owner's answer 5 on §23): when a ruler under the rule of election dies,
//! the eldest adult of the ruling house holds the throne for the tick (the interim, as the
//! rule of E2 gave), and every house of the polity with an adult gets **a vote**: the
//! candidates are the eldest adult of each such house. At the next turn the votes are
//! counted by the weight of each house — its seats (governorships, the throne) and its
//! living adults — and the heaviest candidate takes the throne (`Elected`); a tie keeps the
//! interim. A house that does not vote (a lapsed opportunity, a world without the
//! contract) counts nothing: with no votes the interim stays, as in E2. If every house votes
//! for its own candidate the largest wins and the election is nearly a dynasty again — the
//! honest model with the houses' bargains is E4's (owner's note on §23).
//!
//! **The court's faith and its policy** (owner's answer 6 on §23: decisions of the court,
//! no new verbs). A court whose goal is the union with another faith is offered **the
//! union** every tick it holds the goal: taken, the ruling house takes that faith
//! (`HouseConverted`) — the price is in the world, the provinces of the old faith now
//! alienated from the court. **The policy of conversion** reaches a court by the channels
//! of contact the bank spreads along (`faith.rs`): the draw that would adopt a practice
//! offers the court the policy instead, at `policy.cost_legitimacy` — a court's decision,
//! not a practice caught from a neighbour (§20).
//!
//! **Lifetime.** Opened in a tick's phases (`opened` = that tick), answered at the start of
//! the next tick, before the verbs: the answer meets the world as the opening tick left it,
//! and the effect is checked again — a province taken meanwhile, a candidate dead, a ruler
//! changed: the opportunity is void (`OpportunityVoid`), not an error. A world ticked
//! without the contract — the systems' tests of E2, a game with its bots off — settles what
//! nobody answered by **the rule of E2** (a house takes its chance; nothing else is
//! decided): those systems are measured as they were, one tick later.

use serde::{Deserialize, Serialize};

use crate::event::Event;
use crate::ids::{ActorId, PersonId, ProvinceId};
use crate::params::Params;
use crate::time::Tick;
use crate::verbs::ActorRef;
use crate::world::{World, WorldError};

/// An opportunity open to a decider until the next tick's turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opportunity {
    pub id: u32,
    /// Who answers: a house, or a ruler for its court.
    pub to: ActorRef,
    pub what: Chance,
    /// The tick it was opened in; it is answered in the next tick's turn, or it lapses.
    pub opened: Tick,
}

/// What an opportunity offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Chance {
    /// A house that governs `province` may take the provinces it governs out of `polity`
    /// into a polity of its own (E0 В7, plan §21: the lot makes the chance, the house
    /// decides).
    Secession { polity: ActorId, province: ProvinceId },
    /// The house of `candidate` may take the throne of `polity` from `ruler` (E0 В6, owner's
    /// rule on #290: the lot makes the chance, the candidate's house decides).
    Usurpation { polity: ActorId, candidate: PersonId, ruler: PersonId },
    /// A house's vote for the next ruler of `polity`, elected among `candidates` (the eldest
    /// adult of each house with one); `interim` holds the throne until the count.
    Vote { polity: ActorId, candidates: Vec<PersonId>, interim: PersonId },
    /// The court of `polity` may take `faith` (a goal of union).
    Union { polity: ActorId, faith: crate::ids::ReligionId },
    /// The court of `polity` may adopt the policy of conversion, met at `from`.
    Policy { polity: ActorId, from: ActorId },
}

impl Chance {
    /// The plain name of the kind (events, the sentinel).
    pub fn kind(&self) -> &'static str {
        match self {
            Chance::Secession { .. } => "Secession",
            Chance::Usurpation { .. } => "Usurpation",
            Chance::Vote { .. } => "Vote",
            Chance::Union { .. } => "Union",
            Chance::Policy { .. } => "Policy",
        }
    }
}

/// A vote cast at a turn: the opportunity, who answered, the voting house, its candidate.
type Vote = (u32, ActorRef, crate::ids::HouseId, PersonId);

/// An answer to an opportunity: take it or decline it; a vote names its candidate
/// (declining a vote is abstaining).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reply {
    Take,
    Decline,
    Vote(PersonId),
}

/// The answer of `who` to the opportunity `opportunity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub who: ActorRef,
    pub opportunity: u32,
    pub reply: Reply,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AnswerError {
    #[error("no open opportunity {0}")]
    Unknown(u32),
    #[error("opportunity {0} is not offered to {1:?}")]
    NotYours(u32, ActorRef),
    #[error("opportunity {0} is answered twice")]
    Twice(u32),
    #[error("opportunity {0}: {1}")]
    BadReply(u32, &'static str),
}

impl World {
    /// Open an opportunity for `to`; it waits for the next tick's turn.
    pub(crate) fn open_opportunity(&mut self, to: ActorRef, what: Chance) {
        let o = Opportunity { id: self.next_opportunity, to, what, opened: self.tick };
        self.next_opportunity += 1;
        self.emit(Event::OpportunityOpened { opportunity: o.clone() });
        self.opportunities.push(o);
    }

    /// Whether `who` answers `o`: its decider, or a ruler for its ruling house — but never
    /// a usurpation of its own throne.
    pub fn answers_for(&self, who: ActorRef, o: &Opportunity) -> bool {
        if o.to == who {
            return true;
        }
        match (who, o.to, &o.what) {
            (_, _, Chance::Usurpation { .. }) => false,
            (ActorRef::Ruler(a), ActorRef::House(h), _) => self.alive(a) && self.ruling_house(a) == h,
            _ => false,
        }
    }

    /// The opportunities `who` answers in the coming turn, oldest first.
    pub fn opportunities_for(&self, who: ActorRef) -> Vec<Opportunity> {
        self.opportunities.iter().filter(|o| self.answers_for(who, o)).cloned().collect()
    }

    /// Check a turn's answers of `who` before anything is touched: every opportunity open
    /// and offered to `who`, none answered twice.
    pub fn answers_room(&self, who: ActorRef, answers: &[Answer]) -> Result<(), AnswerError> {
        let mut seen = std::collections::BTreeSet::new();
        for a in answers {
            let o = self.opportunities.iter().find(|o| o.id == a.opportunity).ok_or(AnswerError::Unknown(a.opportunity))?;
            if a.who != who || !self.answers_for(who, o) {
                return Err(AnswerError::NotYours(a.opportunity, a.who));
            }
            if !seen.insert(a.opportunity) {
                return Err(AnswerError::Twice(a.opportunity));
            }
            match (&o.what, a.reply) {
                (Chance::Vote { candidates, .. }, Reply::Vote(c)) if !candidates.contains(&c) => return Err(AnswerError::BadReply(a.opportunity, "not a candidate")),
                (Chance::Vote { .. }, Reply::Take) => return Err(AnswerError::BadReply(a.opportunity, "a vote names its candidate")),
                (Chance::Secession { .. } | Chance::Usurpation { .. } | Chance::Union { .. } | Chance::Policy { .. }, Reply::Vote(_)) => return Err(AnswerError::BadReply(a.opportunity, "not an election")),
                _ => {}
            }
        }
        Ok(())
    }

    /// The rule of E2 for an opportunity nobody answered in a world without the contract:
    /// a house takes its chance; nobody votes (the interim stays, as E2 elected); a court
    /// keeps its faith and takes no policy (E2 had neither decision).
    fn rule_reply(o: &Opportunity) -> Option<Reply> {
        match o.what {
            Chance::Secession { .. } | Chance::Usurpation { .. } => Some(Reply::Take),
            Chance::Vote { .. } => None,
            Chance::Union { .. } | Chance::Policy { .. } => Some(Reply::Decline),
        }
    }

    /// Settle the open opportunities, in the order they were opened: the answers given are
    /// applied; what nobody answered lapses — or, `by_rule`, is settled by the rule of E2,
    /// except what `player` answers (a player's silence is a choice).
    pub(crate) fn settle_opportunities(&mut self, answers: &[Answer], player: Option<ActorRef>, by_rule: bool, params: &Params) -> Result<(), WorldError> {
        let open = std::mem::take(&mut self.opportunities);
        // The votes of each election, counted once all are in.
        let mut elections: std::collections::BTreeMap<ActorId, (PersonId, Vec<Vote>)> = std::collections::BTreeMap::new();
        for o in open {
            let given = answers.iter().find(|a| a.opportunity == o.id);
            let rule = if by_rule && !player.is_some_and(|p| self.answers_for(p, &o)) { Self::rule_reply(&o) } else { None };
            let (by, reply) = match (given, rule) {
                (Some(a), _) => (a.who, a.reply),
                (None, Some(r)) => (o.to, r),
                (None, None) => {
                    self.emit(Event::OpportunityLapsed { id: o.id, to: o.to });
                    continue;
                }
            };
            match (reply, &o.what) {
                (Reply::Decline, _) => self.emit(Event::OpportunityDeclined { id: o.id, by }),
                (Reply::Vote(c), Chance::Vote { polity, interim, .. }) => {
                    let ActorRef::House(h) = o.to else { continue };
                    elections.entry(*polity).or_insert((*interim, Vec::new())).1.push((o.id, by, h, c));
                }
                (Reply::Vote(_), _) => self.emit(Event::OpportunityVoid { id: o.id, why: "not an election".to_string() }),
                (Reply::Take, _) => match self.take_opportunity(&o, params)? {
                    Ok(()) => self.emit(Event::OpportunityTaken { id: o.id, by }),
                    Err(why) => self.emit(Event::OpportunityVoid { id: o.id, why: why.to_string() }),
                },
            }
        }
        for (polity, (interim, votes)) in elections {
            self.count_votes(polity, interim, &votes, params)?;
        }
        Ok(())
    }

    /// The weight of a house in an election (owner's answer 5 on §23): its seats — the
    /// governorships its members hold and the throne — and its living adults.
    pub fn house_weight(&self, h: crate::ids::HouseId, params: &Params) -> i64 {
        let members = self.members(h);
        let polity = self.houses[&h].polity;
        let throne = i64::from(self.alive(polity) && members.contains(&self.actors[&polity].ruler));
        let seats = self.provinces.values().filter(|pr| pr.governor.is_some_and(|g| members.contains(&g))).count() as i64;
        let adults = members.iter().filter(|p| self.is_adult(**p, params)).count() as i64;
        seats + throne + adults
    }

    /// Count an election: every vote by its house's weight; the heaviest candidate takes the
    /// throne from the interim, a tie keeps the interim; a candidate who can no longer reign
    /// (dead, a minor, gone from the polity) counts nothing. The votes are void when the
    /// throne changed hands since it was opened (a usurpation taken first).
    fn count_votes(&mut self, polity: ActorId, interim: PersonId, votes: &[Vote], params: &Params) -> Result<(), WorldError> {
        if !self.alive(polity) || self.actors[&polity].ruler != interim {
            for (id, ..) in votes {
                self.emit(Event::OpportunityVoid { id: *id, why: "the throne changed hands".to_string() });
            }
            return Ok(());
        }
        let can_reign = |w: &World, c: PersonId| w.person_alive(c) && w.is_adult(c, params) && w.houses[&w.persons[&c].house].polity == polity;
        let mut tally: std::collections::BTreeMap<PersonId, i64> = std::collections::BTreeMap::new();
        for (id, by, h, c) in votes {
            self.emit(Event::OpportunityTaken { id: *id, by: *by });
            if can_reign(self, *c) {
                *tally.entry(*c).or_insert(0) += self.house_weight(*h, params);
            }
        }
        let total: i64 = tally.values().sum();
        let top = tally.values().copied().max().unwrap_or(0);
        let leaders: Vec<PersonId> = tally.iter().filter(|(_, w)| **w == top).map(|(c, _)| *c).collect();
        let winner = if leaders.len() == 1 { leaders[0] } else { interim };
        self.emit(Event::Elected { actor: polity, interim, winner, weight: tally.get(&winner).copied().unwrap_or(0) as u64, total: total as u64 });
        if winner == interim {
            return Ok(());
        }
        self.seat_elected(polity, winner, params)
    }

    /// The effect of taking an opportunity, checked again on the world as it stands; the
    /// inner `Err` names why it is void.
    fn take_opportunity(&mut self, o: &Opportunity, params: &Params) -> Result<Result<(), &'static str>, WorldError> {
        match o.what {
            Chance::Secession { polity, province } => {
                let ActorRef::House(house) = o.to else { return Ok(Err("a secession is a house's")) };
                if let Err(why) = self.secession_room(polity, province, house) {
                    return Ok(Err(why));
                }
                self.secede(province, house, params)?;
                Ok(Ok(()))
            }
            Chance::Usurpation { polity, candidate, ruler } => {
                if !self.alive(polity) || self.actors[&polity].ruler != ruler {
                    return Ok(Err("the throne changed hands"));
                }
                if !self.person_alive(candidate) || !self.is_adult(candidate, params) || candidate == ruler {
                    return Ok(Err("the candidate cannot take the throne"));
                }
                if self.houses[&self.persons[&candidate].house].polity != polity {
                    return Ok(Err("the candidate's house left the polity"));
                }
                self.usurp(polity, candidate, params)?;
                Ok(Ok(()))
            }
            Chance::Vote { .. } => Ok(Err("a vote names its candidate")),
            Chance::Union { polity, faith } => {
                if !self.alive(polity) || o.to != ActorRef::Ruler(polity) {
                    return Ok(Err("the polity has fallen"));
                }
                let house = self.ruling_house(polity);
                let from = self.houses[&house].religion;
                if from == faith {
                    return Ok(Err("the court prays so already"));
                }
                self.houses.get_mut(&house).unwrap().religion = faith;
                self.emit(Event::HouseConverted { house, from, to: faith });
                Ok(Ok(()))
            }
            Chance::Policy { polity, from } => {
                if !self.alive(polity) || o.to != ActorRef::Ruler(polity) {
                    return Ok(Err("the polity has fallen"));
                }
                let institution = crate::world::Institution::ConversionPolicy;
                if self.actors[&polity].institutions.contains(&institution) {
                    return Ok(Err("the court has the policy already"));
                }
                self.actors.get_mut(&polity).unwrap().institutions.insert(institution);
                self.pay_legitimacy(polity, params.policy_cost_legitimacy);
                self.emit(Event::Adopted { actor: polity, institution, from });
                Ok(Ok(()))
            }
        }
    }
}
