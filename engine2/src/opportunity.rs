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
}

impl Chance {
    /// The plain name of the kind (events, the sentinel).
    pub fn kind(&self) -> &'static str {
        match self {
            Chance::Secession { .. } => "Secession",
            Chance::Usurpation { .. } => "Usurpation",
        }
    }
}

/// An answer to an opportunity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reply {
    Take,
    Decline,
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
        }
        Ok(())
    }

    /// The rule of E2 for an opportunity nobody answered in a world without the contract:
    /// a house takes its chance.
    fn rule_reply(o: &Opportunity) -> Reply {
        match o.what {
            Chance::Secession { .. } | Chance::Usurpation { .. } => Reply::Take,
        }
    }

    /// Settle the open opportunities, in the order they were opened: the answers given are
    /// applied; what nobody answered lapses — or, `by_rule`, is settled by the rule of E2,
    /// except what `player` answers (a player's silence is a choice).
    pub(crate) fn settle_opportunities(&mut self, answers: &[Answer], player: Option<ActorRef>, by_rule: bool, params: &Params) -> Result<(), WorldError> {
        let open = std::mem::take(&mut self.opportunities);
        for o in open {
            let given = answers.iter().find(|a| a.opportunity == o.id);
            let (by, reply) = match given {
                Some(a) => (a.who, a.reply),
                None if by_rule && !player.is_some_and(|p| self.answers_for(p, &o)) => (o.to, Self::rule_reply(&o)),
                None => {
                    self.emit(Event::OpportunityLapsed { id: o.id, to: o.to });
                    continue;
                }
            };
            match reply {
                Reply::Decline => self.emit(Event::OpportunityDeclined { id: o.id, by }),
                Reply::Take => match self.take_opportunity(&o, params)? {
                    Ok(()) => self.emit(Event::OpportunityTaken { id: o.id, by }),
                    Err(why) => self.emit(Event::OpportunityVoid { id: o.id, why: why.to_string() }),
                },
            }
        }
        Ok(())
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
        }
    }
}
