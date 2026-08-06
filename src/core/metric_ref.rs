use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{self, Visitor};
use std::fmt;
use crate::core::WorldState;

/// A metric key that could not be parsed.
///
/// Scenario definitions are static content, so this is always a content bug. It
/// surfaces at load — from `Deserialize` for TOML-sourced content, from the
/// checked constructors for the scenarios that build their content as Rust
/// literals (`rome_375`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricKeyError(String);

impl fmt::Display for MetricKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MetricKeyError {}

/// A bare metric name — no scope, no prefix, no dot.
///
/// This is the atom every scoped key is built from, and the type that makes the
/// project's recurring bug unrepresentable: `"rome.cohesion"` and
/// `"actor:rome.cohesion"` are both rejected here, so a `Global` ref can no
/// longer be constructed at a key that only looks actor-relative. Every metric
/// scoping defect in this project's history (#19, #20, narrative `key_metrics`)
/// produced exactly such a key.
///
/// It is also the type of the metric fields whose scope comes from the code
/// around them rather than the string (interaction rules, rank bonuses, tags):
/// there, a `global:` prefix was never supported and silently read `0.0`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MetricName(String);

impl MetricName {
    /// Checked constructor — the only way to build one.
    pub fn new(s: &str) -> Result<Self, MetricKeyError> {
        if s.is_empty() {
            return Err(MetricKeyError("metric name must not be empty".to_string()));
        }
        if s.contains(':') {
            return Err(MetricKeyError(format!(
                "metric name '{}' contains ':' — a scope prefix does not belong here",
                s
            )));
        }
        if s.contains('.') {
            return Err(MetricKeyError(format!(
                "metric name '{}' contains '.' — an actor-relative key must be written \
                 'actor:{}' , not as a dotted bare name (a dotted bare name resolves to a \
                 global key that no subsystem reads or writes)",
                s, s
            )));
        }
        Ok(MetricName(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MetricName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for MetricName {
    type Err = MetricKeyError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MetricName::new(s)
    }
}

impl Serialize for MetricName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for MetricName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = MetricName;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a bare metric name (no ':' or '.')")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<MetricName, E> {
                MetricName::new(s).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_str(V)
    }
}

/// An actor id, as it appears inside an `actor:` key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActorId(String);

impl ActorId {
    pub fn new(s: &str) -> Result<Self, MetricKeyError> {
        if s.is_empty() {
            return Err(MetricKeyError("actor id must not be empty".to_string()));
        }
        if s.contains(':') || s.contains('.') {
            return Err(MetricKeyError(format!(
                "actor id '{}' must not contain ':' or '.'",
                s
            )));
        }
        Ok(ActorId(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Reference to a metric in the world state — an *absolute* address.
///
/// The string form is the canonical one and the only one accepted:
/// - `actor:id.metric`
/// - `family:key`
/// - `global:key`, or a bare `key` (no prefix, no dot) as a shorthand for it
///
/// Anything else is a load error. In particular a *dotted, unprefixed* key
/// (`rome.cohesion`) no longer degrades into a `Global` ref at a phantom key —
/// it is rejected outright.
///
/// A ref whose scope is not carried by the string itself is not this type:
/// see [`RelativeMetricRef`] for keys resolved against a runtime target actor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MetricRef {
    /// Actor-specific metric: "actor:id.metric"
    Actor { actor_id: ActorId, metric: MetricName },
    /// Family metric: "family:key"
    Family { key: MetricName },
    /// Global metric: "global:key" or plain key
    Global { key: MetricName },
}

impl MetricRef {
    /// Parse a canonical metric key.
    ///
    /// Fallible by design: the old "invalid format, treat as global" fallback is
    /// what let a malformed key live on as a silent `0.0` for the entire history
    /// of the project.
    pub fn parse(s: &str) -> Result<Self, MetricKeyError> {
        if let Some(key) = s.strip_prefix("family:") {
            Ok(MetricRef::Family { key: MetricName::new(key)? })
        } else if let Some(key) = s.strip_prefix("global:") {
            Ok(MetricRef::Global { key: MetricName::new(key)? })
        } else if let Some(rest) = s.strip_prefix("actor:") {
            let Some((actor_id, metric)) = rest.split_once('.') else {
                return Err(MetricKeyError(format!(
                    "metric key '{}': an 'actor:' key must be 'actor:<id>.<metric>'",
                    s
                )));
            };
            Ok(MetricRef::Actor {
                actor_id: ActorId::new(actor_id)?,
                metric: MetricName::new(metric)?,
            })
        } else {
            // Plain string → Global. `MetricName` rejects a dotted key here, which is
            // exactly the phantom-global shape.
            Ok(MetricRef::Global { key: MetricName::new(s)? })
        }
    }

    /// A metric key from *static scenario content written as a Rust literal*.
    ///
    /// Panics on a malformed key. That is the point: `rome_375` builds its
    /// `auto_deltas`, milestones, victory condition and indicators as Rust
    /// literals rather than TOML, so `Deserialize` never runs on them — without
    /// this, the one scenario that held 9 of 9 broken auto_delta blocks in #19
    /// would be the one scenario the type system did not check. A bad key here is
    /// a content bug and must fail at load, loudly.
    pub fn literal(s: &str) -> Self {
        MetricRef::parse(s)
            .unwrap_or_else(|e| panic!("malformed metric key in scenario content: {}", e))
    }

    /// Checked constructor for an actor-scoped ref.
    pub fn actor(actor_id: &str, metric: &str) -> Result<Self, MetricKeyError> {
        Ok(MetricRef::Actor {
            actor_id: ActorId::new(actor_id)?,
            metric: MetricName::new(metric)?,
        })
    }

    /// Checked constructor for a global ref.
    pub fn global(key: &str) -> Result<Self, MetricKeyError> {
        Ok(MetricRef::Global { key: MetricName::new(key)? })
    }

    /// Checked constructor for a family ref.
    pub fn family(key: &str) -> Result<Self, MetricKeyError> {
        Ok(MetricRef::Family { key: MetricName::new(key)? })
    }

    /// Get the metric value from world_state, defaulting a missing actor or a
    /// missing metric to `0.0`.
    ///
    /// Use [`try_get`](Self::try_get) where the *absence* of an actor must be
    /// distinguishable from a zero value — a `less`-than condition on a dead
    /// actor is satisfied by the `0.0` default and would fire forever.
    pub fn get(&self, world_state: &WorldState) -> f64 {
        self.try_get(world_state).unwrap_or(0.0)
    }

    /// Get the metric value, or `None` when the addressed container is absent.
    ///
    /// `None` means "there is nothing to ask": the actor is not in the world
    /// (never spawned, or removed on collapse), or the scenario has no family
    /// state. A present container with an unset metric still reads `0.0`, which
    /// preserves the historical behaviour for every metric that simply has not
    /// been written yet.
    pub fn try_get(&self, world_state: &WorldState) -> Option<f64> {
        match self {
            MetricRef::Actor { actor_id, metric } => world_state
                .actors
                .get(actor_id.as_str())
                .map(|a| a.metrics.get(metric.as_str()).copied().unwrap_or(0.0)),
            MetricRef::Family { key } => world_state
                .family_state
                .as_ref()
                .map(|fs| fs.metrics.get(Self::family_key(key)).copied().unwrap_or(0.0)),
            MetricRef::Global { key } => {
                Some(world_state.global_metrics.get(key.as_str()).copied().unwrap_or(0.0))
            }
        }
    }

    /// Family metrics are stored unprefixed; content may write either
    /// `family:influence` or `family:family_influence`.
    ///
    /// By the time a `MetricName` exists the `family:` scope prefix is already
    /// gone (`parse` strips it), so only `family_` is left to remove. Delegates
    /// to [`canonical_family_key`] so the runtime read path and the seeding path
    /// cannot drift apart again.
    fn family_key(key: &MetricName) -> &str {
        canonical_family_key(key.as_str())
    }

    /// Apply a delta to the metric in world_state
    pub fn apply(&self, world_state: &mut WorldState, delta: f64) {
        match self {
            MetricRef::Actor { actor_id, metric } => {
                if let Some(actor) = world_state.actors.get_mut(actor_id.as_str()) {
                    let metric_name = metric.as_str();
                    let current = actor.metrics.get(metric_name).copied().unwrap_or(0.0);
                    let new_value = match metric_name {
                        "treasury" => current + delta, // can go negative (debts)
                        "economic_output" | "military_size" | "population" => (current + delta).max(0.0),
                        _ => (current + delta).clamp(0.0, 100.0), // cohesion, legitimacy, etc.
                    };
                    actor.metrics.insert(metric_name.to_string(), new_value);
                }
            }
            MetricRef::Family { key } => {
                let metric_key = Self::family_key(key).to_string();
                if let Some(ref mut fs) = world_state.family_state {
                    let val = fs.metrics.entry(metric_key).or_insert(0.0);
                    let new_value = (*val + delta).clamp(0.0, 100.0);
                    *val = if new_value == 0.0 { 0.0 } else { new_value };
                }
            }
            MetricRef::Global { key } => {
                let val = world_state
                    .global_metrics
                    .entry(key.as_str().to_string())
                    .or_insert(0.0);
                *val = (*val + delta).clamp(0.0, 100.0);
            }
        }
    }
}

/// Canonical runtime form of a family-metric key, from a *raw content* key.
///
/// Content writes family metrics under any of `family:family_influence`,
/// `family:influence` or `family_influence`; runtime stores them unprefixed
/// (`influence`), which is what [`MetricRef::Family`] reads. This is the
/// superset of [`MetricRef::family_key`]: it takes the key before `parse` has
/// removed the `family:` scope, so it strips the scope prefix first and the
/// `family_` name prefix second.
///
/// Seeding `family_state.metrics` with anything but this form is the defect
/// behind §5.H — `get` misses the raw key and reads `0.0`, and `apply` then
/// opens a *second*, canonical entry beside the stale one.
pub fn canonical_family_key(key: &str) -> &str {
    let key = key.strip_prefix("family:").unwrap_or(key);
    key.strip_prefix("family_").unwrap_or(key)
}

/// Normalize a whole raw `initial_family_metrics` map for seeding
/// `FamilyState::metrics`.
///
/// Every path that seeds family state must go through here — the simulator's
/// run modes and the application's fresh-scenario start alike. The two diverging
/// key spaces *were* the defect, so the cure is one function with several
/// callers, not a hand-written `strip_prefix` pair per site.
pub fn normalize_family_metrics(
    raw: &std::collections::HashMap<String, f64>,
) -> std::collections::HashMap<String, f64> {
    raw.iter()
        .map(|(key, value)| (canonical_family_key(key).to_string(), *value))
        .collect()
}

impl fmt::Display for MetricRef {
    /// The canonical key. This is also the serialized form — see `Serialize`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricRef::Actor { actor_id, metric } => write!(f, "actor:{}.{}", actor_id, metric),
            MetricRef::Family { key } => write!(f, "family:{}", key),
            MetricRef::Global { key } => write!(f, "global:{}", key),
        }
    }
}

impl std::str::FromStr for MetricRef {
    type Err = MetricKeyError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MetricRef::parse(s)
    }
}

/// Serialized as the canonical *string*, never as a tagged enum.
///
/// This is load-bearing, not cosmetic: `WorldState` embeds `MetricDisplay` and
/// `GenerationMechanics` (world.rs), so these keys travel inside the JSON save
/// file and inside the payload the frontend reads, where TypeScript parses them
/// as strings (`GlobalMetricsPanel.tsx` strips `global:`). A derived enum
/// representation would break both.
impl Serialize for MetricRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for MetricRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = MetricRef;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a metric key: 'actor:<id>.<metric>', 'family:<key>', 'global:<key>' or a bare key")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<MetricRef, E> {
                MetricRef::parse(s).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_str(V)
    }
}

/// A metric key written *relative to a target actor that only exists at runtime*.
///
/// Random events pick their target with an RNG draw during the tick
/// (`foreground_ids.choose(rng)`), so a `self.`-relative key in their conditions
/// and effects cannot be resolved when the scenario is read — it is a template,
/// not an address. This is the one place the old `parse_scoped` heuristic
/// survives, and the type now says so: without a target it is not a [`MetricRef`].
///
/// Everything else that used to be "actor-relative" (`auto_deltas`, milestone and
/// rank conditions) carries its actor in a *sibling field* of the same struct and
/// is therefore resolved once, at load.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RelativeMetricRef {
    /// `self.metric` — the metric on whichever actor the event targets.
    SelfRelative(MetricName),
    /// A key that carries its own scope and ignores the target.
    Absolute(MetricRef),
}

impl RelativeMetricRef {
    /// A key from static scenario content written as a Rust literal — the common
    /// event pool (`events/common.rs`) is entirely literals. Panics on a bad key.
    pub fn literal(s: &str) -> Self {
        RelativeMetricRef::parse(s)
            .unwrap_or_else(|e| panic!("malformed metric key in scenario content: {}", e))
    }

    pub fn parse(s: &str) -> Result<Self, MetricKeyError> {
        match s.strip_prefix("self.") {
            Some(metric) => Ok(RelativeMetricRef::SelfRelative(MetricName::new(metric)?)),
            None => Ok(RelativeMetricRef::Absolute(MetricRef::parse(s)?)),
        }
    }

    /// Bind this key to the actor the event is firing against.
    pub fn resolve(&self, target: &str) -> Result<MetricRef, MetricKeyError> {
        match self {
            RelativeMetricRef::SelfRelative(metric) => Ok(MetricRef::Actor {
                actor_id: ActorId::new(target)?,
                metric: metric.clone(),
            }),
            RelativeMetricRef::Absolute(r) => Ok(r.clone()),
        }
    }
}

impl fmt::Display for RelativeMetricRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelativeMetricRef::SelfRelative(metric) => write!(f, "self.{}", metric),
            RelativeMetricRef::Absolute(r) => write!(f, "{}", r),
        }
    }
}

impl Serialize for RelativeMetricRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for RelativeMetricRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = RelativeMetricRef;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a metric key, optionally 'self.<metric>'")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<RelativeMetricRef, E> {
                RelativeMetricRef::parse(s).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_str(V)
    }
}

/// Resolve a metric key written relative to a *load-time* actor context.
///
/// This is the load-time half of the old `parse_scoped`, and it exists only to be
/// called from the container-level `Deserialize` impls of the structs that pair a
/// metric string with a sibling `actor_id` (`AutoDelta`, `EventConditionType`).
/// Nothing in the tick loop calls it — by the time the engine runs, every one of
/// these fields is already a `MetricRef`.
///
/// - explicit `global:` / `family:` / `actor:` prefix → honoured as written, even
///   when `actor_id` is set (an actor-scoped auto_delta may still gate on a global)
/// - bare `metric` → that metric on `actor_id`; global when `actor_id` is `None`
pub fn resolve_at_load(s: &str, actor_id: Option<&str>) -> Result<MetricRef, MetricKeyError> {
    let Some(aid) = actor_id else {
        return MetricRef::parse(s);
    };
    if s.contains(':') {
        return MetricRef::parse(s);
    }
    MetricRef::actor(aid, s)
}
