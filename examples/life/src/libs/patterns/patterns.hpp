#pragma once

#include "life/life.hpp"

#include <cstdint>
#include <string_view>

namespace life {

enum class Pattern { Glider, Spaceship, RPentomino, Pulsar, GliderGun };

// Rows of '#' (alive) and '.' (dead).
std::string_view picture(Pattern pattern);

// Draws `pattern` with its top-left corner at (x, y), wrapping at the edges.
void place(Grid& grid, Pattern pattern, int x, int y);

// Makes each cell alive with probability `density`.
void randomize(Grid& grid, double density, std::uint32_t seed);

}
