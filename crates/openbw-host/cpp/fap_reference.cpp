// FAP itself (FAP_DIR): the reference the Rust port of FAP is compared with.
// It needs no game, only the type data of BWAPILIB.

#include "BWAPI.h"
#include "FAP.hpp"

#include <cstddef>
#include <cstring>
#include <exception>
#include <tuple>
#include <type_traits>
#include <vector>

namespace {

using Sink = void (*)(void* ctx, const int* values, std::size_t len);

// A unit of the scenario, as `openbw_host::fap_reference` lays it out.
struct Spec {
  int player, type, x, y, health, shields;
  int armor_upgrades, attack_upgrades, shield_upgrades, flags, elevation;
};
static_assert(sizeof(Spec) == 11 * sizeof(int), "eleven ints per unit");

// `testUnit` of FAP's Tests/TestUnit.hpp, at a position, with its health,
// shields and data.
template <typename T = std::tuple<>>
auto unit(const Spec& s, T data = {}) {
  const BWAPI::UnitType ut(s.type);
  return FAP::makeUnit<T>()
      .setUnitType(ut)
      .setHealth(s.health)
      .setShields(s.shields)
      .setFlying(ut.isFlyer())
      .setElevation(s.elevation)
      .setAttackerCount(ut == BWAPI::UnitTypes::Terran_Bunker ? 4 : 8)
      .setAttackCooldownRemaining()
      .setArmorUpgrades(s.armor_upgrades)
      .setAttackUpgrades(s.attack_upgrades)
      .setShieldUpgrades(s.shield_upgrades)
      .setSpeedUpgrade((s.flags & 1) != 0)
      .setAttackSpeedUpgrade((s.flags & 2) != 0)
      .setRangeUpgrade((s.flags & 4) != 0)
      .setStimmed((s.flags & 8) != 0)
      .setPosition(BWAPI::Position(s.x, s.y))
      .setData(data);
}

int bits(float f) {
  int i;
  std::memcpy(&i, &f, sizeof i);
  return i;
}

// Both players' units, each as the count, then per unit the fields `Unit` of
// `openbw_host::fap_reference` lists, and its tag where the data is one.
template <typename T>
std::vector<int> state(FAP::FastAPproximation<T>& fap) {
  std::vector<int> out;
  const auto [p1, p2] = fap.getState();
  for (const auto* units : {p1, p2}) {
    out.push_back(static_cast<int>(units->size()));
    for (const auto& u : *units) {
      out.insert(out.end(),
                 {u.x, u.y, u.health, u.maxHealth, u.armor, u.shields, u.maxShields, u.shieldArmor, bits(u.speed),
                  bits(u.speedSquared), u.flying ? 1 : 0, u.elevation, u.groundDamage, u.groundCooldown,
                  u.groundMaxRangeSquared, u.groundMinRangeSquared, u.groundDamageType.getID(), u.airDamage,
                  u.airCooldown, u.airMaxRangeSquared, u.airMinRangeSquared, u.airDamageType.getID(),
                  u.unitSize.getID(), u.isOrganic ? 1 : 0, u.didHealThisFrame ? 1 : 0, u.numAttackers,
                  u.attackCooldownRemaining});
      if constexpr (std::is_same_v<T, int>)
        out.push_back(u.data);
    }
  }
  return out;
}

void add_tagged(FAP::FastAPproximation<int>& fap, const Spec* units, std::size_t count) {
  for (std::size_t i = 0; i < count; ++i) {
    if (units[i].player == 1)
      fap.addIfCombatUnitPlayer1(unit(units[i], static_cast<int>(i)));
    else
      fap.addIfCombatUnitPlayer2(unit(units[i], static_cast<int>(i)));
  }
}

}  // namespace

// The runs the properties of FAP observe, each unit tagged with its index in
// `specs`. A message is the reason and the frames of the outcome of a call to
// `simulate` (-1 and -1 for none), then `tagged_state`. `mode`:
// 0: the state, then up to `frames` times `simulate(1)` and the state after it,
//    until a call runs no frame;
// 1: the same on a simulator that ran the fight for `frames` frames and was
//    cleared;
// 2: `simulate(frames)` and the state after it;
// 3: `frames - 1` times `simulate(1)`, then `simulate(2)` and the state after it.
extern "C" void openbw_host_fap_run(const int* specs, std::size_t count, int tank_splash, int mode, int frames,
                                    void* ctx, Sink sink) noexcept {
  try {
    const auto* units = reinterpret_cast<const Spec*>(specs);
    FAP::FastAPproximation<int> fap;
    auto simulate = [&](int n) {
      return tank_splash ? fap.simulate<true>(n) : fap.simulate<false>(n);
    };
    auto send = [&](int reason, int ran) {
      auto s = state(fap);
      s.insert(s.begin(), {reason, ran});
      sink(ctx, s.data(), s.size());
    };
    if (mode == 1) {
      add_tagged(fap, units, count);
      simulate(frames);
      fap.clear();
    }
    add_tagged(fap, units, count);
    switch (mode) {
    case 0:
    case 1:
      send(-1, -1);
      for (int i = 0; i < frames; ++i) {
        const auto o = simulate(1);
        send(o.reason, o.frames);
        if (o.frames == 0)
          break;
      }
      break;
    case 2: {
      const auto o = simulate(frames);
      send(o.reason, o.frames);
      break;
    }
    case 3: {
      for (int i = 0; i + 1 < frames; ++i)
        simulate(1);
      const auto o = simulate(2);
      send(o.reason, o.frames);
      break;
    }
    }
  } catch (const std::exception&) {
  }
}

// Adds the units of `specs` that are combat units, then simulates `steps`
// times `frames` frames, a negative number until the fight is over; sends the
// state before the first step and after each.
extern "C" void openbw_host_fap_reference(const int* specs, std::size_t count, int tank_splash, int steps,
                                          int frames, void* ctx, Sink sink) noexcept {
  try {
    FAP::FastAPproximation<> fap;
    const auto* units = reinterpret_cast<const Spec*>(specs);
    for (std::size_t i = 0; i < count; ++i) {
      if (units[i].player == 1)
        fap.addIfCombatUnitPlayer1(unit(units[i]));
      else
        fap.addIfCombatUnitPlayer2(unit(units[i]));
    }
    auto send = [&] {
      const auto s = state(fap);
      sink(ctx, s.data(), s.size());
    };
    send();
    for (int i = 0; i < steps; ++i) {
      if (tank_splash)
        fap.simulate<true>(frames);
      else
        fap.simulate<false>(frames);
      send();
    }
  } catch (const std::exception&) {
  }
}
