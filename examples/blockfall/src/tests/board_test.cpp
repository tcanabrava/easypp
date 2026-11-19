#include "board/board.hpp"
#include "check/check.hpp"

using namespace blockfall;

int main()
{
    Board board;

    // Walls and floor. A flat I occupies row 1 of its box, columns 0..3.
    CHECK(board.fits({Shape::I, 0, 0, 0}));
    CHECK(board.fits({Shape::I, 0, 6, 0}));
    CHECK(!board.fits({Shape::I, 0, -1, 0}));
    CHECK(!board.fits({Shape::I, 0, 7, 0}));
    CHECK(board.fits({Shape::I, 0, 0, board_height - 2}));
    CHECK(!board.fits({Shape::I, 0, 0, board_height - 1}));
    CHECK(board.fits({Shape::I, 0, 0, -1})); // partly above the top is fine

    // Blocks collide.
    board.set(2, 10, Shape::T);
    CHECK(!board.fits({Shape::I, 0, 0, 9}));
    CHECK(board.fits({Shape::I, 0, 3, 9}));

    // Two full rows clear, the rest falls down by two.
    Board lines;
    for (int x = 0; x < board_width; ++x) {
        lines.set(x, board_height - 1, Shape::L);
        lines.set(x, board_height - 3, Shape::J);
    }
    lines.set(0, board_height - 2, Shape::S); // a non-full row in between
    lines.set(5, board_height - 4, Shape::Z);
    CHECK(lines.clear_lines() == 2);
    CHECK(lines.at(0, board_height - 1) == Shape::S);
    CHECK(lines.at(5, board_height - 2) == Shape::Z);
    CHECK(!lines.at(1, board_height - 1));
    CHECK(!lines.at(5, board_height - 4));
    CHECK(lines.clear_lines() == 0);

    // Locking writes the piece.
    Board locked;
    locked.lock({Shape::O, 0, 0, board_height - 2});
    CHECK(locked.at(1, board_height - 1) == Shape::O);
    CHECK(locked.at(2, board_height - 2) == Shape::O);
    CHECK(!locked.at(0, board_height - 1));
    return 0;
}
