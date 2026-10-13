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
    #[error("plague loss {loss:.4}/year (outbreaks × mortality) is not below the growth {growth:.4}/year: a plague world would die out by its constants")]
    PlagueOutgrowsPeople { loss: f64, growth: f64 },
}

/// The validated constants the engine reads. Fields are typed accessors; the raw table is
/// kept for the guard and for tools.
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    /// E0 П2: logistic growth per tick, parts per million.
    pub population_growth_ppm_per_tick: i64,
    /// E0 П3: share of a province above capacity lost per tick, parts per million.
    pub famine_mortality_ppm_per_tick: i64,
    /// E0 П4: probability of an outbreak per province per tick (where plague exists).
    pub plague_outbreak_probability_per_tick: f64,
    /// E0 П4: share of a province an outbreak takes, parts per million.
    pub plague_mortality_ppm: i64,
    /// E0 П5 (в): migration rate per tick, parts per million, times the fill gap.
    pub migration_rate_ppm_per_tick: i64,
    /// E0 П5 (в): fill gap below which nobody moves, parts per million.
    pub migration_min_gap_ppm: i64,
    /// E0 П1: people per km² by land, as thousandths so capacity is integer arithmetic.
    pub capacity_milli_per_km2: [(Land, i64); 4],
    /// E0 А8: soldiers 1 000 inhabitants can supply.
    pub supply_soldiers_per_1000: i64,
    /// E0 А7: share of the unsupplied soldiers lost per tick, parts per million.
    pub army_attrition_unsupplied_ppm_per_tick: i64,
    /// E0 А5 (в): edges a unit crosses per tick.
    pub movement_provinces_per_tick: u32,
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
        let growth = value("population.growth_per_year")?;
        let famine = value("famine.mortality_per_year")?;
        let outbreaks = value("plague.outbreaks_per_year")?;
        let plague_mortality = value("plague.mortality")?;
        let migration_rate = value("migration.rate_per_tick")?;
        let migration_gap = value("migration.min_gap")?;
        // Owner's guard (PR #282): the mean plague loss must stay below the maximum growth,
        // or extinction is written into the constants rather than played out.
        if outbreaks * plague_mortality >= growth {
            return Err(ParamsError::PlagueOutgrowsPeople { loss: outbreaks * plague_mortality, growth });
        }
        let milli = |key: &str| -> Result<i64, ParamsError> { Ok((value(key)? * 1000.0).round() as i64) };
        let capacity_milli_per_km2 = [
            (Land::Arable, milli("capacity.arable")?),
            (Land::Hills, milli("capacity.hills")?),
            (Land::Steppe, milli("capacity.steppe")?),
            (Land::Desert, milli("capacity.desert")?),
        ];
        let supply = value("supply.soldiers_per_1000_people")?;
        let attrition_year = value("army.attrition_unsupplied_per_year")?;
        let moves = value("movement.provinces_per_tick")?;
        Ok(Params {
            population_growth_ppm_per_tick: (growth / 2.0 * 1_000_000.0).round() as i64,
            famine_mortality_ppm_per_tick: (famine / 2.0 * 1_000_000.0).round() as i64,
            plague_outbreak_probability_per_tick: outbreaks / 2.0,
            plague_mortality_ppm: (plague_mortality * 1_000_000.0).round() as i64,
            migration_rate_ppm_per_tick: (migration_rate * 1_000_000.0).round() as i64,
            migration_min_gap_ppm: (migration_gap * 1_000_000.0).round() as i64,
            capacity_milli_per_km2,
            supply_soldiers_per_1000: supply.round() as i64,
            // Two ticks a year: half the yearly share per tick.
            army_attrition_unsupplied_ppm_per_tick: (attrition_year / 2.0 * 1_000_000.0).round() as i64,
            movement_provinces_per_tick: moves.round() as u32,
            entries,
        })
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
        assert_eq!(p.population_growth_ppm_per_tick, 3500);
        assert_eq!(p.famine_mortality_ppm_per_tick, 50_000);
        assert!((p.plague_outbreak_probability_per_tick - 0.04).abs() < 1e-12);
        assert_eq!(p.plague_mortality_ppm, 30_000);
        assert_eq!(p.migration_rate_ppm_per_tick, 20_000);
        assert_eq!(p.migration_min_gap_ppm, 100_000);
        assert_eq!(p.capacity(250, Land::Arable), 10_000);
        assert_eq!(p.capacity(1000, Land::Steppe), 1000);
        assert_eq!(p.supply_soldiers_per_1000, 150);
        assert_eq!(p.army_attrition_unsupplied_ppm_per_tick, 150_000);
        assert_eq!(p.movement_provinces_per_tick, 1);
    }

    #[test]
    fn plague_that_outgrows_people_is_refused() {
        let text = PARAMS_TOML.replacen("value = 0.03
band = [0.02, 0.05]", "value = 0.05
band = [0.02, 0.05]", 1).replacen("value = 0.08
band = [0.05, 0.15]", "value = 0.15
band = [0.05, 0.15]", 1);
        // 0.15 × 0.05 = 0.0075 ≥ 0.007: refused.
        assert!(matches!(Params::parse(&text), Err(ParamsError::PlagueOutgrowsPeople { .. })));
        assert!(Params::parse(PARAMS_TOML).is_ok());
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
