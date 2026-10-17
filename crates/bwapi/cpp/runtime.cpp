#include "runtime.h"

void bwapi_c_game_init(Game* game) noexcept {
  BWAPI::BroodwarPtr = reinterpret_cast<BWAPI::Game*>(game);
}

void bwapi_c_fatal(const char* msg, size_t len) noexcept {
  std::fwrite("bwapi-rs: ", 1, 10, stderr);
  std::fwrite(msg, 1, len, stderr);
  std::fputc('\n', stderr);
  std::abort();
}
