//! The degeneration sentinel (brief §6, plan §8).
//!
//! Independent of any engine: it sees an [`Observation`] per tick and the typed
//! [`ObsEvent`]s of that tick through the [`Observed`] trait, on neutral string ids. One
//! tool therefore measures both engines on the same scenarios — the proof that the new one
//! is better, instead of the old regression base. The old engine's adapter arrives in E6.
//!
//! Two layers of flags:
//! - **single run** ([`Sentinel`]): pinned to a bound, bound hit, read of an absent actor,
//!   stagnation of the whole world;
//! - **matrix of runs** ([`Matrix`]): `Never` over the event kinds an engine declares
//!   possible; `Always` over **(kind, participant)** pairs — a battle in every game is
//!   normal, the death of the same actor in every game is a degeneration; outcome
//!   independent of the seed or of the player's strategy, compared on the **outcome**
//!   ([`EndState`]: who is alive, who owns what), not on every stock, which would differ by
//!   one person as soon as anything is random; and `history_lock` — the real course of
//!   events repeating in too many runs without a player. The real course lives in this
//!   crate (`history/<scenario>.toml`), so neither engine nor chronicler can read it by
//!   construction (owner's decision, plan §8), and its ids are checked against the world's
//!   starting observation, so a typo cannot make an outcome vacuously true.
//!
//! Thresholds are model parameters (E0 kind (в)); `history_lock`'s share is the owner's
//! (brief §11 №5).

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// A neutral id: whatever string the observed engine uses for the thing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObsId(pub String);

impl From<&str> for ObsId {
    fn from(s: &str) -> Self {
        ObsId(s.to_string())
    }
}

impl std::fmt::Display for ObsId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A rating on a scale with bounds (legitimacy, unity…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rating {
    pub who: ObsId,
    pub name: String,
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
}

/// A stock in world units (people, soldiers, kg).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stock {
    pub who: ObsId,
    pub name: String,
    pub value: i64,
}

/// What the sentinel sees of a world at the end of a tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub tick: u32,
    pub alive: Vec<ObsId>,
    /// `(province, owner)`, for outcomes about territory.
    pub owners: Vec<(ObsId, ObsId)>,
    pub ratings: Vec<Rating>,
    pub stocks: Vec<Stock>,
}

/// The outcome of a run: who is alive and who owns what (brief §1 — later also vassalage
/// and goals). Seed and strategy independence are judged on this, never on the stocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndState {
    pub alive: Vec<ObsId>,
    pub owners: Vec<(ObsId, ObsId)>,
}

impl Observation {
    pub fn outcome(&self) -> EndState {
        EndState { alive: self.alive.clone(), owners: self.owners.clone() }
    }
}

/// The typed events the sentinel understands. Engines map their own events onto these;
/// what has no counterpart goes to `Other` with its kind name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObsEvent {
    TickPlayed,
    Verb { actor: ObsId, verb: String },
    Battle { place: ObsId, a: ObsId, b: ObsId, winner: Option<ObsId> },
    Transfer { place: ObsId, from: ObsId, to: ObsId },
    War(ObsId, ObsId),
    Peace(ObsId, ObsId),
    Died(ObsId),
    Born(ObsId),
    Vassal { vassal: ObsId, of: ObsId },
    Freed(ObsId),
    /// A stock was clamped at a bound of its scale.
    BoundHit { who: ObsId, stock: String, source: String },
    /// The engine read the state of an actor that no longer exists (lesson B44).
    ReadAbsent(ObsId),
    Other(String),
}

impl ObsEvent {
    /// The kind name, the unit of the `Never` flag.
    pub fn kind(&self) -> String {
        match self {
            ObsEvent::TickPlayed => "TickPlayed".into(),
            ObsEvent::Verb { .. } => "Verb".into(),
            ObsEvent::Battle { .. } => "Battle".into(),
            ObsEvent::Transfer { .. } => "Transfer".into(),
            ObsEvent::War(..) => "War".into(),
            ObsEvent::Peace(..) => "Peace".into(),
            ObsEvent::Died(_) => "Died".into(),
            ObsEvent::Born(_) => "Born".into(),
            ObsEvent::Vassal { .. } => "Vassal".into(),
            ObsEvent::Freed(_) => "Freed".into(),
            ObsEvent::BoundHit { .. } => "BoundHit".into(),
            ObsEvent::ReadAbsent(_) => "ReadAbsent".into(),
            ObsEvent::Other(k) => k.clone(),
        }
    }

    /// Who the event is about: the ids that, paired with the kind, make the unit of the
    /// `Always` flag. The place of a battle or a transfer, the parties of a war, the actor
    /// who died…
    pub fn participants(&self) -> Vec<ObsId> {
        match self {
            ObsEvent::TickPlayed | ObsEvent::Other(_) => vec![],
            ObsEvent::Verb { actor, .. } => vec![actor.clone()],
            ObsEvent::Battle { place, .. } => vec![place.clone()],
            ObsEvent::Transfer { place, from, to } => vec![place.clone(), from.clone(), to.clone()],
            ObsEvent::War(a, b) | ObsEvent::Peace(a, b) => vec![a.clone(), b.clone()],
            ObsEvent::Died(id) | ObsEvent::Born(id) | ObsEvent::Freed(id) | ObsEvent::ReadAbsent(id) => vec![id.clone()],
            ObsEvent::Vassal { vassal, of } => vec![vassal.clone(), of.clone()],
            ObsEvent::BoundHit { who, .. } => vec![who.clone()],
        }
    }
}

/// What an engine exposes to the sentinel.
pub trait Observed {
    fn observe(&self) -> Observation;
    /// The events of the tick just played, mapped.
    fn events_of_last_tick(&self) -> Vec<ObsEvent>;
    /// The event kinds this engine's systems can produce. A declared kind that never
    /// appears across a matrix of runs is the `Never` flag. Systems register their kinds as
    /// they arrive (E2).
    fn possible_kinds(&self) -> Vec<String>;
}

/// Model thresholds (E0 kind (в)); the sentinel is checked by its own synthetic tests.
#[derive(Debug, Clone, PartialEq)]
pub struct Thresholds {
    /// A rating sitting on a bound for this many consecutive ticks is pinned.
    pub pinned_ticks: u32,
    /// Nothing in the world changing for this many consecutive ticks is stagnation.
    pub stagnation_ticks: u32,
    /// Share of player-less runs repeating the real course that trips `history_lock`
    /// (brief §11 №5: ≥ 50 %, owner).
    pub history_lock_share: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds { pinned_ticks: 10, stagnation_ticks: 20, history_lock_share: 0.5 }
    }
}

/// A degeneration found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Flag {
    PinnedToBound { who: ObsId, rating: String, ticks: u32 },
    BoundHit { who: ObsId, stock: String, count: u32 },
    ReadAbsent { who: ObsId },
    Stagnation { from_tick: u32, ticks: u32 },
    /// A declared kind that happened in no run.
    Never { kind: String },
    /// This event to this participant happened in every run.
    Always { kind: String, who: ObsId },
    SeedIndependent { strategy: String, runs: usize },
    StrategyIndependent { seed: u64, runs: usize },
    HistoryLock { share: f64, runs: usize },
}

/// Watches one run tick by tick.
#[derive(Debug)]
pub struct Sentinel {
    th: Thresholds,
    last: Option<Observation>,
    pinned: BTreeMap<(ObsId, String), u32>,
    bound_hits: BTreeMap<(ObsId, String), u32>,
    read_absent: BTreeSet<ObsId>,
    unchanged_since: Option<u32>,
    unchanged_ticks: u32,
    stagnation: Option<(u32, u32)>,
    kinds: BTreeSet<String>,
    pairs: BTreeSet<(String, ObsId)>,
    pinned_flags: BTreeMap<(ObsId, String), u32>,
}

impl Sentinel {
    pub fn new(th: Thresholds) -> Self {
        Sentinel {
            th,
            last: None,
            pinned: BTreeMap::new(),
            bound_hits: BTreeMap::new(),
            read_absent: BTreeSet::new(),
            unchanged_since: None,
            unchanged_ticks: 0,
            stagnation: None,
            kinds: BTreeSet::new(),
            pairs: BTreeSet::new(),
            pinned_flags: BTreeMap::new(),
        }
    }

    /// Feed one tick: the observation at its end and its events.
    pub fn watch(&mut self, obs: &Observation, events: &[ObsEvent]) {
        for e in events {
            let kind = e.kind();
            for who in e.participants() {
                self.pairs.insert((kind.clone(), who));
            }
            self.kinds.insert(kind);
            match e {
                ObsEvent::BoundHit { who, stock, .. } => {
                    *self.bound_hits.entry((who.clone(), stock.clone())).or_insert(0) += 1;
                }
                ObsEvent::ReadAbsent(who) => {
                    self.read_absent.insert(who.clone());
                }
                _ => {}
            }
        }
        for r in &obs.ratings {
            let key = (r.who.clone(), r.name.clone());
            let on_bound = r.value <= r.lo || r.value >= r.hi;
            let n = self.pinned.entry(key.clone()).or_insert(0);
            *n = if on_bound { *n + 1 } else { 0 };
            if *n >= self.th.pinned_ticks {
                let best = self.pinned_flags.entry(key).or_insert(0);
                *best = (*best).max(*n);
            }
        }
        // Stagnation: nothing moved and nothing happened but the tick itself.
        let moved = match &self.last {
            Some(prev) => prev.stocks != obs.stocks || prev.ratings != obs.ratings || prev.alive != obs.alive || prev.owners != obs.owners,
            None => true,
        };
        let happened = events.iter().any(|e| !matches!(e, ObsEvent::TickPlayed));
        if moved || happened {
            self.unchanged_since = None;
            self.unchanged_ticks = 0;
        } else {
            let since = *self.unchanged_since.get_or_insert(obs.tick);
            self.unchanged_ticks += 1;
            if self.unchanged_ticks >= self.th.stagnation_ticks {
                self.stagnation = Some((since, self.unchanged_ticks));
            }
        }
        self.last = Some(obs.clone());
    }

    /// The single-run flags so far.
    pub fn flags(&self) -> Vec<Flag> {
        let mut out = Vec::new();
        for ((who, rating), ticks) in &self.pinned_flags {
            out.push(Flag::PinnedToBound { who: who.clone(), rating: rating.clone(), ticks: *ticks });
        }
        for ((who, stock), count) in &self.bound_hits {
            out.push(Flag::BoundHit { who: who.clone(), stock: stock.clone(), count: *count });
        }
        for who in &self.read_absent {
            out.push(Flag::ReadAbsent { who: who.clone() });
        }
        if let Some((from_tick, ticks)) = self.stagnation {
            out.push(Flag::Stagnation { from_tick, ticks });
        }
        out
    }

    /// What the run contributes to a matrix.
    pub fn summary(&self, seed: u64, strategy: &str) -> RunSummary {
        RunSummary {
            seed,
            strategy: strategy.to_string(),
            kinds: self.kinds.clone(),
            pairs: self.pairs.clone(),
            outcome: self.last.as_ref().map(Observation::outcome).unwrap_or(EndState { alive: vec![], owners: vec![] }),
        }
    }
}

/// Drive an observed simulation for `ticks` ticks: `step` plays one tick, the sentinel
/// watches after each. Returns the sentinel and the run's summary.
pub fn run<S: Observed>(sim: &mut S, ticks: u32, seed: u64, strategy: &str, th: Thresholds, mut step: impl FnMut(&mut S)) -> (Sentinel, RunSummary) {
    let mut s = Sentinel::new(th);
    for _ in 0..ticks {
        step(sim);
        s.watch(&sim.observe(), &sim.events_of_last_tick());
    }
    let summary = s.summary(seed, strategy);
    (s, summary)
}

/// One run as the matrix sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub seed: u64,
    /// The player's strategy label; `"none"` is a run without a player.
    pub strategy: String,
    /// Event kinds that occurred.
    pub kinds: BTreeSet<String>,
    /// `(kind, participant)` pairs that occurred.
    pub pairs: BTreeSet<(String, ObsId)>,
    pub outcome: EndState,
}

/// The real course of events after the scenario's start, in the sentinel's own data
/// (`history/<scenario>.toml`): the outcomes a player-less run must not repeat too often.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub scenario: String,
    #[serde(rename = "outcome")]
    pub outcomes: Vec<Outcome>,
}

/// An outcome in the facts of the world.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Outcome {
    #[serde(rename = "dead")]
    Dead { id: ObsId },
    #[serde(rename = "alive")]
    Alive { id: ObsId },
    #[serde(rename = "owns")]
    Owns { province: ObsId, owner: ObsId },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum HistoryError {
    #[error("history file is not valid TOML of the schema: {0}")]
    Toml(String),
    #[error("history names '{0}', which is not in the world's starting observation")]
    UnknownId(ObsId),
}

impl History {
    pub fn parse(text: &str) -> Result<History, HistoryError> {
        toml::from_str(text).map_err(|e| HistoryError::Toml(e.to_string()))
    }

    /// Parse and check every id against the world's starting observation: an unknown id is
    /// an error, not an outcome that comes true by itself.
    pub fn load(text: &str, start: &Observation) -> Result<History, HistoryError> {
        let h = History::parse(text)?;
        h.check(start)?;
        Ok(h)
    }

    pub fn check(&self, start: &Observation) -> Result<(), HistoryError> {
        let actors: BTreeSet<&ObsId> = start.alive.iter().collect();
        let provinces: BTreeSet<&ObsId> = start.owners.iter().map(|(p, _)| p).collect();
        for o in &self.outcomes {
            match o {
                Outcome::Dead { id } | Outcome::Alive { id } => {
                    if !actors.contains(id) {
                        return Err(HistoryError::UnknownId(id.clone()));
                    }
                }
                Outcome::Owns { province, owner } => {
                    if !provinces.contains(province) {
                        return Err(HistoryError::UnknownId(province.clone()));
                    }
                    if !actors.contains(owner) {
                        return Err(HistoryError::UnknownId(owner.clone()));
                    }
                }
            }
        }
        Ok(())
    }

    /// Does this outcome repeat the real course in full? A history with no outcomes names
    /// no course, so nothing repeats it (otherwise an empty file would lock every run).
    pub fn matches(&self, end: &EndState) -> bool {
        !self.outcomes.is_empty()
            && self.outcomes.iter().all(|o| match o {
                Outcome::Dead { id } => !end.alive.contains(id),
                Outcome::Alive { id } => end.alive.contains(id),
                Outcome::Owns { province, owner } => end.owners.iter().any(|(p, a)| p == province && a == owner),
            })
    }
}

/// Flags over several runs.
#[derive(Debug, Default)]
pub struct Matrix {
    runs: Vec<RunSummary>,
}

impl Matrix {
    pub fn new() -> Self {
        Matrix::default()
    }

    pub fn add(&mut self, run: RunSummary) {
        self.runs.push(run);
    }

    /// `possible` — the kinds the engine declares (`Observed::possible_kinds`).
    pub fn flags(&self, th: &Thresholds, possible: &[String], history: Option<&History>) -> Vec<Flag> {
        let mut out = Vec::new();
        if self.runs.is_empty() {
            return out;
        }
        for kind in possible {
            if !self.runs.iter().any(|r| r.kinds.contains(kind)) {
                out.push(Flag::Never { kind: kind.clone() });
            }
        }
        if self.runs.len() >= 2 {
            let mut common: BTreeSet<(String, ObsId)> = self.runs[0].pairs.clone();
            for r in &self.runs[1..] {
                common = common.intersection(&r.pairs).cloned().collect();
            }
            for (kind, who) in common {
                out.push(Flag::Always { kind, who });
            }
        }
        // Seed independence: same strategy, different seeds, identical outcome.
        let mut by_strategy: BTreeMap<&str, Vec<&RunSummary>> = BTreeMap::new();
        for r in &self.runs {
            by_strategy.entry(r.strategy.as_str()).or_default().push(r);
        }
        for (strategy, runs) in &by_strategy {
            let seeds: BTreeSet<u64> = runs.iter().map(|r| r.seed).collect();
            if seeds.len() >= 2 && runs.iter().all(|r| r.outcome == runs[0].outcome) {
                out.push(Flag::SeedIndependent { strategy: strategy.to_string(), runs: runs.len() });
            }
        }
        // Strategy independence: same seed, different strategies, identical outcome.
        let mut by_seed: BTreeMap<u64, Vec<&RunSummary>> = BTreeMap::new();
        for r in &self.runs {
            by_seed.entry(r.seed).or_default().push(r);
        }
        for (seed, runs) in &by_seed {
            let strategies: BTreeSet<&str> = runs.iter().map(|r| r.strategy.as_str()).collect();
            if strategies.len() >= 2 && runs.iter().all(|r| r.outcome == runs[0].outcome) {
                out.push(Flag::StrategyIndependent { seed: *seed, runs: runs.len() });
            }
        }
        if let Some(h) = history {
            let no_player: Vec<&RunSummary> = self.runs.iter().filter(|r| r.strategy == "none").collect();
            if !no_player.is_empty() {
                let repeating = no_player.iter().filter(|r| h.matches(&r.outcome)).count();
                let share = repeating as f64 / no_player.len() as f64;
                if share >= th.history_lock_share {
                    out.push(Flag::HistoryLock { share, runs: no_player.len() });
                }
            }
        }
        out
    }
}
