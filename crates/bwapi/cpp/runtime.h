#pragma once

#include <cstdint>

#include <BWAPI.h>

// Headers of BWAPI 4.4.0. They also fit OpenBW: its fork has every 4.4.0
// method with the same signature and puts its own virtual methods after them,
// so the vtable slots this crate calls are the same on both hosts.
static_assert(BWAPI::CLIENT_VERSION == 10003, "bwapi-rs is built against BWAPI 4.4.0 headers");
