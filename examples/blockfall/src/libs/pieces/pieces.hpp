#pragma once

#include <array>
#include <cstdint>

namespace blockfall {

enum class Shape : std::uint8_t { I, O, T, S, Z, J, L };

inline constexpr int shape_count = 7;

struct Cell {
    int x;
    int y;
};

// The four cells of `shape` in `rotation` (any integer, taken modulo 4),
// relative to the top-left corner of the piece's bounding box. y grows down.
std::array<Cell, 4> cells(Shape shape, int rotation);

}
