//! Economy project: the corrected Ц6 ceiling measure, checked both ways (brief §9.1). Built with
//! `--features census`. Every world, 30 seeds × 300 ticks.
//!
//! The measure: the share of living actor-ticks with pressure ≥ 99 while the threat `T_p` < 90 —
//! "a ceiling without a threat" — under 30 % in every world. It must fail on v1 and on v2 before
//! Ц6, and pass on Ц6 (`fa54353`) and on Ц5 variant (a) (legitimacy pulled at r = 0.03). Every
//! model sets its switches itself, whatever the content holds. For information: the old share at
//! the ceiling, and the share at the ceiling with `T_p` ≥ 90.
//!
//! Then the content check: v2 on as the content stands is the same world as variant (a) set here,
//! tick by tick, bit for bit, in every run.
//!
//! Usage: cargo run --release --features census --bin c6m_probe -- [seeds] [ticks]

use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use rand::SeedableRng;

#[derive(Clone, Copy, PartialEq)]
enum Model { V1, V2BeforeC6, C6, C5a, Content }

impl Model {
    fn label(&self) -> &'static str {
        match self {
            Model::V1 => "v1",
            Model::V2BeforeC6 => "v2 before Ц6 (no pressure pull)",
            Model::C6 => "Ц6 (fa54353)",
            Model::C5a => "Ц5 (a), legitimacy r = 0.03",
            Model::Content => "v2 as in the content",
        }
    }
    /// what the measure must say on this model
    fn must_pass(&self) -> bool { matches!(self, Model::C6 | Model::C5a) }
}

fn worlds(sc: &str) -> &'static [&'static str] {
    match sc {
        "rome_375" => &["none", "balanced", "influence", "wealth"],
        "milan_1477" => &["none", "aggressive"],
        _ => &["none", "balanced", "diplomacy", "military"],
    }
}

/// (living actor-ticks, at the ceiling, at the ceiling with T_p < 90), and a fingerprint of every
/// actor's metrics, bit for bit, every tick
fn run(sc: &str, world: &str, m: Model, seed: u64, ticks: u32) -> ((u64, u64, u64), u64) {
    let db = engine13::db::Db::open_in_memory().unwrap();
    let mut st = engine13::AppState::default();
    engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    if m == Model::Content {
        st.current_scenario.as_mut().unwrap().features.economy_v2 = true;
    } else {
        let s = st.current_scenario.as_mut().unwrap();
        s.features.economy_v2 = m != Model::V1;
        let c6 = matches!(m, Model::C6 | Model::C5a);
        s.economy_v2_pressure_tags_as_level = c6;
        s.economy_v2_pressure_pull = if c6 { Some(0.10) } else { None };
        s.economy_v2_legitimacy_pull = if m == Model::C5a { Some(0.03) } else { None };
    }
    let strategy = (world != "none").then(|| ScriptedStrategy::from_str(world, sc));
    let mut acc = (0, 0, 0);
    let mut fp = std::collections::hash_map::DefaultHasher::new();
    for _ in 0..ticks {
        match &strategy {
            Some(s) => { play_scripted_tick(&mut st, s); }
            None => {
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let ws = st.world_state.as_ref().unwrap();
        let mut ids: Vec<&String> = ws.actors.keys().collect();
        ids.sort();
        for id in ids {
            let mut ms: Vec<(&String, &f64)> = ws.actors[id].metrics.iter().collect();
            ms.sort_by(|a, b| a.0.cmp(b.0));
            for (k, v) in ms { std::hash::Hash::hash(&(id, k, v.to_bits()), &mut fp); }
        }
        for (id, a) in &ws.actors {
            if ws.dead_actor_ids.contains(id) { continue; }
            acc.0 += 1;
            if a.get_metric("external_pressure") >= 99.0 {
                acc.1 += 1;
                if engine13::engine::pressure_threat(ws, id).is_some_and(|t| t < 90.0) { acc.2 += 1; }
            }
        }
    }
    (acc, std::hash::Hasher::finish(&fp))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("# The corrected Ц6 measure, both ways, {seeds} seeds × {ticks} ticks per world\n");
    println!("| scenario | world | model | at the ceiling (old) | at the ceiling with T_p ≥ 90 | **ceiling without a threat (T_p < 90)** |");
    println!("|---|---|---|---|---|---|");
    let models = [Model::V1, Model::V2BeforeC6, Model::C6, Model::C5a];
    let mut passing = [0usize; 4];
    let mut n = 0;
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            n += 1;
            for (i, m) in models.iter().enumerate() {
                let t = (0..seeds).map(|s| run(sc, world, *m, s, ticks).0).fold((0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2));
                let p = |x: u64| 100.0 * x as f64 / t.0.max(1) as f64;
                if p(t.2) < 30.0 { passing[i] += 1; }
                println!("| {sc} | {world} | {} | {:.0} % | {:.0} % | {:.0} % |", m.label(), p(t.1), p(t.1 - t.2), p(t.2));
            }
        }
    }
    println!("\n## Verdict\n");
    println!("| model | worlds passing (< 30 %) | must | as required |");
    println!("|---|---|---|---|");
    for (i, m) in models.iter().enumerate() {
        let passes = passing[i] == n;
        println!("| {} | {} / {n} | {} | {} |", m.label(), passing[i], if m.must_pass() { "pass" } else { "fail" }, if passes == m.must_pass() { "yes" } else { "**no**" });
    }
    let mut same = 0;
    let mut total = 0;
    for sc in ["rome_375", "constantinople_1430", "milan_1477"] {
        for world in worlds(sc) {
            for seed in 0..seeds {
                total += 1;
                if run(sc, world, Model::Content, seed, ticks).1 == run(sc, world, Model::C5a, seed, ticks).1 { same += 1; }
            }
        }
    }
    println!("\n## Content check\n\nv2 as in the content against variant (a): {same} of {total} runs identical, every actor metric every tick.");
}
