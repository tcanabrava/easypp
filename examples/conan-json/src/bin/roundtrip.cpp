#include "zipjson/zipjson.hpp"

#include <iostream>

int main()
{
    const nlohmann::json original = {
        {"project", "easypp"},
        {"libraries", {"zlib", "nlohmann/json", "GoogleTest"}},
    };
    const auto packed = zipjson::pack(original);
    std::cout << packed.original_size << " JSON bytes, " << packed.compressed.size() << " compressed bytes\n"
              << zipjson::unpack(packed).dump(2) << '\n';
}
