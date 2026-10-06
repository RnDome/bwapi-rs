// `bwapi_c_new_module`: the module the host loads. It turns every callback into
// a `BWAPI::Event` and passes it to `bwapi_c_on_event`, defined by the user of
// bwapi_c.h (the Rust session: src/session/lifecycle.rs).

#include "runtime.h"

#include <BWAPI/Event.h>

namespace {

class Module final : public BWAPI::AIModule {
 public:
  explicit Module(void* module) : module(module) {}

  // The user's module is not freed here: with a live session the game may be
  // gone, and dropping the bot could call into BWAPI.
  ~Module() override = default;

  void onStart() override { send(BWAPI::Event::MatchStart()); }
  void onEnd(bool isWinner) override { send(BWAPI::Event::MatchEnd(isWinner)); }
  void onFrame() override { send(BWAPI::Event::MatchFrame()); }
  void onSendText(std::string text) override { send(BWAPI::Event::SendText(text.c_str())); }
  void onReceiveText(BWAPI::Player player, std::string text) override {
    send(BWAPI::Event::ReceiveText(player, text.c_str()));
  }
  void onPlayerLeft(BWAPI::Player player) override { send(BWAPI::Event::PlayerLeft(player)); }
  void onNukeDetect(BWAPI::Position target) override { send(BWAPI::Event::NukeDetect(target)); }
  void onUnitDiscover(BWAPI::Unit unit) override { send(BWAPI::Event::UnitDiscover(unit)); }
  void onUnitEvade(BWAPI::Unit unit) override { send(BWAPI::Event::UnitEvade(unit)); }
  void onUnitShow(BWAPI::Unit unit) override { send(BWAPI::Event::UnitShow(unit)); }
  void onUnitHide(BWAPI::Unit unit) override { send(BWAPI::Event::UnitHide(unit)); }
  void onUnitCreate(BWAPI::Unit unit) override { send(BWAPI::Event::UnitCreate(unit)); }
  void onUnitDestroy(BWAPI::Unit unit) override { send(BWAPI::Event::UnitDestroy(unit)); }
  void onUnitMorph(BWAPI::Unit unit) override { send(BWAPI::Event::UnitMorph(unit)); }
  void onUnitRenegade(BWAPI::Unit unit) override { send(BWAPI::Event::UnitRenegade(unit)); }
  void onSaveGame(std::string gameName) override {
    send(BWAPI::Event::SaveGame(gameName.c_str()));
  }
  void onUnitComplete(BWAPI::Unit unit) override { send(BWAPI::Event::UnitComplete(unit)); }

 private:
  void send(const BWAPI::Event& event) {
    bwapi_c_on_event(module, reinterpret_cast<Game*>(BWAPI::BroodwarPtr),
                     reinterpret_cast<const Event*>(&event));
  }

  void* module;
};

}  // namespace

AIModule* bwapi_c_new_module(void* module) noexcept {
  BWAPI::AIModule* m = new Module(module);
  return reinterpret_cast<AIModule*>(m);
}
