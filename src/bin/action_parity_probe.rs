//! Action parity probe — stage 1 of B42 (docs/TRIAGE.md).
//!
//! Three sources answer "what can the player do": the list the UI draws
//! (`commands::get_actions_with_availability` → `list_actions_with_availability`), the
//! mode-aware list (`commands::get_available_actions`, universal actions in
//! `Consequences`, called by no frontend code) and the path that applies an action
//! (`apply_player_action`). The scripted player goes through the last one only.
//!
//! For every action the scripted player applies, this probe asks what the UI showed for
//! it at the start of that turn: available, or unavailable and why. "At the start of the
//! turn" is a snapshot — spending within a turn only lowers resources, so an action
//! unaffordable at the start stays unaffordable. It also counts turns played in
//! `Consequences`.
//!
//! Usage: cargo run --release --bin action_parity_probe -- [seeds] [ticks]

use engine13::application::actions::UnavailableReason;
use engine13::application::scripted::{apply_scripted_actions, ScriptedStrategy};
use rand::SeedableRng;
use std::collections::{BTreeMap, HashMap};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let worlds: &[(&str, &str)] = &[
        ("rome_375", "balanced"), ("rome_375", "influence"), ("rome_375", "wealth"),
        ("constantinople_1430", "balanced"), ("constantinople_1430", "diplomacy"), ("constantinople_1430", "military"),
        ("milan_1477", "aggressive"),
    ];
    println!("{:<20} {:<10} {:>9} {:>14} {:>16} {:>15} {:>14}", "scenario", "strategy", "applied", "UI: available", "UI: no resource", "UI: condition", "in Consequences");
    for (sc, strat) in worlds {
        let mut tally: BTreeMap<&str, u64> = BTreeMap::new();
        let mut by_action: BTreeMap<String, u64> = BTreeMap::new();
        for seed in 0..seeds {
            let db = engine13::db::Db::open_in_memory().unwrap();
            let mut st = engine13::AppState::default();
            engine13::load_scenario(&mut st, &db, sc.to_string()).unwrap();
            st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
            let strategy = ScriptedStrategy::from_str(strat, sc);
            for _ in 0..ticks {
                let shown: HashMap<String, (bool, Option<UnavailableReason>)> = engine13::commands::get_actions_with_availability(&st)
                    .unwrap()
                    .into_iter()
                    .map(|a| (a.action.id.clone(), (a.available, a.unavailable_reason)))
                    .collect();
                let consequences = st.world_state.as_ref().unwrap().game_mode != engine13::core::GameMode::Scenario;
                let turn = apply_scripted_actions(&mut st, &strategy);
                for id in &turn.applied {
                    *tally.entry("applied").or_default() += 1;
                    if consequences {
                        *tally.entry("consequences").or_default() += 1;
                    }
                    match shown.get(*id) {
                        Some((true, _)) => *tally.entry("available").or_default() += 1,
                        Some((false, Some(UnavailableReason::InsufficientCost { .. }))) => {
                            *tally.entry("cost").or_default() += 1;
                            *by_action.entry(id.to_string()).or_default() += 1;
                        }
                        _ => *tally.entry("condition").or_default() += 1,
                    }
                }
                let ws = st.world_state.as_mut().unwrap();
                let scn = st.current_scenario.as_ref().unwrap();
                engine13::engine::tick(ws, scn, &mut st.event_log, st.rng.as_mut().unwrap());
            }
        }
        let g = |k: &str| *tally.get(k).unwrap_or(&0);
        let pct = |k: &str| 100.0 * g(k) as f64 / g("applied").max(1) as f64;
        println!("{:<20} {:<10} {:>9} {:>13.1}% {:>15.1}% {:>14.1}% {:>13.1}%", sc, strat, g("applied"), pct("available"), pct("cost"), pct("condition"), pct("consequences"));
        let mut v: Vec<_> = by_action.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        if !v.is_empty() {
            println!("     unaffordable in the UI, applied anyway: {}", v.iter().take(4).map(|(k, n)| format!("{k}×{n}")).collect::<Vec<_>>().join(", "));
        }
    }
}
