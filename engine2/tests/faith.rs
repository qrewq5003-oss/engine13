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
//!   with the conversion policy — neither none nor all; the extreme worlds stay uniform.

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
