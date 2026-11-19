#pragma once

#include <map>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

namespace kv {

// A thread-safe map from keys to JSON text. Plain C++: it knows nothing about
// networking or JSON, so it is easy to test on its own.
class Store {
public:
    void set(const std::string& key, std::string value);
    std::optional<std::string> get(const std::string& key) const;
    bool erase(const std::string& key);
    std::vector<std::string> keys() const; // sorted
    std::size_t size() const;

private:
    mutable std::mutex mutex_;
    std::map<std::string, std::string> values_;
};

}
