//! The map of rome_375 (E1b, docs/engine2_map.md §7, §15) compiles through the scenario schema.
//!
//! The province table is the cartographer's output (`tools/map/layout.py` →
//! `public/geodata2/rome_375/provinces.toml`): all of Europe with Roman Africa, the East to the
//! Euphrates and Persia to the edge of the plateau — areas, land classes, neighbours, sea
//! neighbours, cultures, religions, regions and the stub population of §11d. Rome is one actor,
//! `rome` (AGENTS §10). The actors are stubs — one house and one ruler per owner, the capital at
//! the owner's first province — until the scenario's own actors (E7). Compiling checks what the
//! schema checks: owners known, neighbours symmetric and known, sea neighbours only on coasts,
//! regions' provinces known, and no province above the capacity the engine derives for its land.

use std::collections::BTreeMap;

use engine2::params::Params;
use engine2::scenario::ScenarioData;

const PROVINCES: &str = include_str!("../../public/geodata2/rome_375/provinces.toml");

#[test]
fn rome_375_map_compiles_through_the_scenario_schema() {
    let table: toml::Table = toml::from_str(PROVINCES).expect("the province table is TOML");
    let provinces = table["provinces"].as_table().expect("[provinces]");
    let mut capital: BTreeMap<String, String> = BTreeMap::new();
    for (id, p) in provinces {
        let owner = p["owner"].as_str().expect("owner").to_string();
        capital.entry(owner).or_insert_with(|| id.clone());
    }
    assert!(capital.contains_key("rome"), "Rome 375 starts with the unified `rome` actor");
    assert!(!capital.contains_key("rome_west") && !capital.contains_key("rome_east"), "successors only");
    let mut text = String::from(
        "id = \"rome_375_map\"\nstart_year = 375\ndiseases = [\"Plague\"]\n\
         [display_currency]\nname = \"solidus\"\nkg_per_unit = 100.0\n[verbs]\n",
    );
    for (owner, cap) in &capital {
        text += &format!(
            "[actors.{owner}]\nname = \"{owner}\"\ntreasury_kg = 0\ncapital = \"{cap}\"\nsoldiers = 0\n\
             institutions = []\nruler = \"{owner}_ruler\"\nsuccession = \"Blood\"\naspirations = []\n\
             [houses.{owner}_house]\nname = \"{owner}\"\npolity = \"{owner}\"\ntreasury_kg = 0\n\
             culture = \"stub\"\nreligion = \"stub\"\nsoldiers = 0\n\
             [persons.{owner}_ruler]\nname = \"{owner}\"\nhouse = \"{owner}_house\"\nborn = 345\n"
        );
    }
    text += PROVINCES;
    let scenario = ScenarioData::parse(&text).expect("the map parses as a scenario");
    assert!(scenario.provinces.len() >= 100, "{} provinces", scenario.provinces.len());
    scenario.compile(&Params::load().unwrap()).expect("the map compiles through the schema");
}
