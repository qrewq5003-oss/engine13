//! Actions with an absent addressee — measurement after B44 stage 1 (docs/TRIAGE.md).
//!
//! B44 found reads of absent actors in conditions; actions have the mirror problem on
//! the write side. An action's effects are addressed to actors (`actor:byzantium.*`);
//! when the addressee is gone, `apply` finds nobody and the effect is lost, while the
//! cost is still paid by a living source. And an action gated on an absent actor's
//! metric is refused with an ordinary-looking reason («условие не выполнено»).
//!
//! Per world, per patron action (the only list the product shows and applies):
//! - shown available while at least one actor it writes to is absent — all of them
//!   ("void": every effect lost) or some ("partial");
//! - refused on a read of an absent actor, and the reason the player is shown;
//! - scripted applications with an absent addressee: what the living paid (the cost and
//!   every negative effect on a present actor — content writes some prices as effects)
//!   and what still arrived (positive effects on present actors and on globals).
//!
//! The availability is the product's own `action_availability` (B42), evaluated at the
//! start of each tick, before the player acts. Nothing is changed.
//!
//! Usage: cargo run --release --bin action_target_probe -- [seeds] [ticks]

use engine13::application::actions::{action_availability, UnavailableReason};
use engine13::application::scripted::{play_scripted_tick, ScriptedStrategy};
use engine13::core::{ActionCondition, MetricRef, PatronAction, WorldState};
use rand::SeedableRng;
use std::collections::{BTreeMap, BTreeSet};

const WORLDS: &[(&str, &[&str])] = &[
    ("rome_375", &["none", "balanced", "influence", "wealth"]),
    ("constantinople_1430", &["none", "balanced", "diplomacy", "military"]),
    ("milan_1477", &["none", "aggressive"]),
];

fn absent(m: &MetricRef, ws: &WorldState) -> Option<String> {
    match m {
        MetricRef::Actor { actor_id, .. } if !ws.actors.contains_key(actor_id.as_str()) => Some(actor_id.as_str().to_string()),
        _ => None,
    }
}

/// Absent actors the action writes to, and whether every effect is lost.
fn lost_effects(a: &PatronAction, ws: &WorldState) -> (BTreeSet<String>, bool) {
    let gone: BTreeSet<String> = a.effects.keys().filter_map(|m| absent(m, ws)).collect();
    let all = !a.effects.is_empty() && a.effects.keys().all(|m| absent(m, ws).is_some());
    (gone, all)
}

fn reads_absent(a: &PatronAction, ws: &WorldState) -> bool {
    let cond = matches!(&a.available_if, ActionCondition::Metric { metric, .. } if absent(metric, ws).is_some());
    cond || a.cost.keys().any(|m| absent(m, ws).is_some())
}

#[derive(Default)]
struct Row {
    void: (BTreeSet<u64>, u64),
    partial: (BTreeSet<u64>, u64),
    addressees: BTreeSet<String>,
    refused: (BTreeSet<u64>, u64),
    reasons: BTreeSet<String>,
    applied: (BTreeSet<u64>, u64),
    paid: BTreeMap<String, f64>,
    delivered: BTreeMap<String, f64>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seeds: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let ticks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    println!("# actions with an absent addressee: {seeds} seeds × {ticks} ticks per world\n");
    for (scenario, worlds) in WORLDS {
        for world in *worlds {
            let mut rows: BTreeMap<String, Row> = BTreeMap::new();
            for seed in 0..seeds {
                let db = engine13::db::Db::open_in_memory().unwrap();
                let mut st = engine13::AppState::default();
                engine13::load_scenario(&mut st, &db, scenario.to_string()).unwrap();
                st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
                let strategy = (*world != "none").then(|| ScriptedStrategy::from_str(world, scenario));
                for _ in 0..ticks {
                    {
                        let ws = st.world_state.as_ref().unwrap();
                        let sc = st.current_scenario.as_ref().unwrap();
                        for a in &sc.patron_actions {
                            let r = rows.entry(a.id.clone()).or_default();
                            let (gone, all) = lost_effects(a, ws);
                            match action_availability(a, ws, sc) {
                                Ok(()) if all => { r.void.0.insert(seed); r.void.1 += 1; r.addressees.extend(gone); }
                                Ok(()) if !gone.is_empty() => { r.partial.0.insert(seed); r.partial.1 += 1; r.addressees.extend(gone); }
                                Err(reason) if reads_absent(a, ws) => {
                                    r.refused.0.insert(seed); r.refused.1 += 1;
                                    r.reasons.insert(match reason {
                                        UnavailableReason::ConditionNotMet { description } => format!("условие: {description}"),
                                        UnavailableReason::InsufficientCost { resource, .. } => format!("не хватает: {resource}"),
                                        UnavailableReason::ActionsPerTickExhausted { .. } => "лимит действий".into(),
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    match &strategy {
                        Some(s) => {
                            // Addressees are read before the turn: nobody dies mid-turn.
                            type Writes = Vec<(String, f64)>;
                            let before: BTreeMap<String, (BTreeSet<String>, Writes, Writes)> = {
                                let ws = st.world_state.as_ref().unwrap();
                                st.current_scenario.as_ref().unwrap().patron_actions.iter()
                                    .map(|a| {
                                        let live = |m: &&MetricRef| absent(m, ws).is_none();
                                        let paid = a.cost.iter().chain(a.effects.iter().filter(|(_, v)| **v < 0.0))
                                            .filter(|(m, _)| live(m)).map(|(m, c)| (m.to_string(), *c)).collect();
                                        let got = a.effects.iter().filter(|(m, v)| **v > 0.0 && live(m))
                                            .map(|(m, c)| (m.to_string(), *c)).collect();
                                        (a.id.clone(), (lost_effects(a, ws).0, paid, got))
                                    })
                                    .collect()
                            };
                            let turn = play_scripted_tick(&mut st, s);
                            for id in turn.applied {
                                let (gone, paid, got) = &before[id];
                                if gone.is_empty() { continue; }
                                let r = rows.entry(id.to_string()).or_default();
                                r.applied.0.insert(seed); r.applied.1 += 1;
                                for (m, c) in paid { *r.paid.entry(m.clone()).or_default() += c; }
                                for (m, c) in got { *r.delivered.entry(m.clone()).or_default() += c; }
                            }
                        }
                        None => {
                            let ws = st.world_state.as_mut().unwrap();
                            let sc = st.current_scenario.as_ref().unwrap();
                            engine13::engine::tick(ws, sc, &mut st.event_log, st.rng.as_mut().unwrap());
                        }
                    }
                }
            }
            println!("## {scenario} / {world}\n");
            println!("| action | addressee absent | shown available: void (runs/ticks) | partial | refused on an absent read | reason shown | bot applied (runs/times) | paid by the living | still delivered |");
            println!("|---|---|---|---|---|---|---|---|---|");
            for (id, r) in &rows {
                if r.void.1 + r.partial.1 + r.refused.1 + r.applied.1 == 0 { continue; }
                let paid: Vec<String> = r.paid.iter().map(|(m, c)| format!("{m} {c:.0}")).collect();
                let got: Vec<String> = r.delivered.iter().map(|(m, c)| format!("{m} +{c:.0}")).collect();
                println!("| {id} | {} | {}/{} | {}/{} | {}/{} | {} | {}/{} | {} | {} |",
                    r.addressees.iter().cloned().collect::<Vec<_>>().join(", "),
                    r.void.0.len(), r.void.1, r.partial.0.len(), r.partial.1,
                    r.refused.0.len(), r.refused.1, r.reasons.iter().cloned().collect::<Vec<_>>().join("; "),
                    r.applied.0.len(), r.applied.1, paid.join(", "), got.join(", "));
            }
            println!();
        }
    }
}
