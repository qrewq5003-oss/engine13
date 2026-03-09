//! Headless simulation binary for batch testing and balance tuning
//!
//! Usage:
//! ```bash
//! cargo run --bin sim constantinople_1430 50 42
//! cargo run --bin sim constantinople_1430 1000 42  # 1000 ticks for balance testing
//! cargo run --bin sim constantinople_1430 50 batch  # batch mode: 100 runs with seeds 0-99
//! ```

use engine13::{
    core::{Event, EventType, WorldState},
    engine::{tick, EventLog},
    scenarios::registry,
};
use rand::SeedableRng;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("constantinople_1430");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50);
    let batch_mode = args.get(3).map(|s| s == "batch").unwrap_or(false);

    println!("=== ENGINE13 HEADLESS SIMULATION ===");
    println!("Scenario: {}", scenario_id);
    println!("Ticks: {}", ticks);
    
    if batch_mode {
        run_batch(scenario_id, ticks);
    } else {
        let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(42);
        println!("Seed: {}", seed);
        println!();
        run_single(scenario_id, ticks, seed);
    }
}

fn run_single(scenario_id: &str, ticks: u32, seed: u64) {
    let scenario = registry::load_by_id(scenario_id)
        .expect("Unknown scenario");

    let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed, [0u8; 32]);

    // Initialize actors from scenario
    for actor in &scenario.actors {
        if !actor.is_successor_template {
            world.actors.insert(actor.id.clone(), actor.clone());
        }
    }

    let mut stats = SimStats::default();
    let mut event_log = EventLog::new();

    for tick_num in 0..ticks {
        tick(&mut world, &scenario, &mut event_log);
        let events: Vec<Event> = event_log.events.iter()
            .filter(|e| e.tick == tick_num)
            .cloned()
            .collect();
        stats.record(tick_num, &world, &events);

        // Progress indicator every 10 ticks
        if (tick_num + 1) % 10 == 0 {
            eprintln!("Progress: tick {}/{}", tick_num + 1, ticks);
        }
    }

    stats.print_report();
}

fn run_batch(scenario_id: &str, ticks: u32) {
    println!("Running batch mode: 100 runs with seeds 0-99");
    println!();

    let scenario = registry::load_by_id(scenario_id)
        .expect("Unknown scenario");

    let mut collapses: Vec<u32> = vec![];
    let mut victories: Vec<u32> = vec![];
    let mut events_per_run: Vec<u32> = vec![];

    for seed in 0..100u64 {
        let mut world = WorldState::with_seed(scenario.id.clone(), scenario.start_year, seed, [0u8; 32]);

        // Initialize actors from scenario
        for actor in &scenario.actors {
            if !actor.is_successor_template {
                world.actors.insert(actor.id.clone(), actor.clone());
            }
        }

        let mut stats = BatchStats::default();
        let mut event_log = EventLog::new();

        for tick_num in 0..ticks {
            tick(&mut world, &scenario, &mut event_log);
            let events: Vec<Event> = event_log.events.iter()
                .filter(|e| e.tick == tick_num)
                .cloned()
                .collect();
            stats.record(tick_num, &world, &events);

            // Stop early if victory or collapse
            if world.victory_achieved || world.dead_actor_ids.iter().any(|id| id.contains("byzantium")) {
                break;
            }
        }

        if let Some(t) = stats.collapse_tick { collapses.push(t); }
        if let Some(t) = stats.victory_tick { victories.push(t); }
        events_per_run.push(stats.random_events_fired);
    }

    let collapse_pct = collapses.len() as f64 / 100.0 * 100.0;
    let victory_pct = victories.len() as f64 / 100.0 * 100.0;
    let early_collapses = collapses.iter().filter(|&&t| t < 10).count();
    let mid_collapses = collapses.iter().filter(|&&t| t < 20).count();

    let mut sorted_collapses = collapses.clone(); sorted_collapses.sort();
    let mut sorted_victories = victories.clone(); sorted_victories.sort();
    let median_collapse = sorted_collapses.get(sorted_collapses.len() / 2).copied().unwrap_or(0);
    let median_victory = sorted_victories.get(sorted_victories.len() / 2).copied().unwrap_or(0);
    let avg_events = events_per_run.iter().sum::<u32>() as f64 / 100.0;

    println!("=== BALANCE REPORT (100 runs, {} ticks each) ===", ticks);
    println!("Byzantium collapse: {} runs ({:.0}%)", collapses.len(), collapse_pct);
    println!("  median collapse tick: {}", median_collapse);
    println!("  collapses before tick 10: {}", early_collapses);
    println!("  collapses before tick 20: {}", mid_collapses);
    println!("Victory achieved: {} runs ({:.0}%)", victories.len(), victory_pct);
    println!("  median victory tick: {}", median_victory);
    println!("Avg random events per run: {:.1}", avg_events);
}

#[derive(Default)]
struct SimStats {
    pub federation_progress: Vec<f64>,
    pub byzantium_pressure: Vec<f64>,
    pub byzantium_alive: Vec<bool>,
    pub random_events_fired: u32,
    pub military_conflicts: u32,
    pub collapses: Vec<String>,
}

impl SimStats {
    fn record(&mut self, _tick: u32, world: &WorldState, events: &[Event]) {
        // Track federation progress
        self.federation_progress.push(
            world.global_metrics.get("federation_progress").copied().unwrap_or(0.0)
        );

        // Track Byzantium status
        if let Some(byz) = world.actors.get("byzantium") {
            self.byzantium_pressure.push(byz.metrics.external_pressure);
            self.byzantium_alive.push(!world.dead_actor_ids.contains("byzantium"));
        }

        // Count events by type
        for event in events {
            match event.event_type {
                EventType::Threshold => self.random_events_fired += 1,
                EventType::War => self.military_conflicts += 1,
                EventType::Collapse => self.collapses.push(event.actor_id.clone()),
                _ => {}
            }
        }
    }

    fn print_report(&self) {
        println!();
        println!("=== SIMULATION REPORT ===");
        println!("Ticks completed: {}", self.federation_progress.len());

        if let Some(final_fed) = self.federation_progress.last() {
            println!("Federation final: {:.1}", final_fed);
        }
        let max_fed = self.federation_progress.iter().cloned().fold(0.0_f64, f64::max);
        println!("Federation max: {:.1}", max_fed);

        if let Some(&survived) = self.byzantium_alive.last() {
            println!("Byzantium survived: {}", survived);
        }
        let max_pressure = self.byzantium_pressure.iter().cloned().fold(0.0_f64, f64::max);
        println!("Byzantium max pressure: {:.1}", max_pressure);

        println!("Random events fired: {}", self.random_events_fired);
        println!("Military conflicts: {}", self.military_conflicts);

        if !self.collapses.is_empty() {
            println!("Collapses: {}", self.collapses.join(", "));
        }

        println!();
        println!("=== PRESSURE TIMELINE (every 5 ticks) ===");
        for (i, p) in self.byzantium_pressure.iter().enumerate() {
            if i % 5 == 0 {
                let bar = "█".repeat((*p as usize) / 5);
                println!("tick {:3}: {} {:.1}", i, bar, p);
            }
        }

        // Final pressure
        if let Some(&last) = self.byzantium_pressure.last() {
            let bar = "█".repeat((last as usize) / 5);
            println!("tick {:3}: {} {:.1} [FINAL]", self.byzantium_pressure.len() - 1, bar, last);
        }
    }
}

#[derive(Default)]
struct BatchStats {
    pub collapse_tick: Option<u32>,
    pub victory_tick: Option<u32>,
    pub random_events_fired: u32,
}

impl BatchStats {
    fn record(&mut self, tick: u32, world: &WorldState, events: &[Event]) {
        // Check for Byzantium collapse
        if self.collapse_tick.is_none()
            && world.dead_actor_ids.iter().any(|a| a.contains("byzantium")) {
            self.collapse_tick = Some(tick);
        }
        // Check for victory
        if self.victory_tick.is_none() && world.victory_achieved {
            self.victory_tick = Some(tick);
        }
        // Count random events
        self.random_events_fired += events.iter()
            .filter(|e| matches!(e.event_type, EventType::Threshold))
            .count() as u32;
    }
}
