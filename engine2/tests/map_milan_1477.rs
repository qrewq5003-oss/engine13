//! The map of milan_1477 (E1b, docs/engine2_map.md §7) compiles through the scenario schema.
//!
//! The province table is the cartographer's output (`tools/map/layout.py` →
//! `public/geodata2/milan_1477/provinces.toml`): areas, land classes, neighbours, sea
//! neighbours, cultures, religions, regions and the start population of §11d. The actors are
//! stubs — one house and one ruler per owner, the capital at the owner's first province — until
//! the scenario's own actors (E7). Compiling checks what the schema checks: owners known,
//! neighbours symmetric and known, sea neighbours only on coasts, regions' provinces known, and
//! no province above the capacity the engine derives for its land.

use std::collections::BTreeMap;

use engine2::params::Params;
use engine2::scenario::ScenarioData;
use engine2::sim::{Game, Simulation};

const PROVINCES: &str = include_str!("../../public/geodata2/milan_1477/provinces.toml");

/// The two starting armies a source gives (`docs/engine2_map.md` §23, Mallett): Milan «more
/// than 20 thousand» in 1472–1474, Venice «little more than 10 thousand» in the 1470s — the
/// lower ends, for the stub only; the scenario's data will carry the cartographer's numbers.
const SOLDIERS_BY_SOURCE: [(&str, u64); 2] = [("milan", 20_000), ("venice", 10_000)];

/// The scenario's own text for the map's stub: the top keys and, for every owner of the
/// province table, an actor with one house and one ruler, its capital at its first province,
/// no treasury; the starting army by the source where there is one, else left to the engine's
/// rule; every verb of the engine turned on.
fn stub_head() -> String {
    let table: toml::Table = toml::from_str(PROVINCES).expect("the province table is TOML");
    let provinces = table["provinces"].as_table().expect("[provinces]");
    let mut capital: BTreeMap<String, String> = BTreeMap::new();
    for (id, p) in provinces {
        let owner = p["owner"].as_str().expect("owner").to_string();
        capital.entry(owner).or_insert_with(|| id.clone());
    }
    let mut text = String::from(
        "id = \"milan_1477_map\"\nstart_year = 1477\ndiseases = [\"Plague\"]\n\
         [display_currency]\nname = \"ducat\"\nkg_per_unit = 100.0\n[verbs]\n",
    );
    // Every verb of the engine is on: the bots act only by the verbs a scenario turns on, and
    // a stub with none is a world they can do nothing in (the first cut of this stub had none
    // — plan §31.50). The verbs are of the presentation, not of the facts: the hash stands.
    for verb in engine2::verbs::VerbId::ALL {
        text += &format!("{0} = \"{0}\"\n", verb.key());
    }
    for (owner, cap) in &capital {
        // The starting army: the number of a source where one is opened, else left out — the
        // engine's rule (E4, plan §31.50).
        let soldiers = SOLDIERS_BY_SOURCE.iter().find(|(id, _)| id == owner).map(|(_, n)| format!("soldiers = {n}\n")).unwrap_or_default();
        text += &format!(
            "[actors.{owner}]\nname = \"{owner}\"\ntreasury_kg = 0\ncapital = \"{cap}\"\n{soldiers}\
             institutions = []\nruler = \"{owner}_ruler\"\nsuccession = \"Blood\"\naspirations = []\n\
             [houses.{owner}_house]\nname = \"{owner}\"\npolity = \"{owner}\"\ntreasury_kg = 0\n\
             culture = \"stub\"\nreligion = \"stub\"\nsoldiers = 0\n\
             [persons.{owner}_ruler]\nname = \"{owner}\"\nhouse = \"{owner}_house\"\nborn = 1440\n"
        );
    }
    text
}

/// The map as a scenario on stub actors: the stub's text and the province table, one text.
fn stub_scenario() -> String {
    stub_head() + PROVINCES
}

#[test]
fn milan_1477_map_compiles_through_the_scenario_schema() {
    let scenario = ScenarioData::parse(&stub_scenario()).expect("the map parses as a scenario");
    assert!(scenario.provinces.len() >= 50, "{} provinces", scenario.provinces.len());
    let whole = scenario.compile(&Params::load().unwrap()).expect("the map compiles through the schema");
    // The same as the two files a real scenario is kept in (E4, plan §31.49): the province
    // table carries the regions, the scenario's text none — the same facts, the same hash.
    let parts = ScenarioData::parse_parts(&stub_head(), PROVINCES).expect("the two texts join into the scenario");
    let parts = parts.compile(&Params::load().unwrap()).expect("the joined scenario compiles");
    assert!(!whole.facts.regions.is_empty(), "the map's regions are in the facts");
    // The stub turns on every verb of the engine: the bots act only by the verbs a scenario
    // turns on, and a stub with none is a world they can do nothing in — the probe below ran
    // so once and reported it as a run with the bots (plan §31.49, §31.50).
    let on: Vec<engine2::verbs::VerbId> = whole.display.verbs.iter().map(|(verb, _)| *verb).collect();
    assert_eq!(on, engine2::verbs::VerbId::ALL.to_vec(), "every verb of the engine is on in the stub");
    assert_eq!(parts.facts.hash(), whole.facts.hash());
}

/// Probe before the scenario's data (E4, plan §31.49): the map of Milan played with the bots
/// on the stub actors — does a turn panic or fail on real areas, and how long does it take.
/// **It prints and asserts nothing; its numbers are no measure of the scenario** and go into
/// no stop: the actors are stubs — one house and one ruler a polity, no treasury, the
/// starting army by the engine's rule, no aspirations, no relations, every verb of the
/// engine on (with none on the bots can do nothing: the first cut of this probe ran so,
/// plan §31.49). What the bots do on them is the stub's, not Milan's.
#[test]
#[ignore = "probe: prints, asserts nothing"]
fn diagnosis_the_map_of_milan_runs_with_bots_on_stub_actors() {
    let p = Params::load().unwrap();
    let scenario = ScenarioData::parse(&stub_scenario()).expect("the map parses as a scenario");
    eprintln!("the stub: every polity one house and one ruler, no treasury, the starting army by the rule (Milan and Venice by the source), no aspirations, no relations, every verb on — what follows is the stub's, not Milan's; {} provinces", scenario.provinces.len());
    let census = |g: &Game| {
        let w = g.world();
        let people: i64 = w.provinces.keys().map(|q| w.population(*q)).sum();
        let capacity: i64 = w.provinces.values().map(|q| q.capacity as i64).sum();
        let alive = w.actors.keys().filter(|a| w.alive(**a)).count();
        (people, capacity, alive, w.actors.len())
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
            if [50, 100, 200].contains(&played) {
                let (people, _, alive, _) = census(&g);
                marks.push(format!("tick {played}: {alive} alive, fill {:.3}", people as f64 / capacity as f64));
            }
        }
        // After a panic the world may be half-changed: it is read only when the run ended whole.
        let end = if played == 300 {
            let (people, _, alive, _) = census(&g);
            format!("polities alive {alive_0} → {alive} (born in the run {}); people {people_0} → {people}; fill {:.3} → {:.3}", g.world().actors.len() - actors, people_0 as f64 / capacity as f64, people as f64 / capacity as f64)
        } else {
            format!("polities alive at the start {alive_0}; people {people_0}; fill {:.3}", people_0 as f64 / capacity as f64)
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
