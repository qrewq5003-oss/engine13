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
pub const SAVE_FORMAT: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct Envelope {
    format: u32,
    world: World,
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
    Ok(serde_json::to_string(&Envelope { format: SAVE_FORMAT, world: world.clone() })?)
}

/// Deserialize a save of the current format.
pub fn from_save(s: &str) -> Result<World, SaveError> {
    // Read the format first so that a wrong format is reported as such, not as a missing
    // field inside `world`.
    #[derive(Deserialize)]
    struct Head {
        format: u32,
    }
    let head: Head = serde_json::from_str(s)?;
    if head.format != SAVE_FORMAT {
        return Err(SaveError::Format { found: head.format, supported: SAVE_FORMAT });
    }
    let env: Envelope = serde_json::from_str(s)?;
    Ok(env.world)
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
    fn round_trip_is_equal() {
        let w = World::from_facts(&testworld::five_states(), 3).unwrap();
        let s = to_save(&w).unwrap();
        assert_eq!(from_save(&s).unwrap(), w);
    }
}
