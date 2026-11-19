#pragma once

// Networking with Boost.Asio. The header does not include Asio (the server
// hides it behind a pointer), which keeps compile times of users low.

#include "store/store.hpp"

#include <memory>
#include <string>
#include <vector>

namespace kv {

// A TCP server: one C++20 coroutine per connection, with the Asio io_context
// running on `threads` std::threads.
class Server {
public:
    // Port 0 picks any free port; see port().
    Server(Store& store, unsigned short port, int threads);
    ~Server(); // stops and joins the threads

    Server(const Server&) = delete;
    Server& operator=(const Server&) = delete;

    unsigned short port() const;
    int threads() const;
    // Blocks until SIGINT or SIGTERM, then stops.
    void run_until_signal();
    void stop();

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

// Connects, sends each line, and returns one reply line per request.
std::vector<std::string> request(const std::string& host, unsigned short port,
                                 const std::vector<std::string>& lines);

}
