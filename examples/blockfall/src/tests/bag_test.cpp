#include "bag/bag.hpp"
#include "check/check.hpp"

#include <array>

using namespace blockfall;

int main()
{
    // Each round of seven has every shape exactly once.
    Bag bag(1234);
    for (int round = 0; round < 50; ++round) {
        std::array<int, shape_count> seen {};
        for (int i = 0; i < shape_count; ++i) {
            ++seen[static_cast<int>(bag.next())];
        }
        for (int count : seen) {
            CHECK(count == 1);
        }
    }

    // Same seed, same sequence.
    Bag a(42), b(42);
    for (int i = 0; i < 100; ++i) {
        CHECK(a.next() == b.next());
    }
    return 0;
}
