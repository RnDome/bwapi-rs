//! What the tests on real maps do to a map after its analysis.

#![allow(dead_code)]

use bwapi_sdk::bwem::{Map, NeutralKind};

/// The minerals destroyed after the analysis, besides the blocking neutrals.
const OTHER_MINERALS: usize = 8;

/// The unit ids of the neutrals destroyed one by one after the analysis: every
/// blocking one, static buildings first, which opens choke points and changes
/// areas of tiles, then a few other minerals, which leave their bases.
/// `openbw-host`'s `bwem_parity` destroys the same.
pub fn neutrals_to_destroy(map: &Map) -> Vec<i32> {
    let of_kind = |kind| map.neutrals().filter(move |n| n.kind() == kind);
    let mut ids: Vec<i32> = of_kind(NeutralKind::StaticBuilding)
        .chain(of_kind(NeutralKind::Mineral))
        .filter(|n| n.is_blocking())
        .map(|n| n.unit_id())
        .collect();
    ids.extend(
        of_kind(NeutralKind::Mineral)
            .filter(|n| !n.is_blocking())
            .take(OTHER_MINERALS)
            .map(|n| n.unit_id()),
    );
    ids
}
