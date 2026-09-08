//! Siege-reach probe — stage 1 of the "dangling references / besieged reach" task.
//!
//! `check_collapses` (path 3, conquest by exhaustion) requires `besieged`: an armed
//! living actor at distance 1 **in the dying actor's own neighbour list**. Task (D₄)
//! found that this makes an actor immortal the moment its only distance-1 neighbour
//! dies without heir: the entry dangles, nobody can besiege it, and the two other
//! collapse paths need low cohesion that nothing pushes down any more. The huns
//! lived that way in 28 of 30 rome runs.
//!
//! This probe evaluates, on every tick and for every living actor, four readings of
//! "besieged" — own list d = 1 (the engine's), own list d ≤ 2, symmetric d = 1 (an
//! armed actor at distance 1 in EITHER list), symmetric d ≤ 2 — and replays the
//! engine's three-consecutive-ticks danger counter under each, so it can say on
//! which tick the actor WOULD have collapsed under each variant, without a second
//! simulation (task 23's method: counterfactual verdict of the predicate). The
//! engine's own variant must reproduce every actual death tick exactly — that is the
//! probe's self-check (`cf_own1 == actual`).
//!
//! First-order only: a counterfactual death changes the world after it, so a
//! variant's later verdicts on other actors are not trustworthy past the first
//! divergence. The probe reports first deaths; cascades belong to stage 2.
//!
//! Read-only: drives `tick()`, reads metrics. No engine symbol is touched.
//!
//! Usage: cargo run --release --bin siege_probe -- <scenario> <ticks> <seed>

use engine13::{core::WorldState, engine::{tick, EventLog}, scenarios::registry};
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap};

const MIN_MIL: f64 = engine13::engine::interactions::MIN_DEFENSIBLE_MILITARY;
const VARIANTS: [&str; 5] = ["own1", "own2", "sym1", "sym2", "heir"];
const NV: usize = 5;

#[derive(Default, Clone)]
struct Row {
    actual_death: Option<u32>,
    path: String,
    streak: [u32; NV],
    cf_death: [Option<u32>; NV],
    /// Ticks in the conquest band (mil < MIN, leg < 10, ep > 85) but not besieged by
    /// the engine's reading — alive only because of the clause.
    defenceless_ticks: u32,
    /// Of those, ticks where the variant would have read "besieged".
    flip_ticks: [u32; NV],
    has_d1_edge: bool,
    dangling_d1_end: usize,
    living_d1_end: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("rome_375");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(42);

    let scenario = registry::load_by_id(scenario_id).expect("Unknown scenario");
    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario.generation_mechanics.as_ref()
            .map(|g| g.patriarch_start_age).unwrap_or(40) as u32;
        world.family_state = Some(engine13::core::FamilyState {
            metrics: engine13::core::normalize_family_metrics(initial_metrics),
            patriarch_age,
            generation_count: 0,
        });
    }
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    // Last-seen neighbour list per actor: a dying actor is removed inside the tick
    // that kills it, so its list is no longer in `world.actors` when the death is
    // observed; the engine evaluated its predicate with this list.
    let mut last_neighbors: HashMap<String, Vec<(String, u32)>> = HashMap::new();
    // Variant "heir": the border of an heirless dead actor passes to its besieger
    // (the strongest armed actor at distance 1 on either side, ties by id). Kept as
    // an overlay of extra edges on top of the real lists.
    let mut extra: HashMap<String, Vec<(String, u32)>> = HashMap::new();
    let mut seen_deaths = 0usize;

    for t in 0..ticks {
        tick(&mut world, &scenario, &mut event_log, &mut rng);
        let mil_of = |w: &WorldState, id: &str| w.actors.get(id).map(|a| a.get_metric("military_size")).unwrap_or(0.0);

        // Reverse index: for every living actor, the armed living actors that list
        // it, with the distance they list it at.
        let mut listed_by: HashMap<&str, Vec<(u32, bool)>> = HashMap::new();
        for (bid, b) in &world.actors {
            let armed = b.get_metric("military_size") >= MIN_MIL;
            for n in &b.neighbors {
                if world.actors.contains_key(&n.id) && n.id != *bid {
                    listed_by.entry(n.id.as_str()).or_default().push((n.distance, armed));
                }
            }
        }

        // Evaluate the five readings of "besieged" for an actor given its list.
        let besieged_of = |aid: &str, list: &[(String, u32)], extra: &HashMap<String, Vec<(String, u32)>>, listed_by: &HashMap<&str, Vec<(u32, bool)>>| -> [bool; NV] {
            let own = |max_d: u32| list.iter().any(|(nid, d)| *d <= max_d && nid != aid && mil_of(&world, nid) >= MIN_MIL);
            let rev = |max_d: u32| listed_by.get(aid).map(|v| v.iter().any(|(d, armed)| *d <= max_d && *armed)).unwrap_or(false);
            let heir = own(1) || extra.get(aid).map(|v| v.iter().any(|(nid, d)| *d == 1 && nid != aid && mil_of(&world, nid) >= MIN_MIL)).unwrap_or(false);
            [own(1), own(2), own(1) || rev(1), own(2) || rev(2), heir]
        };

        // Living actors: update last-seen lists, then evaluate.
        let mut snapshot: Vec<(String, Vec<(String, u32)>, [f64; 4], Option<u32>)> = Vec::new();
        for (aid, a) in &world.actors {
            let list: Vec<(String, u32)> = a.neighbors.iter().map(|n| (n.id.clone(), n.distance)).collect();
            last_neighbors.insert(aid.clone(), list.clone());
            snapshot.push((aid.clone(), list, [a.get_metric("military_size"), a.get_metric("legitimacy"), a.get_metric("cohesion"), a.get_metric("external_pressure")], a.minimum_survival_ticks));
            let row = rows.entry(aid.clone()).or_default();
            row.has_d1_edge = a.neighbors.iter().any(|n| n.distance == 1);
            row.dangling_d1_end = a.neighbors.iter().filter(|n| n.distance == 1 && !world.actors.contains_key(&n.id)).count();
            row.living_d1_end = a.neighbors.iter().filter(|n| n.distance == 1 && world.actors.contains_key(&n.id)).count();
        }
        // Just-dead actors: evaluated with their final metrics and last-seen list.
        // (The engine evaluated every candidate before removing any, so a neighbour
        // that died on the same tick was still armed at evaluation time; here it is
        // already gone — a same-tick pair is the one known blind spot.)
        for d in &world.dead_actors[seen_deaths..] {
            let g = |k: &str| d.final_metrics.get(k).copied().unwrap_or(0.0);
            let list = last_neighbors.get(&d.id).cloned().unwrap_or_default();
            snapshot.push((d.id.clone(), list, [g("military_size"), g("legitimacy"), g("cohesion"), g("external_pressure")], None));
        }

        for (aid, list, m, min_surv) in &snapshot {
            let row = rows.entry(aid.clone()).or_default();
            let (mil, leg, coh, ep) = (m[0], m[1], m[2], m[3]);
            let besieged = besieged_of(aid, list, &extra, &listed_by);
            let classic = leg < 10.0 && coh < 15.0 && ep > 85.0;
            let internal = leg < 5.0 && coh < 8.0;
            let band = mil < MIN_MIL && leg < 10.0 && ep > 85.0;
            let survival_ok = min_surv.map(|mv| t >= mv).unwrap_or(true);
            if band && !besieged[0] {
                row.defenceless_ticks += 1;
                for v in 0..NV { if besieged[v] { row.flip_ticks[v] += 1; } }
            }
            for v in 0..NV {
                let danger = survival_ok && (classic || internal || (band && besieged[v]));
                if danger {
                    row.streak[v] += 1;
                    if row.streak[v] >= 3 && row.cf_death[v].is_none() { row.cf_death[v] = Some(t); }
                } else {
                    row.streak[v] = 0;
                }
            }
        }

        // Variant "heir": heirless deaths this tick hand their border to the besieger.
        for d in world.dead_actors[seen_deaths..].to_vec() {
            let heir_born = d.successor_ids.iter().any(|s| world.actors.contains_key(&s.id));
            if heir_born { continue; }
            let list = last_neighbors.get(&d.id).cloned().unwrap_or_default();
            // candidates: armed, alive, at d1 in the dead actor's list or listing it at d1
            let mut cands: Vec<String> = list.iter().filter(|(nid, dd)| *dd == 1 && mil_of(&world, nid) >= MIN_MIL).map(|(nid, _)| nid.clone()).collect();
            for (bid, b) in &world.actors {
                if b.neighbors.iter().any(|n| n.id == d.id && n.distance == 1) && b.get_metric("military_size") >= MIN_MIL {
                    cands.push(bid.clone());
                }
            }
            cands.sort(); cands.dedup();
            let Some(besieger) = cands.iter().max_by(|a, b| mil_of(&world, a).partial_cmp(&mil_of(&world, b)).unwrap().then(b.cmp(a))).cloned() else { continue; };
            // besieger takes the dead actor's edges; everyone who listed the dead actor now lists the besieger
            for (nid, dd) in &list {
                if *nid != besieger && world.actors.contains_key(nid) {
                    extra.entry(besieger.clone()).or_default().push((nid.clone(), *dd));
                    extra.entry(nid.clone()).or_default().push((besieger.clone(), *dd));
                }
            }
            for (bid, b) in &world.actors {
                for n in &b.neighbors {
                    if n.id == d.id && *bid != besieger {
                        extra.entry(bid.clone()).or_default().push((besieger.clone(), n.distance));
                        extra.entry(besieger.clone()).or_default().push((bid.clone(), n.distance));
                    }
                }
            }
        }
        seen_deaths = world.dead_actors.len();
    }

    for d in &world.dead_actors {
        let row = rows.entry(d.id.clone()).or_default();
        row.actual_death = Some(d.tick_death);
        let g = |k: &str| d.final_metrics.get(k).copied().unwrap_or(0.0);
        row.path = if g("legitimacy") < 5.0 && g("cohesion") < 8.0 { "internal" }
            else if g("legitimacy") < 10.0 && g("cohesion") < 15.0 && g("external_pressure") > 85.0 { "classic" }
            else { "conquest" }.to_string();
    }

    let fmt = |o: Option<u32>| o.map(|x| x.to_string()).unwrap_or_else(|| "-".into());
    let mut mismatches = 0;
    for (id, r) in &rows {
        if r.actual_death != r.cf_death[0] { mismatches += 1; }
        print!("ACTOR\t{}\t{}\t{}\tactual={}\tpath={}", scenario_id, seed, id, fmt(r.actual_death), if r.path.is_empty() { "-" } else { &r.path });
        for v in 0..NV { print!("\tcf_{}={}", VARIANTS[v], fmt(r.cf_death[v])); }
        print!("\tdefenceless_ticks={}", r.defenceless_ticks);
        for v in 1..NV { print!("\tflip_{}={}", VARIANTS[v], r.flip_ticks[v]); }
        println!("\thas_d1_edge={}\tliving_d1_end={}\tdangling_d1_end={}", r.has_d1_edge, r.living_d1_end, r.dangling_d1_end);
    }
    println!("SUMMARY\t{}\t{}\tactors={}\tdeaths={}\tselfcheck_mismatches={}", scenario_id, seed, rows.len(), world.dead_actors.len(), mismatches);
}
