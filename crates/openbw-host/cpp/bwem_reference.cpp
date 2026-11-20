// BWEM-community itself, run on the game in progress: the reference the Rust port
// of BWEM is compared with. Built only when BWEM_DIR points at its sources.

#include "bwem.h"
#include "mapImpl.h"

#include <cstddef>
#include <exception>
#include <vector>

namespace {

using Sink = void (*)(void* ctx, int kind, const int* values, std::size_t len);

// Record kinds, as `openbw_host::bwem_reference` reads them.
enum Kind : int {
  kHeader = 0,          // max altitude, walk width, walk height, bases found
  kAltitudes = 1,       // every mini tile, row by row
  kMiniTileAreas = 2,   // every mini tile, row by row
  kTileAreas = 3,       // every tile, row by row
  kArea = 4,            // id, top x, top y, mini tiles, group, top left x, y, bottom right x, y
  kChokePoint = 5,      // index, area 1, area 2, end1 x, y, middle x, y, end2 x, y, pseudo, blocked
  kDistances = 6,       // index by index, -1 when unreachable
  kBase = 7,            // area, location x, y, starting, minerals, geysers, blocking minerals,
                        // then their unit ids
  kPaths = 8,           // index, then for each index: length, then the indices of GetPathTo
  kDestroyed = 9,       // unit id: the records after it are the map once that neutral is gone
  kFailed = -1,
};

void send(Sink sink, void* ctx, Kind kind, const std::vector<int>& values) {
  sink(ctx, kind, values.data(), values.size());
}

// What the comparison reads of the map, as it is now.
void dump(BWEM::detail::MapImpl& map, bool found, void* ctx, Sink sink) {
  send(sink, ctx, kHeader, {map.MaxAltitude(), map.WalkSize().x, map.WalkSize().y, found ? 1 : 0});

  std::vector<int> altitudes, areas;
  for (int y = 0; y < map.WalkSize().y; ++y) {
    for (int x = 0; x < map.WalkSize().x; ++x) {
      const auto& m = map.GetMiniTile(BWAPI::WalkPosition(x, y));
      altitudes.push_back(m.Altitude());
      areas.push_back(m.AreaId());
    }
  }
  send(sink, ctx, kAltitudes, altitudes);
  send(sink, ctx, kMiniTileAreas, areas);

  std::vector<int> tiles;
  for (int y = 0; y < map.Size().y; ++y) {
    for (int x = 0; x < map.Size().x; ++x) {
      tiles.push_back(map.GetTile(BWAPI::TilePosition(x, y)).AreaId());
    }
  }
  send(sink, ctx, kTileAreas, tiles);

  for (const auto& area : map.Areas()) {
    send(sink, ctx, kArea,
         {area.Id(), area.Top().x, area.Top().y, area.MiniTiles(), area.GroupId(), area.TopLeft().x,
          area.TopLeft().y, area.BottomRight().x, area.BottomRight().y});
  }

  const auto& cps = map.GetGraph().ChokePoints();
  for (const BWEM::ChokePoint* cp : cps) {
    const auto e1 = cp->Pos(BWEM::ChokePoint::end1);
    const auto mid = cp->Pos(BWEM::ChokePoint::middle);
    const auto e2 = cp->Pos(BWEM::ChokePoint::end2);
    send(sink, ctx, kChokePoint,
         {cp->Index(), cp->GetAreas().first->Id(), cp->GetAreas().second->Id(), e1.x, e1.y, mid.x, mid.y,
          e2.x, e2.y, cp->IsPseudo() ? 1 : 0, cp->Blocked() ? 1 : 0});
  }

  std::vector<int> distances(cps.size() * cps.size(), -1);
  for (const BWEM::ChokePoint* a : cps) {
    for (const BWEM::ChokePoint* b : cps) {
      distances[a->Index() * cps.size() + b->Index()] = a->DistanceFrom(b);
    }
  }
  send(sink, ctx, kDistances, distances);

  std::vector<const BWEM::ChokePoint*> by_index(cps.size());
  for (const BWEM::ChokePoint* cp : cps) by_index[cp->Index()] = cp;
  for (const BWEM::ChokePoint* a : by_index) {
    std::vector<int> values{a->Index()};
    for (const BWEM::ChokePoint* b : by_index) {
      const auto& path = a->GetPathTo(b);
      values.push_back(static_cast<int>(path.size()));
      for (const BWEM::ChokePoint* cp : path) values.push_back(cp->Index());
    }
    send(sink, ctx, kPaths, values);
  }

  for (const auto& area : map.Areas()) {
    for (const auto& base : area.Bases()) {
      std::vector<int> values{area.Id(),
                              base.Location().x,
                              base.Location().y,
                              base.Starting() ? 1 : 0,
                              static_cast<int>(base.Minerals().size()),
                              static_cast<int>(base.Geysers().size()),
                              static_cast<int>(base.BlockingMinerals().size())};
      for (const auto* m : base.Minerals()) values.push_back(m->Unit()->getID());
      for (const auto* g : base.Geysers()) values.push_back(g->Unit()->getID());
      for (const auto* m : base.BlockingMinerals()) values.push_back(m->Unit()->getID());
      send(sink, ctx, kBase, values);
    }
  }
}

}  // namespace

// The map, then the map after each neutral of `destroyed` (unit ids) is gone,
// in that order: `OnMineralDestroyed` for a mineral field,
// `OnStaticBuildingDestroyed` for anything else.
extern "C" void openbw_host_bwem_reference(void* ctx, Sink sink, const int* destroyed,
                                           std::size_t destroyed_len) noexcept {
  try {
    // Never destroyed: on Sparkle 1.1, where neutrals overlap without being
    // aligned, ~Neutral follows the stack past its end and dereferences null.
    auto& map = *new BWEM::detail::MapImpl();
    map.Initialize(BWAPI::BroodwarPtr);
    const bool found = map.FindBasesForStartingLocations();
    dump(map, found, ctx, sink);

    for (std::size_t i = 0; i < destroyed_len; ++i) {
      const BWAPI::Unit unit = BWAPI::Broodwar->getUnit(destroyed[i]);
      if (unit->getType().isMineralField()) {
        map.OnMineralDestroyed(unit);
      } else {
        map.OnStaticBuildingDestroyed(unit);
      }
      send(sink, ctx, kDestroyed, {destroyed[i]});
      dump(map, found, ctx, sink);
    }
  } catch (const std::exception&) {
    sink(ctx, kFailed, nullptr, 0);
  }
}
