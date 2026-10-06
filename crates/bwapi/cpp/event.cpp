// The `Event_*` functions of bwapi_c.h: `bwapi_c_new_module` builds a
// `BWAPI::Event` for every callback and the user reads it through these. The
// event lives for the duration of the callback.

#include "runtime.h"

#include <BWAPI/Event.h>

static const BWAPI::Event* cpp(const Event* event) {
  return reinterpret_cast<const BWAPI::Event*>(event);
}

EventType Event_getType(const Event* self) noexcept {
  return static_cast<int32_t>(cpp(self)->getType());
}

Unit* Event_getUnit(const Event* self) noexcept {
  return reinterpret_cast<Unit*>(cpp(self)->getUnit());
}

Player* Event_getPlayer(const Event* self) noexcept {
  return reinterpret_cast<Player*>(cpp(self)->getPlayer());
}

Position Event_getPosition(const Event* self) noexcept {
  BWAPI::Position p = cpp(self)->getPosition();
  return Position{p.x, p.y};
}

// The bytes stay valid while the event lives.
void Event_getText(const Event* self, const char** ptr, size_t* len) noexcept {
  const std::string& text = cpp(self)->getText();
  *ptr = text.data();
  *len = text.size();
}

bool Event_isWinner(const Event* self) noexcept {
  return cpp(self)->isWinner();
}
