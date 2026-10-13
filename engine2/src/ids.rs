//! Typed numeric ids (plan §1).
//!
//! Numbers are assigned when a scenario is loaded, in the sorted order of its string ids;
//! the strings themselves go to a `Names` table owned by the presentation layer. Inside the
//! engine there is no operation on a name: that is the "engine knows no names" guard held by
//! the type system, not only by a test.

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u32);

        impl $name {
            /// The raw index, for tables indexed by id.
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

id_type!(
    /// A polity: ruler-level actor that owns provinces.
    ActorId
);
id_type!(
    /// A unit of land: population, capacity, income, culture, religion, fortification, port.
    ProvinceId
);
id_type!(
    /// A house: a family inside a polity, possibly holding several seats.
    HouseId
);
id_type!(
    /// A person: ruler, heir, house member.
    PersonId
);
id_type!(
    /// A unit: an army with strength, quality, commander and position on the province graph.
    UnitId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_order_by_number_and_round_trip_as_plain_numbers() {
        let a = ActorId(3);
        let b = ActorId(10);
        assert!(a < b);
        assert_eq!(serde_json::to_string(&b).unwrap(), "10");
        assert_eq!(serde_json::from_str::<ActorId>("10").unwrap(), b);
        assert_eq!(b.index(), 10);
    }
}
