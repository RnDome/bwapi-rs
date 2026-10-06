#pragma once

#include <cstdint>

#include <BWAPI.h>

#include <cstddef>
#include <cstdlib>
#include <cstring>
#include <iterator>
#include <new>
#include <string>
#include <type_traits>
#include <utility>

// Headers of BWAPI 4.4.0. They also fit OpenBW: its fork has every 4.4.0
// method with the same signature and puts its own virtual methods after them,
// so the vtable slots this crate calls are the same on both hosts.
static_assert(BWAPI::CLIENT_VERSION == 10003, "bwapi-c is built against BWAPI 4.4.0 headers");

// The C API this C++ implements: every definition is checked against it.
#include "include/bwapi_c.h"

namespace bwapi_c {

template <class T>
using bare = std::remove_cv_t<std::remove_reference_t<T>>;

// Is `A` (after dropping cv and references) the type `E` declared in bwapi.api?
// Pairs compare member-wise: `std::map` elements have a const key.
template <class A, class E>
struct same_impl : std::is_same<A, E> {};

template <class A1, class A2, class E1, class E2>
struct same_impl<std::pair<A1, A2>, std::pair<E1, E2>>
    : std::bool_constant<std::is_same_v<bare<A1>, E1> && std::is_same_v<bare<A2>, E2>> {};

template <class A, class E>
inline constexpr bool same_v = same_impl<bare<A>, E>::value;

// Element type of a container returned by BWAPI.
template <class C>
using elem_t = decltype(*std::begin(std::declval<const C&>()));

// Room for `n` elements in `out`: memory of the user's bwapi_c_alloc_slice.
template <class Elem>
Elem* alloc_slice(Slice* out, std::size_t n) noexcept {
  out->len = n;
  out->data = nullptr;
  if (n == 0) {
    return nullptr;
  }
  out->data = bwapi_c_alloc_slice(n, sizeof(Elem), alignof(Elem));
  if (out->data == nullptr) {
    std::abort();
  }
  return static_cast<Elem*>(out->data);
}

inline void write_string(Slice* out, const std::string& s) noexcept {
  if (std::uint8_t* p = alloc_slice<std::uint8_t>(out, s.size())) {
    std::memcpy(p, s.data(), s.size());
  }
}

template <class Elem, class C, class F>
void write_vec(Slice* out, const C& c, F convert) noexcept {
  Elem* p = alloc_slice<Elem>(out, c.size());
  for (const auto& e : c) {
    *p++ = convert(e);
  }
}

}  // namespace bwapi_c
