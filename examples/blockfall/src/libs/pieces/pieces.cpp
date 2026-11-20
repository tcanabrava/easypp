#include "pieces.hpp"

namespace blockfall {

namespace {

struct Definition {
    std::array<Cell, 4> base;
    int box; // rotation happens inside a box x box square
};

constexpr std::array<Definition, shape_count> definitions = {{
    {{{{0, 1}, {1, 1}, {2, 1}, {3, 1}}}, 4}, // I
    {{{{1, 0}, {2, 0}, {1, 1}, {2, 1}}}, 0}, // O (does not rotate)
    {{{{1, 0}, {0, 1}, {1, 1}, {2, 1}}}, 3}, // T
    {{{{1, 0}, {2, 0}, {0, 1}, {1, 1}}}, 3}, // S
    {{{{0, 0}, {1, 0}, {1, 1}, {2, 1}}}, 3}, // Z
    {{{{0, 0}, {0, 1}, {1, 1}, {2, 1}}}, 3}, // J
    {{{{2, 0}, {0, 1}, {1, 1}, {2, 1}}}, 3}, // L
}};

}

std::array<Cell, 4> cells(Shape shape, int rotation)
{
    const Definition& def = definitions[static_cast<int>(shape)];
    std::array<Cell, 4> out = def.base;
    if (def.box == 0) {
        return out;
    }
    const int turns = ((rotation % 4) + 4) % 4;
    for (int t = 0; t < turns; ++t) {
        for (Cell& c : out) {
            c = Cell {def.box - 1 - c.y, c.x}; // 90 degrees clockwise
        }
    }
    return out;
}

}
