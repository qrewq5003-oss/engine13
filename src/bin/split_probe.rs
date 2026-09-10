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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("rome_375");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(42);
    let variant = args.get(4).map(|s| s.as_str()).unwrap_or("none");
    assert!(["none", "split", "split_link"].contains(&variant), "variant must be none|split|split_link");

    let scenario = registry::load_by_id(scenario_id).expect("Unknown scenario");
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

        // The emulated split, once, on the tick the trigger milestone has fired.
        if fired_tick.is_none() {
            if let Some((mid, _)) = triggers.iter().find(|(mid, _)| world.milestone_events_fired.contains(mid)) {
                fired_tick = Some(t);
                println!("FIRED\t{}\t{}\t{}\t{}\t{}", scenario_id, seed, variant, t, mid);
            }
        }
        if variant != "none" && !split_done && fired_tick.is_some() {
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
