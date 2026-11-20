#include "zipjson/zipjson.hpp"

#include <stdexcept>

#include <gtest/gtest.h>

TEST(ZipJson, RoundTripsStructuredData)
{
    const nlohmann::json original = {
        {"tags", {"C++", "Conan"}},
        {"count", 3},
        {"ready", true},
    };
    EXPECT_EQ(zipjson::unpack(zipjson::pack(original)), original);
}

TEST(ZipJson, RejectsTruncatedCompressedData)
{
    auto packed = zipjson::pack(nlohmann::json {{"count", 3}});
    packed.compressed.pop_back();
    EXPECT_THROW(zipjson::unpack(packed), std::runtime_error);
}
