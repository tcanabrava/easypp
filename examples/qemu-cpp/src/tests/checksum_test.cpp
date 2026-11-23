#include "checksum/checksum.hpp"

#include <array>

int main()
{
    constexpr std::array<std::uint8_t, 3> bytes {10, 12, 20};
    constexpr std::array<std::uint8_t, 2> wraparound {255, 2};
    return checksum({}) == 0 && checksum(bytes) == 42 && checksum(wraparound) == 1 ? 0 : 1;
}
