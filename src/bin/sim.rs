//! Headless simulation binary for batch testing and balance tuning
//! 
//! Usage:
//! ```bash
//! cargo run --bin sim constantinople_1430 50 42
//! cargo run --bin sim constantinople_1430 1000 42  # 1000 ticks for balance testing
//! ```

use engine13::{
    core::{Event, EventType, WorldState},
    engine::{tick, EventLog},
    scenarios::registry,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let scenario_id = args.get(1).map(|s| s.as_str()).unwrap_or("constantinople_1430");
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50);
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(42);

    println!("=== ENGINE13 HEADLESS SIMULATION ===");
    println!("Scenario: {}", scenario_id);
    println!("Ticks: {}", ticks);
    println!("Seed: {}", seed);
    println!();

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
