//! The presentation model (plan §5): what the UI and the chronicler get, built from the
//! state, the world's names and the scenario's presentation data. Not a mirror of `World`.
//!
//! This is the only module that reads the display currency (`Display::currency`, guard
//! `display_currency_is_read_only_by_the_view`): the engine counts in kilograms of
//! wheat-equivalent, the view converts for showing.

use serde::Serialize;

use crate::agents::{GoalKind, GoalTarget};
use crate::event::Stamped;
use crate::ids::{ActorId, HouseId, PersonId, ProvinceId};
use crate::params::Params;
use crate::scenario::Display;
use crate::time::{HalfYear, Tick};
use crate::verbs::ActorRef;
use crate::world::{Institution, Land, Names, Standing, Succession, TreatyKind, WarAim, World};

/// One polity as shown.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActorView {
    pub id: String,
    pub name: String,
    pub provinces: Vec<String>,
    pub population: i64,
    /// The treasury — **the viewer's own polity's only** (plan §30.9 p. 1, owner's decision
    /// 2026-11-11): another's treasury is hidden, as its inner stocks are (answer 3 of
    /// §28.7), and with no viewer nobody's is shown. The debt below stays open to all: loans
    /// and defaults are public events.
    pub treasury_kg: Option<i64>,
    /// Treasury in the scenario's display currency, for showing only; with `treasury_kg`.
    pub treasury_display: Option<f64>,
    pub debt_kg: i64,
    /// Debt in the scenario's display currency, for showing only.
    pub debt_display: f64,
    pub legitimacy: f64,
    /// The tax rate, parts per million of production (owner's rule 2026-10-17, plan §28.7):
    /// an open fact — the people flee the heavier tax (`population.rs`), so they know it,
    /// and the bot's `flight_worth` reads the neighbours' rates by the same right.
    pub tax_rate_ppm: i64,
    /// The ruler's name, and the regent's while the ruler is a minor.
    pub ruler: String,
    pub regent: Option<String>,
    /// The relation with every other polity, by its id (E2 step 7): what a proposal is
    /// measured against (`VerbOffer::threshold`).
    pub relations: Vec<(String, f64)>,
    /// Whether the polity stands; a fallen one keeps its record and its id.
    pub alive: bool,
    /// The capital, by the province's id.
    pub capital: String,
    /// The lord this polity is a vassal of, by its id.
    pub lord: Option<String>,
    /// How the ruler came to the throne, and the rule the next one will come by: public
    /// facts of an accession (`Event::Succession`).
    pub standing: Standing,
    pub succession: Succession,
    pub institutions: Vec<Institution>,
}

/// A moment of the world's calendar, as shown: the year and its half.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WhenView {
    pub year: i32,
    pub half_year: u8,
}

/// A siege as anyone sees it: who stands under the walls, for how many ticks, of how many
/// the province holds (`World::siege_ticks`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiegeView {
    pub by: String,
    pub ticks: u32,
    pub of: u32,
}

/// One province as shown. Its id is the scenario's — the id its shape carries on the map.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProvinceView {
    pub id: String,
    pub name: String,
    /// The owner, by the polity's id.
    pub owner: String,
    pub population: i64,
    pub land: Land,
    pub coastal: bool,
    /// The polity whose capital it is, by its id.
    pub capital_of: Option<String>,
    /// The governor's name, where a house holds the seat.
    pub governor: Option<String>,
    pub culture: String,
    pub religion: String,
    pub siege: Option<SiegeView>,
    pub in_revolt: bool,
}

/// One unit as shown: where it stands and how many it is, for every unit of the world
/// (plan §28.7: positions and numbers are seen, orders are not).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnitView {
    /// The unit's number: a unit has no name.
    pub id: u32,
    pub owner: String,
    /// The house whose retinue it is, by its id.
    pub house: Option<String>,
    /// Where it stands, by the province's id.
    pub position: String,
    pub soldiers: i64,
    /// Its strength by the battle's measure (`World::unit_strength`) — what a bot reads of
    /// another's units too (`World::war_sides_strength`).
    pub strength: f64,
    pub mercenary: bool,
    /// Whether the viewer commands it (`World::commands`).
    pub yours: bool,
    /// Where it is ordered to, by the province's id — **of a unit the viewer commands
    /// only**: another's orders are hidden (plan §28.7).
    pub destination: Option<String>,
}

/// What a declaration claimed (plan §28.9: a public act): a province, or a polity's submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum AimView {
    Province(String),
    Polity(String),
}

/// A war as shown: the pair (the lower id first, as the world keeps it), since when, who
/// declared and what each side claimed, the provinces each has taken of the other, and
/// the principal pair whose war this one is an obligation of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WarView {
    pub a: String,
    pub b: String,
    pub since: WhenView,
    pub declared_by: Option<String>,
    pub aim: Option<AimView>,
    pub counter_aim: Option<AimView>,
    /// Provinces taken in the war: by `a` from `b`, and by `b` from `a`.
    pub taken: (u32, u32),
    pub obligation_of: Option<(String, String)>,
}

/// A treaty as shown: the pair, its kind, the moment it runs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TreatyView {
    pub a: String,
    pub b: String,
    pub kind: TreatyKind,
    pub until: WhenView,
}

/// The seat a person holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SeatView {
    /// The ruler of the polity, by its id.
    Ruler(String),
    /// The designated heir of the polity.
    Heir(String),
    /// The regent of the polity.
    Regent(String),
    /// The governor of the province, by its id.
    Governor(String),
}

/// A living member of a house as shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonView {
    pub name: String,
    pub born: i32,
    /// The living spouse: the name, and the spouse's house by its id.
    pub spouse: Option<(String, String)>,
    pub seats: Vec<SeatView>,
}

/// A house as shown: its living members; **its treasury only when it is the viewer's**
/// (plan §28.7: another house's treasury is hidden).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HouseView {
    pub id: String,
    pub name: String,
    /// The polity the house lives in, by its id.
    pub polity: String,
    /// Whether it rules that polity.
    pub ruling: bool,
    pub members: Vec<PersonView>,
    pub treasury_kg: Option<i64>,
}

/// What a goal aims at, by the scenario's ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum GoalTargetView {
    None,
    Province(String),
    Polity(String),
    Region(String),
    Religion(String),
    Route(String, String),
}

/// A goal of the viewer's polity with its progress (`World::goal_progress`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GoalView {
    pub kind: GoalKind,
    pub target: GoalTargetView,
    pub since: WhenView,
    pub progress: f64,
}

/// The lines of the viewer's budget of a tick as the world would settle it now
/// (`World::tick_budget`), kg.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BudgetView {
    pub production_kg: i64,
    pub income_kg: i64,
    pub tribute_due_kg: i64,
    pub seats_kg: i64,
    pub court_kg: i64,
    pub interest_kg: i64,
    pub pay_due_kg: i64,
}

/// What only the viewer's polity is shown of itself: its goals — **another's are hidden**
/// (plan §28.7) — what it lives on, its army and the men it can levy this tick.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OwnView {
    pub goals: Vec<GoalView>,
    pub income_per_year_kg: i64,
    pub budget: BudgetView,
    pub army: i64,
    pub levy_this_tick: i64,
}

/// The world as shown.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorldView {
    pub year: i32,
    pub half_year: u8,
    pub currency: String,
    pub actors: Vec<ActorView>,
    /// The viewer's polity, by its id; none in a run without a player.
    pub viewer: Option<String>,
    pub provinces: Vec<ProvinceView>,
    pub units: Vec<UnitView>,
    pub wars: Vec<WarView>,
    pub treaties: Vec<TreatyView>,
    pub houses: Vec<HouseView>,
    /// The viewer's own; none in a run without a player.
    pub own: Option<OwnView>,
}

/// The chronicler's snapshot: the period's facts with names, no numbers the model should
/// quote (lessons of the old engine: forbidden claims, no engine keys).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChronicleSnapshot {
    pub year: i32,
    pub half_year: u8,
    pub alive_actors: Vec<String>,
    /// The typed events of the period, for the narrative layer to word.
    pub events: Vec<Stamped>,
    pub names: Names,
}

fn half(w: &World) -> u8 {
    match w.tick.half_year() {
        HalfYear::First => 1,
        HalfYear::Second => 2,
    }
}

fn when(w: &World, t: Tick) -> WhenView {
    WhenView {
        year: t.year(w.start_year),
        half_year: match t.half_year() {
            HalfYear::First => 1,
            HalfYear::Second => 2,
        },
    }
}

/// The world as the player's role sees it (E5, plan §31.77): everything the registry of
/// the seen holds (plan §28.7, §30.9 p. 1) — the provinces, the units where they stand, the
/// wars, the treaties, the houses and their living members — and, of the viewer's own polity
/// alone, its treasury, its house's treasury, the orders of the units it commands, its goals
/// and its budget. The viewer is the polity the player rules, or the polity the player's
/// house lives in (plan §30.11, an assumption); `None` — a run without a player, and
/// nothing of anybody's own is shown. Derived from the world; nothing here is stored.
pub fn world_view(w: &World, display: &Display, params: &Params, player: Option<ActorRef>) -> WorldView {
    let viewer: Option<ActorId> = player.and_then(|who| match who {
        ActorRef::Ruler(a) => Some(a),
        ActorRef::House(h) => w.houses.get(&h).map(|house| house.polity),
    });
    let viewers_house: Option<HouseId> = player.and_then(|who| match who {
        ActorRef::Ruler(a) => w.actors.contains_key(&a).then(|| w.ruling_house(a)),
        ActorRef::House(h) => w.houses.contains_key(&h).then_some(h),
    });
    let aid = |a: ActorId| w.names.actors[a.index()].0.clone();
    let pid = |p: ProvinceId| w.names.provinces[p.index()].0.clone();
    let hid = |h: HouseId| w.names.houses[h.index()].0.clone();
    let person = |p: PersonId| w.names.persons[p.index()].1.clone();
    let aim = |x: WarAim| match x {
        WarAim::Province(q) => AimView::Province(pid(q)),
        WarAim::Polity(b) => AimView::Polity(aid(b)),
    };
    let currency = &display.currency;
    let actors = w
        .actors
        .iter()
        .map(|(id, a)| {
            let (sid, name) = &w.names.actors[id.index()];
            let treasury_kg = (viewer == Some(*id)).then(|| w.treasury(*id));
            let debt_kg = w.debt(*id);
            ActorView {
                id: sid.clone(),
                name: name.clone(),
                provinces: a.provinces.iter().map(|p| w.names.provinces[p.index()].1.clone()).collect(),
                population: a.provinces.iter().map(|p| w.population(*p)).sum(),
                treasury_kg,
                treasury_display: treasury_kg.map(|kg| kg as f64 / currency.kg_per_unit),
                debt_kg,
                debt_display: debt_kg as f64 / currency.kg_per_unit,
                legitimacy: a.legitimacy,
                tax_rate_ppm: a.tax_rate_ppm,
                ruler: w.names.persons[a.ruler.index()].1.clone(),
                regent: a.regent.map(|r| w.names.persons[r.index()].1.clone()),
                relations: w.actors.keys().filter(|b| *b != id).map(|b| (w.names.actors[b.index()].0.clone(), w.relation(*id, *b))).collect(),
                alive: w.alive(*id),
                capital: pid(a.capital),
                lord: a.vassal_of.map(aid),
                standing: a.standing,
                succession: a.succession,
                institutions: a.institutions.iter().copied().collect(),
            }
        })
        .collect();
    let capitals: std::collections::BTreeMap<ProvinceId, ActorId> = w.actors.iter().filter(|(a, _)| w.alive(**a)).map(|(a, x)| (x.capital, *a)).collect();
    let provinces = w
        .provinces
        .iter()
        .map(|(p, pr)| {
            let (sid, name) = &w.names.provinces[p.index()];
            ProvinceView {
                id: sid.clone(),
                name: name.clone(),
                owner: aid(pr.owner),
                population: w.population(*p),
                land: pr.land,
                coastal: pr.coastal,
                capital_of: capitals.get(p).copied().map(aid),
                governor: pr.governor.map(person),
                culture: w.names.cultures[pr.culture.index()].clone(),
                religion: w.names.religions[pr.religion.index()].clone(),
                siege: pr.siege.map(|s| SiegeView { by: aid(s.by), ticks: s.ticks, of: w.siege_ticks(*p, params) }),
                in_revolt: w.in_revolt(*p),
            }
        })
        .collect();
    let units = w
        .units
        .iter()
        .map(|(u, x)| {
            let yours = player.is_some_and(|who| w.commands(who, *u));
            UnitView {
                id: u.0,
                owner: aid(x.owner),
                house: x.house.map(hid),
                position: pid(x.position),
                soldiers: w.soldiers(*u),
                strength: w.unit_strength(*u),
                mercenary: x.mercenary,
                yours,
                // Another's orders are hidden: only the units the viewer commands show theirs.
                destination: x.destination.filter(|_| yours).map(pid),
            }
        })
        .collect();
    let wars = w
        .wars
        .iter()
        .map(|((a, b), x)| WarView {
            a: aid(*a),
            b: aid(*b),
            since: when(w, x.since),
            declared_by: x.by.map(aid),
            aim: x.aim.map(aim),
            counter_aim: x.counter_aim.map(aim),
            taken: x.taken,
            obligation_of: x.of.map(|(p, q)| (aid(p), aid(q))),
        })
        .collect();
    let treaties = w.treaties.iter().map(|((a, b), t)| TreatyView { a: aid(*a), b: aid(*b), kind: t.kind, until: when(w, t.until) }).collect();
    // The seats of the living, read once: thrones, heirs, regencies, governorships.
    let mut seats: std::collections::BTreeMap<PersonId, Vec<SeatView>> = std::collections::BTreeMap::new();
    for (a, x) in w.actors.iter().filter(|(a, _)| w.alive(**a)) {
        seats.entry(x.ruler).or_default().push(SeatView::Ruler(aid(*a)));
        if let Some(h) = x.heir {
            seats.entry(h).or_default().push(SeatView::Heir(aid(*a)));
        }
        if let Some(r) = x.regent {
            seats.entry(r).or_default().push(SeatView::Regent(aid(*a)));
        }
    }
    for (p, pr) in &w.provinces {
        if let Some(g) = pr.governor {
            seats.entry(g).or_default().push(SeatView::Governor(pid(*p)));
        }
    }
    let mut members: std::collections::BTreeMap<HouseId, Vec<PersonView>> = std::collections::BTreeMap::new();
    for (id, x) in w.persons.iter().filter(|(_, x)| x.died.is_none()) {
        members.entry(x.house).or_default().push(PersonView {
            name: person(*id),
            born: x.born,
            spouse: x.spouse.filter(|s| w.person_alive(*s)).map(|s| (person(s), hid(w.persons[&s].house))),
            seats: seats.remove(id).unwrap_or_default(),
        });
    }
    let houses = w
        .houses
        .iter()
        .map(|(h, x)| {
            let (sid, name) = &w.names.houses[h.index()];
            HouseView {
                id: sid.clone(),
                name: name.clone(),
                polity: aid(x.polity),
                ruling: w.alive(x.polity) && w.ruling_house(x.polity) == *h,
                members: members.remove(h).unwrap_or_default(),
                // Another house's treasury is hidden: the viewer's own house shows its.
                treasury_kg: (viewers_house == Some(*h)).then(|| w.house_treasury(*h)),
            }
        })
        .collect();
    let own = viewer.filter(|a| w.alive(*a)).map(|a| {
        let goal_target = |t: GoalTarget| match t {
            GoalTarget::None => GoalTargetView::None,
            GoalTarget::Province(p) => GoalTargetView::Province(pid(p)),
            GoalTarget::Polity(b) => GoalTargetView::Polity(aid(b)),
            GoalTarget::Region(r) => GoalTargetView::Region(w.names.regions[r.index()].0.clone()),
            GoalTarget::Religion(f) => GoalTargetView::Religion(w.names.religions[f.index()].clone()),
            GoalTarget::Route(p, q) => GoalTargetView::Route(pid(p), pid(q)),
        };
        let b = w.tick_budget(a, params);
        OwnView {
            goals: w.actors[&a].goals.iter().map(|g| GoalView { kind: g.kind, target: goal_target(g.target), since: when(w, g.since), progress: w.goal_progress(a, g) }).collect(),
            income_per_year_kg: w.yearly_income(a, params),
            budget: BudgetView {
                production_kg: b.production,
                income_kg: b.income,
                tribute_due_kg: b.tribute_due,
                seats_kg: b.seats.iter().map(|(_, kg)| kg).sum(),
                court_kg: b.court,
                interest_kg: b.interest,
                pay_due_kg: b.pay_due,
            },
            army: w.army_size(a),
            levy_this_tick: w.recruit_per_tick(a, params),
        }
    });
    WorldView { year: w.year(), half_year: half(w), currency: currency.name.clone(), actors, viewer: viewer.map(aid), provinces, units, wars, treaties, houses, own }
}

pub fn chronicle_snapshot(w: &World, events: &[Stamped]) -> ChronicleSnapshot {
    ChronicleSnapshot {
        year: w.year(),
        half_year: half(w),
        alive_actors: w.actors.keys().map(|id| w.names.actors[id.index()].1.clone()).collect(),
        events: events.to_vec(),
        names: w.names.clone(),
    }
}
