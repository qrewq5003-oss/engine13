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
    /// E0 Э2: output per head per year, kg.
    pub output_per_head_kg: i64,
    /// E0 Э4: the starting tax rate, parts per million of production.
    pub tax_default_ppm: i64,
    /// E0 Э5: court and administration, parts per million of revenue.
    pub court_share_ppm: i64,
    /// E0 А3: pay per soldier per year, kg (outputs × output per head).
    pub pay_per_soldier_kg_per_year: i64,
    /// E0 Э6: interest per tick on the debt, parts per million.
    pub loan_interest_ppm_per_tick: i64,
    /// E0 Э7: debt ceiling in thousandths of a year of revenue.
    pub loan_ceiling_years_milli: i64,
    /// E0 А7: share of the unsupplied soldiers lost per tick, parts per million.
    pub army_attrition_unsupplied_ppm_per_tick: i64,
    /// E0 А5: the reach of a unit in a tick, km (day's march × campaign days).
    pub march_km_per_tick: i64,
    /// (в) The exponent of the strengths in the chance of victory.
    pub battle_strength_exponent: f64,
    /// E0 А6: the winner's losses, parts per million of its soldiers.
    pub battle_losses_winner_ppm: i64,
    /// E0 А6: the loser's losses, parts per million of its soldiers.
    pub battle_losses_loser_ppm: i64,
    /// E0 А9: consecutive ticks of siege a capital holds out.
    pub siege_capital_ticks: u32,
    /// (в) Soldiers a province feeds by foraging per 1 000 inhabitants.
    pub forage_soldiers_per_1000: i64,
    /// (в) E0 П6: people lost per tick of foraging at full load, parts per million.
    pub ruin_loss_ppm_at_full_load: i64,
    /// E0 А1: the levy ceiling, parts per million of the polity's people.
    pub recruit_ceiling_ppm: i64,
    /// (в) E0 А2: the share of the ceiling raised per tick, parts per million.
    pub recruit_rate_ppm_per_tick: i64,
    /// E0 А4: the advance per hired mercenary, kg (outputs × output per head).
    pub mercenary_advance_kg: i64,
    /// E0 В2: yearly mortality by age band, `(first age of the band, share per year)`,
    /// ascending; the last band is open.
    pub mortality_by_age: [(i32, f64); 7],
    /// (в) E0 В2: the elite's factor on the table.
    pub mortality_elite_factor: f64,
    /// E0 В4: births per person of fertile age per tick.
    pub births_per_adult_per_tick: f64,
    /// E0 В1: the age of majority, years.
    pub majority_age: i32,
    /// E0 В3: the base chance of a ruler's violent death per tick (at legitimacy 50).
    pub violent_death_per_tick: f64,
    /// (в) The legitimacy norms by standing (E0 §1.7).
    pub legitimacy_norm_hereditary: f64,
    pub legitimacy_norm_elected: f64,
    pub legitimacy_norm_acclaimed: f64,
    pub legitimacy_norm_regency: f64,
    /// (в) The share of the gap to the norm legitimacy closes per tick.
    pub legitimacy_rate_per_tick: f64,
    /// E0 Х1: a house's share of its seat's tax, parts per million.
    pub seat_income_share_ppm: i64,
    /// E0 Э4: the band the tax verbs move the rate inside, parts per million.
    pub tax_band_ppm: (i64, i64),
    /// (в) One call of a tax verb moves the rate by this, parts per million.
    pub tax_step_ppm: i64,
    /// (в) What raising the tax costs in legitimacy; what lowering it gives.
    pub legitimacy_cost_raise_tax: f64,
    pub legitimacy_gain_lower_tax: f64,
    /// (в) `enforce_order`: its price as parts per million of yearly revenue, and its gain.
    pub order_cost_share_ppm: i64,
    pub order_gain: f64,
    pub entries: BTreeMap<String, Entry>,
}

impl Params {
    /// People a province can carry: `area × density(land)`, rounded down.
    pub fn capacity(&self, area_km2: u64, land: Land) -> u64 {
        let d = self.capacity_milli_per_km2.iter().find(|(l, _)| *l == land).map(|(_, d)| *d).unwrap_or(0);
        (area_km2 as i64 * d / 1000) as u64
    }

    /// The yearly mortality of an age by the table (E0 В2), before the elite factor.
    pub fn mortality_per_year(&self, age: i32) -> f64 {
        self.mortality_by_age.iter().rev().find(|(from, _)| age >= *from).map(|(_, q)| *q).unwrap_or(self.mortality_by_age[0].1)
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
        let band = |key: &str| -> Result<[f64; 2], ParamsError> {
            entries.get(key).and_then(|e| e.band).ok_or_else(|| ParamsError::Missing(key.to_string()))
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
        let output = value("production.output_per_head_kg")?;
        let tax = value("tax.default_rate")?;
        let court = value("court.share_of_revenue")?;
        let pay_outputs = value("army.pay_per_soldier_outputs")?;
        let interest = value("loan.interest_per_year")?;
        let ceiling_years = value("loan.ceiling_years_of_revenue")?;
        let attrition_year = value("army.attrition_unsupplied_per_year")?;
        let km_per_day = value("march.km_per_day")?;
        let days_per_tick = value("march.days_per_tick")?;
        let exponent = value("battle.strength_exponent")?;
        let losses_winner = value("battle.losses_winner")?;
        let losses_loser = value("battle.losses_loser")?;
        let capital_ticks = value("siege.capital_ticks")?;
        let forage = value("forage.soldiers_per_1000_people")?;
        let ruin = value("ruin.population_loss_per_tick_at_full_load")?;
        let levy_ceiling = value("recruit.ceiling_share")?;
        let levy_rate = value("recruit.rate_per_tick")?;
        let advance_outputs = value("mercenaries.advance_outputs")?;
        let mortality_by_age = [
            (0, value("mortality.infant")?),
            (1, value("mortality.child")?),
            (5, value("mortality.youth")?),
            (20, value("mortality.adult")?),
            (40, value("mortality.mature")?),
            (60, value("mortality.old")?),
            (80, value("mortality.aged")?),
        ];
        let elite = value("mortality.elite_factor")?;
        let births = value("births.per_adult_per_year")?;
        let majority = value("majority.age")?;
        let violent = value("violent_death.per_year")?;
        let norm_hereditary = value("legitimacy.norm_hereditary")?;
        let norm_elected = value("legitimacy.norm_elected")?;
        let norm_acclaimed = value("legitimacy.norm_acclaimed")?;
        let norm_regency = value("legitimacy.norm_regency")?;
        let legitimacy_rate = value("legitimacy.rate_per_tick")?;
        let seat_share = value("house.income_share_of_seat")?;
        let tax_band = band("tax.default_rate")?;
        let tax_step = value("tax.step")?;
        let cost_raise_tax = value("legitimacy.cost_raise_tax")?;
        let gain_lower_tax = value("legitimacy.gain_lower_tax")?;
        let order_cost = value("order.cost_share_of_revenue")?;
        let order_gain = value("order.gain")?;
        Ok(Params {
            population_growth_ppm_per_tick: (growth / 2.0 * 1_000_000.0).round() as i64,
            famine_mortality_ppm_per_tick: (famine / 2.0 * 1_000_000.0).round() as i64,
            plague_outbreak_probability_per_tick: outbreaks / 2.0,
            plague_mortality_ppm: (plague_mortality * 1_000_000.0).round() as i64,
            migration_rate_ppm_per_tick: (migration_rate * 1_000_000.0).round() as i64,
            migration_min_gap_ppm: (migration_gap * 1_000_000.0).round() as i64,
            capacity_milli_per_km2,
            output_per_head_kg: output.round() as i64,
            tax_default_ppm: (tax * 1_000_000.0).round() as i64,
            court_share_ppm: (court * 1_000_000.0).round() as i64,
            pay_per_soldier_kg_per_year: (pay_outputs * output).round() as i64,
            loan_interest_ppm_per_tick: (interest / 2.0 * 1_000_000.0).round() as i64,
            loan_ceiling_years_milli: (ceiling_years * 1000.0).round() as i64,
            // Two ticks a year: half the yearly share per tick.
            army_attrition_unsupplied_ppm_per_tick: (attrition_year / 2.0 * 1_000_000.0).round() as i64,
            march_km_per_tick: (km_per_day * days_per_tick).round() as i64,
            battle_strength_exponent: exponent,
            battle_losses_winner_ppm: (losses_winner * 1_000_000.0).round() as i64,
            battle_losses_loser_ppm: (losses_loser * 1_000_000.0).round() as i64,
            siege_capital_ticks: capital_ticks.round() as u32,
            forage_soldiers_per_1000: forage.round() as i64,
            ruin_loss_ppm_at_full_load: (ruin * 1_000_000.0).round() as i64,
            recruit_ceiling_ppm: (levy_ceiling * 1_000_000.0).round() as i64,
            recruit_rate_ppm_per_tick: (levy_rate * 1_000_000.0).round() as i64,
            mercenary_advance_kg: (advance_outputs * output).round() as i64,
            mortality_by_age,
            mortality_elite_factor: elite,
            births_per_adult_per_tick: births / 2.0,
            majority_age: majority.round() as i32,
            violent_death_per_tick: violent / 2.0,
            legitimacy_norm_hereditary: norm_hereditary,
            legitimacy_norm_elected: norm_elected,
            legitimacy_norm_acclaimed: norm_acclaimed,
            legitimacy_norm_regency: norm_regency,
            legitimacy_rate_per_tick: legitimacy_rate,
            seat_income_share_ppm: (seat_share * 1_000_000.0).round() as i64,
            tax_band_ppm: ((tax_band[0] * 1_000_000.0).round() as i64, (tax_band[1] * 1_000_000.0).round() as i64),
            tax_step_ppm: (tax_step * 1_000_000.0).round() as i64,
            legitimacy_cost_raise_tax: cost_raise_tax,
            legitimacy_gain_lower_tax: gain_lower_tax,
            order_cost_share_ppm: (order_cost * 1_000_000.0).round() as i64,
            order_gain,
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
        assert_eq!(p.population_growth_ppm_per_tick, 6000);
        assert_eq!(p.famine_mortality_ppm_per_tick, 50_000);
        assert!((p.plague_outbreak_probability_per_tick - 0.04).abs() < 1e-12);
        assert_eq!(p.plague_mortality_ppm, 30_000);
        assert_eq!(p.migration_rate_ppm_per_tick, 20_000);
        assert_eq!(p.migration_min_gap_ppm, 100_000);
        assert_eq!(p.capacity(250, Land::Arable), 10_000);
        assert_eq!(p.capacity(1000, Land::Steppe), 1000);
        assert_eq!(p.output_per_head_kg, 700);
        assert_eq!(p.tax_default_ppm, 100_000);
        assert_eq!(p.court_share_ppm, 400_000);
        assert_eq!(p.pay_per_soldier_kg_per_year, 2100);
        assert_eq!(p.loan_interest_ppm_per_tick, 40_000);
        assert_eq!(p.loan_ceiling_years_milli, 5000);
        assert_eq!(p.army_attrition_unsupplied_ppm_per_tick, 150_000);
        assert_eq!(p.march_km_per_tick, 2100);
        assert_eq!(p.siege_capital_ticks, 4);
        assert_eq!(p.mercenary_advance_kg, 700);
        assert!((p.mortality_per_year(0) - 0.36).abs() < 1e-12);
        assert!((p.mortality_per_year(3) - 0.065).abs() < 1e-12);
        assert!((p.mortality_per_year(19) - 0.013).abs() < 1e-12);
        assert!((p.mortality_per_year(20) - 0.021).abs() < 1e-12);
        assert!((p.mortality_per_year(95) - 0.30).abs() < 1e-12);
        assert!((p.births_per_adult_per_tick - 0.0345).abs() < 1e-12);
        assert_eq!(p.majority_age, 18);
        assert!((p.violent_death_per_tick - 0.01).abs() < 1e-12);
        assert_eq!(p.seat_income_share_ppm, 20_000);
        assert_eq!(p.tax_band_ppm, (50_000, 200_000));
        assert_eq!(p.tax_step_ppm, 20_000);
    }

    #[test]
    fn plague_that_outgrows_people_is_refused() {
        // Plague at the top of its bands against growth at the bottom of a lower band:
        // 0.15 × 0.05 = 0.0075 ≥ 0.007 — refused. (With the shipped bands the guard cannot
        // trip: 0.0075 < 0.008; it stands against future changes of either band.)
        let text = PARAMS_TOML
            .replacen("value = 0.03
band = [0.02, 0.05]", "value = 0.05
band = [0.02, 0.05]", 1)
            .replacen("value = 0.08
band = [0.05, 0.15]", "value = 0.15
band = [0.05, 0.15]", 1)
            .replacen("value = 0.012
band = [0.008, 0.016]", "value = 0.007
band = [0.005, 0.010]", 1);
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
