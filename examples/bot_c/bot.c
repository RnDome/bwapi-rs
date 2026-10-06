/* A minimal bot in C over bwapi_c.h: greets, prints the frame and sends idle
   workers to the closest mineral field. */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "bwapi_c.h"

/* Required by bwapi_c.h: the memory of every string and vector the library
   returns in a Slice. The caller frees it. */
void* bwapi_c_alloc_slice(size_t count, size_t elemSize, size_t elemAlign) {
  (void)elemAlign; /* malloc is aligned for every type */
  return malloc(count * elemSize);
}

static Unit* closest(Unit* unit, const Unitset* set) {
  Unit* best = NULL;
  int32_t best_distance = 0;
  SetCursor cursor;
  Unitset_begin(set, &cursor);
  while (!UnitsetCursor_atEnd(&cursor)) {
    Unit* other = UnitsetCursor_next(&cursor);
    int32_t distance = Unit_getDistance_Unit(unit, other);
    if (best == NULL || distance < best_distance) {
      best = other;
      best_distance = distance;
    }
  }
  return best;
}

static void on_start(Game* game) {
  Player* self = Game_self(game);
  if (self == NULL) {
    return; /* a replay */
  }

  /* A string: bytes without a terminating zero. */
  Slice name;
  Player_getName(self, &name);
  if (name.len > 0) {
    char text[128];
    snprintf(text, sizeof text, "gl hf from %.*s", (int)name.len, (const char*)name.data);
    Game_sendText(game, text, strlen(text));
  }
  free(name.data);

  /* A vector: `len` elements of the type the function names. */
  Slice starts;
  Game_getStartLocations(game, &starts);
  const TilePosition* tiles = starts.data;
  for (size_t i = 0; i < starts.len; i++) {
    printf("start location %d, %d\n", (int)tiles[i].x, (int)tiles[i].y);
  }
  free(starts.data);
}

static void on_frame(Game* game) {
  char text[64];
  snprintf(text, sizeof text, "C bot, frame %d", (int)Game_getFrameCount(game));
  Position at = {10, 10};
  Game_drawTextScreen(game, at, text, strlen(text));

  Player* self = Game_self(game);
  if (self == NULL) {
    return;
  }
  SetCursor cursor;
  Unitset_begin(Player_getUnits(self), &cursor);
  while (!UnitsetCursor_atEnd(&cursor)) {
    Unit* unit = UnitsetCursor_next(&cursor);
    if (UnitType_isWorker(Unit_getType(unit)) && Unit_isIdle(unit)) {
      Unit* mineral = closest(unit, Game_getMinerals(game));
      if (mineral != NULL) {
        Unit_gather_d(unit, mineral);
      }
    }
  }
}

/* Required by bwapi_c.h: every callback of the module. */
void bwapi_c_on_event(void* module, Game* game, const Event* event) {
  (void)module;
  switch (Event_getType(event)) {
    case EventType_MatchStart:
      on_start(game);
      break;
    case EventType_MatchFrame:
      on_frame(game);
      break;
    default:
      break;
  }
}

/* The entry points the BWAPI host looks for. */
#ifdef _WIN32
#define EXPORT __declspec(dllexport)
#else
#define EXPORT __attribute__((visibility("default")))
#endif

EXPORT void gameInit(Game* game) { bwapi_c_game_init(game); }

EXPORT AIModule* newAIModule(void) { return bwapi_c_new_module(NULL); }
