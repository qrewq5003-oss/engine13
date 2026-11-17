//! The map of milan_1477 (E1b, docs/engine2_map.md §7) and the scenario's own facts (E4, §25)
//! compile through the scenario schema.
//!
//! The province table is the cartographer's output (`tools/map/layout.py` →
//! `public/geodata2/milan_1477/provinces.toml`): areas, land classes, neighbours, sea
//! neighbours, governors, cultures, religions, regions and the start population of §11d. The
//! polities, houses, persons, relations and aspirations are the scenario's own file beside it,
//! `scenario.toml`, written by the cartographer from the sources (docs/engine2_map.md §19, §23,
//! §25). The two texts are one scenario (`ScenarioData::parse_parts`, plan §31.49): a province
//! names its governor in the other file, so the table no longer loads on stub actors.
//! Compiling checks what the schema checks: owners known, neighbours symmetric and known, sea
//! neighbours only on coasts, regions' provinces known, seats held by persons of the polity,
//! and no province above the capacity the engine derives for its land.

use std::collections::BTreeMap;

use engine2::params::Params;
use engine2::scenario::ScenarioData;
use engine2::sim::{Game, Simulation};

const HEAD: &str = include_str!("../../public/geodata2/milan_1477/scenario.toml");
const PROVINCES: &str = include_str!("../../public/geodata2/milan_1477/provinces.toml");

/// The scenario as one text: its own file and the province table after it.
fn whole_scenario() -> String {
    HEAD.to_string() + PROVINCES
}

#[test]
fn milan_1477_map_compiles_through_the_scenario_schema() {
    let scenario = ScenarioData::parse(&whole_scenario()).expect("the map parses as a scenario");
    assert!(scenario.provinces.len() >= 50, "{} provinces", scenario.provinces.len());
    let whole = scenario.compile(&Params::load().unwrap()).expect("the map compiles through the schema");
    // The same as the two files the scenario is kept in (E4, plan §31.49): the province
    // table carries the regions, the scenario's text none — the same facts, the same hash.
    let parts = ScenarioData::parse_parts(HEAD, PROVINCES).expect("the two texts join into the scenario");
    let parts = parts.compile(&Params::load().unwrap()).expect("the joined scenario compiles");
    assert!(!whole.facts.regions.is_empty(), "the map's regions are in the facts");
    // The scenario turns on every verb of the engine: the bots act only by the verbs a
    // scenario turns on, and one with none is a world they can do nothing in — the probe
    // below ran so once and reported it as a run with the bots (plan §31.49, §31.50).
    let on: Vec<engine2::verbs::VerbId> = whole.display.verbs.iter().map(|(verb, _)| *verb).collect();
    assert_eq!(on, engine2::verbs::VerbId::ALL.to_vec(), "every verb of the engine is on in the scenario");
    assert_eq!(parts.facts.hash(), whole.facts.hash());
    // Every owner of the province table is a polity of the scenario's own file, and none of
    // that file is without land (docs/engine2_map.md §25).
    let owners: std::collections::BTreeSet<&String> = scenario.provinces.values().map(|p| &p.owner).collect();
    let actors: std::collections::BTreeSet<&String> = scenario.actors.keys().collect();
    assert_eq!(owners, actors);
}

/// Probe (E4, plan §31.49): the scenario of Milan played with the bots — does a turn panic
/// or fail on real areas, and how long does it take. **It prints and asserts nothing; its
/// numbers are no measure of the scenario** and go into no stop. First written on stub
/// actors, before the scenario's data; now it runs on the scenario's own file
/// (docs/engine2_map.md §25), whose stubs are listed at its head: the rulers of the
/// republics without one ruler, and no treasury anywhere.
#[test]
#[ignore = "probe: prints, asserts nothing"]
fn diagnosis_the_scenario_of_milan_runs_with_bots() {
    let p = Params::load().unwrap();
    let scenario = ScenarioData::parse(&whole_scenario()).expect("the map parses as a scenario");
    eprintln!("the scenario's own file (its stubs are listed at its head): no treasury, the starting army by the rule (Milan and Venice by the source), every verb on; {} provinces, {} polities", scenario.provinces.len(), scenario.actors.len());
    let census = |g: &Game| {
        let w = g.world();
        let people: i64 = w.provinces.keys().map(|q| w.population(*q)).sum();
        let capacity: i64 = w.provinces.values().map(|q| q.capacity as i64).sum();
        let alive = w.actors.keys().filter(|a| w.alive(**a)).count();
        (people, capacity, alive, w.actors.len())
    };
    // The men under arms over the people: the starting army of the rule and of the source.
    let under_arms = |g: &Game| {
        let w = g.world();
        w.units.keys().map(|u| w.soldiers(*u)).sum::<i64>() as f64 / w.provinces.keys().map(|q| w.population(*q)).sum::<i64>().max(1) as f64
    };
    for seed in [1u64, 2, 3] {
        let mut g = match Game::new(&scenario, p.clone(), seed) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("seed {seed}: the game does not start: {e}");
                continue;
            }
        };
        let (people_0, capacity, alive_0, actors) = census(&g);
        let army_0 = under_arms(&g);
        let (mut played, mut worst, mut total) = (0u32, std::time::Duration::ZERO, std::time::Duration::ZERO);
        let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
        let mut stopped = String::from("no panic, no error");
        let mut marks = Vec::new();
        for t in 0..300u32 {
            let at = std::time::Instant::now();
            let turn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.tick(&[])));
            let took = at.elapsed();
            match turn {
                Ok(Ok(report)) => {
                    for e in &report.events {
                        let name = format!("{:?}", e.event);
                        let name = name.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("").to_string();
                        *kinds.entry(name).or_insert(0) += 1;
                    }
                }
                Ok(Err(e)) => {
                    stopped = format!("ERROR at tick {t}: {e}");
                    break;
                }
                Err(_) => {
                    stopped = format!("PANIC at tick {t}");
                    break;
                }
            }
            played += 1;
            total += took;
            worst = worst.max(took);
            if [10, 20, 50, 100, 200].contains(&played) {
                let (people, _, alive, _) = census(&g);
                marks.push(format!("tick {played}: {alive} alive, fill {:.3}, army {:.4}", people as f64 / capacity as f64, under_arms(&g)));
            }
        }
        // After a panic the world may be half-changed: it is read only when the run ended whole.
        let end = if played == 300 {
            let (people, _, alive, _) = census(&g);
            format!(
                "polities alive {alive_0} → {alive} (born in the run {}); people {people_0} → {people}; fill {:.3} → {:.3}; the army over the people {army_0:.4} → {:.4}",
                g.world().actors.len() - actors,
                people_0 as f64 / capacity as f64,
                people as f64 / capacity as f64,
                under_arms(&g)
            )
        } else {
            format!("polities alive at the start {alive_0}; people {people_0}; fill {:.3}; the army over the people at the start {army_0:.4}", people_0 as f64 / capacity as f64)
        };
        eprintln!(
            "seed {seed}: {stopped}; {played} ticks; a turn {:.1} ms on average, {:.1} ms at worst; {end}; {}",
            total.as_secs_f64() * 1000.0 / f64::from(played.max(1)),
            worst.as_secs_f64() * 1000.0,
            marks.join("; ")
        );
        let mut top: Vec<(&String, &u64)> = kinds.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        eprintln!("seed {seed}: events by kind: {}", top.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", "));
    }
}
