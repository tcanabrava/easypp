# asio-kv: a Boost.Asio example for easypp

A JSON key-value store over TCP. It uses three Boost libraries, and easypp
needs no configuration for any of them:

| Library | Kind | What easypp does |
|---|---|---|
| Boost.Asio | header-only | nothing to link |
| Boost.JSON | compiled | `<boost/json.hpp>` links `-lboost_json` |
| Boost.ProgramOptions | compiled | `<boost/program_options.hpp>` links `-lboost_program_options` |

Layout:

```text
src/libs/store      thread-safe map (plain C++)
src/libs/protocol   one JSON object per line (Boost.JSON)
src/libs/net        server: Asio C++20 coroutines on a std::thread pool; client
src/bin/kv_server   --port, --threads (Boost.ProgramOptions); stops on Ctrl+C
src/bin/kv_client   set KEY VALUE | get KEY | del KEY | keys | size, or stdin
src/tests           store, protocol, and an end-to-end server test (16 clients)
```

```sh
easypp run kv_server -- --port 5555 --threads 4      # in one terminal
easypp run kv_client -- set name '"Ada Lovelace"'    # in another
easypp run kv_client -- get name
printf 'set a 1\nkeys\nsize\n' | build/debug/bin/kv_client
easypp test
```
