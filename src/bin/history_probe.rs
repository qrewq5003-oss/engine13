//! History probe — acceptance of B31 points 2–4 (docs/TRIAGE.md, «B31»).
//!
//! One history, one source: the game's event log lives in `AppState`, travels with the
//! save, and is what the chronicler and the action history read. The probe plays a
//! game through the product's own command functions and checks, per scenario:
//!
//!   1. save → load returns the log and the world exactly — every f64 bit, which needs
//!      serde_json's `float_roundtrip` (without it every load moved values by one ulp) —
//!      and the chronicler's evidence window after the load equals the one before the save;
//!   2. the action history holds every occurrence of every action, newest first;
//!   3. a fresh scenario starts with an empty log and history, and an explicit load
//!      brings back that save's own log — not the session's;
//!   4. the `advance_tick` responses, concatenated over the game, equal the log, and
//!      their total size is linear in the log (the old response carried the whole log
//!      every tick);
//!   5. a save written before the log column existed (`event_log_json = '[]'`) loads.
//!
//! Every comparison is inside one process, so the metric drift between processes recorded
//! under B21 does not apply. Logs are compared as `serde_json::Value` (object keys sorted),
//! not as bytes: a `HashMap` rebuilt by deserialization iterates in a new order even in
//! the same process (class B9′), so equal logs serialize to different bytes.
//!
//! The player loop is copied from `sim.rs` without milan's reserve discipline (A29): the
//! probe needs player actions in the log, not a faithful strategy.
//!
//! Usage: cargo run --release --bin history_probe -- [ticks] [seed]

use engine13::application::scripted::ScriptedStrategy;
use engine13::application::{apply_player_action, PlayerActionInput};
use engine13::commands::{self, AppState};
use engine13::core::{Event, EventType};
use engine13::db::Db;
use rand::SeedableRng;

fn fresh(db: &Db, scenario: &str, seed: u64) -> AppState {
    let mut st = AppState::default();
    engine13::load_scenario(&mut st, db, scenario.to_string()).expect("scenario");
    st.rng = Some(rand_chacha::ChaCha8Rng::seed_from_u64(seed));
    st
}

/// Plays `ticks` half-years through `commands::advance_tick`, returning the concatenated
/// responses and their total serialized size.
fn play(st: &mut AppState, ticks: u32) -> (Vec<Event>, usize) {
    let sc = st.current_scenario.as_ref().unwrap().id.clone();
    let strat = ScriptedStrategy::from_str("balanced", &sc);
    let apt = st.current_scenario.as_ref().unwrap().actions_per_tick;
    let (mut shipped, mut bytes) = (Vec::new(), 0usize);
    for _ in 0..ticks {
        let mut applied = 0;
        for id in strat.priority_actions() {
            if applied >= apt {
                break;
            }
            let input = PlayerActionInput { action_id: id.to_string(), target_actor_id: None };
            if apply_player_action(st, &input).is_ok() {
                applied += 1;
            }
        }
        let resp = commands::advance_tick(st, None).expect("tick");
        bytes += serde_json::to_vec(&resp.events).unwrap().len();
        shipped.extend(resp.events);
    }
    (shipped, bytes)
}

fn window_ids(st: &AppState) -> Vec<String> {
    let snap = engine13::build_snapshot(
        st.world_state.as_ref().unwrap(),
        st.current_scenario.as_ref().unwrap(),
        &st.event_log,
    );
    snap.recent_important_events.iter().map(|e| format!("{}@{}", e.id, e.tick)).collect()
}

/// A world as a value: object keys sorted, and arrays of strings sorted too — sets such
/// as `fired_events` are `HashSet`s and serialize in iteration order (class B9′).
fn canon_world(st: &AppState) -> serde_json::Value {
    fn sort_sets(v: serde_json::Value) -> serde_json::Value {
        use serde_json::Value;
        match v {
            Value::Object(m) => Value::Object(m.into_iter().map(|(k, x)| (k, sort_sets(x))).collect()),
            Value::Array(a) if a.iter().all(|x| x.is_string()) => {
                let mut a = a;
                a.sort_by(|x, y| x.as_str().cmp(&y.as_str()));
                Value::Array(a)
            }
            Value::Array(a) => Value::Array(a.into_iter().map(sort_sets).collect()),
            other => other,
        }
    }
    sort_sets(serde_json::to_value(st.world_state.as_ref().unwrap()).unwrap())
}

/// The log as a value with sorted object keys — equal logs compare equal.
fn canon(events: &[Event]) -> serde_json::Value {
    serde_json::to_value(events).unwrap()
}

fn check(ok: bool, what: &str, failures: &mut u32) {
    println!("  [{}] {}", if ok { "ok" } else { "FAIL" }, what);
    if !ok {
        *failures += 1;
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ticks: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300);
    let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(42);
    let scenarios = ["rome_375", "constantinople_1430", "milan_1477"];
    let mut failures = 0u32;

    for (i, sc) in scenarios.iter().enumerate() {
        println!("=== {sc} / {ticks} ticks / seed {seed} ===");
        let db = Db::open_in_memory().unwrap();
        let mut st = fresh(&db, sc, seed);

        // 4. responses concatenate to the log; transfer is linear.
        let (shipped, bytes) = play(&mut st, ticks);
        let log_bytes = serde_json::to_vec(&st.event_log.events).unwrap().len();
        check(
            canon(&shipped) == canon(&st.event_log.events),
            &format!("advance_tick responses concatenate to the log ({} events)", st.event_log.events.len()),
            &mut failures,
        );
        println!(
            "      shipped {:.2} MB over the game, log {:.2} MB (ratio {:.2})",
            bytes as f64 / 1048576.0,
            log_bytes as f64 / 1048576.0,
            bytes as f64 / log_bytes as f64
        );
        check(bytes <= log_bytes * 11 / 10, "transfer is linear: shipped ≤ 1.1 × log", &mut failures);

        // 2. action history keeps every occurrence.
        let actions = st
            .event_log
            .events
            .iter()
            .filter(|e| matches!(e.event_type, EventType::PlayerAction))
            .count();
        let history = commands::get_action_history(&st, usize::MAX);
        let distinct: std::collections::BTreeSet<&str> = history.iter().map(|h| h.action_id.as_str()).collect();
        check(
            actions > 0 && history.len() == actions && distinct.len() < history.len(),
            &format!("history holds every occurrence: {} entries, {} distinct actions", history.len(), distinct.len()),
            &mut failures,
        );
        check(
            history.windows(2).all(|w| w[0].tick >= w[1].tick),
            "history is newest first",
            &mut failures,
        );

        // 1. save → load round trip.
        let saved_log = canon(&st.event_log.events);
        let saved_window = window_ids(&st);
        let saved_world = canon_world(&st);
        let resp = commands::save_game(&mut st, &db, Some("probe".to_string())).expect("save");
        let save_id = resp.save_id.unwrap();
        let save_bytes = db.get_save_by_id(&save_id).unwrap().unwrap().event_log_json.len();
        println!("      event log in the save: {:.2} MB", save_bytes as f64 / 1048576.0);

        // 3. a fresh scenario in the same session starts empty…
        let other = scenarios[(i + 1) % scenarios.len()];
        engine13::load_scenario(&mut st, &db, other.to_string()).unwrap();
        check(
            st.event_log.events.is_empty() && commands::get_action_history(&st, 5).is_empty(),
            &format!("fresh {other}: empty log and history"),
            &mut failures,
        );
        play(&mut st, 5);

        // …and an explicit load brings back the save's own log.
        commands::load_game(&mut st, &db, save_id.clone()).expect("load");
        check(
            canon(&st.event_log.events) == saved_log,
            "load restores the save's log exactly (not the session's)",
            &mut failures,
        );
        check(
            canon_world(&st) == saved_world,
            "load restores the world exactly (every f64 bit)",
            &mut failures,
        );
        check(window_ids(&st) == saved_window && !saved_window.is_empty(),
            &format!("chronicler's window after load equals the one before save ({} events)", saved_window.len()),
            &mut failures,
        );
        check(
            commands::get_action_history(&st, usize::MAX).len() == actions,
            "action history survives the load",
            &mut failures,
        );

        // 5. a save from before the column loads, with an empty log.
        let mut old = db.get_save_by_id(&save_id).unwrap().unwrap();
        old.id = format!("{}__legacy", sc);
        old.event_log_json = "[]".to_string();
        db.insert_save(&old).unwrap();
        let loaded = commands::load_game(&mut st, &db, old.id.clone());
        check(
            loaded.is_ok() && st.event_log.events.is_empty(),
            "a pre-column save (`[]`) loads with an empty log",
            &mut failures,
        );
    }

    println!();
    if failures == 0 {
        println!("ALL CHECKS PASSED");
    } else {
        println!("{failures} CHECK(S) FAILED");
        std::process::exit(1);
    }
}
