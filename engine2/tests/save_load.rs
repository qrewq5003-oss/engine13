//! Acceptance of E1a (plan §4): a run interrupted by save → load continues bit for bit.

use engine2::{from_save, testworld, to_save, Params};

#[test]
fn save_then_load_continues_the_same_run() {
    let seed = 0xA11CE;
    let params = Params::load().unwrap();

    let mut straight = testworld::world(seed);
    let mut straight_reports = Vec::new();
    for _ in 0..300 {
        straight_reports.push(straight.tick(&params).unwrap());
    }

    let mut first = testworld::world(seed);
    let mut reports = Vec::new();
    for _ in 0..150 {
        reports.push(first.tick(&params).unwrap());
    }
    // The save is taken in the middle of accumulated remainders: at least one province
    // carries a non-zero fraction, so the carry itself travels through the save.
    assert!(first.ledger.accounts().any(|a| first.ledger.carry(a).unwrap() != 0), "no remainder to save");
    let save = to_save(&first).unwrap();
    drop(first);
    let mut second = from_save(&save).unwrap();
    for _ in 0..150 {
        reports.push(second.tick(&params).unwrap());
    }

    assert_eq!(reports, straight_reports);
    assert_eq!(to_save(&second).unwrap(), to_save(&straight).unwrap());
    // And the run did move stocks: this is not an empty world.
    assert!(straight_reports.iter().any(|r| r.postings.iter().any(|p| p.milli != 0)));
}

#[test]
fn save_is_json_with_a_format_number_and_floats_round_trip() {
    let mut w = testworld::world(1);
    // A legitimacy value that does not print in shortest form without float_roundtrip.
    w.actors.get_mut(&engine2::ActorId(0)).unwrap().legitimacy = 0.1 + 0.2;
    let s = to_save(&w).unwrap();
    assert!(s.starts_with(&format!("{{\"format\":{},", engine2::SAVE_FORMAT)));
    let back = from_save(&s).unwrap();
    assert_eq!(back.actors[&engine2::ActorId(0)].legitimacy.to_bits(), (0.1f64 + 0.2).to_bits());
}
