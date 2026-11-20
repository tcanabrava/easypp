#include "check/check.hpp"
#include "store/store.hpp"

#include <string>
#include <thread>
#include <vector>

int main()
{
    kv::Store store;
    CHECK(!store.get("a"));
    store.set("b", "2");
    store.set("a", "1");
    store.set("a", "\"one\"");
    CHECK(store.get("a") == "\"one\"");
    CHECK((store.keys() == std::vector<std::string> {"a", "b"}));
    CHECK(store.erase("a"));
    CHECK(!store.erase("a"));
    CHECK(store.size() == 1);

    // Many threads writing at once lose nothing.
    kv::Store shared;
    std::vector<std::thread> writers;
    writers.reserve(8);
    for (int t = 0; t < 8; ++t) {
        writers.emplace_back([&shared, t] {
            for (int i = 0; i < 500; ++i) {
                shared.set(std::to_string(t) + ":" + std::to_string(i), std::to_string(i));
            }
        });
    }
    for (std::thread& w : writers) {
        w.join();
    }
    CHECK(shared.size() == 8 * 500);
    return 0;
}
