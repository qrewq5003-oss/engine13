//! Static list of the readers of named metrics in a scenario — shared by the A46 probes
//! (`a46_readers_probe`, `a46_eco_probe`), so the list is built once, not copied.
//!
//! Threshold readers carry the occupancy keys (`core::census`) they answer to; UI readers
//! carry their band bounds.

#![allow(dead_code)] // each probe uses part of the list

use engine13::core::{ActionCondition, EventConditionType, MetricRef, RelativeMetricRef, Scenario};

pub fn name_of(m: &MetricRef) -> String {
    match m {
        MetricRef::Actor { metric, .. } => metric.as_str().to_string(),
        MetricRef::Family { key } => key.as_str().trim_start_matches("family_").to_string(),
        MetricRef::Global { key } => key.as_str().to_string(),
    }
}

pub fn rel_name(m: &RelativeMetricRef) -> String {
    match m {
        RelativeMetricRef::SelfRelative(n) => n.as_str().to_string(),
        RelativeMetricRef::Absolute(r) => name_of(r),
    }
}

/// A threshold reader: the occupancy keys it answers to.
pub struct Reader { pub metric: String, pub kind: &'static str, pub id: String, pub ctx_prefix: String, pub ctx_suffix: String }

/// A UI reader: the metric and its bands (ascending lower bounds).
pub struct Ui { pub metric: String, pub kind: &'static str, pub id: String, pub key: MetricRef, pub bounds: Vec<f64> }

pub fn readers(sc: &Scenario, metrics: &[&str]) -> (Vec<Reader>, Vec<Ui>) {
    let mut r = Vec::new();
    let mut push = |metric: String, kind: &'static str, id: String, p: String, s: String| {
        if metrics.contains(&metric.as_str()) { r.push(Reader { metric, kind, id, ctx_prefix: p, ctx_suffix: s }); }
    };
    for d in &sc.dependencies {
        push(d.from.as_str().to_string(), "dependency", format!("{} ({:?} {:?})", d.id, d.mode, d.threshold), format!("dependency {}", d.id), String::new());
    }
    for (i, ad) in sc.auto_deltas.iter().enumerate() {
        for c in &ad.conditions {
            push(name_of(&c.metric), "auto-delta condition", format!("[{i}] {} if {} {:?} {}", ad.metric, c.metric, c.operator, c.value),
                format!("auto_delta[{i}] {} | if {}", ad.metric, c.metric), String::new());
        }
        for rc in &ad.ratio_conditions {
            for m in [&rc.metric_a, &rc.metric_b] {
                push(name_of(m), "auto-delta ratio", format!("[{i}] {} if {} / {} {:?} {}", ad.metric, rc.metric_a, rc.metric_b, rc.operator, rc.ratio),
                    format!("auto_delta[{i}] {} | ratio {} / {}", ad.metric, rc.metric_a, rc.metric_b), String::new());
            }
        }
    }
    for a in &sc.patron_actions {
        if let ActionCondition::Metric { metric, operator, value } = &a.available_if {
            push(name_of(metric), "action available_if", format!("{} {:?} {}", a.id, operator, value), format!("action {} | available_if {}", a.id, metric), String::new());
        }
        for m in a.cost.keys() {
            push(name_of(m), "action cost", format!("{} cost {}", a.id, m), format!("action {} | cost {}", a.id, m), String::new());
        }
    }
    for m in &sc.milestone_events {
        if let EventConditionType::Metric { metric, operator, value, .. } = &m.condition.condition_type {
            push(name_of(metric), "milestone", format!("{} {:?} {}", m.id, operator, value), format!("milestone {}", m.id), String::new());
        }
    }
    for rc in &sc.rank_conditions {
        if let EventConditionType::Metric { metric, operator, value, .. } = &rc.condition.condition_type {
            push(name_of(metric), "rank condition", format!("{} {:?} {}", rc.region_id, operator, value), format!("rank_condition {}", rc.region_id), String::new());
        }
    }
    let mut events = engine13::events::common_events();
    events.extend(sc.random_events.iter().cloned());
    for e in &events {
        for c in &e.conditions {
            push(rel_name(&c.metric), "event gate", format!("{} if {} {:?} {}", e.id, c.metric, c.operator, c.value), format!("event {} @", e.id), format!("| if {}", c.metric));
        }
    }
    if let Some(v) = &sc.victory_condition {
        push(name_of(&v.metric), "victory", format!("{} >= {}", v.metric, v.threshold), format!("victory | {}", v.metric), String::new());
    }
    let mut ui = Vec::new();
    for i in &sc.status_indicators {
        ui.push(Ui { metric: name_of(&i.metric), kind: "status indicator", id: i.label.clone(), key: i.metric.clone(), bounds: i.thresholds.iter().map(|t| t.0).collect() });
    }
    for k in &sc.narrative_config.key_metrics {
        ui.push(Ui { metric: name_of(&k.metric), kind: "key metric (chronicler)", id: k.label.clone(), key: k.metric.clone(), bounds: k.bands.iter().map(|b| b.0).collect() });
    }
    for d in &sc.global_metrics_display {
        let mut b = vec![f64::MIN];
        b.extend(d.thresholds.iter().map(|t| t.below));
        ui.push(Ui { metric: name_of(&d.metric), kind: "global panel", id: d.label.clone(), key: d.metric.clone(), bounds: b });
    }
    ui.retain(|u| metrics.contains(&u.metric.as_str()));
    (r, ui)
}

