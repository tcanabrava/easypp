// End to end: the real Asio server on a free port, hit by many client threads.

#include "check/check.hpp"
#include "net/net.hpp"
#include "protocol/protocol.hpp"

#include <atomic>
#include <string>
#include <thread>
#include <vector>

int main()
{
    kv::Store store;
    kv::Server server(store, 0, 4);
    const unsigned short port = server.port();
    CHECK(port != 0);
    CHECK(server.threads() == 4);

    constexpr int clients = 16;
    constexpr int keys_per_client = 50;
    std::atomic<int> good_replies {0};
    std::vector<std::thread> threads;
    threads.reserve(clients);
    for (int c = 0; c < clients; ++c) {
        threads.emplace_back([&, c] {
            std::vector<std::string> requests;
            for (int i = 0; i < keys_per_client; ++i) {
                const std::string key = "c" + std::to_string(c) + "k" + std::to_string(i);
                requests.push_back(kv::make_set(key, std::to_string(i)));
                requests.push_back(kv::make_get(key));
            }
            const auto replies = kv::request("127.0.0.1", port, requests);
            for (int i = 0; i < keys_per_client; ++i) {
                const bool ok = replies[2 * i] == R"({"ok":true})"
                    && replies[2 * i + 1] == R"({"ok":true,"value":)" + std::to_string(i) + "}";
                good_replies += ok ? 1 : 0;
            }
        });
    }
    for (std::thread& t : threads) {
        t.join();
    }
    CHECK(good_replies == clients * keys_per_client);
    CHECK(store.size() == static_cast<std::size_t>(clients * keys_per_client));

    const auto size = kv::request("127.0.0.1", port, {kv::make_size()});
    CHECK(size.size() == 1 && size[0] == R"({"ok":true,"size":800})");
    return 0; // the server's destructor stops the io_context and joins its threads
}
