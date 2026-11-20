#pragma once

#include "pieces/pieces.hpp"

#include <array>
#include <optional>

namespace blockfall {

inline constexpr int board_width = 10;
inline constexpr int board_height = 20;

struct Piece {
    Shape shape;
    int rotation;
    int x; // board position of the bounding box's top-left corner
    int y;
};

class Board {
public:
    // True if every cell of `piece` is inside the walls and floor and on an
    // empty square. Cells above the top (y < 0) are allowed.
    bool fits(const Piece& piece) const;

    // Writes the piece's cells into the board.
    void lock(const Piece& piece);

    // Removes full rows, moves the rows above down, returns how many were removed.
    int clear_lines();

    std::optional<Shape> at(int x, int y) const;
    void set(int x, int y, std::optional<Shape> value);

private:
    std::array<std::optional<Shape>, board_width * board_height> grid_ {};
};

}
