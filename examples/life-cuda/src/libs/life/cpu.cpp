#include "life.hpp"

namespace life {

Board seeded_board()
{
    Board board {};
    unsigned int random = 42;
    for (auto& cell : board) {
        random = 1664525u * random + 1013904223u;
        cell = (random >> 24) < 74;
    }
    return board;
}

int population(const Board& board)
{
    int count = 0;
#pragma omp parallel for num_threads(4) reduction(+ : count)
    for (int i = 0; i < width * height; ++i) {
        count += board[i];
    }
    return count;
}

Board reference_step(const Board& board)
{
    Board next {};
#pragma omp parallel for num_threads(4)
    for (int y = 0; y < height; ++y) {
        for (int x = 0; x < width; ++x) {
            int count = 0;
            for (int dy : {-1, 0, 1}) {
                for (int dx : {-1, 0, 1}) {
                    if (dx != 0 || dy != 0) {
                        count += board[((y + dy + height) % height) * width + (x + dx + width) % width];
                    }
                }
            }
            next[y * width + x] = count == 3 || (count == 2 && board[y * width + x]);
        }
    }
    return next;
}

}
