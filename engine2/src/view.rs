//! The presentation model (plan §5): what the UI and the chronicler get, built from the
//! state, the world's names and the scenario's presentation data. Not a mirror of `World`.
//!
//! This is the only module that reads the display currency (`Display::currency`, guard
//! `display_currency_is_read_only_by_the_view`): the engine counts in kilograms of
//! wheat-equivalent, the view converts for showing.

use serde::Serialize;

use crate::event::Stamped;
use crate::scenario::Display;
use crate::time::HalfYear;
use crate::world::{Names, World};

/// One polity as shown.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActorView {
    pub id: String,
    pub name: String,
    pub provinces: Vec<String>,
    pub population: i64,
    pub treasury_kg: i64,
    /// Treasury in the scenario's display currency, for showing only.
    pub treasury_display: f64,
    pub debt_kg: i64,
    /// Debt in the scenario's display currency, for showing only.
    pub debt_display: f64,
    pub legitimacy: f64,
    /// The ruler's name, and the regent's while the ruler is a minor.
    pub ruler: String,
    pub regent: Option<String>,
}

/// The world as shown.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorldView {
    pub year: i32,
    pub half_year: u8,
    pub currency: String,
    pub actors: Vec<ActorView>,
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

pub fn world_view(w: &World, display: &Display) -> WorldView {
    let currency = &display.currency;
    let actors = w
        .actors
        .iter()
        .map(|(id, a)| {
            let (sid, name) = &w.names.actors[id.index()];
            let treasury_kg = w.treasury(*id);
            let debt_kg = w.debt(*id);
            ActorView {
                id: sid.clone(),
                name: name.clone(),
                provinces: a.provinces.iter().map(|p| w.names.provinces[p.index()].1.clone()).collect(),
                population: a.provinces.iter().map(|p| w.population(*p)).sum(),
                treasury_kg,
                treasury_display: treasury_kg as f64 / currency.kg_per_unit,
                debt_kg,
                debt_display: debt_kg as f64 / currency.kg_per_unit,
                legitimacy: a.legitimacy,
                ruler: w.names.persons[a.ruler.index()].1.clone(),
                regent: a.regent.map(|r| w.names.persons[r.index()].1.clone()),
            }
        })
        .collect();
    WorldView { year: w.year(), half_year: half(w), currency: currency.name.clone(), actors }
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
