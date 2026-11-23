#pragma once

#include <cstdint>
#include <span>

std::uint8_t checksum(std::span<const std::uint8_t> bytes);
