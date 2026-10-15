//! Religion and culture (brief §2 «Религия и культура», plan §20): the tongue and the faith
//! of a province's majority and of a house; conversion and assimilation along E0 Р1; the
//! friction of two courts in the relation of their polities (`diplomacy.rs`); the
//! alienation of a province from its owner as the entry of collapse.
//!
//! The engine never reads which tongue or faith is which: the scenario's strings are
//! interned to `CultureId`/`ReligionId` at compile time, and every rule compares ids for
//! equality alone. A polity speaks and prays as its **ruling house** does (owner's answer on
//! §20), so a new dynasty may change the faith of the court — a house raised from the people
//! takes its capital's.
//!
//! **Conversion** (Р1). A province keeps its majority and one minority on the move
//! (`Shift`): the share won to another faith grows along the logistic curve
//! `s' = s + r·s·(1 − s)` from `conversion.seed_share`; at half the majority turns
//! (`Converted`) and the share is spent — the former majority is gone (owner's note on
//! §20: one number, not a mix; the people are not moved between accounts, the sum of the
//! population is untouched). Two channels: **rule** — the owner's court prays otherwise
//! **and holds `Institution::ConversionPolicy`** (owner's correction on §20: rule converts
//! far from always — Venetian Crete, Ottoman Rumelia; Bulliet's curve is Iran, where
//! conversion was rewarded), at the full rate; **neighbourhood** — more than half of the
//! province's neighbours, by land and by sea, pray one other faith, at
//! `conversion.neighbourhood_factor` of the rate. Rule comes first; a change of target
//! starts the share again from the seed; with no channel the share stays where it is.
//! **Assimilation** of the tongue goes by the neighbourhood alone: no court has a policy
//! for the tongue in E2 (Crete stayed Greek-speaking as it stayed Orthodox).
//!
//! **Phase** (after diplomacy, before the people): provinces in canonical order.

use crate::event::{Channel, Event};
use crate::ids::{ActorId, CultureId, ProvinceId, ReligionId};
use crate::params::Params;
use crate::world::{Shift, World, WorldError};

impl World {
    /// The faith of a polity's court: its ruling house's.
    pub fn polity_religion(&self, a: ActorId) -> ReligionId {
        self.houses[&self.persons[&self.actors[&a].ruler].house].religion
    }

    /// The tongue of a polity's court: its ruling house's.
    pub fn polity_culture(&self, a: ActorId) -> CultureId {
        self.houses[&self.persons[&self.actors[&a].ruler].house].culture
    }

    /// A province whose majority prays or speaks otherwise than its owner's court: the
    /// entry of collapse (В7, secession by culture and region), with no effect on the take
    /// in E2 (owner's answer on §20: Э8 stays clean).
    pub fn alienated(&self, p: ProvinceId) -> bool {
        let pr = &self.provinces[&p];
        pr.religion != self.polity_religion(pr.owner) || pr.culture != self.polity_culture(pr.owner)
    }

    /// The one value more than half of a province's neighbours (land and sea) share, if
    /// it differs from the province's own.
    fn neighbourhood_majority<T: Copy + Ord>(&self, p: ProvinceId, own: T, of: impl Fn(ProvinceId) -> T) -> Option<T> {
        let pr = &self.provinces[&p];
        let all: Vec<ProvinceId> = pr.neighbours.iter().chain(pr.sea_neighbours.iter()).copied().collect();
        if all.is_empty() {
            return None;
        }
        let mut counts: std::collections::BTreeMap<T, usize> = std::collections::BTreeMap::new();
        for n in &all {
            *counts.entry(of(*n)).or_insert(0) += 1;
        }
        counts.into_iter().find(|(v, c)| *v != own && *c * 2 > all.len()).map(|(v, _)| v)
    }

    /// The faith a province is drawn to and at what rate: the owner's court under the
    /// conversion policy first, else the neighbourhood's majority.
    pub fn conversion_channel(&self, p: ProvinceId, params: &Params) -> Option<(ReligionId, Channel, i64)> {
        let pr = &self.provinces[&p];
        let court = self.polity_religion(pr.owner);
        if court != pr.religion && self.actors[&pr.owner].institutions.contains(&crate::world::Institution::ConversionPolicy) {
            return Some((court, Channel::Rule, params.conversion_rate_ppm_per_tick));
        }
        self.neighbourhood_majority(p, pr.religion, |n| self.provinces[&n].religion)
            .map(|to| (to, Channel::Neighbourhood, (params.conversion_rate_ppm_per_tick as f64 * params.conversion_neighbourhood_factor).round() as i64))
    }

    /// The tongue a province is drawn to: its neighbourhood's majority, at the
    /// neighbourhood's rate — the rule has no channel for the tongue.
    pub fn assimilation_channel(&self, p: ProvinceId, params: &Params) -> Option<(CultureId, i64)> {
        let own = self.provinces[&p].culture;
        self.neighbourhood_majority(p, own, |n| self.provinces[&n].culture)
            .map(|to| (to, (params.conversion_rate_ppm_per_tick as f64 * params.conversion_neighbourhood_factor).round() as i64))
    }

    /// One step of the logistic curve on a share in parts per million.
    fn grow(share_ppm: i64, rate_ppm: i64) -> i64 {
        (share_ppm + rate_ppm * share_ppm / 1_000_000 * (1_000_000 - share_ppm) / 1_000_000).min(1_000_000)
    }

    /// Advance a minority on the move towards `to` at `rate`; true when the majority turns.
    fn advance<T: Copy + PartialEq>(shift: &mut Option<Shift<T>>, to: T, rate_ppm: i64, seed_ppm: i64) -> bool {
        let current = match shift {
            Some(s) if s.to == to => s.share_ppm,
            _ => seed_ppm,
        };
        let next = Self::grow(current, rate_ppm);
        if next >= 500_000 {
            *shift = None;
            true
        } else {
            *shift = Some(Shift { to, share_ppm: next });
            false
        }
    }

    /// The phase of faith and tongue.
    pub(crate) fn phase_faith(&mut self, params: &Params) -> Result<(), WorldError> {
        let provinces: Vec<ProvinceId> = self.provinces.keys().copied().collect();
        for p in provinces {
            if let Some((to, by, rate)) = self.conversion_channel(p, params) {
                let pr = self.provinces.get_mut(&p).unwrap();
                let from = pr.religion;
                if Self::advance(&mut pr.converting, to, rate, params.conversion_seed_ppm) {
                    pr.religion = to;
                    self.emit(Event::Converted { province: p, from, to, by });
                }
            }
            if let Some((to, rate)) = self.assimilation_channel(p, params) {
                let pr = self.provinces.get_mut(&p).unwrap();
                let from = pr.culture;
                if Self::advance(&mut pr.assimilating, to, rate, params.conversion_seed_ppm) {
                    pr.culture = to;
                    self.emit(Event::Assimilated { province: p, from, to });
                }
            }
        }
        Ok(())
    }
}
