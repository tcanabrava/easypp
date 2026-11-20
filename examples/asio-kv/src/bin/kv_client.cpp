// Talks to kv_server.
//
//   kv_client set name '"Ada"'      (values are JSON; other text is sent as a string)
//   kv_client set langs '["C++", "Rust"]'
//   kv_client get name
//   kv_client del name
//   kv_client keys | size
//   kv_client                       read one command per line from stdin

#include "net/net.hpp"
#include "protocol/protocol.hpp"

#include <boost/program_options.hpp>
#include <boost/system/system_error.hpp>

#include <iostream>
#include <sstream>
#include <string>
#include <vector>

namespace po = boost::program_options;

namespace {

std::vector<std::string> words_of(const std::string& line)
{
    // The first two words split on spaces; the rest of the line is the value.
    std::istringstream in(line);
    std::vector<std::string> words;
    std::string word;
    for (int i = 0; i < 2 && in >> word; ++i) {
        words.push_back(word);
    }
    std::string rest;
    std::getline(in >> std::ws, rest);
    if (!rest.empty()) {
        words.push_back(rest);
    }
    return words;
}

}

int main(int argc, char* argv[])
{
    po::options_description options("kv_client options");
    options.add_options()("help,h", "show this help")("host", po::value<std::string>()->default_value("127.0.0.1"),
        "server address")("port,p", po::value<unsigned short>()->default_value(5555), "server port")(
        "command", po::value<std::vector<std::string>>(), "set KEY VALUE | get KEY | del KEY | keys | size");
    po::positional_options_description positional;
    positional.add("command", -1);

    po::variables_map args;
    try {
        po::store(po::command_line_parser(argc, argv).options(options).positional(positional).run(), args);
        po::notify(args);
    } catch (const po::error& e) {
        std::cerr << "kv_client: " << e.what() << "\n" << options;
        return 2;
    }
    if (args.count("help") != 0) {
        std::cout << options;
        return 0;
    }

    std::vector<std::string> requests;
    if (args.count("command") != 0) {
        requests.push_back(kv::make_request(args["command"].as<std::vector<std::string>>()));
    } else {
        for (std::string line; std::getline(std::cin, line);) {
            if (!line.empty()) {
                requests.push_back(kv::make_request(words_of(line)));
            }
        }
    }
    for (const std::string& r : requests) {
        if (r.empty()) {
            std::cerr << "kv_client: commands are: set KEY VALUE | get KEY | del KEY | keys | size\n";
            return 2;
        }
    }

    try {
        for (const std::string& reply :
            kv::request(args["host"].as<std::string>(), args["port"].as<unsigned short>(), requests)) {
            std::cout << reply << '\n';
        }
    } catch (const boost::system::system_error& e) {
        std::cerr << "kv_client: " << e.code().message() << '\n';
        return 1;
    } catch (const std::exception& e) {
        std::cerr << "kv_client: " << e.what() << '\n';
        return 1;
    }
    return 0;
}
