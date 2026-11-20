// A JSON key-value store over TCP: Boost.Asio coroutines on a thread pool.
//
//   kv_server --port 5555 --threads 4
//
// Stop it with Ctrl+C.

#include "net/net.hpp"

#include <boost/program_options.hpp>
#include <boost/system/system_error.hpp>

#include <cstdio>
#include <iostream>
#include <thread>

namespace po = boost::program_options;

namespace {

int run(int argc, char* argv[])
{
    po::options_description options("kv_server options");
    options.add_options()("help,h", "show this help")(
        "port,p", po::value<unsigned short>()->default_value(5555), "TCP port (0 = any free port)")("threads,t",
        po::value<int>()->default_value(static_cast<int>(std::thread::hardware_concurrency())),
        "threads running the Asio io_context");

    po::variables_map args;
    try {
        po::store(po::parse_command_line(argc, argv, options), args);
        po::notify(args);
    } catch (const po::error& e) {
        std::cerr << "kv_server: " << e.what() << "\n" << options;
        return 2;
    }
    if (args.count("help") != 0) {
        std::cout << options;
        return 0;
    }

    try {
        kv::Store store;
        kv::Server server(store, args["port"].as<unsigned short>(), args["threads"].as<int>());
        std::printf("kv_server: listening on port %u with %d threads\n", server.port(), server.threads());
        std::fflush(stdout);
        server.run_until_signal();
        std::printf("kv_server: stopped with %zu keys\n", store.size());
    } catch (const boost::system::system_error& e) {
        // e.what() adds a long source location; the message alone is clearer.
        std::fprintf(stderr, "kv_server: %s\n", e.code().message().c_str());
        return 1;
    } catch (const std::exception& e) {
        std::fprintf(stderr, "kv_server: %s\n", e.what());
        return 1;
    }
    return 0;
}

}

int main(int argc, char* argv[])
{
    // Errors are handled in run(); this only makes sure nothing escapes main.
    try {
        return run(argc, argv);
    } catch (const std::exception& e) {
        std::fprintf(stderr, "kv_server: %s\n", e.what());
    } catch (...) {
        std::fprintf(stderr, "kv_server: unknown error\n");
    }
    return 1;
}
