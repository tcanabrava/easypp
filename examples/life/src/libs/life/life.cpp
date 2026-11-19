#include "life.hpp"

#include <omp.h>

#include <algorithm>
#include <cassert>

namespace life {

Grid::Grid(int width, int height)
    : width_(width)
    , height_(height)
    , cells_(static_cast<std::size_t>(width) * static_cast<std::size_t>(height), 0)
{
}

int Grid::index(int x, int y) const
{
    x = ((x % width_) + width_) % width_;
    y = ((y % height_) + height_) % height_;
    return y * width_ + x;
}

bool Grid::alive(int x, int y) const
{
    return cells_[index(x, y)] != 0;
}

void Grid::set(int x, int y, bool alive)
{
    cells_[index(x, y)] = alive ? 1 : 0;
}

void Grid::toggle(int x, int y)
{
    cells_[index(x, y)] ^= 1;
}

void Grid::clear()
{
    std::fill(cells_.begin(), cells_.end(), 0);
}

int Grid::population() const
{
    int count = 0;
    for (std::uint8_t c : cells_) {
        count += c;
    }
    return count;
}

namespace {

// Next state of one row. Neighbours wrap around the edges.
int step_row(const Grid& from, Grid& to, int y)
{
    int alive = 0;
    for (int x = 0; x < from.width(); ++x) {
        int neighbours = 0;
        for (int dy = -1; dy <= 1; ++dy) {
            for (int dx = -1; dx <= 1; ++dx) {
                if ((dx != 0 || dy != 0) && from.alive(x + dx, y + dy)) {
                    ++neighbours;
                }
            }
        }
        const bool next = neighbours == 3 || (neighbours == 2 && from.alive(x, y));
        to.set(x, y, next);
        alive += next ? 1 : 0;
    }
    return alive;
}

}

int step(const Grid& from, Grid& to)
{
    assert(from.width() == to.width() && from.height() == to.height());
    int alive = 0;
    // Each row only writes its own cells in `to`, so rows are independent.
#pragma omp parallel for schedule(static) reduction(+ : alive)
    for (int y = 0; y < from.height(); ++y) {
        alive += step_row(from, to, y);
    }
    return alive;
}

int step_serial(const Grid& from, Grid& to)
{
    int alive = 0;
    for (int y = 0; y < from.height(); ++y) {
        alive += step_row(from, to, y);
    }
    return alive;
}

int threads()
{
    return omp_get_max_threads();
}

}
