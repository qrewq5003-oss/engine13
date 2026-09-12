//! Split probe — stage 1 of the `triggers_collapse → on_collapse` task.
//!
//! `ENGINE13_ARCHITECTURE.md` says a milestone with `triggers_collapse` "runs the
//! actor's on_collapse"; the engine only switches the game mode and writes a
//! `Collapse` event. In rome_375 the milestone `rome_splits` tells the chronicler
//! "the empire split" while Rome lives on whole. This probe EMULATES what the
//! architecture describes — at the tick `rome_splits` fires, Rome dies and its two
//! authored heirs (`rome_west`, `rome_east`) are born exactly as `check_collapses`
//! would bear them (template verbatim, Foreground, own authored edges) — and lets
//! the world run on, so the cost of the missing mechanic can be read from a full
//! simulation before any engine code is written.
//!
//! Variants (4th argument):
//!   none        — control: the engine as it is (no split);
//!   split       — the split as `check_collapses` would do it today for TWO heirs:
//!                 no retargeting, so every actor that listed `rome` keeps a dangling
//!                 entry;
//!   split_link  — `split` + each heir's authored edges written back into the
//!                 neighbours' lists (the spawn rule of PR #52 applied to heirs).
//!   shrink_formula — (A₃) "split as shrink": the condition actor STAYS as the
//!                 western heir (renamed after the `rome_west` template), its
//!                 metrics cut to the 0.45 share by the architecture's split
//!                 formula (cohesion 20, legitimacy 30, ep × 1.3 included); only
//!                 `rome_east` is born, by the same formula with share 0.55 from
//!                 the LIVING parent's metrics, with its authored edges written
//!                 back to the neighbours. Every content site addressing `rome`
//!                 keeps addressing the West; the limes edges survive.
//!   shrink_keep — as shrink_formula, but the West keeps its own cohesion,
//!                 legitimacy and external_pressure (only the shares — population,
//!                 military, treasury, economy, quality — are cut).
//!
//! In the `none` variant the probe also prints `COH` rows (rome.cohesion per
//! tick) so the trigger's threshold/duration can be replayed offline.
//!
//! Read-only with respect to the engine: the emulation edits the world between
//! ticks the way the engine's own successor branch does. RNG is drawn only by the
//! engine.
//!
//! Usage: cargo run --release --bin split_probe -- <scenario> <ticks> <seed> <none|split|split_link>

use engine13::{core::{Event, EventType, WorldState}, engine::{tick, EventLog}, scenarios::registry};
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap, HashSet};

const KEYS: [&str; 8] = [
    "external_pressure", "cohesion", "legitimacy", "population",
    "military_size", "military_quality", "economic_output", "treasury",
];

fn fmt(m: &HashMap<String, f64>) -> String {
    KEYS.iter().map(|k| format!("{}={:.1}", k, m.get(*k).copied().unwrap_or(0.0))).collect::<Vec<_>>().join(" ")
}

/// The architecture's split formula («Формула раскола»), applied to a parent's metrics
/// with a given share.
fn split_share(parent: &HashMap<String, f64>, share: f64, trauma: bool) -> HashMap<String, f64> {
    let g = |k: &str| parent.get(k).copied().unwrap_or(0.0);
    let mut m = parent.clone();
    m.insert("population".into(), g("population") * share);
    m.insert("military_size".into(), g("military_size") * share * 0.7);
    m.insert("treasury".into(), g("treasury") * share * 0.5);
    m.insert("military_quality".into(), g("military_quality") * 0.8);
    m.insert("economic_output".into(), g("economic_output") * 0.7);
    if trauma {
        m.insert("cohesion".into(), 20.0);
        m.insert("legitimacy".into(), 30.0);
        m.insert("external_pressure".into(), (g("external_pressure") * 1.3).min(100.0));
    }
    m
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("rome_375");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(42);
    let variant = args.get(4).map(|s| s.as_str()).unwrap_or("none");
    assert!(["none", "split", "split_link", "shrink_formula", "shrink_keep"].contains(&variant), "variant must be none|split|split_link|shrink_formula|shrink_keep");

    let mut scenario = registry::load_by_id(scenario_id).expect("Unknown scenario");
    // Threshold sweep for the `rome_splits` calibration check: E13_SPLIT_T / E13_SPLIT_D
    // override the value and duration of the `triggers_collapse` milestone's condition.
    // Done here rather than in the engine — the probe already owns the scenario object,
    // so no engine code is touched and no RNG draw moves.
    {
        let t: Option<f64> = std::env::var("E13_SPLIT_T").ok().and_then(|v| v.parse().ok());
        let d: Option<u32> = std::env::var("E13_SPLIT_D").ok().and_then(|v| v.parse().ok());
        if t.is_some() || d.is_some() {
            for m in scenario.milestone_events.iter_mut().filter(|m| m.triggers_collapse) {
                if let Some(d) = d {
                    m.condition.duration = Some(d);
                }
                if let Some(t) = t {
                    if let engine13::core::EventConditionType::Metric { value, .. } =
                        &mut m.condition.condition_type
                    {
                        *value = t;
                    }
                }
            }
        }
    }
    let scenario = scenario;
    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed);
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }
    if let Some(ref initial_metrics) = scenario.initial_family_metrics {
        let patriarch_age = scenario.generation_mechanics.as_ref().map(|g| g.patriarch_start_age).unwrap_or(40) as u32;
        world.family_state = Some(engine13::core::FamilyState {
            metrics: engine13::core::normalize_family_metrics(initial_metrics),
            patriarch_age,
            generation_count: 0,
        });
    }
    world.generation_mechanics = scenario.generation_mechanics.clone();
    world.generation_length = scenario.generation_length;

    // The milestone(s) carrying triggers_collapse and the actor each one names.
    let triggers: Vec<(String, Option<String>)> = scenario.milestone_events.iter()
        .filter(|m| m.triggers_collapse)
        .map(|m| (m.id.clone(), match &m.condition.condition_type {
            engine13::core::EventConditionType::Metric { actor_id, .. } => actor_id.clone(),
            engine13::core::EventConditionType::ActorState { actor_id, .. } => Some(actor_id.clone()),
            engine13::core::EventConditionType::Tick { .. } => None,
        }))
        .collect();

    let mut event_log = EventLog::new();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    let mut alive: HashSet<String> = world.actors.keys().cloned().collect();
    let mut seen_deaths = 0usize;
    let mut born_tick: BTreeMap<String, u32> = BTreeMap::new();
    let mut split_done = false;
    let mut split_tick: Option<u32> = None;
    let mut fired_tick: Option<u32> = None;

    for t in 0..ticks {
        tick(&mut world, &scenario, &mut event_log, &mut rng);

        // Engine deaths this tick.
        for d in world.dead_actors[seen_deaths..].iter().cloned() {
            println!("DEATH\t{}\t{}\t{}\t{}\t{}\t{}", scenario_id, seed, variant, t, d.id, fmt(&d.final_metrics));
        }
        seen_deaths = world.dead_actors.len();
        let now: HashSet<String> = world.actors.keys().cloned().collect();
        for id in now.difference(&alive) {
            born_tick.entry(id.clone()).or_insert(t);
            let a = &world.actors[id];
            println!("BIRTH\t{}\t{}\t{}\t{}\t{}\tneighbors={}\t{}", scenario_id, seed, variant, t, id, a.neighbors.len(), fmt(&a.metrics));
        }
        alive = now;

        if variant == "none" {
            if let Some(r) = world.actors.get("rome") {
                println!("COH\t{}\t{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{:.1}", scenario_id, seed, t, r.get_metric("cohesion"), r.get_metric("military_size"), r.get_metric("external_pressure"), r.get_metric("legitimacy"));
            }
        }
        // (A₃) shrink: parent stays, one heir is born from the living parent's metrics.
        if variant.starts_with("shrink") && !split_done && fired_tick.is_some() {
            let (_, actor_id) = &triggers[0];
            if let Some(actor_id) = actor_id {
                if world.actors.contains_key(actor_id) {
                    split_done = true;
                    split_tick = Some(t);
                    let parent_metrics = world.actors[actor_id].metrics.clone();
                    let heirs = world.actors[actor_id].on_collapse.clone();
                    let west_w = heirs.iter().find(|h| h.id == "rome_west").map(|h| h.weight).unwrap_or(0.45);
                    let east = heirs.iter().find(|h| h.id != "rome_west").cloned().expect("an eastern heir");
                    let sum: f64 = heirs.iter().map(|h| h.weight).sum();
                    let (ws, es) = (west_w / sum, east.weight / sum);
                    // West = the parent, shrunk (and renamed after its template).
                    if let Some(tpl) = scenario.actors.iter().find(|a| a.id == "rome_west") {
                        let p = world.actors.get_mut(actor_id).unwrap();
                        p.name = tpl.name.clone(); p.name_short = tpl.name_short.clone();
                    }
                    {
                        let p = world.actors.get_mut(actor_id).unwrap();
                        p.metrics = split_share(&parent_metrics, ws, variant == "shrink_formula");
                    }
                    println!("SPLIT\t{}\t{}\t{}\t{}\t{}\theirs={}\t{}", scenario_id, seed, variant, t, actor_id, east.id, fmt(&parent_metrics));
                    println!("WEST\t{}\t{}\t{}\t{}\t{}\t{}", scenario_id, seed, variant, t, actor_id, fmt(&world.actors[actor_id].metrics));
                    // East = born from the living parent by the formula.
                    if let Some(tpl) = scenario.actors.iter().find(|a| a.id == east.id) {
                        let mut new_actor = tpl.clone();
                        new_actor.metrics = split_share(&parent_metrics, es, true);
                        engine13::core::actor::ensure_default_metrics(&mut new_actor.metrics);
                        new_actor.narrative_status = engine13::core::NarrativeStatus::Foreground;
                        new_actor.is_successor_template = false;
                        let edges = new_actor.neighbors.clone();
                        let name = new_actor.name.clone();
                        world.actors.insert(east.id.clone(), new_actor);
                        event_log.add(Event::new(format!("birth_{}", east.id), t, world.year, east.id.clone(), EventType::Birth, true,
                            format!("Держава {} возникла: восточная половина державы {}", name, world.actors[actor_id].name)));
                        for edge in &edges {
                            if let Some(other) = world.actors.get_mut(&edge.id) {
                                if !other.neighbors.iter().any(|n| n.id == east.id) {
                                    other.neighbors.push(engine13::core::Neighbor { id: east.id.clone(), distance: edge.distance, border_type: edge.border_type.clone() });
                                }
                            }
                        }
                        born_tick.insert(east.id.clone(), t);
                        let a = &world.actors[&east.id];
                        println!("BIRTH\t{}\t{}\t{}\t{}\t{}\tneighbors={}\t{}", scenario_id, seed, variant, t, east.id, a.neighbors.len(), fmt(&a.metrics));
                    }
                    alive = world.actors.keys().cloned().collect();
                }
            }
        }
        // The emulated split, once, on the tick the trigger milestone has fired.
        if fired_tick.is_none() {
            if let Some((mid, _)) = triggers.iter().find(|(mid, _)| world.milestone_events_fired.contains(mid)) {
                fired_tick = Some(t);
                println!("FIRED\t{}\t{}\t{}\t{}\t{}", scenario_id, seed, variant, t, mid);
            }
        }
        if (variant == "split" || variant == "split_link") && !split_done && fired_tick.is_some() {
            let (_, actor_id) = &triggers[0];
            if let Some(actor_id) = actor_id {
                if let Some(parent) = world.actors.remove(actor_id) {
                    split_done = true;
                    split_tick = Some(t);
                    let parent_name = parent.name.clone();
                    let parent_neighbors = parent.neighbors.clone();
                    let heirs = parent.on_collapse.clone();
                    world.dead_actors.push(engine13::core::DeadActor {
                        id: actor_id.clone(),
                        name: parent_name.clone(),
                        tick_death: t,
                        year_death: world.year,
                        final_metrics: engine13::core::actor::metrics_to_snapshot(&parent.metrics),
                        successor_ids: heirs.iter().map(|s| engine13::core::SuccessorWeight { id: s.id.clone(), weight: s.weight }).collect(),
                    });
                    world.dead_actor_ids.insert(actor_id.clone());
                    event_log.add(Event::new(format!("death_{}", actor_id), t, world.year, actor_id.clone(), EventType::Death, true,
                        format!("Держава {} прекратила существование", parent_name)));
                    println!("SPLIT\t{}\t{}\t{}\t{}\t{}\theirs={}\t{}", scenario_id, seed, variant, t, actor_id,
                        heirs.iter().map(|h| h.id.clone()).collect::<Vec<_>>().join(","), fmt(&parent.metrics));
                    for heir in &heirs {
                        if world.dead_actor_ids.contains(&heir.id) || world.actors.contains_key(&heir.id) { continue; }
                        let Some(tpl) = scenario.actors.iter().find(|a| a.id == heir.id) else { continue; };
                        let mut new_actor = tpl.clone();
                        engine13::core::actor::ensure_default_metrics(&mut new_actor.metrics);
                        new_actor.narrative_status = engine13::core::NarrativeStatus::Foreground;
                        new_actor.is_successor_template = false;
                        if new_actor.neighbors.is_empty() { new_actor.neighbors = parent_neighbors.clone(); }
                        let edges = new_actor.neighbors.clone();
                        let name = new_actor.name.clone();
                        world.actors.insert(heir.id.clone(), new_actor);
                        event_log.add(Event::new(format!("birth_{}", heir.id), t, world.year, heir.id.clone(), EventType::Birth, true,
                            format!("Держава {} возникла на месте, которое занимала держава {}", name, parent_name)));
                        if variant == "split_link" {
                            for edge in &edges {
                                if let Some(other) = world.actors.get_mut(&edge.id) {
                                    if !other.neighbors.iter().any(|n| n.id == heir.id) {
                                        other.neighbors.push(engine13::core::Neighbor { id: heir.id.clone(), distance: edge.distance, border_type: edge.border_type.clone() });
                                    }
                                }
                            }
                        }
                        born_tick.insert(heir.id.clone(), t);
                        let a = &world.actors[&heir.id];
                        println!("BIRTH\t{}\t{}\t{}\t{}\t{}\tneighbors={}\t{}", scenario_id, seed, variant, t, heir.id, a.neighbors.len(), fmt(&a.metrics));
                    }
                    alive = world.actors.keys().cloned().collect();
                }
            }
        }
    }

    for (id, bt) in &born_tick {
        if let Some(a) = world.actors.get(id) {
            let living_d1 = a.neighbors.iter().filter(|n| n.distance == 1 && world.actors.contains_key(&n.id)).count();
            println!("ALIVE\t{}\t{}\t{}\t{}\tborn={}\tneighbors={}\tliving_d1={}\t{}", scenario_id, seed, variant, id, bt, a.neighbors.len(), living_d1, fmt(&a.metrics));
        }
        let death = world.dead_actors.iter().find(|d| &d.id == id).map(|d| d.tick_death);
        println!("FATE\t{}\t{}\t{}\t{}\tborn={}\tdeath={}", scenario_id, seed, variant, id, bt, death.map(|d| d.to_string()).unwrap_or_else(|| "-".into()));
    }
    if variant.starts_with("shrink") {
        if let Some(a) = world.actors.get("rome") {
            let living_d1 = a.neighbors.iter().filter(|n| n.distance == 1 && world.actors.contains_key(&n.id)).count();
            println!("ALIVE\t{}\t{}\t{}\trome\tborn=0\tneighbors={}\tliving_d1={}\t{}", scenario_id, seed, variant, a.neighbors.len(), living_d1, fmt(&a.metrics));
        }
    }
    // Who still lists the dead parent at distance 1 (dangling) at the end.
    if let Some((_, Some(pid))) = triggers.first() {
        let dangling: Vec<String> = world.actors.iter().filter(|(_, a)| a.neighbors.iter().any(|n| &n.id == pid && n.distance == 1)).map(|(id, _)| id.clone()).collect();
        let mut dangling = dangling; dangling.sort();
        println!("DANGLING\t{}\t{}\t{}\tparent={}\talive_listing_parent_d1=[{}]", scenario_id, seed, variant, pid, dangling.join(","));
    }
    println!("SUMMARY\t{}\t{}\t{}\tfired={}\tsplit={}\tdeaths={}\talive_end={}", scenario_id, seed, variant,
        fired_tick.map(|x| x.to_string()).unwrap_or_else(|| "-".into()), split_tick.map(|x| x.to_string()).unwrap_or_else(|| "-".into()),
        world.dead_actors.len(), world.actors.len());
}
