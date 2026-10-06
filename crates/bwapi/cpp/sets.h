#pragma once

#include "runtime.h"

namespace bwapi_c {

// Cursor over a live set (`const XSet&` with a stable address), kept in the
// caller's `SetCursor`. It is moved by copying bytes, so it must stay trivially
// copyable.
template <class Set>
struct SetCursor {
  typename Set::const_iterator it;
  typename Set::const_iterator end;
  std::size_t remaining;
};

// A set returned by value. libstdc++ `unordered_set` points into itself, so the
// set lives in the C++ heap, is moved there by its move constructor, and the
// cursor is taken only after the set reached its final address.
template <class Set>
struct QueryState {
  Set set;
  typename Set::const_iterator it;
  std::size_t remaining;

  explicit QueryState(Set&& s) : set(std::move(s)), it(set.cbegin()), remaining(set.size()) {}
};

}  // namespace bwapi_c
