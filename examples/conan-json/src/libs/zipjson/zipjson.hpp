#pragma once

#include <cstddef>
#include <vector>

#include <nlohmann/json.hpp>

namespace zipjson {
struct Document {
    std::vector<unsigned char> compressed;
    std::size_t original_size;
};

Document pack(const nlohmann::json& value);
nlohmann::json unpack(const Document& document);
}
