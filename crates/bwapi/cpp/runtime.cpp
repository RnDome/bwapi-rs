#include "runtime.h"

void bwapi_c_game_init(Game* game) noexcept {
  BWAPI::BroodwarPtr = reinterpret_cast<BWAPI::Game*>(game);
}
