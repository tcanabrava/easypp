#include "board.hpp"

namespace blockfall {

namespace {

int index(int x, int y)
{
    return y * board_width + x;
}

}

bool Board::fits(const Piece& piece) const
{
    for (Cell c : cells(piece.shape, piece.rotation)) {
        const int x = piece.x + c.x;
        const int y = piece.y + c.y;
        if (x < 0 || x >= board_width || y >= board_height) {
            return false;
        }
        if (y >= 0 && grid_[index(x, y)]) {
            return false;
        }
    }
    return true;
}

void Board::lock(const Piece& piece)
{
    for (Cell c : cells(piece.shape, piece.rotation)) {
        const int x = piece.x + c.x;
        const int y = piece.y + c.y;
        if (x >= 0 && x < board_width && y >= 0 && y < board_height) {
            grid_[index(x, y)] = piece.shape;
        }
    }
}

int Board::clear_lines()
{
    int cleared = 0;
    int write = board_height - 1;
    for (int y = board_height - 1; y >= 0; --y) {
        bool full = true;
        for (int x = 0; x < board_width; ++x) {
            full = full && grid_[index(x, y)].has_value();
        }
        if (full) {
            ++cleared;
            continue;
        }
        if (write != y) {
            for (int x = 0; x < board_width; ++x) {
                grid_[index(x, write)] = grid_[index(x, y)];
            }
        }
        --write;
    }
    for (int y = write; y >= 0; --y) {
        for (int x = 0; x < board_width; ++x) {
            grid_[index(x, y)].reset();
        }
    }
    return cleared;
}

std::optional<Shape> Board::at(int x, int y) const
{
    if (x < 0 || x >= board_width || y < 0 || y >= board_height) {
        return std::nullopt;
    }
    return grid_[index(x, y)];
}

void Board::set(int x, int y, std::optional<Shape> value)
{
    if (x >= 0 && x < board_width && y >= 0 && y < board_height) {
        grid_[index(x, y)] = value;
    }
}

}
