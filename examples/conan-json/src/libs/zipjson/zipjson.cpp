#include "zipjson.hpp"

#include <stdexcept>
#include <string>

#include <zlib.h>

namespace zipjson {
Document pack(const nlohmann::json& value)
{
    const auto text = value.dump();
    uLongf size = compressBound(text.size());
    Document document {std::vector<unsigned char>(size), text.size()};
    if (compress(document.compressed.data(), &size, reinterpret_cast<const Bytef*>(text.data()), text.size()) != Z_OK) {
        throw std::runtime_error("could not compress JSON");
    }
    document.compressed.resize(size);
    return document;
}

nlohmann::json unpack(const Document& document)
{
    std::string text(document.original_size, '\0');
    uLongf size = text.size();
    if (uncompress(reinterpret_cast<Bytef*>(text.data()), &size, document.compressed.data(), document.compressed.size())
            != Z_OK
        || size != document.original_size) {
        throw std::runtime_error("invalid compressed JSON");
    }
    return nlohmann::json::parse(text);
}
}
