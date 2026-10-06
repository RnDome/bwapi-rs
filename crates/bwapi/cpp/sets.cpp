// Iteration over BWAPI sets: the set functions of bwapi_c.h. Every set type gets
// the same functions.
//
// Protocol: `atEnd` reports exhaustion; `next` has the precondition `!atEnd`
// and returns `*it++`. `remaining` is decremented together with the iterator.

#include "sets.h"

#define BWAPI_C_SETS(CSET, CQUERY, CELEM, SET, ELEM)                                                   \
  static_assert(std::is_same_v<SET::value_type, ELEM*>, #SET " must hold " #ELEM "*");                 \
  static_assert(std::is_trivially_copyable_v<bwapi_c::SetCursor<SET>>,                                \
                "set cursor is moved by copying bytes");                                               \
  static_assert(sizeof(bwapi_c::SetCursor<SET>) <= sizeof(SetCursor),                                 \
                "set cursor does not fit SetCursor");                                                  \
  static_assert(alignof(bwapi_c::SetCursor<SET>) <= alignof(SetCursor),                               \
                "set cursor is over-aligned for SetCursor");                                           \
                                                                                                       \
  namespace CSET##_cpp {                                                                               \
  using Cursor = bwapi_c::SetCursor<SET>;                                                              \
  using Query = bwapi_c::QueryState<SET>;                                                              \
  inline const SET* set(const CSET* s) { return reinterpret_cast<const SET*>(s); }                     \
  inline Cursor* cursor(SetCursor* c) { return std::launder(reinterpret_cast<Cursor*>(c)); }           \
  inline const Cursor* cursor(const SetCursor* c) {                                                    \
    return std::launder(reinterpret_cast<const Cursor*>(c));                                           \
  }                                                                                                    \
  inline Query* query(CQUERY* q) { return reinterpret_cast<Query*>(q); }                               \
  inline const Query* query(const CQUERY* q) { return reinterpret_cast<const Query*>(q); }             \
  }                                                                                                    \
                                                                                                       \
  size_t CSET##_size(const CSET* self) noexcept { return CSET##_cpp::set(self)->size(); }              \
                                                                                                       \
  bool CSET##_contains(const CSET* self, CELEM* elem) noexcept {                                       \
    return CSET##_cpp::set(self)->count(reinterpret_cast<ELEM*>(elem)) != 0;                           \
  }                                                                                                    \
                                                                                                       \
  void CSET##_begin(const CSET* self, SetCursor* cursor) noexcept {                                    \
    const SET* set = CSET##_cpp::set(self);                                                            \
    new (cursor) CSET##_cpp::Cursor{set->cbegin(), set->cend(), set->size()};                          \
  }                                                                                                    \
                                                                                                       \
  bool CSET##Cursor_atEnd(const SetCursor* self) noexcept {                                            \
    const auto* c = CSET##_cpp::cursor(self);                                                          \
    return c->it == c->end;                                                                            \
  }                                                                                                    \
                                                                                                       \
  CELEM* CSET##Cursor_next(SetCursor* self) noexcept {                                                 \
    auto* c = CSET##_cpp::cursor(self);                                                                \
    --c->remaining;                                                                                    \
    return reinterpret_cast<CELEM*>(*c->it++);                                                         \
  }                                                                                                    \
                                                                                                       \
  size_t CSET##Cursor_remaining(const SetCursor* self) noexcept {                                      \
    return CSET##_cpp::cursor(self)->remaining;                                                        \
  }                                                                                                    \
                                                                                                       \
  bool CQUERY##_atEnd(const CQUERY* self) noexcept {                                                   \
    const auto* q = CSET##_cpp::query(self);                                                           \
    return q->it == q->set.cend();                                                                     \
  }                                                                                                    \
                                                                                                       \
  CELEM* CQUERY##_next(CQUERY* self) noexcept {                                                        \
    auto* q = CSET##_cpp::query(self);                                                                 \
    --q->remaining;                                                                                    \
    return reinterpret_cast<CELEM*>(*q->it++);                                                         \
  }                                                                                                    \
                                                                                                       \
  size_t CQUERY##_remaining(const CQUERY* self) noexcept {                                             \
    return CSET##_cpp::query(self)->remaining;                                                         \
  }                                                                                                    \
                                                                                                       \
  void CQUERY##_release(CQUERY* self) noexcept { delete CSET##_cpp::query(self); }

BWAPI_C_SETS(Unitset, UnitQuery, Unit, BWAPI::Unitset, BWAPI::UnitInterface)
BWAPI_C_SETS(Playerset, PlayerQuery, Player, BWAPI::Playerset, BWAPI::PlayerInterface)
BWAPI_C_SETS(Forceset, ForceQuery, Force, BWAPI::Forceset, BWAPI::ForceInterface)
BWAPI_C_SETS(Bulletset, BulletQuery, Bullet, BWAPI::Bulletset, BWAPI::BulletInterface)
BWAPI_C_SETS(Regionset, RegionQuery, Region, BWAPI::Regionset, BWAPI::RegionInterface)
