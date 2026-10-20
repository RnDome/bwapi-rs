//! The records of `tests/property`, read from the port: what the checks take,
//! apart from the checks. `openbw-host` reads the same records from C++.

#![allow(dead_code)]

use bwapi_sdk::bwem::{Map, Node};

use crate::property::bwem::{self, Analysis};

/// What the analysis of the port gives.
pub fn analysis(map: &Map, bases_found: bool) -> Analysis {
    let xy = |x: i32, y: i32| (x, y);
    let mut cps: Vec<_> = map.areas().flat_map(|a| a.choke_points()).collect();
    cps.sort_by_key(|cp| cp.id());
    cps.dedup_by_key(|cp| cp.id());
    Analysis {
        size: (map.size().x, map.size().y),
        max_altitude: i32::from(map.max_altitude()),
        altitudes: map.mini_tiles().map(|m| i32::from(m.altitude())).collect(),
        mini_tile_areas: map.mini_tiles().map(|m| i32::from(m.area_id())).collect(),
        tile_areas: map.tiles().map(|t| i32::from(t.area_id())).collect(),
        areas: map
            .areas()
            .map(|a| bwem::Area {
                id: i32::from(a.id().get()),
                top: xy(a.top().x, a.top().y),
                mini_tiles: a.size_in_mini_tiles(),
                group: i32::from(a.group()),
                top_left: xy(a.bounds().0.x, a.bounds().0.y),
                bottom_right: xy(a.bounds().1.x, a.bounds().1.y),
            })
            .collect(),
        choke_points: cps
            .iter()
            .map(|cp| {
                let (a, b) = cp.areas();
                let node = |n| {
                    let w = cp.node(n);
                    (w.x, w.y)
                };
                bwem::ChokePoint {
                    index: cp.id().index() as i32,
                    areas: (i32::from(a.id().get()), i32::from(b.id().get())),
                    nodes: [node(Node::End1), node(Node::Middle), node(Node::End2)],
                    pseudo: cp.is_pseudo(),
                    blocked: cp.is_blocked(),
                    geometry: cp.geometry().iter().map(|w| (w.x, w.y)).collect(),
                }
            })
            .collect(),
        bases: map
            .areas()
            .flat_map(|a| a.bases())
            .map(|b| bwem::Base {
                area: i32::from(b.area().id().get()),
                location: (b.location().x, b.location().y),
                starting: b.is_starting(),
                minerals: b.minerals().map(|m| m.unit_id()).collect(),
                geysers: b.geysers().map(|g| g.unit_id()).collect(),
                blocking_minerals: b.blocking_minerals().map(|m| m.unit_id()).collect(),
            })
            .collect(),
        bases_found,
        distances: cps
            .iter()
            .flat_map(|a| cps.iter().map(move |b| a.distance_to(b.id()).unwrap_or(-1)))
            .collect(),
        // An unreachable choke point has an empty path.
        paths: cps
            .iter()
            .flat_map(|a| {
                cps.iter().map(move |b| {
                    a.path_to(b.id())
                        .map(|p| p.iter().map(|cp| cp.id().index() as i32).collect())
                        .unwrap_or_default()
                })
            })
            .collect(),
    }
}
