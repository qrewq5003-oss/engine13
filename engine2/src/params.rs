//! The engine's constants (plan §1, §9): one `params.toml` for every scenario, each entry
//! with its kind and its E0 band. Loading validates; the guard `params_in_range` is the
//! same validation run over the file in the repository.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::world::Land;

/// The kind of a constant (E0 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Kind {
    /// (а) engine input with a source and a band.
    #[serde(rename = "a")]
    Input,
    /// (б) observed bound: a band the engine must produce; never an input.
    #[serde(rename = "b")]
    Observed,
    /// (в) model parameter without a source; checked through a (б) row.
    #[serde(rename = "c")]
    Model,
}

/// One entry of the file, before validation.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub kind: Kind,
    pub value: Option<f64>,
    pub band: Option<[f64; 2]>,
    pub unit: String,
    /// (в) only: the (б) row of E0 §5 it is checked through.
    pub checked_by: Option<String>,
    /// (а)/(б): the E0 §5 row the band comes from.
    pub e0: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParamsError {
    #[error("params file is not valid TOML of the schema: {0}")]
    Toml(String),
    #[error("{0}: kind (a) needs value and band")]
    InputIncomplete(String),
    #[error("{0}: kind (b) is an observed bound and must not carry a value")]
    ObservedHasValue(String),
    #[error("{0}: kind (b) needs a band")]
    ObservedNoBand(String),
    #[error("{0}: kind (c) needs value and checked_by")]
    ModelIncomplete(String),
    #[error("{0}: kinds (a) and (b) need e0 — without a source no entry")]
    NoSource(String),
    #[error("{name}: checked_by '{row}' names no (b) row of this file")]
    CheckedByUnknown { name: String, row: String },
    #[error("{name}: value {value} outside band [{lo}, {hi}]")]
    OutOfBand { name: String, value: f64, lo: f64, hi: f64 },
    #[error("{0}: band must be ordered")]
    BandUnordered(String),
    #[error("missing parameter {0}")]
    Missing(String),
}

/// The validated constants the engine reads. Fields are typed accessors; the raw table is
/// kept for the guard and for tools.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    /// Stand-in of E1: share of the gap to capacity the population closes per tick, as
    /// parts per million so the ledger arithmetic stays integer.
    pub population_pull_to_capacity_ppm: i64,
    /// E0 П1: people per km² by land, as thousandths so capacity is integer arithmetic.
    pub capacity_milli_per_km2: [(Land, i64); 4],
    pub entries: BTreeMap<String, Entry>,
}

impl Params {
    /// People a province can carry: `area × density(land)`, rounded down.
    pub fn capacity(&self, area_km2: u64, land: Land) -> u64 {
        let d = self.capacity_milli_per_km2.iter().find(|(l, _)| *l == land).map(|(_, d)| *d).unwrap_or(0);
        (area_km2 as i64 * d / 1000) as u64
    }
}

/// The file shipped with the crate.
pub const PARAMS_TOML: &str = include_str!("../params.toml");

impl Params {
    /// Parse and validate the crate's own `params.toml`.
    pub fn load() -> Result<Params, ParamsError> {
        Params::parse(PARAMS_TOML)
    }

    pub fn parse(text: &str) -> Result<Params, ParamsError> {
        let table: BTreeMap<String, BTreeMap<String, Entry>> = toml::from_str(text).map_err(|e| ParamsError::Toml(e.to_string()))?;
        let mut entries = BTreeMap::new();
        for (group, inner) in table {
            for (key, entry) in inner {
                let name = format!("{group}.{key}");
                validate(&name, &entry)?;
                entries.insert(name, entry);
            }
        }
        // A (c) row is checked through a (b) row: that row must exist in this file.
        let observed: std::collections::BTreeSet<&str> =
            entries.values().filter(|e| e.kind == Kind::Observed).filter_map(|e| e.e0.as_deref()).collect();
        for (name, e) in &entries {
            if e.kind == Kind::Model {
                let row = e.checked_by.clone().unwrap_or_default();
                if !observed.contains(row.as_str()) {
                    return Err(ParamsError::CheckedByUnknown { name: name.clone(), row });
                }
            }
        }
        let value = |key: &str| -> Result<f64, ParamsError> {
            entries.get(key).and_then(|e| e.value).ok_or_else(|| ParamsError::Missing(key.to_string()))
        };
        let pull = value("population.pull_to_capacity")?;
        let milli = |key: &str| -> Result<i64, ParamsError> { Ok((value(key)? * 1000.0).round() as i64) };
        let capacity_milli_per_km2 = [
            (Land::Arable, milli("capacity.arable")?),
            (Land::Hills, milli("capacity.hills")?),
            (Land::Steppe, milli("capacity.steppe")?),
            (Land::Desert, milli("capacity.desert")?),
        ];
        Ok(Params { population_pull_to_capacity_ppm: (pull * 1_000_000.0).round() as i64, capacity_milli_per_km2, entries })
    }
}

fn validate(name: &str, e: &Entry) -> Result<(), ParamsError> {
    if let Some([lo, hi]) = e.band {
        if lo > hi {
            return Err(ParamsError::BandUnordered(name.to_string()));
        }
    }
    let sourced = e.e0.as_deref().is_some_and(|s| !s.is_empty());
    match e.kind {
        Kind::Input => {
            let (Some(v), Some([lo, hi])) = (e.value, e.band) else {
                return Err(ParamsError::InputIncomplete(name.to_string()));
            };
            if v < lo || v > hi {
                return Err(ParamsError::OutOfBand { name: name.to_string(), value: v, lo, hi });
            }
            if !sourced {
                return Err(ParamsError::NoSource(name.to_string()));
            }
        }
        Kind::Observed => {
            if e.value.is_some() {
                return Err(ParamsError::ObservedHasValue(name.to_string()));
            }
            if e.band.is_none() {
                return Err(ParamsError::ObservedNoBand(name.to_string()));
            }
            if !sourced {
                return Err(ParamsError::NoSource(name.to_string()));
            }
        }
        Kind::Model => {
            if e.value.is_none() || e.checked_by.as_deref().is_none_or(str::is_empty) {
                return Err(ParamsError::ModelIncomplete(name.to_string()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_file_loads() {
        let p = Params::load().unwrap();
        assert_eq!(p.population_pull_to_capacity_ppm, 1000);
        assert_eq!(p.capacity(250, Land::Arable), 10_000);
        assert_eq!(p.capacity(1000, Land::Steppe), 1000);
    }

    #[test]
    fn kinds_are_validated() {
        let a_out = "[x.y]\nkind = \"a\"\nvalue = 2.0\nband = [0.0, 1.0]\nunit = \"u\"\ne0 = \"X\"\n";
        assert!(matches!(Params::parse(a_out), Err(ParamsError::OutOfBand { .. })));
        let a_nosrc = "[x.y]\nkind = \"a\"\nvalue = 0.5\nband = [0.0, 1.0]\nunit = \"u\"\n";
        assert!(matches!(Params::parse(a_nosrc), Err(ParamsError::NoSource(_))));
        let b_val = "[x.y]\nkind = \"b\"\nvalue = 0.5\nband = [0.0, 1.0]\nunit = \"u\"\ne0 = \"X\"\n";
        assert!(matches!(Params::parse(b_val), Err(ParamsError::ObservedHasValue(_))));
        let b_nosrc = "[x.y]\nkind = \"b\"\nband = [0.0, 1.0]\nunit = \"u\"\n";
        assert!(matches!(Params::parse(b_nosrc), Err(ParamsError::NoSource(_))));
        let c_noref = "[x.y]\nkind = \"c\"\nvalue = 0.5\nunit = \"u\"\n";
        assert!(matches!(Params::parse(c_noref), Err(ParamsError::ModelIncomplete(_))));
        let c_badref = "[x.y]\nkind = \"c\"\nvalue = 0.5\nunit = \"u\"\nchecked_by = \"Я9\"\n";
        assert!(matches!(Params::parse(c_badref), Err(ParamsError::CheckedByUnknown { .. })));
        let unknown = "[x.y]\nkind = \"c\"\nvalue = 0.5\nunit = \"u\"\nchecked_by = \"П7\"\nfoo = 1\n";
        assert!(matches!(Params::parse(unknown), Err(ParamsError::Toml(_))));
    }
}
