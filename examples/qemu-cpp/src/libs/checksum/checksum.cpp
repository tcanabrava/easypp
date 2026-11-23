#include "checksum.hpp"

std::uint8_t checksum(std::span<const std::uint8_t> bytes)
{
    std::uint8_t sum = 0;
    for (const auto byte : bytes) {
        sum = static_cast<std::uint8_t>(sum + byte);
    }
    return sum;
}
