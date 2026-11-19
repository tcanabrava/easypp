#include "check/check.hpp"
#include "pieces/pieces.hpp"

#include <algorithm>

using namespace blockfall;

namespace {

bool same_cells(std::array<Cell, 4> a, std::array<Cell, 4> b)
{
    auto less = [](Cell l, Cell r) { return l.y != r.y ? l.y < r.y : l.x < r.x; };
    std::sort(a.begin(), a.end(), less);
    std::sort(b.begin(), b.end(), less);
    for (int i = 0; i < 4; ++i) {
        if (a[i].x != b[i].x || a[i].y != b[i].y) {
            return false;
        }
    }
    return true;
}

}

int main()
{
    for (int s = 0; s < shape_count; ++s) {
        const Shape shape = static_cast<Shape>(s);
        for (int rotation = 0; rotation < 4; ++rotation) {
            const auto c = cells(shape, rotation);
            // Four distinct cells inside a 4x4 box.
            for (int i = 0; i < 4; ++i) {
                CHECK(c[i].x >= 0 && c[i].x < 4 && c[i].y >= 0 && c[i].y < 4);
                for (int j = i + 1; j < 4; ++j) {
                    CHECK(c[i].x != c[j].x || c[i].y != c[j].y);
                }
            }
        }
        // Four quarter turns are a full turn, in either direction.
        CHECK(same_cells(cells(shape, 0), cells(shape, 4)));
        CHECK(same_cells(cells(shape, 3), cells(shape, -1)));
    }

    // O never changes; T pointing up becomes T pointing right.
    CHECK(same_cells(cells(Shape::O, 0), cells(Shape::O, 1)));
    CHECK(same_cells(cells(Shape::T, 1), {{{1, 0}, {1, 1}, {2, 1}, {1, 2}}}));
    return 0;
}
