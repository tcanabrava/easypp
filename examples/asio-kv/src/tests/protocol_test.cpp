#include "check/check.hpp"
#include "protocol/protocol.hpp"

#include <string>

using kv::handle;

namespace {

bool contains(const std::string& text, const std::string& part)
{
    return text.find(part) != std::string::npos;
}

}

int main()
{
    kv::Store store;
    CHECK(handle(store, kv::make_set("name", "\"Ada\"")) == R"({"ok":true})");
    CHECK(handle(store, kv::make_set("langs", R"(["C++", "Rust"])")) == R"({"ok":true})");
    CHECK(handle(store, kv::make_set("plain", "not json")) == R"({"ok":true})");

    CHECK(handle(store, kv::make_get("name")) == R"({"ok":true,"value":"Ada"})");
    CHECK(handle(store, kv::make_get("langs")) == R"({"ok":true,"value":["C++","Rust"]})");
    CHECK(handle(store, kv::make_get("plain")) == R"({"ok":true,"value":"not json"})");
    CHECK(handle(store, kv::make_keys()) == R"({"ok":true,"keys":["langs","name","plain"]})");
    CHECK(handle(store, kv::make_size()) == R"({"ok":true,"size":3})");
    CHECK(handle(store, kv::make_del("name")) == R"({"ok":true,"deleted":true})");
    CHECK(handle(store, kv::make_del("name")) == R"({"ok":true,"deleted":false})");

    // Bad requests get an error reply, never an exception.
    CHECK(contains(handle(store, kv::make_get("name")), R"("ok":false)"));
    CHECK(contains(handle(store, "{oops"), "invalid JSON"));
    CHECK(contains(handle(store, "[1,2]"), "must be a JSON object"));
    CHECK(contains(handle(store, R"({"op":"fly","key":"x"})"), "unknown op"));
    CHECK(contains(handle(store, R"({"op":"get"})"), R"(missing \"key\")")); // JSON-escaped quotes

    CHECK(kv::make_request({"set", "k"}).empty());
    CHECK(kv::make_request({"keys"}) == kv::make_keys());
    return 0;
}
