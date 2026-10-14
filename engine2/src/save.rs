//! Save and load (plan §4).
//!
//! Rules:
//! - the save is the whole [`World`], JSON with `float_roundtrip`, wrapped in an envelope
//!   with a format number;
//! - `#[serde(default)]` is forbidden in anything that is saved (guard test): an
//!   incompatible change bumps [`SAVE_FORMAT`] instead of a silent default;
//! - a load continues the same run bit for bit, because randomness is keyed by the seed
//!   that travels with the world (there is no stream position to lose).

use serde::{Deserialize, Serialize};

use crate::world::World;

/// Bump on any incompatible change of [`World`] or its parts.
pub const SAVE_FORMAT: u32 = 10;

/// Written envelope: borrows the world, no copy.
#[derive(Debug, Serialize)]
struct EnvelopeRef<'a> {
    format: u32,
    world: &'a World,
}

/// Read envelope: the format first, the world as raw JSON, so that a wrong format is
/// reported as such and not as a missing field inside `world`. `deny_unknown_fields`: a
/// save with a field this build does not know is refused, not loaded with a silent gap
/// (plan §4).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: u32,
    world: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("save format {found} is not the supported format {supported}")]
    Format { found: u32, supported: u32 },
    #[error("save is not valid JSON of the current format: {0}")]
    Json(#[from] serde_json::Error),
}

/// Serialize a world. Infallible for a well-formed world; the `Result` is only serde's.
pub fn to_save(world: &World) -> Result<String, SaveError> {
    Ok(serde_json::to_string(&EnvelopeRef { format: SAVE_FORMAT, world })?)
}

/// Deserialize a save of the current format.
pub fn from_save(s: &str) -> Result<World, SaveError> {
    let env: Envelope = serde_json::from_str(s)?;
    if env.format != SAVE_FORMAT {
        return Err(SaveError::Format { found: env.format, supported: SAVE_FORMAT });
    }
    Ok(serde_json::from_value(env.world)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testworld;

    #[test]
    fn wrong_format_is_reported_as_format() {
        let err = from_save(r#"{"format":999,"world":{}}"#).unwrap_err();
        assert!(matches!(err, SaveError::Format { found: 999, .. }), "{err}");
    }

    #[test]
    fn unknown_field_anywhere_is_refused() {
        let w = testworld::world(3);
        let s = to_save(&w).unwrap();
        let extra_top = s.replacen("{\"format\":", "{\"extra\":1,\"format\":", 1);
        assert!(matches!(from_save(&extra_top).unwrap_err(), SaveError::Json(_)));
        let extra_inner = s.replacen("\"seed\":", "\"extra\":1,\"seed\":", 1);
        assert!(s.contains("\"seed\":"), "the inner probe must hit a real field");
        assert!(matches!(from_save(&extra_inner).unwrap_err(), SaveError::Json(_)));
    }

    #[test]
    fn round_trip_is_equal() {
        let w = testworld::world(3);
        let s = to_save(&w).unwrap();
        assert_eq!(from_save(&s).unwrap(), w);
    }
}
