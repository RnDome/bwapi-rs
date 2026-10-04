#pragma once

#include <BWAPI.h>

// The host resolves BWAPILIB symbols at dlopen, so the headers must match the
// OpenBW host. Upstream BWAPI headers differ in vtables and inline code.
#ifndef OPENBW_BWAPI
#error "bwapi-rs must be built against OpenBW/bwapi headers"
#endif
