#pragma once

#include "pieces/pieces.hpp"

#include <array>
#include <cstdint>
#include <random>

namespace blockfall {

// Deals shapes in shuffled rounds of seven, so every shape shows up once per
// round and droughts are short.
class Bag {
public:
    explicit Bag(std::uint32_t seed);

    Shape next();

private:
    std::mt19937 rng_;
    std::array<Shape, shape_count> round_ {};
    int index_ = shape_count;
};

}
