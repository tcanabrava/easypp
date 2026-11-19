#include "check/check.hpp"
#include "life/life.hpp"
#include "patterns/patterns.hpp"

using namespace life;

namespace {

Grid run(Grid grid, int generations)
{
    Grid next(grid.width(), grid.height());
    for (int i = 0; i < generations; ++i) {
        step(grid, next);
        std::swap(grid, next);
    }
    return grid;
}

void still_life_and_oscillator()
{
    Grid block(8, 8);
    block.set(3, 3, true);
    block.set(4, 3, true);
    block.set(3, 4, true);
    block.set(4, 4, true);
    CHECK(run(block, 5) == block);

    Grid blinker(8, 8);
    for (int x = 2; x < 5; ++x) {
        blinker.set(x, 4, true);
    }
    const Grid turned = run(blinker, 1);
    CHECK(turned.alive(3, 3) && turned.alive(3, 4) && turned.alive(3, 5));
    CHECK(!turned.alive(2, 4) && !turned.alive(4, 4));
    CHECK(run(blinker, 2) == blinker);
}

void glider_moves_diagonally_and_wraps()
{
    Grid start(12, 12);
    place(start, Pattern::Glider, 0, 0);
    Grid moved(12, 12);
    place(moved, Pattern::Glider, 1, 1);
    CHECK(run(start, 4) == moved);

    // 48 generations move it 12 cells: once around the torus.
    CHECK(run(start, 48) == start);
}

void parallel_matches_serial()
{
    Grid a(301, 177); // odd sizes so rows do not split evenly between threads
    randomize(a, 0.3, 99);
    Grid b = a;
    Grid na(a.width(), a.height());
    Grid nb(a.width(), a.height());
    for (int i = 0; i < 60; ++i) {
        const int pa = step(a, na);
        const int pb = step_serial(b, nb);
        CHECK(pa == pb);
        CHECK(na == nb);
        CHECK(pa == na.population());
        std::swap(a, na);
        std::swap(b, nb);
    }
    CHECK(threads() >= 1);
}

void patterns_have_the_right_size()
{
    Grid gun(40, 12);
    place(gun, Pattern::GliderGun, 0, 0);
    CHECK(gun.population() == 36);
    Grid pulsar(20, 20);
    place(pulsar, Pattern::Pulsar, 0, 0);
    CHECK(pulsar.population() == 48);
    CHECK(run(pulsar, 3) == pulsar); // period 3
}

}

int main()
{
    still_life_and_oscillator();
    glider_moves_diagonally_and_wraps();
    parallel_matches_serial();
    patterns_have_the_right_size();
    return 0;
}
