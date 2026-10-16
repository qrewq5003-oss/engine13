//! Worlds in parallel (E3, owner's rule after #293): a run of many worlds — the acceptance
//! over generated worlds, the sentinel's matrix, the held-out seeds — plays each world on
//! its own thread and collects the results **in the order of the items**. Every world is
//! deterministic by itself (its own seed, its own draws, no shared state; `HashMap` is
//! forbidden in the engine by a guard), so the results are byte for byte those of a run
//! one after another — tested (`tests/par.rs`). `ENGINE2_SEQUENTIAL=1` plays them one after
//! another, for a debugger; `ENGINE2_THREADS=n` caps the threads (owner's rule after #297:
//! the chain ran out of memory when the build, the link, the tests and sixteen worlds ran at
//! once).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// The number of threads a parallel run uses: one with `ENGINE2_SEQUENTIAL` set, else
/// `ENGINE2_THREADS` when it names a number of at least one, else the machine's.
pub fn threads() -> usize {
    if std::env::var_os("ENGINE2_SEQUENTIAL").is_some() {
        return 1;
    }
    if let Some(n) = std::env::var("ENGINE2_THREADS").ok().and_then(|v| v.trim().parse::<usize>().ok()).filter(|n| *n >= 1) {
        return n;
    }
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

/// `f` over every item, on up to [`threads`] threads, the results in the order of the items.
/// A panic in any item propagates when the run ends, as it would one after another.
pub fn map<I, T, F>(items: &[I], f: F) -> Vec<T>
where
    I: Sync,
    T: Send,
    F: Fn(&I) -> T + Sync,
{
    let n = items.len();
    let workers = threads().min(n);
    if workers <= 1 {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<T>>> = (0..n).map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= n {
                    break;
                }
                let r = f(&items[i]);
                *slots[i].lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
            });
        }
    });
    slots.into_iter().map(|m| m.into_inner().unwrap_or_else(|e| e.into_inner()).expect("every item ran")).collect()
}
