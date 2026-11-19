#include "bag.hpp"

#include <algorithm>

namespace blockfall {

Bag::Bag(std::uint32_t seed)
    : rng_(seed)
{
    for (int i = 0; i < shape_count; ++i) {
        round_[i] = static_cast<Shape>(i);
    }
}

Shape Bag::next()
{
    if (index_ == shape_count) {
        std::shuffle(round_.begin(), round_.end(), rng_);
        index_ = 0;
    }
    return round_[index_++];
}

}
