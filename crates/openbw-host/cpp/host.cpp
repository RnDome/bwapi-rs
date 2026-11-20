// A melee game of the OpenBW fork of BWAPI inside the test process: the module
// of a Rust bot is handed to the game directly instead of being loaded by
// BWAPILauncher. The steps follow BWAPILauncher and the tests of BWEM-community.

#include "BWAPI/GameImpl.h"
#include "BW/BWData.h"

#include <cstdio>
#include <cstdlib>
#include <exception>
#include <string>

extern "C" int openbw_host_play(const char* map, void (*game_init)(void*),
                                void* (*new_module)(), int frames, int race) noexcept {
  try {
    // Headless: OpenBW opens a window unless told not to.
    setenv("OPENBW_ENABLE_UI", "0", 1);
    // The bot in slot 0, at the first start location of the map: a local
    // patch of OpenBW reads this; without it the seed shuffles the slots and
    // the start location is random.
    setenv("OPENBW_FIXED_SLOTS", "1", 1);
    // The same game on every run unless a seed is given: the seed picks the
    // race of the computer, whose units of the start shift the ids of the
    // neutrals. Also a local patch of OpenBW.
    setenv("OPENBW_GAME_SEED", "1", 0);

    BW::GameOwner owner;
    owner.setPrintTextCallback([](const char* str) {
      std::string s;
      while (*str) {
        char c = *str++;
        if (static_cast<unsigned char>(c) >= 0x20 || c == 9 || c == 10 || c == 13) s += c;
      }
      std::printf("%s\n", s.c_str());
    });

    BWAPI::BroodwarImpl_handle h(owner.getGame());
    game_init(static_cast<BWAPI::Game*>(&*h));
    h->setAIModule(static_cast<BWAPI::AIModule*>(new_module()));
    h->setCharacterName("bwapi");
    h->setGameType(BWAPI::GameTypes::Melee);
    if (!h->bwgame.setMapFileName(map)) {
      std::fprintf(stderr, "openbw-host: cannot open the map %s\n", map);
      return 1;
    }
    h->createSinglePlayerGame([&h, race]() {
      if (!h->self()) {
        h->switchToPlayer(h->getPlayer(0));
      } else {
        h->self()->setRace(BWAPI::Race(race));
        h->startGame();
      }
    });

    for (int frame = 0; frame < frames && !h->bwgame.gameOver(); ++frame) {
      h->update();
      h->bwgame.nextFrame();
    }
    h->update();
    h->onGameEnd();
    h->bwgame.leaveGame();
    return 0;
  } catch (const std::exception& e) {
    std::fprintf(stderr, "openbw-host: %s\n", e.what());
    return 1;
  }
}
