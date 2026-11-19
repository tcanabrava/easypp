#include "store.hpp"

namespace kv {

void Store::set(const std::string& key, std::string value)
{
    std::lock_guard lock(mutex_);
    values_[key] = std::move(value);
}

std::optional<std::string> Store::get(const std::string& key) const
{
    std::lock_guard lock(mutex_);
    auto it = values_.find(key);
    if (it == values_.end()) {
        return std::nullopt;
    }
    return it->second;
}

bool Store::erase(const std::string& key)
{
    std::lock_guard lock(mutex_);
    return values_.erase(key) > 0;
}

std::vector<std::string> Store::keys() const
{
    std::lock_guard lock(mutex_);
    std::vector<std::string> out;
    out.reserve(values_.size());
    for (const auto& [key, value] : values_) {
        out.push_back(key);
    }
    return out;
}

std::size_t Store::size() const
{
    std::lock_guard lock(mutex_);
    return values_.size();
}

}
