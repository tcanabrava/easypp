#include "checksum/checksum.hpp"

#include <array>
#include <iostream>

int main()
{
    constexpr std::array<std::uint8_t, 3> bytes {10, 12, 20};
#if defined(__aarch64__)
    constexpr auto architecture = "ARM64";
#else
    constexpr auto architecture = "native";
#endif
    std::cout << architecture << " checksum: " << static_cast<unsigned>(checksum(bytes)) << '\n';
}
