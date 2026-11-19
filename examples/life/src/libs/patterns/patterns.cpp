#include "patterns.hpp"

#include <random>

namespace life {

std::string_view picture(Pattern pattern)
{
    switch (pattern) {
    case Pattern::Glider:
        return ".#.\n"
               "..#\n"
               "###\n";
    case Pattern::Spaceship:
        return ".#..#\n"
               "#....\n"
               "#...#\n"
               "####.\n";
    case Pattern::RPentomino:
        return ".##\n"
               "##.\n"
               ".#.\n";
    case Pattern::Pulsar:
        return "..###...###..\n"
               ".............\n"
               "#....#.#....#\n"
               "#....#.#....#\n"
               "#....#.#....#\n"
               "..###...###..\n"
               ".............\n"
               "..###...###..\n"
               "#....#.#....#\n"
               "#....#.#....#\n"
               "#....#.#....#\n"
               ".............\n"
               "..###...###..\n";
    case Pattern::GliderGun:
        return "........................#...........\n"
               "......................#.#...........\n"
               "............##......##............##\n"
               "...........#...#....##............##\n"
               "##........#.....#...##..............\n"
               "##........#...#.##....#.#...........\n"
               "..........#.....#.......#...........\n"
               "...........#...#....................\n"
               "............##......................\n";
    }
    return "";
}

void place(Grid& grid, Pattern pattern, int x, int y)
{
    int row = 0;
    int column = 0;
    for (char c : picture(pattern)) {
        if (c == '\n') {
            ++row;
            column = 0;
            continue;
        }
        if (c == '#') {
            grid.set(x + column, y + row, true);
        }
        ++column;
    }
}

void randomize(Grid& grid, double density, std::uint32_t seed)
{
    std::mt19937 rng(seed);
    std::bernoulli_distribution alive(density);
    for (int y = 0; y < grid.height(); ++y) {
        for (int x = 0; x < grid.width(); ++x) {
            grid.set(x, y, alive(rng));
        }
    }
}

}
