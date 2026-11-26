#include "runtime.h"

void bwapi_c_game_init(Game* game) noexcept {
  BWAPI::BroodwarPtr = reinterpret_cast<BWAPI::Game*>(game);
}

Game* bwapi_c_game() noexcept {
  return reinterpret_cast<Game*>(BWAPI::BroodwarPtr);
}
