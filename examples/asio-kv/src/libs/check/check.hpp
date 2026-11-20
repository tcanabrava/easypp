#pragma once

// Minimal test helper shared by src/tests (a header-only internal library).
// Unlike assert(), CHECK also works in release builds (-DNDEBUG).

#include <cstdio>
#include <cstdlib>

#define CHECK(condition)                                                                                               \
    do {                                                                                                               \
        if (!(condition)) {                                                                                            \
            std::fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #condition);                         \
            std::exit(1);                                                                                              \
        }                                                                                                              \
    } while (false)
