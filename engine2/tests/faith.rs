//! E2 step 8 acceptance (plan §20): religion and culture.
//!
//! - the tongue and the faith of a province and of a house are facts of the scenario,
//!   strings interned in sorted order; the polity prays and speaks as its ruling house does;
//! - two courts of another faith or tongue have the norm of their relation lower by the
//!   rows, and both frictions add;
//! - a province whose majority differs from its owner's court is alienated: the entry of
//!   collapse, with no effect on the take;
//! - a run with faiths saves and loads bit for bit;
//! - the generator gives worlds a few tongues and faiths, mixed borders and some polities
//!   with the conversion policy — neither none nor all; the extreme worlds stay uniform;
//! - under the conversion policy a province of another faith converts along the logistic
//!   curve of Р1 and turns at half, the former majority gone; the share at a century is
//!   inside Р2; without the policy rule converts nothing in four centuries (Crete) and only
//!   the neighbourhood drifts, slower; a change of target starts from the seed; the tongue
//!   follows the neighbourhood alone; on generated worlds conversions are neither never nor
//!   always.

use engine2::sim::{Game, Simulation};
use engine2::{from_save, testworld, to_save, worldgen, ActorId, CultureId, HouseId, Institution, Params, ProvinceId, ReligionId};

fn params() -> Params {
    Params::load().unwrap()
}

#[test]
fn faith_and_tongue_are_facts_of_the_scenario_interned_in_sorted_order_and_the_polity_prays_as_its_ruling_house_does() {
    let mut s = testworld::five_states_scenario();
    // Polity 1 prays otherwise; its house too; one province of polity 0 as well.
    for (k, p) in s.provinces.iter_mut() {
        if p.owner == "s1" || k == "p05" {
            p.religion = "f1".to_string();
        }
        if p.owner == "s2" {
            p.culture = "c1".to_string();
        }
    }
    s.houses.get_mut("h1").unwrap().religion = "f1".to_string();
    s.houses.get_mut("h2").unwrap().culture = "c1".to_string();
    let c = s.compile(&params()).unwrap();
    assert_eq!(c.names.religions, vec!["f0".to_string(), "f1".to_string()]);
    assert_eq!(c.names.cultures, vec!["c0".to_string(), "c1".to_string()]);
    let g = Game::new(&s, params(), 1).unwrap();
    let w = g.world();
    let (s0, s1, s2) = (ActorId(0), ActorId(1), ActorId(2));
    assert_eq!(w.polity_religion(s1), ReligionId(1));
    assert_eq!(w.polity_religion(s0), ReligionId(0));
    assert_eq!(w.polity_culture(s2), CultureId(1));
    assert_eq!(w.houses[&HouseId(1)].religion, ReligionId(1));
    assert_eq!(w.provinces[&ProvinceId(5)].religion, ReligionId(1));
    assert!(w.provinces.values().all(|p| p.converting.is_none() && p.assimilating.is_none()), "nothing on the move at the start");
    // Alienation: the province of another faith under polity 0; polity 1's own are not.
    assert!(w.alienated(ProvinceId(5)));
    assert!(!w.alienated(ProvinceId(0)));
    assert!(w.actors[&s1].provinces.iter().all(|p| !w.alienated(*p)), "a polity whose court prays as its people do");
    // The take is untouched by alienation (owner's answer on §20): the same share collected.
    let plain = Game::new(&testworld::five_states_scenario(), params(), 1).unwrap();
    assert_eq!(w.collection_share_ppm(s0, &params()), plain.world().collection_share_ppm(s0, &params()));
    assert_eq!(w.yearly_income(s0, &params()), plain.world().yearly_income(s0, &params()));
}

#[test]
fn two_courts_of_another_faith_or_tongue_have_the_norm_of_their_relation_lower_by_the_rows() {
    let p = params();
    let mut w = testworld::world(2);
    let (a, b) = (ActorId(0), ActorId(1));
    let base = w.relation_norm(a, b, &p);
    let house_b = w.persons[&w.actors[&b].ruler].house;
    w.houses.get_mut(&house_b).unwrap().religion = ReligionId(1);
    assert!((w.relation_norm(a, b, &p) - (base - p.relation_norm_faith)).abs() < 1e-9, "another faith at the court");
    w.houses.get_mut(&house_b).unwrap().culture = CultureId(1);
    assert!((w.relation_norm(a, b, &p) - (base - p.relation_norm_faith - p.relation_norm_culture)).abs() < 1e-9, "another tongue as well: the frictions add");
    // The people's faith does not count here, the court's does.
    let mut w = testworld::world(2);
    for pr in w.actors[&b].provinces.clone() {
        w.provinces.get_mut(&pr).unwrap().religion = ReligionId(1);
    }
    assert!((w.relation_norm(a, b, &p) - base).abs() < 1e-9, "a court of the same faith over people of another: no friction between the courts");
    assert!(w.actors[&b].provinces.iter().all(|pr| w.alienated(*pr)), "but every province of that polity is alienated from its court");
    // A new dynasty raised from the people takes its capital's faith (owner's answer 6).
    let mut w = testworld::world(3);
    let capital = w.actors[&b].capital;
    w.provinces.get_mut(&capital).unwrap().religion = ReligionId(1);
    for member in w.members(house_b) {
        w.persons.get_mut(&member).unwrap().died = Some(w.tick);
    }
    let before = w.houses.len();
    w.tick(&p).unwrap();
    assert!(w.houses.len() > before, "a house rose");
    let new_house = HouseId(before as u32);
    assert_eq!(w.houses[&new_house].religion, ReligionId(1));
    assert_eq!(w.polity_religion(b), ReligionId(1), "the court now prays as its capital does");
}

#[test]
fn a_run_with_faiths_saves_and_loads_bit_for_bit() {
    let p = params();
    let mut w = testworld::world(4);
    w.provinces.get_mut(&ProvinceId(7)).unwrap().religion = ReligionId(1);
    w.provinces.get_mut(&ProvinceId(8)).unwrap().converting = Some(engine2::Shift { to: ReligionId(1), share_ppm: 20_000 });
    w.houses.get_mut(&HouseId(3)).unwrap().culture = CultureId(1);
    let mut straight = w.clone();
    for _ in 0..20 {
        straight.tick(&p).unwrap();
    }
    for _ in 0..10 {
        w.tick(&p).unwrap();
    }
    let save = to_save(&w).unwrap();
    assert!(save.contains("\"religion\"") && save.contains("\"converting\"") && save.contains("\"cultures\""));
    let mut loaded = from_save(&save).unwrap();
    for _ in 0..10 {
        loaded.tick(&p).unwrap();
    }
    assert_eq!(to_save(&loaded).unwrap(), to_save(&straight).unwrap());
}

#[test]
fn the_generator_mixes_tongues_and_faiths_and_gives_the_policy_to_some_and_the_extreme_worlds_stay_uniform() {
    let p = params();
    let (mut mixed_provinces, mut provinces, mut with_policy, mut polities, mut newcomer_houses) = (0usize, 0usize, 0usize, 0usize, 0usize);
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let g = Game::new(&s, params(), seed).unwrap();
        let w = g.world();
        let (nc, nr) = (w.names.cultures.len(), w.names.religions.len());
        assert!((2..=4).contains(&nc) && (2..=3).contains(&nr), "seed {seed}: {nc} tongues, {nr} faiths");
        provinces += w.provinces.len();
        mixed_provinces += w.provinces.keys().filter(|q| w.alienated(**q)).count();
        polities += w.actors.len();
        with_policy += w.actors.values().filter(|a| a.institutions.contains(&Institution::ConversionPolicy)).count();
        newcomer_houses += w.houses.iter().filter(|(_, h)| w.polity_religion(h.polity) != h.religion || w.polity_culture(h.polity) != h.culture).count();
        for (id, a) in &w.actors {
            let capital = &w.provinces[&a.capital];
            let ruling = &w.houses[&w.persons[&a.ruler].house];
            assert_eq!((ruling.culture, ruling.religion), (capital.culture, capital.religion), "seed {seed} {id:?}: the ruling house prays and speaks as its capital does");
        }
    }
    let mixed = mixed_provinces as f64 / provinces as f64;
    let policy = with_policy as f64 / polities as f64;
    eprintln!("alienated provinces at the start {mixed:.3} of {provinces}; polities with the conversion policy {policy:.2} of {polities}; houses praying or speaking otherwise than their court {newcomer_houses}");
    assert!(mixed > 0.02 && mixed < 0.5, "the mixed march exists and is a march, not the whole land: {mixed:.3}");
    assert!(policy > 0.05 && policy < 0.7, "the policy is held by some, not by none or all: {policy:.2}");
    for (name, s) in worldgen::extreme_worlds(&p) {
        let c = s.compile(&p).unwrap();
        assert_eq!((c.names.cultures.len(), c.names.religions.len()), (1, 1), "{name}: a sketch of geography stays uniform");
        assert!(s.actors.values().all(|a| !a.institutions.contains(&Institution::ConversionPolicy)), "{name}");
    }
}

use engine2::event::Channel;
use engine2::{Event, ScenarioData, World};

/// The test world with polity 1 of another faith (court and people) and one province of
/// polity 0 — p05, next to the capital — of that faith too; `policy` gives polity 0 the
/// conversion policy.
fn mixed_scenario(policy: bool) -> ScenarioData {
    let mut s = testworld::five_states_scenario();
    for (k, p) in s.provinces.iter_mut() {
        if p.owner == "s1" || k == "p05" {
            p.religion = "f1".to_string();
        }
    }
    s.houses.get_mut("h1").unwrap().religion = "f1".to_string();
    if policy {
        s.actors.get_mut("s0").unwrap().institutions.push(Institution::ConversionPolicy);
    }
    s
}

fn world_of(s: &ScenarioData, seed: u64) -> World {
    let c = s.compile(&params()).unwrap();
    World::from_facts(&c.facts, c.names, &params(), seed).unwrap()
}

#[test]
fn under_the_conversion_policy_a_province_of_another_faith_converts_along_the_logistic_curve_and_turns_at_half() {
    let p = params();
    let mut w = world_of(&mixed_scenario(true), 21);
    let (s0, p05) = (ActorId(0), ProvinceId(5));
    assert_eq!(w.conversion_channel(p05, &p).map(|(to, by, _)| (to, by)), Some((ReligionId(0), Channel::Rule)));
    let people = w.population(p05);
    let mut shares = Vec::new();
    let mut turned_at = None;
    for tick in 0..300u32 {
        let r = w.tick(&p).unwrap();
        if let Some(c) = w.provinces[&p05].converting {
            assert_eq!(c.to, ReligionId(0));
            shares.push(c.share_ppm);
        }
        if r.events.iter().any(|e| matches!(e.event, Event::Converted { province, from, to, by: Channel::Rule } if province == p05 && from == ReligionId(1) && to == ReligionId(0))) {
            turned_at = Some(tick);
            break;
        }
    }
    // The curve: starts at the seed, grows faster then slower, never jumps.
    assert_eq!(shares[0], p.conversion_seed_ppm + p.conversion_rate_ppm_per_tick * p.conversion_seed_ppm / 1_000_000 * (1_000_000 - p.conversion_seed_ppm) / 1_000_000);
    assert!(shares.windows(2).all(|x| x[1] > x[0]), "monotone");
    let steps: Vec<i64> = shares.windows(2).map(|x| x[1] - x[0]).collect();
    assert!(steps.last().unwrap() > &(steps[0] * 5), "the curve accelerates up to the turn: the step at the end is many times the first");
    // Half a century at least, under two centuries: Bulliet's curve, not a stroke.
    let t = turned_at.expect("the majority turns within 150 years");
    assert!((100..300).contains(&t), "turned at tick {t}");
    // Р2: the share won a century after the change of rule is inside the band.
    let band = p.entries["conversion.observed_share_after_a_century"].band.unwrap();
    // (The turn comes just before the century here, so the share is read at the turn.)
    let at_century = shares.get(199).or(shares.last()).copied().unwrap() as f64 / 1e6;
    assert!((band[0]..=band[1]).contains(&at_century), "Р2 at a century: {at_century:.3} in {band:?}");
    eprintln!("Р2 by rule: share at a century {at_century:.3}, the majority turned at tick {t} ({} years)", t / 2);
    // After the turn: the former majority is gone (one number), the province is at one
    // with its court, nothing is on the move, the people are all still there.
    assert_eq!(w.provinces[&p05].religion, ReligionId(0));
    assert!(w.provinces[&p05].converting.is_none(), "the former majority vanishes with the turn (owner's note on §20)");
    assert!(!w.alienated(p05));
    assert!(w.conversion_channel(p05, &p).is_none());
    assert!(w.population(p05) > 0 && (w.population(p05) - people).abs() < people / 2, "conversion moves no people");
    let _ = s0;
}

#[test]
fn without_the_policy_rule_converts_nothing_in_four_centuries_and_only_the_neighbourhood_drifts() {
    let p = params();
    // Venetian Crete: a court of another faith over the province, no policy — nothing moves
    // by rule, however long. The province's neighbours pray as it does.
    let mut w = world_of(&mixed_scenario(false), 22);
    let p05 = ProvinceId(5);
    // Its neighbours pray as it does (p00 and p10 of polity 0 made f1 too): no majority of
    // another faith around it, and a court of another faith without the policy.
    for q in [ProvinceId(0), ProvinceId(10)] {
        w.provinces.get_mut(&q).unwrap().religion = ReligionId(1);
    }
    assert!(w.alienated(p05));
    assert_eq!(w.conversion_channel(p05, &p), None, "no policy, no rule channel; no neighbourhood of another faith");
    for _ in 0..800 {
        w.tick(&p).unwrap();
    }
    assert_eq!(w.provinces[&p05].religion, ReligionId(1), "still of its own faith after four centuries");
    assert!(w.provinces[&p05].converting.is_none());
    // The neighbourhood: a province most of whose neighbours pray otherwise drifts to them
    // at the factor's rate — p05's neighbours are p00 (f0), p10 (f0), p06 (f1): put p00 and
    // p10 to f1 and the drift starts, by the neighbourhood, slower than the rule.
    let mut w = world_of(&mixed_scenario(false), 23);
    let p0 = ProvinceId(0);
    w.provinces.get_mut(&p0).unwrap().religion = ReligionId(0);
    // Make polity 0's own province p10 (neighbour of p05) of faith f1 too and p05 of f0:
    // then p05 (f0) has neighbours p00 f0, p10 f1, p06 f1 → two of three otherwise.
    w.provinces.get_mut(&p05).unwrap().religion = ReligionId(0);
    w.provinces.get_mut(&ProvinceId(10)).unwrap().religion = ReligionId(1);
    let (to, by, rate) = w.conversion_channel(p05, &p).expect("the neighbourhood's majority");
    assert_eq!((to, by), (ReligionId(1), Channel::Neighbourhood));
    assert_eq!(rate, (p.conversion_rate_ppm_per_tick as f64 * p.conversion_neighbourhood_factor).round() as i64);
    let mut turned_at = None;
    for tick in 0..600u32 {
        let r = w.tick(&p).unwrap();
        if r.events.iter().any(|e| matches!(e.event, Event::Converted { province, by: Channel::Neighbourhood, .. } if province == p05)) {
            turned_at = Some(tick);
            break;
        }
    }
    let t = turned_at.expect("the neighbourhood turns the province in the end");
    assert!(t > 300, "slower than the rule: turned at tick {t}");
    eprintln!("by neighbourhood: the majority turned at tick {t} ({} years)", t / 2);
}

#[test]
fn a_change_of_target_starts_the_share_from_the_seed_again_and_the_tongue_follows_the_neighbourhood_alone() {
    let p = params();
    let mut w = world_of(&mixed_scenario(true), 24);
    let (s0, p05) = (ActorId(0), ProvinceId(5));
    for _ in 0..60 {
        w.tick(&p).unwrap();
    }
    let grown = w.provinces[&p05].converting.unwrap().share_ppm;
    assert!(grown > 2 * p.conversion_seed_ppm, "thirty years in: well past the seed ({grown})");
    // The court changes faith (a new house): the target changes and the share begins again.
    let house0 = w.persons[&w.actors[&s0].ruler].house;
    w.houses.get_mut(&house0).unwrap().religion = ReligionId(1);
    assert!(!w.alienated(p05), "now at one with its court: no channel by rule");
    assert_ne!(w.conversion_channel(p05, &p).map(|(_, by, _)| by), Some(Channel::Rule));
    // Another province of polity 0, of the old faith, now converts towards f1 from the seed.
    let p0 = ProvinceId(0);
    assert_eq!(w.conversion_channel(p0, &p).map(|(to, by, _)| (to, by)), Some((ReligionId(1), Channel::Rule)));
    w.tick(&p).unwrap();
    let started = w.provinces[&p0].converting.unwrap();
    assert_eq!(started.to, ReligionId(1));
    assert!(started.share_ppm < 2 * p.conversion_seed_ppm, "from the seed, not from the other's progress");
    // The tongue: the policy does not assimilate — a province of another tongue under the
    // court stays; only a neighbourhood of another tongue draws it.
    let mut w = world_of(&mixed_scenario(true), 25);
    for q in [p05, ProvinceId(0), ProvinceId(10), ProvinceId(6)] {
        w.provinces.get_mut(&q).unwrap().culture = CultureId(1);
    }
    assert!(w.alienated(p05), "another tongue than the court's");
    assert!(w.assimilation_channel(p05, &p).is_none(), "the rule has no channel for the tongue, and the neighbours speak as the province does");
    for _ in 0..400 {
        w.tick(&p).unwrap();
    }
    assert_eq!(w.provinces[&p05].culture, CultureId(1), "Crete stays Greek-speaking");
    for q in [ProvinceId(0), ProvinceId(10), ProvinceId(6)] {
        w.provinces.get_mut(&q).unwrap().culture = CultureId(1);
    }
    w.provinces.get_mut(&p05).unwrap().culture = CultureId(0);
    assert_eq!(w.assimilation_channel(p05, &p).map(|(to, _)| to), Some(CultureId(1)));
    let mut turned = false;
    for _ in 0..800 {
        let r = w.tick(&p).unwrap();
        if r.events.iter().any(|e| matches!(e.event, Event::Assimilated { province, to, .. } if province == p05 && to == CultureId(1))) {
            turned = true;
            break;
        }
    }
    assert!(turned, "the neighbourhood assimilates in the end");
}

#[test]
fn on_generated_worlds_conversions_happen_but_not_everywhere() {
    let p = params();
    let (mut alienated_at_start, mut converted, mut assimilated, mut alienated_at_end) = (0usize, 0usize, 0usize, 0usize);
    for seed in 1..=20u64 {
        let s = worldgen::generate(seed, &p);
        let mut g = Game::new(&s, params(), seed).unwrap();
        alienated_at_start += g.world().provinces.keys().filter(|q| g.world().alienated(**q)).count();
        for _ in 0..300 {
            let r = g.tick(&[]).unwrap();
            converted += r.events.iter().filter(|e| matches!(e.event, Event::Converted { .. })).count();
            assimilated += r.events.iter().filter(|e| matches!(e.event, Event::Assimilated { .. })).count();
        }
        alienated_at_end += g.world().provinces.keys().filter(|q| g.world().alienated(**q)).count();
        for pr in g.world().provinces.values() {
            if let Some(c) = pr.converting {
                assert!(c.share_ppm > 0 && c.share_ppm < 500_000, "seed {seed}: a share on the move stays below half");
            }
        }
    }
    eprintln!("20 worlds × 300 ticks: alienated provinces {alienated_at_start} at the start, {alienated_at_end} at the end; {converted} conversions, {assimilated} assimilations");
    assert!(converted > 0, "conversions are not never");
    assert!(alienated_at_end > 0, "and not always: the mixed march does not vanish in 150 years");
}
