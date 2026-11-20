// BWEB v1.17 itself, on BWEM-community, run on the game in progress: the
// reference the Rust port of BWEB is compared with. Built only when BWEM_DIR and
// BWEB_DIR point at their sources.

#include <cfloat>

#include "BWEB.h"
#include "mapImpl.h"

#include <chrono>
#include <cstddef>
#include <exception>
#include <vector>

namespace {

using Sink = void (*)(void* ctx, int kind, const double* values, std::size_t len);

// Record kinds, as `openbw_host::bweb_reference` reads them.
enum Kind : int {
  kHeader = 0,           // map width, height
  kWalkable = 1,         // per walk type: every tile, row by row
  kUsed = 2,             // every tile: the type id, -1 for none
  kReserved = 3,         // every tile
  kPlaceable = 4,        // per placed type: every tile
  kOccupied = 5,         // per placed type: every tile, units or creep near the building;
                         // none for a type that is not a resource depot
  kGroundDistances = 6,  // point by point
  kClosestChokeTile = 7, // choke point index, then x, y per point
  kStation = 8,          // location x, y, main, natural, choke index or -1, centroid x, y,
                         // small x, y, medium x, y, pocket x, y, defense angle, then defense x, y
  kStarting = 9,         // main location x, y, natural location x, y; -1 for none
  kPaths = 10,           // per point: for each point the length, then the indices of GetPath
  kSearch = 11,          // per search: reachable, distance, then the tiles x, y
  kBlock = 12,           // tile x, y, width, height, center x, y, proxy, defensive, type, then
                         // for small, medium, large, wide: the count, then the tiles x, y
  kWall = 13,            // the wall of the race's preset, if found: defense angle, choke
                         // point index, area id, then for small, medium, large, openings:
                         // the count, then the tiles x, y; then the count of defense rows,
                         // and per row: the row, the count, the tiles x, y
  kTime = 14,            // seconds of: BWEM, BWEB, walkable, placeable, occupied,
                         // choke point queries, searches
  kSnapshot = 15,        // the stage, 0 after onStart, 1 after the stations, 2 after the
                         // wall, 3 after the blocks; then every tile, row by row: the type
                         // id used or -1; then every tile: reserved
  kFailed = -1,
};

// The seconds since the last lap, for `kTime`.
class Laps {
 public:
  double lap() {
    const auto now = std::chrono::steady_clock::now();
    const double seconds = std::chrono::duration<double>(now - last_).count();
    last_ = now;
    return seconds;
  }

 private:
  std::chrono::steady_clock::time_point last_ = std::chrono::steady_clock::now();
};

void send(Sink sink, void* ctx, Kind kind, const std::vector<double>& values) {
  sink(ctx, kind, values.data(), values.size());
}

// Ground units or creep around a building at `t`: `canBuildHere` checks them,
// the port does not.
bool occupied(BWAPI::TilePosition t, BWAPI::UnitType type) {
  const BWAPI::TilePosition rb = t + type.tileSize();
  if (!BWAPI::Broodwar->getUnitsInRectangle(BWAPI::Position(t) - BWAPI::Position(64, 64),
                                            BWAPI::Position(rb) + BWAPI::Position(64, 64))
           .empty())
    return true;
  for (int x = t.x; x < rb.x; ++x)
    for (int y = t.y; y < rb.y; ++y)
      if (BWAPI::Broodwar->hasCreep(x, y)) return true;
  return false;
}

}  // namespace

extern "C" void openbw_host_bweb_reference(void* ctx, Sink sink, const int* walk_types, std::size_t walk_len,
                                           const int* place_types, std::size_t place_len, const int* points,
                                           std::size_t point_len, const int* path_types, std::size_t path_type_len,
                                           const int* path_tiles, std::size_t tile_len,
                                           int first_mode) noexcept {
  try {
    Laps laps;
    auto& bwem = static_cast<BWEM::detail::MapImpl&>(BWEM::Map::Instance());
    bwem.Initialize(BWAPI::BroodwarPtr);
    bwem.FindBasesForStartingLocations();
    const double bwem_seconds = laps.lap();
    const int width = BWAPI::Broodwar->mapWidth();
    const int height = BWAPI::Broodwar->mapHeight();
    // The used and reserved tiles at a stage of the analysis.
    const auto snapshot = [&](int stage) {
      std::vector<double> values{double(stage)};
      for (int y = 0; y < height; ++y)
        for (int x = 0; x < width; ++x) {
          const auto type = BWEB::Map::isUsed(BWAPI::TilePosition(x, y));
          values.push_back(type == BWAPI::UnitTypes::None ? -1.0 : double(type.getID()));
        }
      for (int y = 0; y < height; ++y)
        for (int x = 0; x < width; ++x) values.push_back(BWEB::Map::isReserved(BWAPI::TilePosition(x, y)));
      send(sink, ctx, kSnapshot, values);
    };
    BWEB::Map::onStart();
    snapshot(0);
    BWEB::Stations::findStations();
    snapshot(1);
    // The wall of the race at the starting natural, then the blocks, as bots
    // on BWEB do: blocks keep off the wall.
    // The race walls read the natural, its base and its choke point without a
    // check: where one is missing, there is no wall, as in the port.
    const auto race = BWAPI::Broodwar->self()->getRace();
    const auto wall_natural = BWEB::Stations::getStartingNatural();
    BWEB::Wall* wall = nullptr;
    if (wall_natural && wall_natural->getBase() && wall_natural->getChokepoint()) {
      if (race == BWAPI::Races::Zerg) wall = BWEB::Walls::createZergWall();
      if (race == BWAPI::Races::Terran) wall = BWEB::Walls::createTerranWall();
      if (race == BWAPI::Races::Protoss) wall = BWEB::Walls::createProtossWall();
    }
    snapshot(2);
    BWEB::Blocks::findBlocks();
    snapshot(3);
    const double bweb_seconds = laps.lap();

    send(sink, ctx, kHeader, {double(width), double(height)});

    const auto every_tile = [&](auto f) {
      std::vector<double> values;
      for (int y = 0; y < height; ++y)
        for (int x = 0; x < width; ++x) values.push_back(f(BWAPI::TilePosition(x, y)));
      return values;
    };

    for (std::size_t i = 0; i < walk_len; ++i) {
      const BWAPI::UnitType type(walk_types[i]);
      send(sink, ctx, kWalkable, every_tile([&](BWAPI::TilePosition t) { return BWEB::Map::isWalkable(t, type); }));
    }
    send(sink, ctx, kUsed, every_tile([](BWAPI::TilePosition t) {
           const auto type = BWEB::Map::isUsed(t);
           return type == BWAPI::UnitTypes::None ? -1.0 : double(type.getID());
         }));
    send(sink, ctx, kReserved, every_tile([](BWAPI::TilePosition t) { return BWEB::Map::isReserved(t); }));
    const double walkable_seconds = laps.lap();
    double placeable_seconds = 0.0, occupied_seconds = 0.0;
    for (std::size_t i = 0; i < place_len; ++i) {
      const BWAPI::UnitType type(place_types[i]);
      send(sink, ctx, kPlaceable, every_tile([&](BWAPI::TilePosition t) { return BWEB::Map::isPlaceable(type, t); }));
      placeable_seconds += laps.lap();
      if (type.isResourceDepot())
        send(sink, ctx, kOccupied, every_tile([&](BWAPI::TilePosition t) { return occupied(t, type); }));
      else
        send(sink, ctx, kOccupied, {});
      occupied_seconds += laps.lap();
    }

    std::vector<BWAPI::Position> positions;
    for (std::size_t i = 0; i + 1 < point_len; i += 2) positions.emplace_back(points[i], points[i + 1]);
    std::vector<double> distances;
    for (const auto& a : positions)
      for (const auto& b : positions) distances.push_back(BWEB::Map::getGroundDistance(a, b));
    send(sink, ctx, kGroundDistances, distances);

    for (const auto& a : positions) {
      std::vector<double> values;
      for (const auto& b : positions) {
        const auto& path = bwem.GetPath(a, b);
        values.push_back(double(path.size()));
        for (const BWEM::ChokePoint* cp : path) values.push_back(cp->Index());
      }
      send(sink, ctx, kPaths, values);
    }

    for (const BWEM::ChokePoint* cp : bwem.GetGraph().ChokePoints()) {
      std::vector<double> values{double(cp->Index())};
      for (const auto& p : positions) {
        const auto tile = BWEB::Map::getClosestChokeTile(cp, p);
        values.push_back(tile.x);
        values.push_back(tile.y);
      }
      send(sink, ctx, kClosestChokeTile, values);
    }

    for (const auto& station : BWEB::Stations::getStations()) {
      const auto* base = station.getBase();
      const auto* choke = station.getChokepoint();
      std::vector<double> values{double(base->Location().x),
                                 double(base->Location().y),
                                 double(station.isMain()),
                                 double(station.isNatural()),
                                 choke ? double(choke->Index()) : -1.0,
                                 double(station.getResourceCentroid().x),
                                 double(station.getResourceCentroid().y),
                                 double(station.getSmallPosition().x),
                                 double(station.getSmallPosition().y),
                                 double(station.getMediumPosition().x),
                                 double(station.getMediumPosition().y),
                                 double(station.getPocketDefense().x),
                                 double(station.getPocketDefense().y),
                                 station.getDefenseAngle()};
      for (const auto& t : station.getDefenses()) {
        values.push_back(t.x);
        values.push_back(t.y);
      }
      send(sink, ctx, kStation, values);
    }

    if (wall) {
      std::vector<double> values{wall->getDefenseAngle(),
                                 wall->getChokePoint() ? double(wall->getChokePoint()->Index()) : -1.0,
                                 wall->getArea() ? double(wall->getArea()->Id()) : -1.0};
      for (const auto* tiles :
           {&wall->getSmallTiles(), &wall->getMediumTiles(), &wall->getLargeTiles(), &wall->getOpenings()}) {
        values.push_back(double(tiles->size()));
        for (const auto& t : *tiles) {
          values.push_back(t.x);
          values.push_back(t.y);
        }
      }
      const auto& rows = wall->getAllDefenses();
      values.push_back(double(rows.size()));
      for (const auto& [row, tiles] : rows) {
        values.push_back(row);
        values.push_back(double(tiles.size()));
        for (const auto& t : tiles) {
          values.push_back(t.x);
          values.push_back(t.y);
        }
      }
      send(sink, ctx, kWall, values);
    }

    for (auto& block : BWEB::Blocks::getBlocks()) {
      std::vector<double> values{double(block.getTilePosition().x),
                                 double(block.getTilePosition().y),
                                 double(block.width()),
                                 double(block.height()),
                                 double(block.getCenter().x),
                                 double(block.getCenter().y),
                                 double(block.isProxy()),
                                 double(block.isDefensive()),
                                 double(int(block.getType()))};
      for (const auto* tiles : {&block.getSmallTiles(), &block.getMediumTiles(), &block.getLargeTiles(),
                                &block.getWideTiles()}) {
        values.push_back(double(tiles->size()));
        for (const auto& t : *tiles) {
          values.push_back(t.x);
          values.push_back(t.y);
        }
      }
      send(sink, ctx, kBlock, values);
    }

    const auto location = [](const BWEB::Station* s) {
      return s ? s->getBase()->Location() : BWAPI::TilePosition(-1, -1);
    };
    const auto main = location(BWEB::Stations::getStartingMain());
    const auto natural = location(BWEB::Stations::getStartingNatural());
    send(sink, ctx, kStarting, {double(main.x), double(main.y), double(natural.x), double(natural.y)});

    const double choke_seconds = laps.lap();

    // Searches between the tiles, in this order: the cache of unreachable tiles
    // carries over from one to the next.
    std::vector<BWAPI::TilePosition> tiles;
    for (std::size_t i = 0; i + 1 < tile_len; i += 2) tiles.emplace_back(path_tiles[i], path_tiles[i + 1]);
    const auto cost = [](const BWAPI::TilePosition& t) { return double((t.x * 7 + t.y * 13) % 5) * 8.0; };
    for (std::size_t k = 0; k < path_type_len; ++k) {
      const BWAPI::UnitType type(path_types[k]);
      for (const auto& source : tiles) {
        for (const auto& target : tiles) {
          for (int mode = first_mode; mode < 4; ++mode) {
            BWEB::Path path(source, target, type);
            const auto terrain = [&](const BWAPI::TilePosition& t) { return path.terrainWalkable(t); };
            const auto unit = [&](const BWAPI::TilePosition& t) { return path.unitWalkable(t); };
            if (mode == 0) path.generateAS_w(terrain);
            if (mode == 1) path.generateAS(cost, terrain);
            if (mode == 2) path.generateJPS(terrain);
            if (mode == 3) path.generateJPS(unit);
            std::vector<double> values{double(path.isReachable()), path.getDistance()};
            for (const auto& t : path.getTiles()) {
              values.push_back(t.x);
              values.push_back(t.y);
            }
            send(sink, ctx, kSearch, values);
          }
        }
      }
    }
    const double search_seconds = laps.lap();
    send(sink, ctx, kTime,
         {bwem_seconds, bweb_seconds, walkable_seconds, placeable_seconds, occupied_seconds, choke_seconds,
          search_seconds});
  } catch (const std::exception&) {
    sink(ctx, kFailed, nullptr, 0);
  }
}
