//! Worlds in parallel (`engine2::par`, owner's rule after #293): the same worlds played on
//! threads and one after another give the same results byte for byte, in the order of the
//! items; a panic in one world is not lost.

use engine2::sim::{Game, Simulation};
use engine2::{worldgen, Params, TickReport};

/// One generated world with the bots: every tick's report and the save at the end.
fn play(seed: u64) -> (Vec<TickReport>, String) {
    let p = Params::load().unwrap();
    let s = worldgen::generate(seed, &p);
    let mut g = Game::new(&s, p, seed).unwrap();
    let reports = (0..60).map(|_| g.tick(&[]).unwrap()).collect();
    (reports, g.to_save().unwrap())
}

#[test]
fn worlds_on_threads_give_byte_for_byte_what_they_give_one_after_another_in_the_order_of_the_seeds() {
    let seeds: Vec<u64> = (1..=8).collect();
    let parallel = engine2::par::map(&seeds, |s| play(*s));
    let sequential: Vec<(Vec<TickReport>, String)> = seeds.iter().map(|s| play(*s)).collect();
    assert_eq!(parallel.len(), seeds.len());
    for (i, (a, b)) in parallel.iter().zip(&sequential).enumerate() {
        assert_eq!(a.1, b.1, "seed {}: the save differs", seeds[i]);
        assert_eq!(a.0, b.0, "seed {}: the reports differ", seeds[i]);
    }
    // In the order of the items, whatever order the threads finished in.
    let order = engine2::par::map(&(0..64u64).collect::<Vec<_>>(), |x| {
        std::thread::sleep(std::time::Duration::from_micros((64 - x) * 50));
        *x
    });
    assert_eq!(order, (0..64).collect::<Vec<_>>());
    assert!(engine2::par::threads() >= 1);
}

#[test]
fn a_panic_in_one_world_fails_the_run() {
    let r = std::panic::catch_unwind(|| engine2::par::map(&[1u64, 2, 3, 4], |x| if *x == 3 { panic!("world 3") } else { *x }));
    assert!(r.is_err(), "the panic of world 3 is not swallowed");
}
