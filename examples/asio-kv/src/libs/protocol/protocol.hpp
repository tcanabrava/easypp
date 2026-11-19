#pragma once

// The wire protocol: one JSON object per line, in both directions.
//
//   {"op":"set","key":"k","value":<any JSON>}  ->  {"ok":true}
//   {"op":"get","key":"k"}                     ->  {"ok":true,"value":...} or {"ok":false,"error":"..."}
//   {"op":"del","key":"k"}                     ->  {"ok":true,"deleted":true|false}
//   {"op":"keys"}                              ->  {"ok":true,"keys":[...]}
//   {"op":"size"}                              ->  {"ok":true,"size":N}
//
// Boost.JSON is only used in protocol.cpp, but easypp links it into everything
// that uses this library.

#include "store/store.hpp"

#include <string>
#include <string_view>
#include <vector>

namespace kv {

// Answers one request line. Never throws: bad input gets an error reply.
std::string handle(Store& store, std::string_view request);

// Request lines for the client. `value` is JSON text; anything that is not
// valid JSON is sent as a JSON string.
std::string make_set(const std::string& key, const std::string& value);
std::string make_get(const std::string& key);
std::string make_del(const std::string& key);
std::string make_keys();
std::string make_size();

// Builds a request from words, e.g. {"set", "name", "\"Ada\""}. Returns an
// empty string if the words are not a valid command.
std::string make_request(const std::vector<std::string>& words);

}
