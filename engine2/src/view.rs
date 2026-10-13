//! The presentation model (plan §5): what the UI and the chronicler get, built from the
//! state and the `Names` table. Not a mirror of `World`.
//!
//! This is the only module that reads the display currency (`Names::currency`, guard
//! `display_currency_is_read_only_by_the_view`): the engine counts in kilograms of
//! wheat-equivalent, the view converts for showing.

use serde::Serialize;

use crate::event::Stamped;
use crate::scenario::Names;
use crate::time::HalfYear;
use crate::world::World;

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
    pub legitimacy: f64,
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
/// quote (E0 §3 of the old engine's lessons: forbidden claims, no engine keys).
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

pub fn world_view(w: &World, names: &Names) -> WorldView {
    let currency = &names.currency;
    let actors = w
        .actors
        .iter()
        .map(|(id, a)| {
            let (sid, name) = &names.actors[id.index()];
            let treasury_kg = w.treasury(*id);
            ActorView {
                id: sid.clone(),
                name: name.clone(),
                provinces: a.provinces.iter().map(|p| names.provinces[p.index()].1.clone()).collect(),
                population: a.provinces.iter().map(|p| w.population(*p)).sum(),
                treasury_kg,
                treasury_display: treasury_kg as f64 / currency.kg_per_unit,
                legitimacy: a.legitimacy,
            }
        })
        .collect();
    WorldView { year: w.year(), half_year: half(w), currency: currency.name.clone(), actors }
}

pub fn chronicle_snapshot(w: &World, names: &Names, events: &[Stamped]) -> ChronicleSnapshot {
    ChronicleSnapshot {
        year: w.year(),
        half_year: half(w),
        alive_actors: w.actors.keys().map(|id| names.actors[id.index()].1.clone()).collect(),
        events: events.to_vec(),
        names: names.clone(),
    }
}
