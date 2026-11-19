#pragma once

#include <cstdint>
#include <vector>

namespace life {

// A Conway's Game of Life board whose edges wrap around (a torus).
class Grid {
public:
    Grid(int width, int height);

    int width() const { return width_; }
    int height() const { return height_; }

    // Coordinates outside the board wrap around.
    bool alive(int x, int y) const;
    void set(int x, int y, bool alive);
    void toggle(int x, int y);
    void clear();
    int population() const;

    bool operator==(const Grid& other) const = default;

private:
    int index(int x, int y) const;

    int width_;
    int height_;
    std::vector<std::uint8_t> cells_;
};

// Computes the next generation of `from` into `to` (same size). Rows are
// shared out between OpenMP threads. Returns the new population.
int step(const Grid& from, Grid& to);

// The same rule on one thread, used to check the parallel version.
int step_serial(const Grid& from, Grid& to);

// How many threads OpenMP will use for `step`.
int threads();

}
