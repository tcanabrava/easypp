#pragma once

// Drawing with cairo, shared by the GTK window and the headless PNG mode.
// Including <cairo.h> here is enough for easypp to give every user of this
// library cairo's compile and link flags.

#include "life/life.hpp"

#include <cairo.h>

namespace life::view {

struct Layout {
    double cell; // side of one cell, in pixels
    double origin_x; // top-left corner of the board
    double origin_y;
};

// The largest square cells that fit the board into width x height, centred.
Layout fit(const Grid& grid, int width, int height);

void draw(cairo_t* cr, const Grid& grid, int width, int height);

// The cell under pixel (px, py), or false if the point is outside the board.
bool cell_at(const Grid& grid, int width, int height, double px, double py, int& x, int& y);

// Renders the board to a PNG file. Returns false on failure.
bool save_png(const Grid& grid, int width, int height, const char* path);

}
