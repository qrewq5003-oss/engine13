//! Acceptance of E1a (plan §4): a run interrupted by save → load continues bit for bit.

use engine2::{from_save, testworld, to_save, World};

#[test]
fn save_then_load_continues_the_same_run() {
    let facts = testworld::five_states();
    let seed = 0xA11CE;

    let mut straight = World::from_facts(&facts, seed).unwrap();
    let mut straight_reports = Vec::new();
    for _ in 0..300 {
        straight_reports.push(straight.tick().unwrap());
    }

    let mut first = World::from_facts(&facts, seed).unwrap();
    let mut reports = Vec::new();
    for _ in 0..150 {
        reports.push(first.tick().unwrap());
    }
    let save = to_save(&first).unwrap();
    drop(first);
    let mut second = from_save(&save).unwrap();
    for _ in 0..150 {
        reports.push(second.tick().unwrap());
    }

    assert_eq!(reports, straight_reports);
    assert_eq!(to_save(&second).unwrap(), to_save(&straight).unwrap());
}

#[test]
fn save_is_json_with_a_format_number_and_floats_round_trip() {
    let mut w = World::from_facts(&testworld::five_states(), 1).unwrap();
    // A legitimacy value that does not print in shortest form without float_roundtrip.
    w.actors.get_mut(&engine2::ActorId(0)).unwrap().legitimacy = 0.1 + 0.2;
    let s = to_save(&w).unwrap();
    assert!(s.starts_with(&format!("{{\"format\":{},", engine2::SAVE_FORMAT)));
    let back = from_save(&s).unwrap();
    assert_eq!(back.actors[&engine2::ActorId(0)].legitimacy.to_bits(), (0.1f64 + 0.2).to_bits());
}
