#include "protocol.hpp"

#include <boost/json.hpp>

namespace json = boost::json;

namespace kv {

namespace {

std::string reply(const json::object& object)
{
    return json::serialize(object);
}

std::string error(std::string_view message)
{
    return reply({{"ok", false}, {"error", message}});
}

const json::string* string_field(const json::object& request, std::string_view name)
{
    const json::value* field = request.if_contains(name);
    return field != nullptr ? field->if_string() : nullptr;
}

std::string request(const json::object& object)
{
    return json::serialize(object);
}

}

std::string handle(Store& store, std::string_view line)
{
    boost::system::error_code ec;
    json::value parsed = json::parse(line, ec);
    if (ec) {
        return error("invalid JSON: " + ec.message());
    }
    const json::object* object = parsed.if_object();
    if (object == nullptr) {
        return error("a request must be a JSON object");
    }
    const json::string* op = string_field(*object, "op");
    if (op == nullptr) {
        return error("missing \"op\"");
    }

    if (*op == "keys") {
        json::array keys;
        for (const std::string& key : store.keys()) {
            keys.emplace_back(key);
        }
        return reply({{"ok", true}, {"keys", std::move(keys)}});
    }
    if (*op == "size") {
        return reply({{"ok", true}, {"size", store.size()}});
    }

    const json::string* key = string_field(*object, "key");
    if (key == nullptr) {
        return error("missing \"key\"");
    }
    const std::string name(key->c_str(), key->size());

    if (*op == "set") {
        const json::value* value = object->if_contains("value");
        if (value == nullptr) {
            return error("missing \"value\"");
        }
        store.set(name, json::serialize(*value));
        return reply({{"ok", true}});
    }
    if (*op == "get") {
        auto value = store.get(name);
        if (!value) {
            return error("no such key: " + name);
        }
        return reply({{"ok", true}, {"value", json::parse(*value)}});
    }
    if (*op == "del") {
        return reply({{"ok", true}, {"deleted", store.erase(name)}});
    }
    return error("unknown op: " + std::string(op->c_str(), op->size()));
}

std::string make_set(const std::string& key, const std::string& value)
{
    boost::system::error_code ec;
    json::value parsed = json::parse(value, ec);
    if (ec) {
        parsed = json::string(value);
    }
    return request({{"op", "set"}, {"key", key}, {"value", std::move(parsed)}});
}

std::string make_get(const std::string& key)
{
    return request({{"op", "get"}, {"key", key}});
}

std::string make_del(const std::string& key)
{
    return request({{"op", "del"}, {"key", key}});
}

std::string make_keys()
{
    return request({{"op", "keys"}});
}

std::string make_size()
{
    return request({{"op", "size"}});
}

std::string make_request(const std::vector<std::string>& words)
{
    if (words.size() == 3 && words[0] == "set") {
        return make_set(words[1], words[2]);
    }
    if (words.size() == 2 && words[0] == "get") {
        return make_get(words[1]);
    }
    if (words.size() == 2 && words[0] == "del") {
        return make_del(words[1]);
    }
    if (words.size() == 1 && words[0] == "keys") {
        return make_keys();
    }
    if (words.size() == 1 && words[0] == "size") {
        return make_size();
    }
    return "";
}

}
