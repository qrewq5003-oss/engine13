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

const PROVINCES: &str = include_str!("../../public/geodata2/milan_1477/provinces.toml");

#[test]
fn milan_1477_map_compiles_through_the_scenario_schema() {
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
    for (owner, cap) in &capital {
        text += &format!(
            "[actors.{owner}]\nname = \"{owner}\"\ntreasury_kg = 0\ncapital = \"{cap}\"\nsoldiers = 0\n\
             institutions = []\nruler = \"{owner}_ruler\"\nsuccession = \"Blood\"\naspirations = []\n\
             [houses.{owner}_house]\nname = \"{owner}\"\npolity = \"{owner}\"\ntreasury_kg = 0\n\
             culture = \"stub\"\nreligion = \"stub\"\nsoldiers = 0\n\
             [persons.{owner}_ruler]\nname = \"{owner}\"\nhouse = \"{owner}_house\"\nborn = 1440\n"
        );
    }
    text += PROVINCES;
    let scenario = ScenarioData::parse(&text).expect("the map parses as a scenario");
    assert!(scenario.provinces.len() >= 50, "{} provinces", scenario.provinces.len());
    scenario.compile(&Params::load().unwrap()).expect("the map compiles through the schema");
}
