#include "view.hpp"

#include <algorithm>
#include <cmath>

namespace life::view {

Layout fit(const Grid& grid, int width, int height)
{
    const double cell = std::max(1.0,
        std::floor(std::min(static_cast<double>(width) / grid.width(), static_cast<double>(height) / grid.height())));
    return Layout {cell, (width - cell * grid.width()) / 2.0, (height - cell * grid.height()) / 2.0};
}

void draw(cairo_t* cr, const Grid& grid, int width, int height)
{
    const Layout layout = fit(grid, width, height);

    cairo_set_source_rgb(cr, 0.08, 0.09, 0.11);
    cairo_paint(cr);

    cairo_set_source_rgb(cr, 0.12, 0.13, 0.16);
    cairo_rectangle(cr, layout.origin_x, layout.origin_y, layout.cell * grid.width(), layout.cell * grid.height());
    cairo_fill(cr);

    // Living cells, with a one-pixel gap when cells are big enough to show it.
    const double gap = layout.cell >= 4 ? 1.0 : 0.0;
    cairo_set_source_rgb(cr, 0.45, 0.85, 0.62);
    for (int y = 0; y < grid.height(); ++y) {
        for (int x = 0; x < grid.width(); ++x) {
            if (grid.alive(x, y)) {
                cairo_rectangle(cr, layout.origin_x + x * layout.cell, layout.origin_y + y * layout.cell,
                    layout.cell - gap, layout.cell - gap);
            }
        }
    }
    cairo_fill(cr);
}

bool cell_at(const Grid& grid, int width, int height, double px, double py, int& x, int& y)
{
    const Layout layout = fit(grid, width, height);
    x = static_cast<int>(std::floor((px - layout.origin_x) / layout.cell));
    y = static_cast<int>(std::floor((py - layout.origin_y) / layout.cell));
    return x >= 0 && y >= 0 && x < grid.width() && y < grid.height();
}

bool save_png(const Grid& grid, int width, int height, const char* path)
{
    cairo_surface_t* surface = cairo_image_surface_create(CAIRO_FORMAT_RGB24, width, height);
    cairo_t* cr = cairo_create(surface);
    draw(cr, grid, width, height);
    cairo_destroy(cr);
    const bool ok = cairo_surface_write_to_png(surface, path) == CAIRO_STATUS_SUCCESS;
    cairo_surface_destroy(surface);
    return ok;
}

}
