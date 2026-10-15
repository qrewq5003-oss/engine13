//! The `Simulation` contract (plan §5) and its implementation on the E1 world.
//!
//! The shell talks to this trait only: create from a scenario, tick with the player's
//! verbs, view for the UI, snapshot for the chronicler, save and load. The sentinel reads
//! the game through `sentinel::Observed` (`observe.rs`).
//!
//! A load checks that the save belongs to the scenario it is loaded with — scenario id and
//! the hash of the compiled facts — so a run of one scenario cannot continue as another
//! (AGENTS §8, owner's review of PR #278).

use crate::economy::VerbError;
use crate::ids::ActorId;
use crate::event::{Event, Stamped};
use crate::opportunity::{Answer, AnswerError, Opportunity};
use crate::params::Params;
use crate::world::Institution;
use crate::save::{self, SaveError};
use crate::scenario::{Display, ScenarioData, SchemaError};
use crate::verbs::{ActorRef, Order, Target, VerbCall, VerbId, VerbOffer};
use crate::view::{self, ChronicleSnapshot, WorldView};
use crate::world::{TickReport, World, WorldError};

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("{0}")]
    Schema(#[from] SchemaError),
    #[error("{0}")]
    World(#[from] WorldError),
    #[error("{0}")]
    Save(#[from] SaveError),
    #[error("save belongs to scenario '{save}' (facts {save_hash:#x}), not to '{scenario}' (facts {scenario_hash:#x})")]
    WrongScenario { save: String, save_hash: u64, scenario: String, scenario_hash: u64 },
    #[error("verb {0:?} is not enabled by the scenario")]
    VerbNotEnabled(VerbId),
    #[error("{0}")]
    Verb(#[from] VerbError),
    #[error("{0}")]
    Order(#[from] crate::movement::OrderError),
    #[error("{0}")]
    Answer(#[from] AnswerError),
}

/// What the player brings to a tick (E3): calls of verbs, orders to units, answers to the
/// opportunities opened in the last tick (`opportunity.rs`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Turn {
    pub calls: Vec<VerbCall>,
    pub orders: Vec<Order>,
    pub answers: Vec<Answer>,
}

pub trait Simulation: Sized {
    fn new(scenario: &ScenarioData, params: Params, seed: u64) -> Result<Self, SimError>;
    /// Play one tick with the player's calls. Returns exactly what the tick did. A call of
    /// a verb the scenario does not enable is an error, and nothing is played.
    fn tick(&mut self, player: &[VerbCall]) -> Result<TickReport, SimError> {
        self.tick_turn(player, &[])
    }
    /// Play one tick with the player's calls and orders to units (E3 (б): orders are a
    /// channel of the contract, not a verb). An order to a unit the caller does not command
    /// is an error, and nothing is played.
    fn tick_turn(&mut self, player: &[VerbCall], orders: &[Order]) -> Result<TickReport, SimError> {
        self.play(&Turn { calls: player.to_vec(), orders: orders.to_vec(), answers: Vec::new() })
    }
    /// Play one tick with the player's whole turn: calls, orders and answers to the
    /// opportunities open to the player (E3 (в)). An answer to an opportunity that is not
    /// open, not the player's, or answered twice is an error, and nothing is played; an
    /// opportunity the player leaves unanswered lapses.
    fn play(&mut self, turn: &Turn) -> Result<TickReport, SimError>;
    /// The verbs on offer to `who`, computed by the one function agents use too.
    fn verbs(&self, who: ActorRef) -> Vec<VerbOffer>;
    /// The opportunities `who` answers in the coming turn (E3 (в)), the same the bots get.
    fn opportunities(&self, who: ActorRef) -> Vec<Opportunity>;
    fn view(&self) -> WorldView;
    fn chronicle(&self) -> ChronicleSnapshot;
    fn to_save(&self) -> Result<String, SimError>;
    /// Restore a run: the scenario and params are the shell's, the world is the save's.
    fn from_save(scenario: &ScenarioData, params: Params, save: &str) -> Result<Self, SimError>;
}

/// The order every call of a tick is applied in (owner's rule on #291): by the polity of the
/// caller — a ruler's polity, a house's polity — the ruler before the houses, the houses by
/// id, then the order the calls came in. The player's calls stand at the place of the
/// player's actor, a bot's at its own: one order for all.
pub fn application_order(world: &World, calls: &[VerbCall]) -> Vec<VerbCall> {
    let mut v: Vec<(usize, VerbCall)> = calls.iter().copied().enumerate().collect();
    v.sort_by_key(|(i, c)| (caller_key(world, c.actor), *i));
    v.into_iter().map(|(_, c)| c).collect()
}

/// The place of a caller in the one order of a tick: its polity, the ruler before the
/// houses, the houses by id.
fn caller_key(world: &World, who: ActorRef) -> (ActorId, u8, u32) {
    match who {
        ActorRef::Ruler(a) => (a, 0, 0),
        ActorRef::House(h) => (world.houses.get(&h).map(|x| x.polity).unwrap_or(ActorId(u32::MAX)), 1, h.0),
    }
}

/// The orders of a tick in the same one order for all.
pub fn order_application(world: &World, orders: &[Order]) -> Vec<Order> {
    let mut v: Vec<(usize, Order)> = orders.iter().copied().enumerate().collect();
    v.sort_by_key(|(i, o)| (caller_key(world, o.actor), *i));
    v.into_iter().map(|(_, o)| o).collect()
}

/// The bond a call makes between a pair — a peace or a war — that the other side of the
/// pair may make in the same tick too; the second is nothing.
fn bond_of(call: &VerbCall) -> Option<(VerbId, (ActorId, ActorId))> {
    match (call.verb, call.actor, call.target) {
        (VerbId::MakePeace | VerbId::DeclareWar, ActorRef::Ruler(a), Target::Actor(b)) if a != b => Some((call.verb, crate::army::pair(a, b))),
        _ => None,
    }
}

/// A run: the world plus what the scenario contributed besides facts.
#[derive(Debug)]
pub struct Game {
    world: World,
    display: Display,
    params: Params,
    /// Events of the last played tick, for the chronicle snapshot.
    last_events: Vec<Stamped>,
    /// The actor the player plays, if any (E3): its decisions are the player's calls, every
    /// other living polity is a bot. `None` — a run without a player (the sentinel's
    /// player-less run, the paired run's bot side).
    player: Option<ActorRef>,
    /// Calls preempted at application, by verb and reason — the player's and the bots'
    /// alike. Everyone decides on the state at the tick's start (brief §3), so an earlier
    /// call of the same tick can close a later one — two houses asking for the same bride;
    /// what was not on offer at the decision is never called by a bot (B42, tested), and a
    /// player's call that is not on offer is an error before anything is touched.
    preempted: std::collections::BTreeMap<String, u64>,
    /// Whether the bots decide at all. On in a game; the acceptance tests of the systems of
    /// E2 drive the world by hand and switch them off (`set_bots`).
    bots: bool,
}

impl Game {
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Who the player plays; `None` — nobody, every polity is a bot.
    pub fn player(&self) -> Option<ActorRef> {
        self.player
    }

    /// Give the player a role (E3): that actor's calls come from `tick(player)`, the bots
    /// leave it alone. `None` hands it back to the bots.
    pub fn set_player(&mut self, who: Option<ActorRef>) {
        self.player = who;
    }

    /// Calls preempted so far by an earlier call of the same tick (the player's and the
    /// bots' alike).
    pub fn preempted(&self) -> u64 {
        self.preempted.values().sum()
    }

    /// The preempted calls by verb and reason.
    pub fn preempted_reasons(&self) -> &std::collections::BTreeMap<String, u64> {
        &self.preempted
    }

    /// Switch the bots on or off. Off, every polity waits for the player's calls — the
    /// world of E2 as the tests of its systems drive it; on (the default), every living
    /// polity but the player's decides every tick.
    pub fn set_bots(&mut self, on: bool) {
        self.bots = on;
    }

    pub fn bots(&self) -> bool {
        self.bots
    }

    /// The constants this run plays with (the sentinel's observer reads the horizons).
    pub fn params(&self) -> &Params {
        &self.params
    }

    /// Mutable access for the acceptance tests (orders, injected stocks) until the verbs and
    /// agents of E3 give orders through the contract. Behind the `testing` feature so that
    /// nothing in a game build can change the world past the rules (review of PR #283).
    #[cfg(feature = "testing")]
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn display(&self) -> &Display {
        &self.display
    }

    /// Events of the last played tick (the chronicle and the sentinel read them).
    pub fn last_events(&self) -> &[Stamped] {
        &self.last_events
    }

    /// The one availability-and-cost function (lesson B42), the same for the player and
    /// for agents. Internal politics (E2 step 6b) prices the tax verbs in legitimacy, the
    /// economy (E2 step 4) decides the loan, the army (E2 step 5) the levy, the disbanding
    /// and the hire, diplomacy (E2 step 7) war, peace, alliance, marriage, the embassy and
    /// the gift — a proposal's offer carries the threshold the other side needs.
    fn offers(&self, who: ActorRef) -> Vec<VerbOffer> {
        self.display
            .verbs
            .iter()
            .map(|(verb, _)| {
                let fallen = match who {
                    ActorRef::Ruler(a) => !self.world.alive(a),
                    ActorRef::House(_) => false,
                };
                if !fallen && crate::diplomacy::is_diplomatic(*verb) {
                    let o = self.world.diplomacy_offer(*verb, who, &self.params);
                    return VerbOffer { verb: *verb, available: o.available, cost_kg: o.cost_kg, cost_legitimacy: o.cost_legitimacy, why: o.why, threshold: o.threshold };
                }
                let mut cost_kg = 0;
                let mut cost_legitimacy = 0;
                let (available, why) = match (verb, who) {
                    (_, ActorRef::Ruler(_)) if fallen => (false, Some("the polity has fallen")),
                    (VerbId::RaiseTax | VerbId::LowerTax | VerbId::EnforceOrder, ActorRef::House(_)) => (false, Some("a house does not rule the polity")),
                    (VerbId::EnforceOrder, ActorRef::Ruler(a)) => {
                        cost_kg = self.world.order_cost(a, &self.params);
                        cost_legitimacy = -(self.params.order_gain.round() as i64);
                        if cost_kg <= 0 {
                            (false, Some("no revenue to pay from"))
                        } else if self.world.treasury(a) < cost_kg {
                            (false, Some("cannot pay"))
                        } else {
                            (true, None)
                        }
                    }
                    (VerbId::RaiseTax, ActorRef::Ruler(a)) => {
                        cost_legitimacy = self.params.legitimacy_cost_raise_tax.round() as i64;
                        match self.world.tax_room(a, VerbId::RaiseTax, &self.params) {
                            Ok(()) => (true, None),
                            Err(why) => (false, Some(why)),
                        }
                    }
                    (VerbId::LowerTax, ActorRef::Ruler(a)) => {
                        cost_legitimacy = self.world.tax_step_cost(a, false, &self.params).round() as i64;
                        match self.world.tax_room(a, VerbId::LowerTax, &self.params) {
                            Ok(()) => (true, None),
                            Err(why) => (false, Some(why)),
                        }
                    }
                    (VerbId::Recruit, ActorRef::Ruler(a)) => {
                        if self.world.recruit_per_tick(a, &self.params) > 0 {
                            (true, None)
                        } else {
                            (false, Some("at the levy ceiling"))
                        }
                    }
                    (VerbId::Disband, ActorRef::Ruler(a)) => {
                        if self.world.units_of(a).is_empty() {
                            (false, Some("no unit to disband"))
                        } else {
                            (true, None)
                        }
                    }
                    (VerbId::HireMercenaries, ActorRef::Ruler(a)) => {
                        let n = self.world.hire_count(a, &self.params);
                        if n > 0 {
                            cost_kg = n * self.params.mercenary_advance_kg;
                            (true, None)
                        } else if self.world.recruit_room(a, &self.params) <= 0 {
                            (false, Some("at the levy ceiling"))
                        } else {
                            (false, Some("cannot pay an advance"))
                        }
                    }
                    (VerbId::TakeLoan, ActorRef::Ruler(a)) => {
                        if !self.world.actors.get(&a).is_some_and(|x| x.institutions.contains(&Institution::Bank)) {
                            (false, Some("no bank"))
                        } else if self.world.credit_closed(a) {
                            (false, Some("in default: no credit"))
                        } else if self.world.loan_room(a, &self.params) <= 0 {
                            (false, Some("at the debt ceiling"))
                        } else {
                            (true, None)
                        }
                    }
                    (VerbId::TakeLoan, ActorRef::House(_)) => (false, Some("a house does not borrow for the polity")),
                    _ => (false, Some("not implemented until its system (E2) and the agents (E3)")),
                };
                VerbOffer { verb: *verb, available, cost_kg, cost_legitimacy, why: why.map(str::to_string), threshold: None }
            })
            .collect()
    }

    fn enabled(&self, verb: VerbId) -> bool {
        self.display.verbs.iter().any(|(v, _)| *v == verb)
    }
}

impl Simulation for Game {
    fn new(scenario: &ScenarioData, params: Params, seed: u64) -> Result<Game, SimError> {
        let c = scenario.compile(&params)?;
        let world = World::from_facts(&c.facts, c.names, &params, seed)?;
        Ok(Game { world, display: c.display, params, last_events: Vec::new(), player: None, preempted: std::collections::BTreeMap::new(), bots: true })
    }

    fn play(&mut self, turn: &Turn) -> Result<TickReport, SimError> {
        let (player, orders) = (&turn.calls[..], &turn.orders[..]);
        // Every order of the player must be possible on the state as it stands.
        for o in orders {
            self.world.order_room(o)?;
        }
        // Every answer of the player to an opportunity open to the player's role.
        if let Some(a) = turn.answers.first() {
            match self.player {
                Some(p) => self.world.answers_room(p, &turn.answers)?,
                None => return Err(AnswerError::NotYours(a.opportunity, a.who).into()),
            }
        }
        // Validate every call before touching the world, so an error leaves nothing
        // half-played: the verb enabled, and every call of the player on offer on the state
        // as it stands. Tried on a copy in the order of application (a loan and the hire it
        // pays for); a call refused there but on offer by itself is a conflict of the tick,
        // not an error — the same rule as a bot's.
        if let Some(bad) = player.iter().find(|c| !self.enabled(c.verb)) {
            return Err(SimError::VerbNotEnabled(bad.verb));
        }
        if !player.is_empty() {
            let mut probe = self.world.clone();
            let mut bonded: std::collections::BTreeSet<(VerbId, (ActorId, ActorId))> = std::collections::BTreeSet::new();
            for call in application_order(&self.world, player) {
                let bond = bond_of(&call);
                if bond.is_some_and(|k| bonded.contains(&k)) {
                    continue;
                }
                match probe.apply_verb(&call, &self.params) {
                    Ok(()) => bonded.extend(bond),
                    Err(e) => {
                        let mut alone = self.world.clone();
                        if alone.apply_verb(&call, &self.params).is_err() {
                            return Err(e.into());
                        }
                    }
                }
            }
        }
        // First all decide — the bots on the state as it stands, before any call lands —
        // then all is applied in **one order for all** (owner's rule on #291): by the
        // polity of the caller, the player's calls at the place of the player's actor. A
        // player first would win every conflict of the tick, and the paired run player
        // against bot would measure the queue, not the strategy.
        let enabled: Vec<VerbId> = self.display.verbs.iter().map(|(v, _)| *v).collect();
        let mut calls: Vec<VerbCall> = player.to_vec();
        let mut all_orders: Vec<Order> = orders.to_vec();
        let mut answers: Vec<Answer> = turn.answers.clone();
        if self.bots {
            answers.extend(self.world.bot_answers(self.player, &self.params));
            all_orders.extend(self.world.bot_orders_all(self.player, &self.params));
            calls.extend(self.world.decide_all(&enabled, self.player, &self.params));
        }
        // The answers to the last tick's opportunities first: they answer the world as that
        // tick left it. With the bots off what nobody answered is settled by the rule of E2,
        // the player's silence excepted.
        self.world.settle_opportunities(&answers, self.player, !self.bots, &self.params)?;
        let mut bonded: std::collections::BTreeSet<(VerbId, (ActorId, ActorId))> = std::collections::BTreeSet::new();
        let mut disbanded: std::collections::BTreeSet<crate::ids::UnitId> = std::collections::BTreeSet::new();
        for call in application_order(&self.world, &calls) {
            // A peace or a war the other side of the pair already made this tick is nothing
            // (owner's rule on #291): both proposed it, it is made once.
            let bond = bond_of(&call);
            if bond.is_some_and(|k| bonded.contains(&k)) {
                continue;
            }
            match self.world.apply_verb(&call, &self.params) {
                Ok(()) => {
                    bonded.extend(bond);
                    if let (VerbId::Disband, Target::Unit(u)) = (call.verb, call.target) {
                        disbanded.insert(u);
                    }
                    self.world.emit(Event::Verb { actor: call.actor, verb: call.verb, target: call.target });
                }
                Err(e) => {
                    *self.preempted.entry(format!("{:?}: {e}", call.verb)).or_insert(0) += 1;
                    self.world.emit(Event::Preempted { actor: call.actor, verb: call.verb, target: call.target });
                }
            }
        }
        // Then the orders, in the same one order for all; an order to a unit the tick's
        // calls disbanded is preempted like a call.
        for o in order_application(&self.world, &all_orders) {
            // An order to a unit its caller disbanded this tick is nothing.
            if disbanded.contains(&o.unit) {
                continue;
            }
            if let Err(e) = self.world.apply_order(&o) {
                *self.preempted.entry(format!("Order: {e}")).or_insert(0) += 1;
            }
        }
        let report = self.world.tick(&self.params)?;
        self.last_events = report.events.clone();
        Ok(report)
    }

    fn verbs(&self, who: ActorRef) -> Vec<VerbOffer> {
        self.offers(who)
    }

    fn opportunities(&self, who: ActorRef) -> Vec<Opportunity> {
        self.world.opportunities_for(who)
    }

    fn view(&self) -> WorldView {
        view::world_view(&self.world, &self.display)
    }

    fn chronicle(&self) -> ChronicleSnapshot {
        view::chronicle_snapshot(&self.world, &self.last_events)
    }

    fn to_save(&self) -> Result<String, SimError> {
        Ok(save::to_save(&self.world)?)
    }

    fn from_save(scenario: &ScenarioData, params: Params, s: &str) -> Result<Game, SimError> {
        let c = scenario.compile(&params)?;
        let world = save::from_save(s)?;
        let scenario_hash = c.facts.hash();
        if world.scenario_id != c.facts.scenario_id || world.facts_hash != scenario_hash {
            return Err(SimError::WrongScenario {
                save: world.scenario_id.clone(),
                save_hash: world.facts_hash,
                scenario: c.facts.scenario_id.clone(),
                scenario_hash,
            });
        }
        Ok(Game { world, display: c.display, params, last_events: Vec::new(), player: None, preempted: std::collections::BTreeMap::new(), bots: true })
    }
}
