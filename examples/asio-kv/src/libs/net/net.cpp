#include "net.hpp"

#include "protocol/protocol.hpp"

#include <boost/asio.hpp>

#include <future>
#include <thread>

namespace asio = boost::asio;
using asio::ip::tcp;

namespace kv {

namespace {

// One client connection: read a line, answer it, repeat until it hangs up.
asio::awaitable<void> session(tcp::socket socket, Store& store)
{
    std::string buffer;
    try {
        for (;;) {
            const std::size_t n
                = co_await asio::async_read_until(socket, asio::dynamic_buffer(buffer), '\n', asio::use_awaitable);
            std::string line = buffer.substr(0, n - 1);
            buffer.erase(0, n);
            if (!line.empty() && line.back() == '\r') {
                line.pop_back();
            }
            const std::string answer = handle(store, line) + "\n";
            co_await asio::async_write(socket, asio::buffer(answer), asio::use_awaitable);
        }
    } catch (const boost::system::system_error&) {
        // The client closed the connection (or the server is stopping).
    }
}

asio::awaitable<void> listen(tcp::acceptor& acceptor, Store& store)
{
    for (;;) {
        tcp::socket socket = co_await acceptor.async_accept(asio::use_awaitable);
        asio::co_spawn(acceptor.get_executor(), session(std::move(socket), store), asio::detached);
    }
}

}

struct Server::Impl {
    Impl(Store& store, unsigned short port, int thread_count)
        : acceptor(io, tcp::endpoint(tcp::v4(), port))
        , signals(io, SIGINT, SIGTERM)
    {
        asio::co_spawn(io, listen(acceptor, store), asio::detached);
        signals.async_wait([this](const boost::system::error_code& ec, int) {
            if (!ec) {
                stopped.set_value();
            }
        });
        for (int i = 0; i < thread_count; ++i) {
            threads.emplace_back([this] { io.run(); });
        }
    }

    void stop()
    {
        io.stop();
        for (std::thread& t : threads) {
            if (t.joinable()) {
                t.join();
            }
        }
    }

    asio::io_context io;
    tcp::acceptor acceptor;
    asio::signal_set signals;
    std::promise<void> stopped;
    std::vector<std::thread> threads;
};

Server::Server(Store& store, unsigned short port, int threads)
    : impl_(std::make_unique<Impl>(store, port, threads < 1 ? 1 : threads))
{
}

Server::~Server()
{
    impl_->stop();
}

unsigned short Server::port() const
{
    return impl_->acceptor.local_endpoint().port();
}

int Server::threads() const
{
    return static_cast<int>(impl_->threads.size());
}

void Server::run_until_signal()
{
    impl_->stopped.get_future().wait();
    impl_->stop();
}

void Server::stop()
{
    impl_->stop();
}

std::vector<std::string> request(const std::string& host, unsigned short port, const std::vector<std::string>& lines)
{
    asio::io_context io;
    tcp::resolver resolver(io);
    tcp::socket socket(io);
    asio::connect(socket, resolver.resolve(host, std::to_string(port)));

    std::vector<std::string> replies;
    std::string buffer;
    for (const std::string& line : lines) {
        asio::write(socket, asio::buffer(line + "\n"));
        const std::size_t n = asio::read_until(socket, asio::dynamic_buffer(buffer), '\n');
        replies.push_back(buffer.substr(0, n - 1));
        buffer.erase(0, n);
    }
    return replies;
}

}
